// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Functionality for standalone Python distributions. */

use {
    super::binary::{
        EmbeddedPythonBinaryData, EmbeddedResourcesBlobs, PythonBinaryBuilder, PythonLinkingInfo,
    },
    super::bytecode::BytecodeCompiler,
    super::config::{EmbeddedPythonConfig, RawAllocator},
    super::distribution::{
        is_stdlib_test_package, resolve_python_distribution_from_location, DistributionExtractLock,
        ExtensionModuleFilter, PythonDistribution, PythonDistributionLocation,
        PythonModuleSuffixes,
    },
    super::distutils::prepare_hacked_distutils,
    super::embedded_resource::EmbeddedPythonResourcesPrePackaged,
    super::fsscan::{
        find_python_resources, is_package_from_path, walk_tree_files, PythonFileResource,
    },
    super::libpython::{derive_importlib, link_libpython, ImportlibBytecode},
    super::resource::{
        BytecodeModule, BytecodeOptimizationLevel, DataLocation, ExtensionModuleData,
        PythonResource, ResourceData, SourceModule,
    },
    crate::app_packaging::resource::{FileContent, FileManifest},
    crate::licensing::NON_GPL_LICENSES,
    anyhow::{anyhow, Context, Result},
    copy_dir::copy_dir,
    serde::{Deserialize, Serialize},
    slog::{info, warn},
    std::collections::{BTreeMap, BTreeSet, HashMap},
    std::convert::TryFrom,
    std::hash::BuildHasher,
    std::io::{BufRead, BufReader, Read},
    std::iter::FromIterator,
    std::path::{Path, PathBuf},
    tempdir::TempDir,
};

// This needs to be kept in sync with *compiler.py
const PYOXIDIZER_STATE_DIR: &str = "state/pyoxidizer";

#[cfg(windows)]
const PYTHON_EXE_BASENAME: &str = "python.exe";

#[cfg(unix)]
const PYTHON_EXE_BASENAME: &str = "python3";

#[cfg(windows)]
const PIP_EXE_BASENAME: &str = "pip3.exe";

#[cfg(unix)]
const PIP_EXE_BASENAME: &str = "pip3";

#[derive(Debug, Deserialize)]
struct LinkEntry {
    name: String,
    path_static: Option<String>,
    path_dynamic: Option<String>,
    framework: Option<bool>,
    system: Option<bool>,
}

impl LinkEntry {
    /// Convert the instance to a `LibraryDepends`.
    fn to_library_depends(&self, python_path: &Path) -> LibraryDepends {
        LibraryDepends {
            name: self.name.clone(),
            static_path: match &self.path_static {
                Some(p) => Some(python_path.join(p)),
                None => None,
            },
            dynamic_path: match &self.path_dynamic {
                Some(p) => Some(python_path.join(p)),
                None => None,
            },
            framework: match &self.framework {
                Some(v) => *v,
                None => false,
            },
            system: match &self.system {
                Some(v) => *v,
                None => false,
            },
        }
    }
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
    shared_lib: Option<String>,
    variant: String,
}

#[derive(Debug, Deserialize)]
struct PythonBuildCoreInfo {
    objs: Vec<String>,
    links: Vec<LinkEntry>,
    shared_lib: Option<String>,
    static_lib: Option<String>,
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
    link_mode: Option<String>,
    build_info: PythonBuildInfo,
    licenses: Option<Vec<String>>,
    license_path: Option<String>,
    tcl_library_path: Option<String>,
}

fn parse_python_json(path: &Path) -> Result<PythonJsonMain> {
    if !path.exists() {
        panic!("PYTHON.json does not exist; are you using an up-to-date Python distribution that conforms with our requirements?");
    }

    let buf = std::fs::read(path)?;

    let v: PythonJsonMain = serde_json::from_slice(&buf)?;

    Ok(v)
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

#[derive(Debug)]
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

    let bin_dir = if p.join("Scripts").exists() || cfg!(windows) {
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
        unix_lib_dir
    } else if cfg!(windows) {
        p.join("Lib")
    } else {
        unix_lib_dir
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
        .unwrap_or_else(|_| {
            panic!(
                "failed to run {} {}",
                python_paths.python_exe.display(),
                args.join(" ")
            )
        });
    {
        let stdout = cmd.stdout.as_mut().unwrap();
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            warn!(logger, "{}", line.unwrap());
        }
    }
}

pub fn choose_variant<S: BuildHasher>(
    extensions: &[ExtensionModule],
    variants: &Option<HashMap<String, String, S>>,
) -> ExtensionModule {
    if let Some(variants) = variants {
        if let Some(preferred) = variants.get(&extensions[0].module) {
            let mut desired = extensions[0].clone();

            for em in extensions {
                if &em.variant == preferred {
                    desired = em.clone();
                    break;
                }
            }

            desired
        } else {
            extensions[0].clone()
        }
    } else {
        extensions[0].clone()
    }
}

/// Describes a library dependency.
///
/// If the license fields are Some value, then license metadata was
/// present in the distribution. If the values are None, then license
/// metadata is not known.
#[derive(Clone, Debug, PartialEq)]
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
#[derive(Clone, Debug, PartialEq)]
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

    /// Path to a shared library providing this extension module.
    pub shared_library: Option<PathBuf>,

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

#[derive(Debug)]
pub struct PythonDistributionMinimalInfo {
    pub flavor: String,
    pub version: String,
    pub os: String,
    pub arch: String,
    pub py_module_count: usize,
}

/// Describes how libpython is linked in a standalone distribution.
#[derive(Clone, Debug, PartialEq)]
pub enum StandaloneDistributionLinkMode {
    Static,
    Dynamic,
}

/// Represents a standalone Python distribution.
///
/// This is a Python distributed produced by the `python-build-standalone`
/// project. It is derived from a tarball containing a `PYTHON.json` file
/// describing the distribution.
#[allow(unused)]
#[derive(Clone, Debug)]
pub struct StandaloneDistribution {
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

    /// How libpython is linked in this distribution.
    pub link_mode: StandaloneDistributionLinkMode,

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

    /// Filesystem location of pythonXY shared library for this distribution.
    ///
    /// Only set if `link_mode` is `StandaloneDistributionLinkMode::Dynamic`.
    pub libpython_shared_library: Option<PathBuf>,

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

impl StandaloneDistribution {
    pub fn from_location(
        logger: &slog::Logger,
        location: &PythonDistributionLocation,
        distributions_dir: &Path,
    ) -> Result<Self> {
        let (archive_path, extract_path) =
            resolve_python_distribution_from_location(logger, location, distributions_dir)?;

        Self::from_tar_zst_file(logger, &archive_path, &extract_path)
    }

    /// Create an instance from a .tar.zst file.
    ///
    /// The distribution will be extracted to ``extract_dir`` if necessary.
    pub fn from_tar_zst_file(
        logger: &slog::Logger,
        path: &Path,
        extract_dir: &Path,
    ) -> Result<Self> {
        let basename = path
            .file_name()
            .ok_or_else(|| anyhow!("unable to determine filename"))?
            .to_string_lossy();

        if !basename.ends_with(".tar.zst") {
            return Err(anyhow!("unhandled distribution format: {}", path.display()));
        }

        let fh = std::fs::File::open(path)
            .with_context(|| format!("unable to open {}", path.display()))?;

        let reader = BufReader::new(fh);
        warn!(logger, "reading data from Python distribution...");

        Self::from_tar_zst(reader, &extract_dir)
    }

    /// Extract and analyze a standalone distribution from a zstd compressed tar stream.
    pub fn from_tar_zst<R: Read>(source: R, extract_dir: &Path) -> Result<Self> {
        let dctx = zstd::stream::Decoder::new(source)?;

        Self::from_tar(dctx, extract_dir)
    }

    /// Extract and analyze a standalone distribution from a tar stream.
    pub fn from_tar<R: Read>(source: R, extract_dir: &Path) -> Result<Self> {
        let mut tf = tar::Archive::new(source);

        {
            let _lock = DistributionExtractLock::new(extract_dir)?;

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
                        std::fs::set_permissions(entry.path(), permissions).with_context(|| {
                            format!("unable to mark {} as writable", entry.path().display())
                        })?;
                    }
                }
            }
        }

        Self::from_directory(extract_dir)
    }

    /// Obtain an instance by scanning a directory containing an extracted distribution.
    pub fn from_directory(dist_dir: &Path) -> Result<Self> {
        let mut objs_core: BTreeMap<PathBuf, PathBuf> = BTreeMap::new();
        let mut links_core: Vec<LibraryDepends> = Vec::new();
        let mut extension_modules: BTreeMap<String, Vec<ExtensionModule>> = BTreeMap::new();
        let mut includes: BTreeMap<String, PathBuf> = BTreeMap::new();
        let mut libraries: BTreeMap<String, PathBuf> = BTreeMap::new();
        let frozen_c: Vec<u8> = Vec::new();
        let mut py_modules: BTreeMap<String, PathBuf> = BTreeMap::new();
        let mut resources: BTreeMap<String, BTreeMap<String, PathBuf>> = BTreeMap::new();
        let mut license_infos: BTreeMap<String, Vec<LicenseInfo>> = BTreeMap::new();

        for entry in std::fs::read_dir(dist_dir)? {
            let entry = entry?;

            match entry.file_name().to_str() {
                Some("python") => continue,
                Some(value) => panic!("unexpected entry in distribution root directory: {}", value),
                _ => panic!("error listing root directory of Python distribution"),
            };
        }

        let python_path = dist_dir.join("python");

        for entry in std::fs::read_dir(&python_path)? {
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
            let license_text = std::fs::read_to_string(&license_path).with_context(|| {
                format!("unable to read Python license {}", license_path.display())
            })?;

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
            let depends = entry.to_library_depends(&python_path);

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
                    let depends = link.to_library_depends(&python_path);

                    if let Some(p) = &depends.static_path {
                        libraries.insert(depends.name.clone(), p.clone());
                    }

                    links.push(depends);
                }

                if let Some(ref license_paths) = entry.license_paths {
                    let mut licenses = Vec::new();

                    for license_path in license_paths {
                        let license_path = python_path.join(license_path);
                        let license_text = std::fs::read_to_string(&license_path)
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
                    shared_library: match &entry.shared_lib {
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

        let (link_mode, libpython_shared_library) = if let Some(ref v) = pi.link_mode {
            if v == "static" {
                (StandaloneDistributionLinkMode::Static, None)
            } else if v == "shared" {
                (
                    StandaloneDistributionLinkMode::Dynamic,
                    Some(python_path.join(pi.build_info.core.shared_lib.unwrap())),
                )
            } else {
                return Err(anyhow!("unhandled link mode: {}", v));
            }
        } else {
            (StandaloneDistributionLinkMode::Static, None)
        };

        Ok(Self {
            flavor: pi.python_flavor.clone(),
            version: pi.python_version.clone(),
            os: pi.os.clone(),
            arch: pi.arch.clone(),
            python_exe: python_exe_path(dist_dir)?,
            stdlib_path,
            link_mode,
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
            libpython_shared_library,
            py_modules,
            resources,
            license_infos,
            venv_base,
        })
    }

    pub fn as_minimal_info(&self) -> PythonDistributionMinimalInfo {
        PythonDistributionMinimalInfo {
            flavor: self.flavor.clone(),
            version: self.version.clone(),
            os: self.os.clone(),
            arch: self.arch.clone(),
            py_module_count: self.py_modules.len(),
        }
    }

    fn as_embedded_python_resources_pre_packaged(
        &self,
        logger: &slog::Logger,
        extension_module_filter: &ExtensionModuleFilter,
        preferred_extension_module_variants: Option<HashMap<String, String>>,
        include_sources: bool,
        include_resources: bool,
        include_test: bool,
    ) -> Result<EmbeddedPythonResourcesPrePackaged> {
        let mut embedded = EmbeddedPythonResourcesPrePackaged::default();

        // We can only embed extension modules on statically linked distributions.
        if self.link_mode == StandaloneDistributionLinkMode::Static {
            for ext in self.filter_extension_modules(
                logger,
                extension_module_filter,
                preferred_extension_module_variants,
            )? {
                embedded.add_extension_module(&ext);
            }
        }

        for source in self.source_modules()? {
            if !include_test && is_stdlib_test_package(&source.package()) {
                continue;
            }

            if include_sources {
                embedded.add_source_module(&source);
            }

            embedded
                .add_bytecode_module(&source.as_bytecode_module(BytecodeOptimizationLevel::Zero));
        }

        if include_resources {
            for resource in self.resource_datas()? {
                if !include_test && is_stdlib_test_package(&resource.package) {
                    continue;
                }

                embedded.add_resource(&resource);
            }
        }

        Ok(embedded)
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

        prepare_hacked_distutils(logger, &self.stdlib_path.join("distutils"), &venv_base, &[])
            .unwrap();

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
}

impl PythonDistribution for StandaloneDistribution {
    fn clone_box(&self) -> Box<dyn PythonDistribution> {
        Box::new(self.clone())
    }

    fn python_exe_path(&self) -> &Path {
        &self.python_exe
    }

    fn python_major_minor_version(&self) -> String {
        self.version[0..3].to_string()
    }

    fn python_module_suffixes(&self) -> Result<PythonModuleSuffixes> {
        // TODO convey the suffixes in the PYTHON.json file so we can avoid having
        // to invoke the Python interpreter.
        PythonModuleSuffixes::resolve_from_python_exe(&self.python_exe)
    }

    fn create_bytecode_compiler(&self) -> Result<BytecodeCompiler> {
        BytecodeCompiler::new(&self.python_exe)
    }

    fn resolve_importlib_bytecode(&self) -> Result<ImportlibBytecode> {
        let mod_bootstrap_path = &self.py_modules["importlib._bootstrap"];
        let mod_bootstrap_external_path = &self.py_modules["importlib._bootstrap_external"];

        let bootstrap_source = std::fs::read(&mod_bootstrap_path)?;
        let bootstrap_external_source = std::fs::read(&mod_bootstrap_external_path)?;

        let mut compiler = self.create_bytecode_compiler()?;

        derive_importlib(&bootstrap_source, &bootstrap_external_source, &mut compiler)
    }

    fn as_python_executable_builder(
        &self,
        logger: &slog::Logger,
        name: &str,
        config: &EmbeddedPythonConfig,
        extension_module_filter: &ExtensionModuleFilter,
        preferred_extension_module_variants: Option<HashMap<String, String>>,
        include_sources: bool,
        include_resources: bool,
        include_test: bool,
    ) -> Result<Box<dyn PythonBinaryBuilder>> {
        let mut resources = self.as_embedded_python_resources_pre_packaged(
            logger,
            extension_module_filter,
            preferred_extension_module_variants.clone(),
            include_sources,
            include_resources,
            include_test,
        )?;

        // Always ensure minimal extension modules are present, otherwise we get
        // missing symbol errors at link time.
        if self.link_mode == StandaloneDistributionLinkMode::Static {
            for ext in
                self.filter_extension_modules(&logger, &ExtensionModuleFilter::Minimal, None)?
            {
                if !resources.extension_modules.contains_key(&ext.module) {
                    resources.add_extension_module(&ext);
                }
            }
        }

        let python_exe = self.python_exe.clone();
        let importlib_bytecode = self.resolve_importlib_bytecode()?;

        Ok(Box::new(StandalonePythonExecutableBuilder {
            exe_name: name.to_string(),
            distribution: self.clone(),
            resources,
            config: config.clone(),
            python_exe,
            importlib_bytecode,
            extension_module_filter: extension_module_filter.clone(),
            extension_module_variants: preferred_extension_module_variants.clone(),
        }))
    }

    #[allow(clippy::if_same_then_else)]
    fn filter_extension_modules(
        &self,
        logger: &slog::Logger,
        filter: &ExtensionModuleFilter,
        variants: Option<HashMap<String, String>>,
    ) -> Result<Vec<ExtensionModule>> {
        let mut res = Vec::new();

        for (name, ext_variants) in &self.extension_modules {
            match filter {
                ExtensionModuleFilter::Minimal => {
                    let ext_variants = ext_variants
                        .iter()
                        .filter_map(|em| {
                            if em.builtin_default || em.required {
                                Some(em.clone())
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<ExtensionModule>>();

                    if !ext_variants.is_empty() {
                        res.push(choose_variant(&ext_variants, &variants));
                    }
                }

                ExtensionModuleFilter::All => {
                    res.push(choose_variant(&ext_variants, &variants));
                }

                ExtensionModuleFilter::NoLibraries => {
                    let ext_variants = ext_variants
                        .iter()
                        .filter_map(|em| {
                            if em.links.is_empty() {
                                Some(em.clone())
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<ExtensionModule>>();

                    if !ext_variants.is_empty() {
                        res.push(choose_variant(&ext_variants, &variants));
                    }
                }

                ExtensionModuleFilter::NoGPL => {
                    let ext_variants = ext_variants
                        .iter()
                        .filter_map(|em| {
                            if em.links.is_empty() {
                                Some(em.clone())
                            // Public domain is always allowed.
                            } else if em.license_public_domain == Some(true) {
                                Some(em.clone())
                            // Use explicit license list if one is defined.
                            } else if let Some(ref licenses) = em.licenses {
                                // We filter through an allow list because it is safer. (No new GPL
                                // licenses can slip through.)
                                if licenses
                                    .iter()
                                    .all(|license| NON_GPL_LICENSES.contains(&license.as_str()))
                                {
                                    Some(em.clone())
                                } else {
                                    None
                                }
                            } else {
                                // In lack of evidence that it isn't GPL, assume GPL.
                                // TODO consider improving logic here, like allowing known system
                                // and framework libraries to be used.
                                warn!(logger, "unable to determine {} is not GPL; ignoring", &name);
                                None
                            }
                        })
                        .collect::<Vec<ExtensionModule>>();

                    if !ext_variants.is_empty() {
                        res.push(choose_variant(&ext_variants, &variants));
                    }
                }
            }
        }

        // Do a sanity pass to ensure we got all builtin default or required extension modules.
        let added: BTreeSet<String> = BTreeSet::from_iter(res.iter().map(|em| em.module.clone()));

        for (name, ext_variants) in &self.extension_modules {
            let required = ext_variants
                .iter()
                .any(|em| em.builtin_default || em.required);

            if required && !added.contains(name) {
                return Err(anyhow!("required extension module {} missing", name));
            }
        }

        Ok(res)
    }

    fn source_modules(&self) -> Result<Vec<SourceModule>> {
        self.py_modules
            .iter()
            .map(|(name, path)| {
                let is_package = is_package_from_path(&path);

                Ok(SourceModule {
                    name: name.clone(),
                    source: DataLocation::Path(path.clone()),
                    is_package,
                })
            })
            .collect()
    }

    fn resource_datas(&self) -> Result<Vec<ResourceData>> {
        let mut res = Vec::new();

        for (package, inner) in self.resources.iter() {
            for (name, path) in inner.iter() {
                res.push(ResourceData {
                    package: package.clone(),
                    name: name.clone(),
                    data: DataLocation::Path(path.clone()),
                });
            }
        }

        Ok(res)
    }

    /// Ensure pip is available to run in the distribution.
    fn ensure_pip(&self, logger: &slog::Logger) -> Result<PathBuf> {
        let dist_prefix = self.base_dir.join("python").join("install");
        let python_paths = resolve_python_paths(&dist_prefix, &self.version);

        let pip_path = python_paths.bin_dir.join(PIP_EXE_BASENAME);

        if !pip_path.exists() {
            warn!(logger, "{} doesnt exist", pip_path.display().to_string());
            invoke_python(&python_paths, &logger, &["-m", "ensurepip"]);
        }

        Ok(pip_path)
    }

    fn resolve_distutils(
        &self,
        logger: &slog::Logger,
        dest_dir: &Path,
        extra_python_paths: &[&Path],
    ) -> Result<HashMap<String, String>> {
        // We only need to patch distutils if the distribution is statically linked.
        if self.link_mode == StandaloneDistributionLinkMode::Static {
            prepare_hacked_distutils(
                logger,
                &self.stdlib_path.join("distutils"),
                dest_dir,
                extra_python_paths,
            )
        } else {
            Ok(HashMap::new())
        }
    }

    fn filter_compatible_python_resources(
        &self,
        logger: &slog::Logger,
        resources: &[PythonResource],
    ) -> Result<Vec<PythonResource>> {
        Ok(resources
            .iter()
            .filter(|resource| match resource {
                // Standalone extension modules are only compatible with distributions
                // that are dynamically linked.
                // TODO we should be able to support dynamically linked extension
                // modules outside of Windows.
                PythonResource::ExtensionModule { .. } => {
                    if self.link_mode == StandaloneDistributionLinkMode::Static {
                        warn!(
                            logger,
                            "ignoring extension module {} because not compatible with statically linked distribution",
                            resource.full_name()
                        );
                        false
                    } else {
                        true
                    }
                }

                // Only look at the raw object files if the distribution produces
                // them.
                // TODO have PythonDistribution expose API to determine this.
                PythonResource::BuiltExtensionModule(_) =>
                    self.link_mode == StandaloneDistributionLinkMode::Static,

                PythonResource::ModuleSource { .. } => true,
                PythonResource::ModuleBytecodeRequest { .. } => true,
                PythonResource::ModuleBytecode { .. } => true,
                PythonResource::Resource { .. } => true,
            })
            .cloned()
            .collect())
    }
}

/// A self-contained Python executable before it is compiled.
#[derive(Clone, Debug)]
pub struct StandalonePythonExecutableBuilder {
    /// The name of the executable to build.
    exe_name: String,

    /// The Python distribution being used to build this executable.
    ///
    /// TODO replace with just the elements needed to link in order to avoid
    /// a .clone().
    distribution: StandaloneDistribution,

    /// Python resources to be embedded in the binary.
    resources: EmbeddedPythonResourcesPrePackaged,

    /// Configuration of the embedded Python interpreter.
    config: EmbeddedPythonConfig,

    /// Path to python executable that can be invoked at build time.
    python_exe: PathBuf,

    /// Bytecode for importlib bootstrap modules.
    importlib_bytecode: ImportlibBytecode,

    /// Extension module filter to apply.
    extension_module_filter: ExtensionModuleFilter,

    /// Preferred extension module variants.
    extension_module_variants: Option<HashMap<String, String>>,
}

impl StandalonePythonExecutableBuilder {
    /// Build a Python library suitable for linking.
    ///
    /// This will take the underlying distribution, resources, and
    /// configuration and produce a new executable binary.
    fn resolve_python_linking_info(
        &self,
        logger: &slog::Logger,
        host: &str,
        target: &str,
        opt_level: &str,
    ) -> Result<PythonLinkingInfo> {
        let resources = self.resources.package(logger, &self.python_exe)?;

        let libpythonxy_filename;
        let mut cargo_metadata: Vec<String> = Vec::new();
        let libpythonxy_data;
        let libpython_filename: Option<PathBuf>;
        let libpyembeddedconfig_data: Option<Vec<u8>>;
        let libpyembeddedconfig_filename: Option<PathBuf>;

        match self.distribution.link_mode {
            StandaloneDistributionLinkMode::Static => {
                let temp_dir = TempDir::new("pyoxidizer-build-exe")?;
                let temp_dir_path = temp_dir.path();

                warn!(
                    logger,
                    "generating custom link library containing Python..."
                );
                let library_info = link_libpython(
                    logger,
                    &self.distribution,
                    &resources,
                    &temp_dir_path,
                    host,
                    target,
                    opt_level,
                )?;

                libpythonxy_filename =
                    PathBuf::from(library_info.libpython_path.file_name().unwrap());
                cargo_metadata.extend(library_info.cargo_metadata);

                libpythonxy_data = std::fs::read(&library_info.libpython_path)?;
                libpython_filename = None;
                libpyembeddedconfig_filename = Some(PathBuf::from(
                    library_info.libpyembeddedconfig_path.file_name().unwrap(),
                ));
                libpyembeddedconfig_data =
                    Some(std::fs::read(&library_info.libpyembeddedconfig_path)?);
            }

            StandaloneDistributionLinkMode::Dynamic => {
                libpythonxy_filename = PathBuf::from("pythonXY.lib");
                libpythonxy_data = Vec::new();
                libpython_filename = self.distribution.libpython_shared_library.clone();
                libpyembeddedconfig_filename = None;
                libpyembeddedconfig_data = None;
            }
        }

        Ok(PythonLinkingInfo {
            libpythonxy_filename,
            libpythonxy_data,
            libpython_filename,
            libpyembeddedconfig_filename,
            libpyembeddedconfig_data,
            cargo_metadata,
        })
    }
}

impl PythonBinaryBuilder for StandalonePythonExecutableBuilder {
    fn clone_box(&self) -> Box<dyn PythonBinaryBuilder> {
        Box::new(self.clone())
    }

    fn name(&self) -> String {
        self.exe_name.clone()
    }

    fn python_exe_path(&self) -> &Path {
        &self.python_exe
    }

    fn source_modules(&self) -> &BTreeMap<String, SourceModule> {
        &self.resources.source_modules
    }

    fn bytecode_modules(&self) -> &BTreeMap<String, BytecodeModule> {
        &self.resources.bytecode_modules
    }

    fn resources(&self) -> &BTreeMap<String, BTreeMap<String, Vec<u8>>> {
        &self.resources.resources
    }

    fn extension_modules(&self) -> &BTreeMap<String, ExtensionModule> {
        &self.resources.extension_modules
    }

    fn extension_module_datas(&self) -> &BTreeMap<String, ExtensionModuleData> {
        &self.resources.extension_module_datas
    }

    fn add_source_module(&mut self, module: &SourceModule) {
        self.resources.add_source_module(module);
    }

    fn add_bytecode_module(&mut self, module: &BytecodeModule) {
        self.resources.add_bytecode_module(module);
    }

    fn add_resource(&mut self, resource: &ResourceData) {
        self.resources.add_resource(resource);
    }

    fn add_extension_module(&mut self, extension_module: &ExtensionModule) {
        self.resources.add_extension_module(extension_module);
    }

    fn add_extension_module_data(&mut self, extension_module: &ExtensionModuleData) {
        self.resources.add_extension_module_data(extension_module);
    }

    fn filter_resources_from_files(
        &mut self,
        logger: &slog::Logger,
        files: &[&Path],
        glob_patterns: &[&str],
    ) -> Result<()> {
        self.resources
            .filter_from_files(logger, files, glob_patterns)
    }

    fn requires_jemalloc(&self) -> bool {
        self.config.raw_allocator == RawAllocator::Jemalloc
    }

    fn as_embedded_python_binary_data(
        &self,
        logger: &slog::Logger,
        host: &str,
        target: &str,
        opt_level: &str,
    ) -> Result<EmbeddedPythonBinaryData> {
        let linking_info = self.resolve_python_linking_info(logger, host, target, opt_level)?;

        let resources =
            EmbeddedResourcesBlobs::try_from(self.resources.package(logger, &self.python_exe)?)?;
        warn!(
            logger,
            "deriving custom importlib modules to support in-memory importing"
        );
        let importlib = self.importlib_bytecode.clone();

        Ok(EmbeddedPythonBinaryData {
            config: self.config.clone(),
            linking_info,
            importlib,
            resources,
            host: host.to_string(),
            target: target.to_string(),
        })
    }

    fn extra_install_files(&self, logger: &slog::Logger, prefix: &str) -> Result<FileManifest> {
        let mut m = FileManifest::default();

        if self.distribution.link_mode == StandaloneDistributionLinkMode::Dynamic {
            if let Some(p) = &self.distribution.libpython_shared_library {
                let manifest_path = Path::new(prefix).join(p.file_name().unwrap());
                let content = FileContent {
                    data: std::fs::read(&p)?,
                    executable: false,
                };

                m.add_file(&manifest_path, &content)?;
            }

            for em in self.distribution.filter_extension_modules(
                logger,
                &self.extension_module_filter,
                self.extension_module_variants.clone(),
            )? {
                if let Some(p) = &em.shared_library {
                    m.add_file(
                        &Path::new(prefix).join(p.file_name().unwrap()),
                        &FileContent {
                            data: std::fs::read(p)?,
                            executable: false,
                        },
                    )?;
                }
            }
        }

        Ok(m)
    }
}

#[cfg(test)]
pub mod tests {
    use {
        super::*, crate::py_packaging::standalone_distribution::ExtensionModuleFilter,
        crate::testutil::*, std::ops::Deref,
    };

    pub fn get_standalone_executable_builder(
        logger: &slog::Logger,
    ) -> Result<StandalonePythonExecutableBuilder> {
        let distribution = get_default_distribution()?;
        let mut resources = EmbeddedPythonResourcesPrePackaged::default();

        // We need to add minimal extension modules so builds actually work. If they are missing,
        // we'll get missing symbol errors during linking.
        if distribution.link_mode == StandaloneDistributionLinkMode::Static {
            for ext in distribution.filter_extension_modules(
                logger,
                &ExtensionModuleFilter::Minimal,
                None,
            )? {
                resources.add_extension_module(&ext);
            }
        }

        let config = EmbeddedPythonConfig::default();

        let python_exe = distribution.python_exe.clone();
        let importlib_bytecode = distribution.resolve_importlib_bytecode()?;

        Ok(StandalonePythonExecutableBuilder {
            exe_name: "testapp".to_string(),
            distribution: distribution.deref().deref().clone(),
            resources,
            config,
            python_exe,
            importlib_bytecode,
            extension_module_filter: ExtensionModuleFilter::Minimal,
            extension_module_variants: None,
        })
    }

    pub fn get_embedded(logger: &slog::Logger) -> Result<EmbeddedPythonBinaryData> {
        let exe = get_standalone_executable_builder(logger)?;
        exe.as_embedded_python_binary_data(&get_logger()?, env!("HOST"), env!("HOST"), "0")
    }

    #[test]
    fn test_write_embedded_files() -> Result<()> {
        let logger = get_logger()?;
        let embedded = get_embedded(&logger)?;
        let temp_dir = tempdir::TempDir::new("pyoxidizer-test")?;

        embedded.write_files(temp_dir.path())?;

        Ok(())
    }
}
