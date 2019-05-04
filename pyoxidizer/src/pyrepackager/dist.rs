// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use url::Url;

use super::config::Config;
use super::fsscan::{find_python_resources, walk_tree_files, PythonResourceType};

#[derive(Debug, Deserialize)]
struct LinkEntry {
    name: String,
    path_static: Option<String>,
    path_dynamic: Option<String>,
    framework: Option<bool>,
    system: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct PythonBuildExtensionInfo {
    in_core: bool,
    init_fn: String,
    links: Vec<LinkEntry>,
    objs: Vec<String>,
    static_lib: Option<String>,
    variant: String,
}

#[derive(Debug, Deserialize)]
struct PythonBuildCoreInfo {
    objs: Vec<String>,
    links: Vec<LinkEntry>,
}

#[derive(Debug, Deserialize)]
struct PythonBuildInfo {
    core: PythonBuildCoreInfo,
    extensions: BTreeMap<String, Vec<PythonBuildExtensionInfo>>,
}

#[derive(Debug, Deserialize)]
struct PythonJsonMain {
    arch: String,
    os: String,
    python_exe: String,
    python_flavor: String,
    python_include: String,
    python_stdlib: String,
    python_version: String,
    version: String,
    build_info: PythonBuildInfo,
}

fn parse_python_json(path: &Path) -> PythonJsonMain {
    if !path.exists() {
        panic!("PYTHON.json does not exist; are you using an up-to-date Python distribution that conforms with our requirements?");
    }

    let buf = fs::read(path).expect("failed to read PYTHON.json");

    let v: PythonJsonMain = serde_json::from_slice(&buf).expect("failed to parse JSON");

    v
}

/// Represents contents of the config.c/config.c.in file.
#[derive(Debug)]
#[allow(unused)]
pub struct ConfigC {
    pub init_funcs: Vec<String>,
    pub init_mods: BTreeMap<String, String>,
}

/// Describes a library dependency.
#[derive(Clone, Debug)]
pub struct LibraryDepends {
    /// Name of the library we depend on.
    pub name: String,

    /// Path to a file providing a static version of this library.
    pub static_path: Option<PathBuf>,

    /// Path to a file providing a dynamic version of this library.
    pub dynamic_path: Option<PathBuf>,

    /// Whether this is a system framework.
    pub framework: bool,

    /// Whether this is a system library.
    pub system: bool,
}

/// Describes an extension module in a Python distribution.
#[derive(Clone, Debug)]
pub struct ExtensionModule {
    /// Name of the Python module this extension module provides.
    pub module: String,

    /// Module initialization function.
    ///
    /// If None, there is no module initialization function. This is
    /// typically represented as NULL in Python's inittab.
    pub init_fn: Option<String>,

    /// Whether the extension module is built-in by default.
    ///
    /// Some extension modules are always compiled into libpython.
    /// This field will be true for those modules.
    pub builtin_default: bool,

    /// Whether the extension module can be disabled.
    ///
    /// On some distributions, built-in extension modules cannot be
    /// disabled. This field describes whether they can be.
    pub disableable: bool,

    /// Compiled object files providing this extension module.
    pub object_paths: Vec<PathBuf>,

    /// Path to static library providing this extension module.
    pub static_library: Option<PathBuf>,

    /// Library linking metadata.
    pub links: Vec<LibraryDepends>,

    /// Name of the variant of this extension module.
    pub variant: String,
}

fn link_entry_to_library_depends(entry: &LinkEntry, python_path: &PathBuf) -> LibraryDepends {
    LibraryDepends {
        name: entry.name.clone(),
        static_path: match &entry.path_static {
            Some(p) => Some(python_path.join(p)),
            None => None,
        },
        dynamic_path: match &entry.path_dynamic {
            Some(_p) => panic!("dynamic_path not yet supported"),
            None => None,
        },
        framework: match &entry.framework {
            Some(v) => *v,
            None => false,
        },
        system: match &entry.system {
            Some(v) => *v,
            None => false,
        },
    }
}

/// Represents a parsed Python distribution.
///
/// Distribution info is typically derived from a tarball containing a
/// Python install and its build artifacts.
#[allow(unused)]
#[derive(Debug)]
pub struct PythonDistributionInfo {
    /// Directory where distribution lives in the filesystem.
    pub temp_dir: tempdir::TempDir,

    /// Python distribution flavor.
    pub flavor: String,

    /// Python version string.
    pub version: String,

    /// Operating system this Python runs on.
    pub os: String,

    /// Architecture this Python runs on.
    pub arch: String,

    /// Object files providing the core Python implementation.
    ///
    /// Keys are relative paths. Values are filesystem paths.
    pub objs_core: BTreeMap<PathBuf, PathBuf>,

    /// Linking information for the core Python implementation.
    pub links_core: Vec<LibraryDepends>,

    /// Extension modules available to this distribution.
    pub extension_modules: BTreeMap<String, Vec<ExtensionModule>>,

    pub frozen_c: Vec<u8>,

    /// Include files for Python.
    ///
    /// Keys are relative paths. Values are filesystem paths.
    pub includes: BTreeMap<String, PathBuf>,

    /// Static libraries available for linking.
    ///
    /// Keys are library names, without the "lib" prefix or file extension.
    /// Values are filesystem paths where library is located.
    pub libraries: BTreeMap<String, PathBuf>,

    pub py_modules: BTreeMap<String, PathBuf>,

    /// Non-module Python resource files.
    ///
    /// Keys are full module/resource names. Values are filesystem paths.
    pub resources: BTreeMap<String, PathBuf>,
}

/// Extract useful information from the files constituting a Python distribution.
///
/// Passing in a data structure with raw file data within is inefficient. But
/// it makes things easier to implement and allows us to do things like consume
/// tarballs without filesystem I/O.
pub fn analyze_python_distribution_data(
    temp_dir: tempdir::TempDir,
) -> Result<PythonDistributionInfo, &'static str> {
    let mut objs_core: BTreeMap<PathBuf, PathBuf> = BTreeMap::new();
    let mut links_core: Vec<LibraryDepends> = Vec::new();
    let mut extension_modules: BTreeMap<String, Vec<ExtensionModule>> = BTreeMap::new();
    let mut includes: BTreeMap<String, PathBuf> = BTreeMap::new();
    let mut libraries: BTreeMap<String, PathBuf> = BTreeMap::new();
    let frozen_c: Vec<u8> = Vec::new();
    let mut py_modules: BTreeMap<String, PathBuf> = BTreeMap::new();
    let mut resources: BTreeMap<String, PathBuf> = BTreeMap::new();

    for entry in fs::read_dir(temp_dir.path()).unwrap() {
        let entry = entry.expect("unable to get directory entry");

        match entry.file_name().to_str() {
            Some("python") => continue,
            Some(value) => panic!("unexpected entry in distribution root directory: {}", value),
            _ => panic!("error listing root directory of Python distribution"),
        };
    }

    let python_path = temp_dir.path().join("python");

    for entry in fs::read_dir(&python_path).unwrap() {
        let entry = entry.expect("unable to get directory entry");

        match entry.file_name().to_str() {
            Some("build") => continue,
            Some("install") => continue,
            Some("lib") => continue,
            Some("LICENSE.rst") => continue,
            Some("PYTHON.json") => continue,
            Some(value) => panic!("unexpected entry in python/ directory: {}", value),
            _ => panic!("error listing python/ directory"),
        };
    }

    let python_json_path = python_path.join("PYTHON.json");
    let pi = parse_python_json(&python_json_path);

    // Collect object files for libpython.
    for obj in &pi.build_info.core.objs {
        let rel_path = PathBuf::from(obj);
        let full_path = python_path.join(obj);

        objs_core.insert(rel_path, full_path);
    }

    for entry in &pi.build_info.core.links {
        let depends = link_entry_to_library_depends(entry, &python_path);

        if let Some(p) = &depends.static_path {
            libraries.insert(depends.name.clone(), p.clone());
        }

        links_core.push(depends);
    }

    // Collect extension modules.
    for (module, variants) in &pi.build_info.extensions {
        let mut ems: Vec<ExtensionModule> = Vec::new();

        for entry in variants.iter() {
            let object_paths = entry.objs.iter().map(|p| python_path.join(p)).collect();
            let mut links = Vec::new();

            for link in &entry.links {
                let depends = link_entry_to_library_depends(link, &python_path);

                if let Some(p) = &depends.static_path {
                    libraries.insert(depends.name.clone(), p.clone());
                }

                links.push(depends);
            }

            ems.push(ExtensionModule {
                module: module.clone(),
                init_fn: Some(entry.init_fn.clone()),
                builtin_default: entry.in_core,
                disableable: !entry.in_core,
                object_paths,
                static_library: match &entry.static_lib {
                    Some(p) => Some(python_path.join(p)),
                    None => None,
                },
                links,
                variant: entry.variant.clone(),
            });
        }

        extension_modules.insert(module.clone(), ems);
    }

    let include_path = python_path.join(pi.python_include);

    for entry in walk_tree_files(&include_path) {
        let full_path = entry.path();
        let rel_path = full_path
            .strip_prefix(&include_path)
            .expect("unable to strip prefix");
        includes.insert(
            String::from(rel_path.to_str().expect("path to string")),
            full_path.to_path_buf(),
        );
    }

    let stdlib_path = python_path.join(pi.python_stdlib);

    for entry in find_python_resources(&stdlib_path) {
        match entry.flavor {
            PythonResourceType::Resource => {
                resources.insert(entry.name.clone(), entry.path);
                ()
            }
            PythonResourceType::Source => {
                py_modules.insert(entry.name.clone(), entry.path);
                ()
            }
            _ => (),
        };
    }

    Ok(PythonDistributionInfo {
        flavor: pi.python_flavor.clone(),
        version: pi.python_version.clone(),
        os: pi.os.clone(),
        arch: pi.arch.clone(),
        temp_dir,
        extension_modules,
        frozen_c,
        includes,
        links_core,
        libraries,
        objs_core,
        py_modules,
        resources,
    })
}

/// Extract Python distribution data from a tar archive.
pub fn analyze_python_distribution_tar<R: Read>(
    source: R,
) -> Result<PythonDistributionInfo, &'static str> {
    let mut tf = tar::Archive::new(source);

    let temp_dir =
        tempdir::TempDir::new("python-distribution").expect("could not create temp directory");
    let temp_dir_path = temp_dir.path();

    tf.unpack(&temp_dir_path)
        .expect("unable to extract tar archive");

    analyze_python_distribution_data(temp_dir)
}

/// Extract Python distribution data from a zstandard compressed tar archive.
pub fn analyze_python_distribution_tar_zst<R: Read>(
    source: R,
) -> Result<PythonDistributionInfo, &'static str> {
    let dctx = zstd::stream::Decoder::new(source).unwrap();

    analyze_python_distribution_tar(dctx)
}

/// Obtain a local Path for a Python distribution tar archive.
///
/// Takes a parsed config and a cache directory as input. Usually the cache
/// directory is the OUT_DIR for the invocation of a Cargo build script.
/// A Python distribution will be fetched according to the configuration and a
/// copy of the archive placed in ``cache_dir``. If the archive already exists
/// in ``cache_dir``, it will be verified and returned.
///
/// Local filesystem paths are preferred over remote URLs if both are defined.
pub fn resolve_python_distribution_archive(config: &Config, cache_dir: &Path) -> PathBuf {
    let expected_hash = hex::decode(&config.python_distribution_sha256).unwrap();

    let basename = match &config.python_distribution_path {
        Some(path) => {
            let p = Path::new(path);
            p.file_name().unwrap().to_str().unwrap().to_string()
        }
        None => match &config.python_distribution_url {
            Some(url) => {
                let url = Url::parse(url).expect("failed to parse URL");
                url.path_segments()
                    .expect("cannot be base path")
                    .last()
                    .expect("could not get last element")
                    .to_string()
            }
            None => panic!("neither local path nor URL defined for distribution"),
        },
    };

    let cache_path = cache_dir.join(basename);

    if cache_path.exists() {
        let mut hasher = Sha256::new();
        let mut fh = File::open(&cache_path).unwrap();
        let mut data = Vec::new();
        fh.read_to_end(&mut data).unwrap();
        hasher.input(data);

        let file_hash = hasher.result().to_vec();

        // We don't care about timing side-channels from the string compare.
        if file_hash == expected_hash {
            return cache_path;
        }
    }

    match &config.python_distribution_path {
        Some(path) => {
            let mut hasher = Sha256::new();
            let mut fh = File::open(path).unwrap();
            let mut data = Vec::new();
            fh.read_to_end(&mut data).unwrap();
            hasher.input(data);

            let file_hash = hasher.result().to_vec();

            if file_hash != expected_hash {
                panic!("sha256 of Python distribution does not validate");
            }

            std::fs::copy(path, &cache_path).unwrap();
            cache_path
        }
        None => match &config.python_distribution_url {
            Some(url) => {
                let mut data: Vec<u8> = Vec::new();

                let mut response = reqwest::get(url).expect("unable to perform HTTP request");
                response
                    .read_to_end(&mut data)
                    .expect("unable to download URL");

                let mut hasher = Sha256::new();
                hasher.input(&data);

                let url_hash = hasher.result().to_vec();
                if url_hash != expected_hash {
                    panic!("sha256 of Python distribution does not validate");
                }

                fs::write(&cache_path, data).expect("unable to write file");
                cache_path
            }
            None => panic!("expected distribution path or URL"),
        },
    }
}
