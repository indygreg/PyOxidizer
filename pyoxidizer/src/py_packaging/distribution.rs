// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use anyhow::{anyhow, Context, Result};
use copy_dir::copy_dir;
use fs2::FileExt;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use slog::{info, warn};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::fs::{create_dir_all, File};
use std::io::{BufRead, BufReader, Cursor, Read};
use std::path::{Path, PathBuf};
use url::Url;
use uuid::Uuid;

use super::distutils::prepare_hacked_distutils;
use super::fsscan::{
    find_python_resources, is_package_from_path, walk_tree_files, PythonFileResource,
};
use super::resource::{ResourceData, SourceModule};

use crate::licensing::NON_GPL_LICENSES;
use crate::python_distributions::CPYTHON_BY_TRIPLE;

#[cfg(windows)]
const PYTHON_EXE_BASENAME: &str = "python.exe";

#[cfg(unix)]
const PYTHON_EXE_BASENAME: &str = "python3";

#[cfg(windows)]
const PIP_EXE_BASENAME: &str = "pip3.exe";

#[cfg(unix)]
const PIP_EXE_BASENAME: &str = "pip3";

// This needs to be kept in sync with *compiler.py
const PYOXIDIZER_STATE_DIR: &str = "state/pyoxidizer";

const STDLIB_TEST_PACKAGES: &[&str] = &[
    "bsddb.test",
    "ctypes.test",
    "distutils.tests",
    "email.test",
    "idlelib.idle_test",
    "json.tests",
    "lib-tk.test",
    "lib2to3.tests",
    "sqlite3.test",
    "test",
    "tkinter.test",
    "unittest.test",
];

pub fn is_stdlib_test_package(name: &str) -> bool {
    for package in STDLIB_TEST_PACKAGES {
        let prefix = format!("{}.", package);

        if &name == package || name.starts_with(&prefix) {
            return true;
        }
    }

    false
}

#[derive(Clone, Debug, PartialEq)]
pub enum PythonDistributionLocation {
    Local { local_path: String, sha256: String },
    Url { url: String, sha256: String },
}

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
    licenses: Option<Vec<String>>,
    license_paths: Option<Vec<String>>,
    license_public_domain: Option<bool>,
    links: Vec<LinkEntry>,
    objs: Vec<String>,
    required: bool,
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
    licenses: Option<Vec<String>>,
    license_path: Option<String>,
    tcl_library_path: Option<String>,
}

fn parse_python_json(path: &Path) -> Result<PythonJsonMain> {
    if !path.exists() {
        panic!("PYTHON.json does not exist; are you using an up-to-date Python distribution that conforms with our requirements?");
    }

    let buf = fs::read(path)?;

    let v: PythonJsonMain = serde_json::from_slice(&buf)?;

    Ok(v)
}

/// Represents contents of the config.c/config.c.in file.
#[derive(Debug)]
#[allow(unused)]
pub struct ConfigC {
    pub init_funcs: Vec<String>,
    pub init_mods: BTreeMap<String, String>,
}

/// Describes a library dependency.
///
/// If the license fields are Some value, then license metadata was
/// present in the distribution. If the values are None, then license
/// metadata is not known.
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

    /// Whether the extension must be loaded to initialize Python.
    pub required: bool,

    /// Name of the variant of this extension module.
    pub variant: String,

    /// SPDX license shortnames that apply to this library dependency.
    pub licenses: Option<Vec<String>>,

    /// Path to file holding license text for this library.
    pub license_paths: Option<Vec<PathBuf>>,

    /// Whether the license for this library is in the public domain.
    pub license_public_domain: Option<bool>,
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

/// Describes license information for a library.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LicenseInfo {
    /// SPDX license shortnames.
    pub licenses: Vec<String>,
    /// Suggested filename for the license.
    pub license_filename: String,
    /// Text of the license.
    pub license_text: String,
}

/// Represents a parsed Python distribution.
///
/// Distribution info is typically derived from a tarball containing a
/// Python install and its build artifacts.
#[allow(unused)]
#[derive(Clone, Debug)]
pub struct ParsedPythonDistribution {
    /// Directory where distribution lives in the filesystem.
    pub base_dir: PathBuf,

    /// Python distribution flavor.
    pub flavor: String,

    /// Python version string.
    pub version: String,

    /// Operating system this Python runs on.
    pub os: String,

    /// Architecture this Python runs on.
    pub arch: String,

    /// Path to Python interpreter executable.
    pub python_exe: PathBuf,

    /// Path to Python standard library.
    pub stdlib_path: PathBuf,

    /// SPDX license shortnames that apply to this distribution.
    ///
    /// Licenses only cover the core distribution. Licenses for libraries
    /// required by extensions are stored next to the extension's linking
    /// info.
    pub licenses: Option<Vec<String>>,

    /// Path to file holding license text for this distribution.
    pub license_path: Option<PathBuf>,

    /// Path to Tcl library files.
    pub tcl_library_path: Option<PathBuf>,

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
    /// Keys are package names. Values are maps of resource name to data for the resource
    /// within that package.
    pub resources: BTreeMap<String, BTreeMap<String, PathBuf>>,

    /// Describes license info for things in this distribution.
    pub license_infos: BTreeMap<String, Vec<LicenseInfo>>,

    /// Path to copy of hacked dist to use for packaging rules venvs
    pub venv_base: PathBuf,
}

#[derive(Debug)]
pub struct PythonDistributionMinimalInfo {
    pub flavor: String,
    pub version: String,
    pub os: String,
    pub arch: String,
    pub extension_modules: Vec<String>,
    pub libraries: Vec<String>,
    pub py_module_count: usize,
}

/// Denotes methods to filter extension modules.
#[derive(Clone, Debug, PartialEq)]
pub enum ExtensionModuleFilter {
    Minimal,
    All,
    NoLibraries,
    NoGPL,
}

pub struct PythonPaths {
    pub prefix: PathBuf,
    pub bin_dir: PathBuf,
    pub python_exe: PathBuf,
    pub stdlib: PathBuf,
    pub site_packages: PathBuf,
    pub pyoxidizer_state_dir: PathBuf,
}

/// Resolve the location of Python modules given a base install path.
pub fn resolve_python_paths(base: &Path, python_version: &str) -> PythonPaths {
    let prefix = base.to_path_buf();

    let p = prefix.clone();

    let bin_dir = if p.join("Scripts").exists() {
        p.join("Scripts")
    } else {
        p.join("bin")
    };

    let python_exe = if bin_dir.join(PYTHON_EXE_BASENAME).exists() {
        bin_dir.join(PYTHON_EXE_BASENAME)
    } else {
        p.join(PYTHON_EXE_BASENAME)
    };

    let mut pyoxidizer_state_dir = p.clone();
    pyoxidizer_state_dir.extend(PYOXIDIZER_STATE_DIR.split('/'));

    let unix_lib_dir = p
        .join("lib")
        .join(format!("python{}", &python_version[0..3]));

    let stdlib = if unix_lib_dir.exists() {
        unix_lib_dir.clone()
    } else {
        p.join("Lib")
    };

    let site_packages = stdlib.join("site-packages");

    PythonPaths {
        prefix,
        bin_dir,
        python_exe,
        stdlib,
        site_packages,
        pyoxidizer_state_dir,
    }
}

pub fn invoke_python(python_paths: &PythonPaths, logger: &slog::Logger, args: &[&str]) {
    let site_packages_s = python_paths.site_packages.display().to_string();

    if site_packages_s.starts_with("\\\\?\\") {
        panic!("Unexpected Windows UNC path in site-packages path");
    }

    info!(logger, "setting PYTHONPATH {}", site_packages_s);

    let mut extra_envs = HashMap::new();
    extra_envs.insert("PYTHONPATH".to_string(), site_packages_s);

    info!(
        logger,
        "running {} {}",
        python_paths.python_exe.display(),
        args.join(" ")
    );

    let mut cmd = std::process::Command::new(&python_paths.python_exe)
        .args(args)
        .envs(&extra_envs)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect(
            format!(
                "failed to run {} {}",
                python_paths.python_exe.display(),
                args.join(" ")
            )
            .as_str(),
        );
    {
        let stdout = cmd.stdout.as_mut().unwrap();
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            warn!(logger, "{}", line.unwrap());
        }
    }
}

impl ParsedPythonDistribution {
    pub fn from_path(
        logger: &slog::Logger,
        path: &Path,
        extract_dir: &Path,
    ) -> Result<ParsedPythonDistribution> {
        let mut fh =
            fs::File::open(path).with_context(|| format!("unable to open {}", path.display()))?;

        let mut python_distribution_data = Vec::new();
        fh.read_to_end(&mut python_distribution_data)?;
        let dist_cursor = Cursor::new(python_distribution_data);
        warn!(logger, "reading data from Python distribution...");
        analyze_python_distribution_tar_zst(dist_cursor, &extract_dir)
    }

    pub fn as_minimal_info(&self) -> PythonDistributionMinimalInfo {
        PythonDistributionMinimalInfo {
            flavor: self.flavor.clone(),
            version: self.version.clone(),
            os: self.os.clone(),
            arch: self.arch.clone(),
            extension_modules: self.extension_modules.keys().cloned().collect_vec(),
            libraries: self.libraries.keys().cloned().collect_vec(),
            py_module_count: self.py_modules.len(),
        }
    }

    /// Ensure pip is available to run in the distribution.
    pub fn ensure_pip(&self, logger: &slog::Logger) -> PathBuf {
        let dist_prefix = self.base_dir.join("python").join("install");
        let python_paths = resolve_python_paths(&dist_prefix, &self.version);

        let pip_path = python_paths.bin_dir.join(PIP_EXE_BASENAME);

        if !pip_path.exists() {
            warn!(logger, "{} doesnt exist", pip_path.display().to_string());
            invoke_python(&python_paths, &logger, &["-m", "ensurepip"]);
        }

        pip_path
    }

    /// Duplicate the python distribution, with distutils hacked
    pub fn create_hacked_base(&self, logger: &slog::Logger) -> PythonPaths {
        let venv_base = self.venv_base.clone();

        let venv_dir_s = self.venv_base.display().to_string();

        if !venv_base.exists() {
            let dist_prefix = self.base_dir.join("python").join("install");

            copy_dir(&dist_prefix, &venv_base).unwrap();

            let dist_prefix_s = dist_prefix.display().to_string();
            warn!(
                logger,
                "copied {} to create hacked base {}", dist_prefix_s, venv_dir_s
            );
        }

        let python_paths = resolve_python_paths(&venv_base, &self.version);

        invoke_python(&python_paths, &logger, &["-m", "ensurepip"]);

        prepare_hacked_distutils(logger, self, &venv_base, &[]).unwrap();

        python_paths
    }

    /// Create a venv from the distribution at path.
    pub fn create_venv(&self, logger: &slog::Logger, path: &Path) -> PythonPaths {
        let venv_dir_s = path.display().to_string();

        // This will recreate it, if it was deleted
        let python_paths = self.create_hacked_base(&logger);

        if path.exists() {
            warn!(logger, "re-using {} {}", "venv", venv_dir_s);
        } else {
            warn!(logger, "creating {} {}", "venv", venv_dir_s);
            invoke_python(&python_paths, &logger, &["-m", "venv", venv_dir_s.as_str()]);
        }

        resolve_python_paths(&path, &self.version)
    }

    /// Create or re-use an existing venv
    pub fn prepare_venv(
        &self,
        logger: &slog::Logger,
        venv_dir_path: &Path,
    ) -> Result<(PythonPaths, HashMap<String, String>)> {
        let python_paths = self.create_venv(logger, &venv_dir_path);

        let mut extra_envs = HashMap::new();

        let prefix_s = python_paths.prefix.display().to_string();

        let venv_path_bin_s = python_paths.bin_dir.display().to_string();

        let path_separator = if cfg!(windows) { ";" } else { ":" };

        if let Ok(path) = std::env::var("PATH") {
            extra_envs.insert(
                "PATH".to_string(),
                format!("{}{}{}", venv_path_bin_s, path_separator, path),
            );
        } else {
            extra_envs.insert("PATH".to_string(), venv_path_bin_s);
        }

        let site_packages_s = python_paths.site_packages.display().to_string();
        if site_packages_s.starts_with("\\\\?\\") {
            panic!("unexpected Windows UNC path in site-packages");
        }

        extra_envs.insert("VIRTUAL_ENV".to_string(), prefix_s);
        extra_envs.insert("PYTHONPATH".to_string(), site_packages_s);

        extra_envs.insert("PYOXIDIZER".to_string(), "1".to_string());

        Ok((python_paths, extra_envs))
    }

    /// Obtain resolved `SourceModule` instances for this distribution.
    ///
    /// This effectively resolves the raw file content for .py files into
    /// `SourceModule` instances.
    pub fn source_modules(&self) -> Result<Vec<SourceModule>> {
        self.py_modules
            .iter()
            .map(|(name, path)| {
                let is_package = is_package_from_path(&path);
                let source = fs::read(&path)?;

                Ok(SourceModule {
                    name: name.clone(),
                    source,
                    is_package,
                })
            })
            .collect()
    }

    /// Obtain resolved `ResourceData` instances for this distribution.
    ///
    /// This effectively resolves the raw file content for resource files
    /// into `ResourceData` instances.
    pub fn resources_data(&self) -> Result<Vec<ResourceData>> {
        let mut res = Vec::new();

        for (package, inner) in self.resources.iter() {
            for (name, path) in inner.iter() {
                res.push(ResourceData {
                    package: package.clone(),
                    name: name.clone(),
                    data: fs::read(&path)?,
                });
            }
        }

        Ok(res)
    }

    pub fn filter_extension_modules(
        &self,
        logger: &slog::Logger,
        filter: &ExtensionModuleFilter,
    ) -> Vec<ExtensionModule> {
        let mut res = Vec::new();

        for (name, variants) in &self.extension_modules {
            match filter {
                ExtensionModuleFilter::Minimal => {
                    let em = &variants[0];

                    if em.builtin_default || em.required {
                        res.push(em.clone());
                    }
                }

                ExtensionModuleFilter::All => {
                    let em = &variants[0];
                    res.push(em.clone());
                }

                ExtensionModuleFilter::NoLibraries => {
                    for em in variants {
                        if em.links.is_empty() {
                            res.push(em.clone());

                            break;
                        }
                    }
                }

                ExtensionModuleFilter::NoGPL => {
                    for em in variants {
                        let suitable = if em.links.is_empty() {
                            true
                        } else {
                            // Public domain is always allowed.
                            if em.license_public_domain == Some(true) {
                                true
                            // Use explicit license list if one is defined.
                            } else if let Some(ref licenses) = em.licenses {
                                // We filter through an allow list because it is safer. (No new GPL
                                // licenses can slip through.)
                                licenses
                                    .iter()
                                    .all(|license| NON_GPL_LICENSES.contains(&license.as_str()))
                            } else {
                                // In lack of evidence that it isn't GPL, assume GPL.
                                // TODO consider improving logic here, like allowing known system
                                // and framework libraries to be used.
                                warn!(logger, "unable to determine {} is not GPL; ignoring", &name);
                                false
                            }
                        };

                        if suitable {
                            res.push(em.clone());

                            break;
                        }
                    }
                }
            }
        }

        res
    }
}

fn parse_python_json_from_distribution(dist_dir: &Path) -> Result<PythonJsonMain> {
    let python_json_path = dist_dir.join("python").join("PYTHON.json");
    parse_python_json(&python_json_path)
}

/// Resolve the path to a `python` executable in a Python distribution.
pub fn python_exe_path(dist_dir: &Path) -> Result<PathBuf> {
    let pi = parse_python_json_from_distribution(dist_dir)?;

    Ok(dist_dir.join("python").join(&pi.python_exe))
}

/// Extract useful information from the files constituting a Python distribution.
///
/// Passing in a data structure with raw file data within is inefficient. But
/// it makes things easier to implement and allows us to do things like consume
/// tarballs without filesystem I/O.
pub fn analyze_python_distribution_data(dist_dir: &Path) -> Result<ParsedPythonDistribution> {
    let mut objs_core: BTreeMap<PathBuf, PathBuf> = BTreeMap::new();
    let mut links_core: Vec<LibraryDepends> = Vec::new();
    let mut extension_modules: BTreeMap<String, Vec<ExtensionModule>> = BTreeMap::new();
    let mut includes: BTreeMap<String, PathBuf> = BTreeMap::new();
    let mut libraries: BTreeMap<String, PathBuf> = BTreeMap::new();
    let frozen_c: Vec<u8> = Vec::new();
    let mut py_modules: BTreeMap<String, PathBuf> = BTreeMap::new();
    let mut resources: BTreeMap<String, BTreeMap<String, PathBuf>> = BTreeMap::new();
    let mut license_infos: BTreeMap<String, Vec<LicenseInfo>> = BTreeMap::new();

    for entry in fs::read_dir(dist_dir)? {
        let entry = entry?;

        match entry.file_name().to_str() {
            Some("python") => continue,
            Some(value) => panic!("unexpected entry in distribution root directory: {}", value),
            _ => panic!("error listing root directory of Python distribution"),
        };
    }

    let python_path = dist_dir.join("python");

    for entry in fs::read_dir(&python_path)? {
        let entry = entry?;

        match entry.file_name().to_str() {
            Some("build") => continue,
            Some("install") => continue,
            Some("lib") => continue,
            Some("licenses") => continue,
            Some("LICENSE.rst") => continue,
            Some("PYTHON.json") => continue,
            Some(value) => panic!("unexpected entry in python/ directory: {}", value),
            _ => panic!("error listing python/ directory"),
        };
    }

    let pi = parse_python_json_from_distribution(dist_dir)?;

    if let Some(ref python_license_path) = pi.license_path {
        let license_path = python_path.join(python_license_path);
        let license_text = fs::read_to_string(&license_path)
            .with_context(|| format!("unable to read Python license {}", license_path.display()))?;

        let mut licenses = Vec::new();
        licenses.push(LicenseInfo {
            licenses: pi.licenses.clone().unwrap(),
            license_filename: "LICENSE.python.txt".to_string(),
            license_text,
        });

        license_infos.insert("python".to_string(), licenses);
    }

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

            if let Some(ref license_paths) = entry.license_paths {
                let mut licenses = Vec::new();

                for license_path in license_paths {
                    let license_path = python_path.join(license_path);
                    let license_text = fs::read_to_string(&license_path)
                        .with_context(|| "unable to read license file")?;

                    licenses.push(LicenseInfo {
                        licenses: entry.licenses.clone().unwrap(),
                        license_filename: license_path
                            .file_name()
                            .unwrap()
                            .to_str()
                            .unwrap()
                            .to_string(),
                        license_text,
                    });
                }

                license_infos.insert(module.clone(), licenses);
            }

            ems.push(ExtensionModule {
                module: module.clone(),
                init_fn: Some(entry.init_fn.clone()),
                builtin_default: entry.in_core,
                disableable: !entry.in_core,
                license_public_domain: entry.license_public_domain,
                license_paths: match entry.license_paths {
                    Some(ref refs) => Some(refs.iter().map(|p| python_path.join(p)).collect()),
                    None => None,
                },
                licenses: entry.licenses.clone(),
                object_paths,
                required: entry.required,
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
        match entry {
            PythonFileResource::Resource(resource) => {
                if !resources.contains_key(&resource.package) {
                    resources.insert(resource.package.clone(), BTreeMap::new());
                }

                resources
                    .get_mut(&resource.package)
                    .unwrap()
                    .insert(resource.stem.clone(), resource.path);
            }
            PythonFileResource::Source {
                full_name, path, ..
            } => {
                py_modules.insert(full_name.clone(), path);
            }
            _ => {}
        };
    }

    let venv_base = dist_dir.parent().unwrap().join("hacked_base");

    Ok(ParsedPythonDistribution {
        flavor: pi.python_flavor.clone(),
        version: pi.python_version.clone(),
        os: pi.os.clone(),
        arch: pi.arch.clone(),
        python_exe: python_exe_path(dist_dir)?,
        stdlib_path,
        licenses: pi.licenses.clone(),
        license_path: match pi.license_path {
            Some(ref path) => Some(PathBuf::from(path)),
            None => None,
        },
        tcl_library_path: match pi.tcl_library_path {
            Some(ref path) => Some(PathBuf::from(path)),
            None => None,
        },
        base_dir: dist_dir.to_path_buf(),
        extension_modules,
        frozen_c,
        includes,
        links_core,
        libraries,
        objs_core,
        py_modules,
        resources,
        license_infos,
        venv_base,
    })
}

/// Extract Python distribution data from a tar archive.
pub fn analyze_python_distribution_tar<R: Read>(
    source: R,
    extract_dir: &Path,
) -> Result<ParsedPythonDistribution> {
    let mut tf = tar::Archive::new(source);

    // Multiple threads or processes could race to extract the archive.
    // So we use a lock file to ensure exclusive access.
    // TODO use more granular lock based on the output directory (possibly
    // by putting lock in output directory itself).
    let lock_path = extract_dir
        .parent()
        .unwrap()
        .join("distribution-extract-lock");

    let file = File::create(&lock_path)
        .with_context(|| format!("could not create {}", lock_path.display()))?;

    file.lock_exclusive()
        .with_context(|| format!("failed to obtain lock for {}", lock_path.display()))?;

    // The content of the distribution could change between runs. But caching
    // the extraction does keep things fast.
    let test_path = extract_dir.join("python").join("PYTHON.json");
    if !test_path.exists() {
        std::fs::create_dir_all(extract_dir)?;
        let absolute_path = std::fs::canonicalize(extract_dir)?;
        tf.unpack(&absolute_path)
            .with_context(|| "unable to extract tar archive")?;

        // Ensure unpacked files are writable. We've had issues where we
        // consume archives with read-only file permissions. When we later
        // copy these files, we can run into trouble overwriting a read-only
        // file.
        let walk = walkdir::WalkDir::new(&absolute_path);
        for entry in walk.into_iter() {
            let entry = entry?;

            let metadata = entry.metadata()?;
            let mut permissions = metadata.permissions();

            if permissions.readonly() {
                permissions.set_readonly(false);
                fs::set_permissions(entry.path(), permissions).with_context(|| {
                    format!("unable to mark {} as writable", entry.path().display())
                })?;
            }
        }
    }

    file.unlock()
        .with_context(|| format!("releasing lock on {}", lock_path.display()))?;

    analyze_python_distribution_data(extract_dir)
}

/// Extract Python distribution data from a zstandard compressed tar archive.
pub fn analyze_python_distribution_tar_zst<R: Read>(
    source: R,
    extract_dir: &Path,
) -> Result<ParsedPythonDistribution> {
    let dctx = zstd::stream::Decoder::new(source)?;

    analyze_python_distribution_tar(dctx, extract_dir)
}

fn sha256_path(path: &PathBuf) -> Vec<u8> {
    let mut hasher = Sha256::new();
    let fh = File::open(&path).unwrap();
    let mut reader = std::io::BufReader::new(fh);

    let mut buffer = [0; 32768];

    loop {
        let count = reader.read(&mut buffer).expect("error reading");
        if count == 0 {
            break;
        }
        hasher.input(&buffer[..count]);
    }

    hasher.result().to_vec()
}

pub fn get_http_client() -> reqwest::Result<reqwest::Client> {
    let mut builder = reqwest::ClientBuilder::new();

    for (key, value) in std::env::vars() {
        let key = key.to_lowercase();
        if key.ends_with("_proxy") {
            let end = key.len() - "_proxy".len();
            let schema = &key[..end];

            if let Ok(url) = Url::parse(&value) {
                if let Some(proxy) = match schema {
                    "http" => Some(reqwest::Proxy::http(url.as_str())),
                    "https" => Some(reqwest::Proxy::https(url.as_str())),
                    _ => None,
                } {
                    if let Ok(proxy) = proxy {
                        builder = builder.proxy(proxy);
                    }
                }
            }
        }
    }

    builder.build()
}

/// Ensure a Python distribution at a URL is available in a local directory.
///
/// The path to the downloaded and validated file is returned.
pub fn download_distribution(url: &str, sha256: &str, cache_dir: &Path) -> PathBuf {
    let expected_hash = hex::decode(sha256).expect("could not parse SHA256 hash");
    let u = Url::parse(url).expect("failed to parse URL");

    let basename = u
        .path_segments()
        .expect("cannot be base path")
        .last()
        .expect("could not get final URL path element")
        .to_string();

    let cache_path = cache_dir.join(basename);

    if cache_path.exists() {
        let file_hash = sha256_path(&cache_path);

        // We don't care about timing side-channels from the string compare.
        if file_hash == expected_hash {
            return cache_path;
        }
    }

    let mut data: Vec<u8> = Vec::new();

    println!("downloading {}", u);
    let client = get_http_client().expect("unable to get HTTP client");
    let mut response = client
        .get(u.as_str())
        .send()
        .expect("unable to perform HTTP request");
    response
        .read_to_end(&mut data)
        .expect("unable to download URL");

    let mut hasher = Sha256::new();
    hasher.input(&data);

    let url_hash = hasher.result().to_vec();
    if url_hash != expected_hash {
        panic!("sha256 of Python distribution does not validate");
    }

    let mut temp_cache_path = cache_path.clone();
    temp_cache_path.set_file_name(format!("{}.tmp", Uuid::new_v4()));

    fs::write(&temp_cache_path, data).expect("unable to write file");

    fs::rename(&temp_cache_path, &cache_path)
        .or_else(|e| {
            fs::remove_file(&temp_cache_path).expect("unable to remove temp file");

            if cache_path.exists() {
                download_distribution(url, sha256, cache_dir);
                return Ok(());
            }

            Err(e)
        })
        .expect("unable to rename downloaded file");

    cache_path
}

pub fn copy_local_distribution(path: &PathBuf, sha256: &str, cache_dir: &Path) -> PathBuf {
    let expected_hash = hex::decode(sha256).expect("could not parse SHA256 hash");
    let basename = path.file_name().unwrap().to_str().unwrap().to_string();
    let cache_path = cache_dir.join(basename);

    if cache_path.exists() {
        let file_hash = sha256_path(&cache_path);

        if file_hash == expected_hash {
            println!(
                "existing {} passes SHA-256 integrity check",
                cache_path.display()
            );
            return cache_path;
        }
    }

    let source_hash = sha256_path(&path);

    if source_hash != expected_hash {
        panic!("sha256 of Python distribution does not validate");
    }

    println!("copying {}", path.display());
    std::fs::copy(path, &cache_path).unwrap();

    cache_path
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
pub fn resolve_python_distribution_archive(
    dist: &PythonDistributionLocation,
    cache_dir: &Path,
) -> PathBuf {
    if !cache_dir.exists() {
        create_dir_all(cache_dir).unwrap();
    }

    match dist {
        PythonDistributionLocation::Local { local_path, sha256 } => {
            let p = PathBuf::from(local_path);
            copy_local_distribution(&p, sha256, cache_dir)
        }
        PythonDistributionLocation::Url { url, sha256 } => {
            download_distribution(url, sha256, cache_dir)
        }
    }
}

/// Resolve a parsed distribution from a location and local filesystem path.
///
/// The distribution will be copied and extracted into the destination
/// directory. It will be parsed from the extracted location.
///
/// The created files outlive the returned object.
pub fn resolve_parsed_distribution(
    logger: &slog::Logger,
    location: &PythonDistributionLocation,
    dest_dir: &Path,
) -> Result<ParsedPythonDistribution> {
    warn!(logger, "resolving Python distribution {:?}", location);
    let path = resolve_python_distribution_archive(location, dest_dir);
    warn!(
        logger,
        "Python distribution available at {}",
        path.display()
    );

    let distribution_hash = match location {
        PythonDistributionLocation::Local { sha256, .. } => sha256,
        PythonDistributionLocation::Url { sha256, .. } => sha256,
    };

    let distribution_path = dest_dir.join(format!("python.{}", distribution_hash));

    ParsedPythonDistribution::from_path(logger, &path, &distribution_path)
}

/// Resolve the default Python distribution for a build target.
pub fn default_distribution(
    logger: &slog::Logger,
    target: &str,
    dest_dir: &Path,
) -> Result<ParsedPythonDistribution> {
    let dist = CPYTHON_BY_TRIPLE.get(target).ok_or(anyhow!(
        "could not find default Python distribution for {}",
        target
    ))?;

    let location = PythonDistributionLocation::Url {
        url: dist.url.clone(),
        sha256: dist.sha256.clone(),
    };

    resolve_parsed_distribution(logger, &location, dest_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;

    #[test]
    fn test_default_distribution() -> Result<()> {
        let logger = get_logger()?;
        let target = env!("HOST");

        let temp_dir = tempdir::TempDir::new("pyoxidizer-test")?;

        default_distribution(&logger, target, temp_dir.path())?;

        Ok(())
    }
}
