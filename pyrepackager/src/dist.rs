// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use itertools::Itertools;
use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{PathBuf, Path};

use crate::fsscan::{find_python_resources, PythonResourceType, walk_tree_files};

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
}

#[derive(Debug, Deserialize)]
struct PythonBuildCoreInfo {
    objs: Vec<String>,
    links: Vec<LinkEntry>,
}

#[derive(Debug, Deserialize)]
struct PythonBuildInfo {
    core: PythonBuildCoreInfo,
    extensions: BTreeMap<String, PythonBuildExtensionInfo>,
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
}

#[derive(Clone, Debug)]
pub struct SetupEntry {
    pub module: String,
    pub object_filenames: Vec<String>,
    pub libraries: Vec<String>,
    pub frameworks: Vec<String>,
}

/// Parse a line in CPython's Setup.dist/Setup.local file.
fn parse_setup_line(modules: &mut BTreeMap<String, SetupEntry>, line: &str) {
    let line = match line.find("#") {
        Some(idx) => &line[0..idx],
        None => &line,
    };

    if line.len() < 1 {
        return;
    }

    // Lines have format: <module_name> <args>
    let words = line.split_whitespace().collect_vec();

    if words.len() < 2 {
        return;
    }

    let module = words[0];
    let mut object_filenames: Vec<String> = Vec::new();
    let mut libraries: Vec<String> = Vec::new();
    let mut frameworks: Vec<String> = Vec::new();

    for (idx, &word) in words.iter().enumerate() {
        // Object files are the basename of sources with the extension changed.
        if word.ends_with(".c") {
            let p = PathBuf::from(&word);
            let p = p.with_extension("o");
            let basename = p.file_name().unwrap().to_str().unwrap();
            object_filenames.push(basename.to_string());

        }
        else if word.starts_with("-l") {
            libraries.push(word[2..].to_string());
        }
        else if word == "-framework" {
            frameworks.push(String::from(words[idx + 1]));
        }
    }

    let entry = SetupEntry {
        module: module.to_string(),
        object_filenames,
        libraries,
        frameworks,
    };

    modules.insert(module.to_string(), entry);
}

/// Parse CPython's Setup.dist file.
fn parse_setup_dist(modules: &mut BTreeMap<String, SetupEntry>, data: &Vec<u8>) {
    let reader = BufReader::new(&**data);

    let mut found_start = false;

    for line in reader.lines() {
        let line = line.expect("could not obtain line");
        if !found_start {
            if line.starts_with("PYTHONPATH=") {
                found_start = true;
                continue;
            }
        }

        parse_setup_line(modules, &line);
    }
}

/// Parse CPython's Setup.local file.
fn parse_setup_local(modules: &mut BTreeMap<String, SetupEntry>, data: &Vec<u8>) {
    let reader = BufReader::new(&**data);

    for line in reader.lines() {
        let line = line.expect("could not obtain line");

        // Everything after the *disabled* line can be ignored.
        if line == "*disabled*" {
            break;
        }
        else if line == "*static*" {
            continue;
        }

        parse_setup_line(modules, &line);
    }
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
        framework: false,
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

    /// Object files providing the core Python implementation.
    ///
    /// Keys are relative paths. Values are filesystem paths.
    pub objs_core: BTreeMap<PathBuf, PathBuf>,

    /// Linking information for the core Python implementation.
    pub links_core: Vec<LibraryDepends>,

    /// Extension modules available to this distribution.
    pub extension_modules: BTreeMap<String, ExtensionModule>,

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
pub fn analyze_python_distribution_data(temp_dir: tempdir::TempDir) -> Result<PythonDistributionInfo, &'static str> {
    let mut objs_core: BTreeMap<PathBuf, PathBuf> = BTreeMap::new();
    let mut links_core: Vec<LibraryDepends> = Vec::new();
    let mut extension_modules: BTreeMap<String, ExtensionModule> = BTreeMap::new();
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
    for (module, entry) in &pi.build_info.extensions {
        let object_paths = entry.objs.iter().map(|p| python_path.join(p)).collect();
        let mut links = Vec::new();

        for link in &entry.links {
            let depends = link_entry_to_library_depends(link, &python_path);

            if let Some(p) = &depends.static_path {
                libraries.insert(depends.name.clone(), p.clone());
            }

            links.push(depends);
        }

        extension_modules.insert(module.clone(), ExtensionModule {
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
        });
    }

    let include_path = python_path.join(pi.python_include);

    for entry in walk_tree_files(&include_path) {
        let full_path = entry.path();
        let rel_path = full_path.strip_prefix(&include_path).expect("unable to strip prefix");
        includes.insert(String::from(rel_path.to_str().expect("path to string")), full_path.to_path_buf());
    }

    let stdlib_path = python_path.join(pi.python_stdlib);

    for entry in find_python_resources(&stdlib_path) {
        match entry.flavor {
            PythonResourceType::Resource => {
                resources.insert(entry.name.clone(), entry.path);
                ()
            },
            PythonResourceType::Source => {
                py_modules.insert(entry.name.clone(), entry.path);
                ()
            }
            _ => (),
        };
    }

    Ok(PythonDistributionInfo {
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
pub fn analyze_python_distribution_tar<R: Read>(source: R) -> Result<PythonDistributionInfo, &'static str> {
    let mut tf = tar::Archive::new(source);

    let temp_dir = tempdir::TempDir::new("python-distribution").expect("could not create temp directory");
    let temp_dir_path = temp_dir.path();

    tf.unpack(&temp_dir_path).expect("unable to extract tar archive");

    analyze_python_distribution_data(temp_dir)
}

/// Extract Python distribution data from a zstandard compressed tar archive.
pub fn analyze_python_distribution_tar_zst<R: Read>(source: R) -> Result<PythonDistributionInfo, &'static str> {
    let dctx = zstd::stream::Decoder::new(source).unwrap();

    analyze_python_distribution_tar(dctx)
}
