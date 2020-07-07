// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Functionality for standalone Python distributions. */

use {
    super::binary::{
        EmbeddedPythonBinaryData, EmbeddedResourcesBlobs, LibpythonLinkMode, PythonBinaryBuilder,
        PythonLinkingInfo,
    },
    super::config::{EmbeddedPythonConfig, RawAllocator},
    super::distribution::{
        is_stdlib_test_package, resolve_python_distribution_from_location, BinaryLibpythonLinkMode,
        DistributionExtractLock, ExtensionModuleFilter, PythonDistribution,
        PythonDistributionLocation,
    },
    super::distutils::prepare_hacked_distutils,
    super::embedded_resource::{EmbeddedPythonResources, PrePackagedResources},
    super::libpython::link_libpython,
    super::packaging_tool::{find_resources, pip_install, read_virtualenv, setup_py_install},
    crate::app_packaging::resource::FileContent,
    crate::licensing::NON_GPL_LICENSES,
    anyhow::{anyhow, Context, Result},
    copy_dir::copy_dir,
    lazy_static::lazy_static,
    python_packaging::bytecode::BytecodeCompiler,
    python_packaging::filesystem_scanning::{find_python_resources, walk_tree_files},
    python_packaging::module_util::{is_package_from_path, PythonModuleSuffixes},
    python_packaging::resource::{
        BytecodeOptimizationLevel, DataLocation, PythonExtensionModule,
        PythonModuleBytecodeFromSource, PythonModuleSource, PythonPackageDistributionResource,
        PythonPackageResource, PythonResource,
    },
    python_packaging::resource_collection::PythonResourcesPolicy,
    serde::{Deserialize, Serialize},
    slog::{info, warn},
    std::collections::{BTreeMap, BTreeSet, HashMap},
    std::convert::TryFrom,
    std::hash::BuildHasher,
    std::io::{BufRead, BufReader, Read},
    std::iter::FromIterator,
    std::path::{Path, PathBuf},
    std::sync::Arc,
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

lazy_static! {
    /// Distribution extensions with known problems on Linux.
    ///
    /// These will never be packaged.
    pub static ref BROKEN_EXTENSIONS_LINUX: Vec<String> = vec![
        // Linking issues.
        "_crypt".to_string(),
        // Linking issues.
        "nis".to_string(),
    ];

    /// Distribution extensions with known problems on macOS.
    ///
    /// These will never be packaged.
    pub static ref BROKEN_EXTENSIONS_MACOS: Vec<String> = vec![
        // curses and readline have linking issues.
        "curses".to_string(),
        "_curses_panel".to_string(),
        "readline".to_string(),
    ];
}

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
    inittab_object: String,
    inittab_source: String,
    inittab_cflags: Vec<String>,
    object_file_format: String,
}

#[derive(Debug, Deserialize)]
struct PythonJsonMain {
    version: String,
    target_triple: String,
    optimizations: String,
    python_tag: String,
    python_abi_tag: Option<String>,
    python_platform_tag: String,
    python_implementation_cache_tag: String,
    python_implementation_hex_version: u64,
    python_implementation_name: String,
    python_implementation_version: Vec<String>,
    python_version: String,
    python_major_minor_version: String,
    python_paths: HashMap<String, String>,
    python_exe: String,
    python_stdlib_test_packages: Vec<String>,
    python_suffixes: HashMap<String, Vec<String>>,
    python_bytecode_magic_number: String,
    python_symbol_visibility: String,
    python_extension_module_loading: Vec<String>,
    libpython_link_mode: String,
    crt_features: Vec<String>,
    run_tests: String,
    build_info: PythonBuildInfo,
    licenses: Option<Vec<String>>,
    license_path: Option<String>,
    tcl_library_path: Option<String>,
    tcl_library_paths: Option<Vec<String>>,
}

fn parse_python_json(path: &Path) -> Result<PythonJsonMain> {
    if !path.exists() {
        return Err(anyhow!("PYTHON.json does not exist; are you using an up-to-date Python distribution that conforms with our requirements?"));
    }

    let buf = std::fs::read(path)?;

    let value: serde_json::Value = serde_json::from_slice(&buf)?;
    let o = value
        .as_object()
        .ok_or_else(|| anyhow!("PYTHON.json does not parse to an object"))?;

    match o.get("version") {
        Some(version) => {
            let version = version
                .as_str()
                .ok_or_else(|| anyhow!("unable to parse version as a string"))?;

            if version != "5" {
                return Err(anyhow!(
                    "expected version 5 standalone distribution; found version {}",
                    version
                ));
            }
        }
        None => return Err(anyhow!("version key not present in PYTHON.json")),
    }

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
    extensions: &[DistributionExtensionModule],
    variants: &Option<HashMap<String, String, S>>,
) -> DistributionExtensionModule {
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
pub struct DistributionExtensionModule {
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

    /// Rust target triple that this distribution runs on.
    pub target_triple: String,

    /// PEP 425 Python tag value.
    pub python_tag: String,

    /// PEP 425 Python ABI tag.
    pub python_abi_tag: Option<String>,

    /// PEP 425 Python platform tag.
    pub python_platform_tag: String,

    /// Python version string.
    pub version: String,

    /// Path to Python interpreter executable.
    pub python_exe: PathBuf,

    /// Path to Python standard library.
    pub stdlib_path: PathBuf,

    /// How libpython is linked in this distribution.
    link_mode: StandaloneDistributionLinkMode,

    /// Symbol visibility for Python symbols.
    python_symbol_visibility: String,

    /// Capabilities of distribution to load extension modules.
    extension_module_loading: Vec<String>,

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
    libpython_shared_library: Option<PathBuf>,

    /// Extension modules available to this distribution.
    pub extension_modules: BTreeMap<String, Vec<DistributionExtensionModule>>,

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

    /// Path to object file defining _PyImport_Inittab.
    pub inittab_object: PathBuf,

    /// Compiler flags to use to build object containing _PyImport_Inittab.
    pub inittab_cflags: Vec<String>,

    /// Tag to apply to bytecode files.
    ///
    /// e.g. `cpython-37`.
    pub cache_tag: String,

    /// Suffixes for Python module types.
    module_suffixes: PythonModuleSuffixes,
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
    #[allow(clippy::cognitive_complexity)]
    pub fn from_directory(dist_dir: &Path) -> Result<Self> {
        let mut objs_core: BTreeMap<PathBuf, PathBuf> = BTreeMap::new();
        let mut links_core: Vec<LibraryDepends> = Vec::new();
        let mut extension_modules: BTreeMap<String, Vec<DistributionExtensionModule>> =
            BTreeMap::new();
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
                Some(value) => {
                    return Err(anyhow!(
                        "unexpected entry in distribution root directory: {}",
                        value
                    ))
                }
                _ => {
                    return Err(anyhow!(
                        "error listing root directory of Python distribution"
                    ))
                }
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
                Some(value) => {
                    return Err(anyhow!("unexpected entry in python/ directory: {}", value))
                }
                _ => return Err(anyhow!("error listing python/ directory")),
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

        let module_suffixes = PythonModuleSuffixes {
            source: pi
                .python_suffixes
                .get("source")
                .ok_or_else(|| anyhow!("distribution does not define source suffixes"))?
                .clone(),
            bytecode: pi
                .python_suffixes
                .get("bytecode")
                .ok_or_else(|| anyhow!("distribution does not define bytecode suffixes"))?
                .clone(),
            debug_bytecode: pi
                .python_suffixes
                .get("debug_bytecode")
                .ok_or_else(|| anyhow!("distribution does not define debug bytecode suffixes"))?
                .clone(),
            optimized_bytecode: pi
                .python_suffixes
                .get("optimized_bytecode")
                .ok_or_else(|| anyhow!("distribution does not define optimized bytecode suffixes"))?
                .clone(),
            extension: pi
                .python_suffixes
                .get("extension")
                .ok_or_else(|| anyhow!("distribution does not define extension suffixes"))?
                .clone(),
        };

        // Collect extension modules.
        for (module, variants) in &pi.build_info.extensions {
            let mut ems: Vec<DistributionExtensionModule> = Vec::new();

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

                ems.push(DistributionExtensionModule {
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

        let include_path = if let Some(p) = pi.python_paths.get("include") {
            python_path.join(p)
        } else {
            return Err(anyhow!("include path not defined in distribution"));
        };

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

        let stdlib_path = if let Some(p) = pi.python_paths.get("stdlib") {
            python_path.join(p)
        } else {
            return Err(anyhow!("stdlib path not defined in distribution"));
        };

        for entry in find_python_resources(
            &stdlib_path,
            &pi.python_implementation_cache_tag,
            &module_suffixes,
        ) {
            match entry? {
                PythonResource::Resource(resource) => {
                    if !resources.contains_key(&resource.leaf_package) {
                        resources.insert(resource.leaf_package.clone(), BTreeMap::new());
                    }

                    resources.get_mut(&resource.leaf_package).unwrap().insert(
                        resource.relative_name.clone(),
                        match resource.data {
                            DataLocation::Path(path) => path,
                            DataLocation::Memory(_) => {
                                return Err(anyhow!(
                                    "should not have received in-memory resource data"
                                ))
                            }
                        },
                    );
                }
                PythonResource::ModuleSource(source) => match source.source {
                    DataLocation::Path(path) => {
                        py_modules.insert(source.name.clone(), path);
                    }
                    DataLocation::Memory(_) => {
                        return Err(anyhow!("should not have received in-memory source data"))
                    }
                },
                _ => {}
            };
        }

        let venv_base = dist_dir.parent().unwrap().join("hacked_base");

        let (link_mode, libpython_shared_library) = if pi.libpython_link_mode == "static" {
            (StandaloneDistributionLinkMode::Static, None)
        } else if pi.libpython_link_mode == "shared" {
            (
                StandaloneDistributionLinkMode::Dynamic,
                Some(python_path.join(pi.build_info.core.shared_lib.unwrap())),
            )
        } else {
            return Err(anyhow!("unhandled link mode: {}", pi.libpython_link_mode));
        };

        let inittab_object = python_path.join(pi.build_info.inittab_object);

        Ok(Self {
            base_dir: dist_dir.to_path_buf(),
            target_triple: pi.target_triple,
            python_tag: pi.python_tag,
            python_abi_tag: pi.python_abi_tag,
            python_platform_tag: pi.python_platform_tag,
            version: pi.python_version.clone(),
            python_exe: python_exe_path(dist_dir)?,
            stdlib_path,
            link_mode,
            python_symbol_visibility: pi.python_symbol_visibility,
            extension_module_loading: pi.python_extension_module_loading,
            licenses: pi.licenses.clone(),
            license_path: match pi.license_path {
                Some(ref path) => Some(PathBuf::from(path)),
                None => None,
            },
            tcl_library_path: match pi.tcl_library_path {
                Some(ref path) => Some(PathBuf::from(path)),
                None => None,
            },

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
            inittab_object,
            inittab_cflags: pi.build_info.inittab_cflags,
            cache_tag: pi.python_implementation_cache_tag,
            module_suffixes,
        })
    }

    /// Duplicate the python distribution, with distutils hacked
    #[allow(unused)]
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
    #[allow(unused)]
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
    #[allow(unused)]
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

    /// Whether the distribution is capable of loading filed-based Python extension modules.
    pub fn is_extension_module_file_loadable(&self) -> bool {
        self.extension_module_loading
            .contains(&"shared-library".to_string())
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

    fn cache_tag(&self) -> &str {
        &self.cache_tag
    }

    fn python_module_suffixes(&self) -> Result<PythonModuleSuffixes> {
        Ok(self.module_suffixes.clone())
    }

    fn create_bytecode_compiler(&self) -> Result<BytecodeCompiler> {
        BytecodeCompiler::new(&self.python_exe)
    }

    fn as_python_executable_builder(
        &self,
        logger: &slog::Logger,
        host_triple: &str,
        target_triple: &str,
        name: &str,
        libpython_link_mode: BinaryLibpythonLinkMode,
        resources_policy: &PythonResourcesPolicy,
        config: &EmbeddedPythonConfig,
        extension_module_filter: &ExtensionModuleFilter,
        preferred_extension_module_variants: Option<HashMap<String, String>>,
        include_sources: bool,
        include_resources: bool,
        include_test: bool,
    ) -> Result<Box<dyn PythonBinaryBuilder>> {
        let python_exe = self.python_exe.clone();

        let (supports_static, supports_dynamic) = if self.target_triple.contains("pc-windows") {
            // On Windows, the symbol visibility dictates which link modes are
            // supported.
            if self.python_symbol_visibility == "dllexport" {
                (false, true)
            } else {
                (true, false)
            }
        } else if self.target_triple.contains("linux-musl") {
            // Musl binaries don't support dynamic linking.
            (true, false)
        } else {
            // Elsewhere we can choose which link mode to use.
            (true, true)
        };

        let link_mode = match libpython_link_mode {
            BinaryLibpythonLinkMode::Default => {
                if supports_static {
                    LibpythonLinkMode::Static
                } else if supports_dynamic {
                    LibpythonLinkMode::Dynamic
                } else {
                    return Err(anyhow!("no link modes supported; please report this bug"));
                }
            }
            BinaryLibpythonLinkMode::Static => {
                if !supports_static {
                    return Err(anyhow!(
                        "Python distribution does not support statically linking libpython"
                    ));
                }

                LibpythonLinkMode::Static
            }
            BinaryLibpythonLinkMode::Dynamic => {
                if !supports_dynamic {
                    return Err(anyhow!(
                        "Python distribution does not support dynamically linking libpython"
                    ));
                }

                LibpythonLinkMode::Dynamic
            }
        };

        let mut builder = Box::new(StandalonePythonExecutableBuilder {
            host_triple: host_triple.to_string(),
            target_triple: target_triple.to_string(),
            exe_name: name.to_string(),
            // TODO can we avoid this clone()?
            distribution: Arc::new(Box::new(self.clone())),
            link_mode,
            resources_policy: resources_policy.clone(),
            resources: PrePackagedResources::new(resources_policy, &self.cache_tag),
            config: config.clone(),
            python_exe,
            extension_module_filter: extension_module_filter.clone(),
            extension_module_variants: preferred_extension_module_variants,
        });

        builder.add_distribution_resources(
            logger,
            extension_module_filter,
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
                builder
                    .resources
                    .add_builtin_distribution_extension_module(&ext)?;
            }
        }

        Ok(builder)
    }

    #[allow(clippy::if_same_then_else, clippy::eq_op)]
    fn filter_extension_modules(
        &self,
        logger: &slog::Logger,
        filter: &ExtensionModuleFilter,
        variants: Option<HashMap<String, String>>,
    ) -> Result<Vec<DistributionExtensionModule>> {
        let mut res = Vec::new();

        for (name, ext_variants) in &self.extension_modules {
            // TODO use target triple
            if (cfg!(target_os = "linux") && BROKEN_EXTENSIONS_LINUX.contains(name))
                || (cfg!(target_os = "macos") && BROKEN_EXTENSIONS_MACOS.contains(name))
            {
                info!(
                    logger,
                    "ignoring extension module {} because it is broken on this platform", name
                );
                continue;
            }

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
                        .collect::<Vec<DistributionExtensionModule>>();

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
                        .collect::<Vec<DistributionExtensionModule>>();

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
                        .collect::<Vec<DistributionExtensionModule>>();

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

    fn source_modules(&self) -> Result<Vec<PythonModuleSource>> {
        self.py_modules
            .iter()
            .map(|(name, path)| {
                let is_package = is_package_from_path(&path);

                Ok(PythonModuleSource {
                    name: name.clone(),
                    source: DataLocation::Path(path.clone()),
                    is_package,
                    cache_tag: self.cache_tag.clone(),
                })
            })
            .collect()
    }

    fn resource_datas(&self) -> Result<Vec<PythonPackageResource>> {
        let mut res = Vec::new();

        for (package, inner) in self.resources.iter() {
            for (name, path) in inner.iter() {
                res.push(PythonPackageResource {
                    leaf_package: package.clone(),
                    relative_name: name.clone(),
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
        libpython_link_mode: LibpythonLinkMode,
        dest_dir: &Path,
        extra_python_paths: &[&Path],
    ) -> Result<HashMap<String, String>> {
        match libpython_link_mode {
            // We need to patch distutils if the distribution is statically linked.
            LibpythonLinkMode::Static => prepare_hacked_distutils(
                logger,
                &self.stdlib_path.join("distutils"),
                dest_dir,
                extra_python_paths,
            ),
            LibpythonLinkMode::Dynamic => Ok(HashMap::new()),
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
                // Extension modules defined as shared libraries are only compatible
                // with some configurations.
                PythonResource::ExtensionModuleDynamicLibrary { .. } => {
                    if self.is_extension_module_file_loadable() {
                        true
                    } else {
                        warn!(logger, "ignoring extension module {} because it isn't loadable for the target configuration",
                            resource.full_name());
                        false
                    }
                }

                // Only look at the raw object files if the distribution produces
                // them.
                // TODO have PythonDistribution expose API to determine this.
                PythonResource::ExtensionModuleStaticallyLinked(_) =>
                    self.link_mode == StandaloneDistributionLinkMode::Static,

                PythonResource::ModuleSource { .. } => true,
                PythonResource::ModuleBytecodeRequest { .. } => true,
                PythonResource::ModuleBytecode { .. } => true,
                PythonResource::Resource { .. } => true,
                PythonResource::DistributionResource(_) => true,
                PythonResource::EggFile(_) => false,
                PythonResource::PathExtension(_) => false,
            })
            .cloned()
            .collect())
    }
}

/// A self-contained Python executable before it is compiled.
#[derive(Clone, Debug)]
pub struct StandalonePythonExecutableBuilder {
    /// The target triple we are running on.
    host_triple: String,

    /// The target triple we are building for.
    target_triple: String,

    /// The name of the executable to build.
    exe_name: String,

    /// The Python distribution being used to build this executable.
    distribution: Arc<Box<StandaloneDistribution>>,

    /// How libpython should be linked.
    link_mode: LibpythonLinkMode,

    /// Policy to apply to added resources.
    resources_policy: PythonResourcesPolicy,

    /// Python resources to be embedded in the binary.
    resources: PrePackagedResources,

    /// Configuration of the embedded Python interpreter.
    config: EmbeddedPythonConfig,

    /// Path to python executable that can be invoked at build time.
    python_exe: PathBuf,

    /// Extension module filter to apply.
    extension_module_filter: ExtensionModuleFilter,

    /// Preferred extension module variants.
    extension_module_variants: Option<HashMap<String, String>>,
}

impl StandalonePythonExecutableBuilder {
    /// Whether we're building for a target that supports loading extension modules
    /// from memory.
    fn supports_in_memory_dynamically_linked_extension_loading(&self) -> bool {
        // Loading from memory is only supported on Windows where symbols are
        // declspec(dllexport) and the distribution is capable of loading
        // shared library extensions.
        self.target_triple.contains("pc-windows")
            && self.distribution.python_symbol_visibility == "dllexport"
            && self
                .distribution
                .extension_module_loading
                .contains(&"shared-library".to_string())
    }

    #[allow(clippy::too_many_arguments)]
    fn add_distribution_resources(
        &mut self,
        logger: &slog::Logger,
        extension_module_filter: &ExtensionModuleFilter,
        include_sources: bool,
        include_resources: bool,
        include_test: bool,
    ) -> Result<()> {
        for ext in self.distribution.filter_extension_modules(
            logger,
            extension_module_filter,
            self.extension_module_variants.clone(),
        )? {
            self.add_distribution_extension_module(&ext)?;
        }

        for source in self.distribution.source_modules()? {
            if !include_test && is_stdlib_test_package(&source.package()) {
                continue;
            }

            if include_sources {
                self.add_module_source(&source)?;
            }

            self.add_module_bytecode(&source.as_bytecode_module(BytecodeOptimizationLevel::Zero))?;
        }

        if include_resources {
            for resource in self.distribution.resource_datas()? {
                if !include_test && is_stdlib_test_package(&resource.leaf_package) {
                    continue;
                }

                self.add_package_resource(&resource)?;
            }
        }

        Ok(())
    }

    /// Build a Python library suitable for linking.
    ///
    /// This will take the underlying distribution, resources, and
    /// configuration and produce a new executable binary.
    fn resolve_python_linking_info(
        &self,
        logger: &slog::Logger,
        opt_level: &str,
        resources: &EmbeddedPythonResources,
    ) -> Result<PythonLinkingInfo> {
        let libpythonxy_filename;
        let mut cargo_metadata: Vec<String> = Vec::new();
        let libpythonxy_data;
        let libpython_filename: Option<PathBuf>;
        let libpyembeddedconfig_data: Option<Vec<u8>>;
        let libpyembeddedconfig_filename: Option<PathBuf>;

        match self.link_mode {
            LibpythonLinkMode::Static => {
                let temp_dir = TempDir::new("pyoxidizer-build-exe")?;
                let temp_dir_path = temp_dir.path();

                warn!(
                    logger,
                    "generating custom link library containing Python..."
                );
                let library_info = link_libpython(
                    logger,
                    &self.distribution,
                    resources,
                    &temp_dir_path,
                    &self.host_triple,
                    &self.target_triple,
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

            LibpythonLinkMode::Dynamic => {
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

    fn libpython_link_mode(&self) -> LibpythonLinkMode {
        self.link_mode
    }

    fn python_resources_policy(&self) -> &PythonResourcesPolicy {
        &self.resources_policy
    }

    fn python_exe_path(&self) -> &Path {
        &self.python_exe
    }

    fn in_memory_module_sources(&self) -> BTreeMap<String, PythonModuleSource> {
        self.resources.get_in_memory_module_sources()
    }

    fn in_memory_package_resources(&self) -> BTreeMap<String, BTreeMap<String, Vec<u8>>> {
        self.resources.get_in_memory_package_resources()
    }

    fn pip_install(
        &self,
        logger: &slog::Logger,
        verbose: bool,
        install_args: &[String],
        extra_envs: &HashMap<String, String>,
    ) -> Result<Vec<PythonResource>> {
        pip_install(
            logger,
            &**self.distribution,
            self.link_mode,
            verbose,
            install_args,
            extra_envs,
        )
    }

    fn read_package_root(
        &self,
        logger: &slog::Logger,
        path: &Path,
        packages: &[String],
    ) -> Result<Vec<PythonResource>> {
        Ok(find_resources(&logger, &**self.distribution, path, None)?
            .iter()
            .filter_map(|x| {
                if x.is_in_packages(packages) {
                    Some(x.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>())
    }

    fn read_virtualenv(&self, logger: &slog::Logger, path: &Path) -> Result<Vec<PythonResource>> {
        read_virtualenv(logger, &**self.distribution, path)
    }

    fn setup_py_install(
        &self,
        logger: &slog::Logger,
        package_path: &Path,
        verbose: bool,
        extra_envs: &HashMap<String, String>,
        extra_global_arguments: &[String],
    ) -> Result<Vec<PythonResource>> {
        setup_py_install(
            logger,
            &**self.distribution,
            self.link_mode,
            package_path,
            verbose,
            extra_envs,
            extra_global_arguments,
        )
    }

    fn add_in_memory_module_source(&mut self, module: &PythonModuleSource) -> Result<()> {
        self.resources.add_in_memory_module_source(module)
    }

    fn add_relative_path_module_source(
        &mut self,
        prefix: &str,
        module: &PythonModuleSource,
    ) -> Result<()> {
        self.resources
            .add_relative_path_module_source(module, prefix)
    }

    fn add_in_memory_module_bytecode(
        &mut self,
        module: &PythonModuleBytecodeFromSource,
    ) -> Result<()> {
        self.resources.add_in_memory_module_bytecode(module)
    }

    fn add_relative_path_module_bytecode(
        &mut self,
        prefix: &str,
        module: &PythonModuleBytecodeFromSource,
    ) -> Result<()> {
        self.resources
            .add_relative_path_module_bytecode(module, prefix)
    }

    fn add_in_memory_package_resource(&mut self, resource: &PythonPackageResource) -> Result<()> {
        self.resources.add_in_memory_package_resource(resource)
    }

    fn add_relative_path_package_resource(
        &mut self,
        prefix: &str,
        resource: &PythonPackageResource,
    ) -> Result<()> {
        self.resources
            .add_relative_path_package_resource(prefix, resource)
    }

    fn add_in_memory_package_distribution_resource(
        &mut self,
        resource: &PythonPackageDistributionResource,
    ) -> Result<()> {
        self.resources
            .add_in_memory_package_distribution_resource(resource)
    }

    fn add_relative_path_package_distribution_resource(
        &mut self,
        prefix: &str,
        resource: &PythonPackageDistributionResource,
    ) -> Result<()> {
        self.resources
            .add_relative_path_package_distribution_resource(prefix, resource)
    }

    fn add_builtin_distribution_extension_module(
        &mut self,
        extension_module: &DistributionExtensionModule,
    ) -> Result<()> {
        self.resources
            .add_builtin_distribution_extension_module(extension_module)
    }

    fn add_in_memory_distribution_extension_module(
        &mut self,
        extension_module: &DistributionExtensionModule,
    ) -> Result<()> {
        if !self.supports_in_memory_dynamically_linked_extension_loading() {
            return Err(anyhow!(
                "loading extension modules from memory not supported by this build configuration"
            ));
        }

        self.resources
            .add_in_memory_distribution_extension_module(extension_module)
    }

    fn add_relative_path_distribution_extension_module(
        &mut self,
        prefix: &str,
        extension_module: &DistributionExtensionModule,
    ) -> Result<()> {
        if self.distribution.is_extension_module_file_loadable() {
            self.resources
                .add_relative_path_distribution_extension_module(prefix, extension_module)
        } else {
            Err(anyhow!(
                "loading extension modules from files not supported by this build configuration"
            ))
        }
    }

    fn add_distribution_extension_module(
        &mut self,
        extension_module: &DistributionExtensionModule,
    ) -> Result<()> {
        // Distribution extensions are special in that we allow them to be
        // builtin extensions, even if it violates the resources policy that prohibits
        // memory loading.

        // Builtins always get added as such.
        if extension_module.builtin_default {
            return self.add_builtin_distribution_extension_module(extension_module);
        }

        match self.resources_policy.clone() {
            PythonResourcesPolicy::InMemoryOnly => match self.link_mode {
                LibpythonLinkMode::Static => {
                    self.add_builtin_distribution_extension_module(extension_module)
                }
                LibpythonLinkMode::Dynamic => {
                    self.add_in_memory_distribution_extension_module(extension_module)
                }
            },
            PythonResourcesPolicy::FilesystemRelativeOnly(prefix) => match self.link_mode {
                LibpythonLinkMode::Static => {
                    self.add_builtin_distribution_extension_module(extension_module)
                }
                LibpythonLinkMode::Dynamic => {
                    self.add_relative_path_distribution_extension_module(&prefix, extension_module)
                }
            },
            PythonResourcesPolicy::PreferInMemoryFallbackFilesystemRelative(prefix) => {
                match self.link_mode {
                    LibpythonLinkMode::Static => {
                        self.add_builtin_distribution_extension_module(extension_module)
                    }
                    LibpythonLinkMode::Dynamic => {
                        // Try in-memory and fall back to file-based if that fails.
                        let mut res =
                            self.add_in_memory_distribution_extension_module(extension_module);

                        if res.is_err() {
                            res = self.add_relative_path_distribution_extension_module(
                                &prefix,
                                extension_module,
                            )
                        }

                        res
                    }
                }
            }
        }
    }

    fn add_in_memory_dynamic_extension_module(
        &mut self,
        extension_module: &PythonExtensionModule,
    ) -> Result<()> {
        if self.supports_in_memory_dynamically_linked_extension_loading()
            && extension_module.extension_data.is_some()
        {
            self.resources
                .add_in_memory_extension_module_shared_library(
                    &extension_module.name,
                    extension_module.is_package,
                    &extension_module
                        .extension_data
                        .as_ref()
                        .unwrap()
                        .resolve()?,
                )
        } else if !extension_module.object_file_data.is_empty() {
            // TODO we shouldn't be adding a builtin extension module from this API.
            self.resources
                .add_builtin_extension_module(extension_module)
        } else if extension_module.extension_data.is_some() {
            Err(anyhow!(
                "loading extension modules from memory not supported by this build configuration"
            ))
        } else {
            Err(anyhow!(
                "cannot load extension module from memory due to missing object files"
            ))
        }
    }

    fn add_relative_path_dynamic_extension_module(
        &mut self,
        prefix: &str,
        extension_module: &PythonExtensionModule,
    ) -> Result<()> {
        if extension_module.extension_data.is_none() {
            return Err(anyhow!(
                "extension module instance has no shared library data"
            ));
        }

        if self.distribution.is_extension_module_file_loadable() {
            self.resources
                .add_relative_path_extension_module(extension_module, prefix)
        } else {
            Err(anyhow!(
                "loading extension modules from files not supported by this build configuration"
            ))
        }
    }

    fn add_dynamic_extension_module(
        &mut self,
        extension_module: &PythonExtensionModule,
    ) -> Result<()> {
        if extension_module.extension_data.is_none() {
            return Err(anyhow!(
                "extension module instance has no shared library data"
            ));
        }

        match self.resources_policy {
            PythonResourcesPolicy::InMemoryOnly => {
                if self.supports_in_memory_dynamically_linked_extension_loading() {
                    self.resources
                        .add_in_memory_extension_module_shared_library(
                            &extension_module.name,
                            extension_module.is_package,
                            &extension_module
                                .extension_data
                                .as_ref()
                                .unwrap()
                                .resolve()?,
                        )
                } else {
                    Err(anyhow!("in-memory-only resources policy active but in-memory extension module importing not supported by this configuration: cannot load {}", extension_module.name))
                }
            }
            PythonResourcesPolicy::FilesystemRelativeOnly(ref prefix) => {
                if self.distribution.is_extension_module_file_loadable() {
                    self.resources
                        .add_relative_path_extension_module(extension_module, prefix)
                } else {
                    Err(anyhow!("filesystem-relative-only policy active but file-based extension module loading not supported by this configuration"))
                }
            }
            PythonResourcesPolicy::PreferInMemoryFallbackFilesystemRelative(ref prefix) => {
                if self.supports_in_memory_dynamically_linked_extension_loading() {
                    self.resources
                        .add_in_memory_extension_module_shared_library(
                            &extension_module.name,
                            extension_module.is_package,
                            &extension_module
                                .extension_data
                                .as_ref()
                                .unwrap()
                                .resolve()?,
                        )
                } else if self.distribution.is_extension_module_file_loadable() {
                    self.resources
                        .add_relative_path_extension_module(extension_module, prefix)
                } else {
                    Err(anyhow!("prefer-in-memory-fallback-filesystem-relative policy active but could not find a mechanism to add an extension module"))
                }
            }
        }
    }

    fn add_static_extension_module(
        &mut self,
        extension_module: &PythonExtensionModule,
    ) -> Result<()> {
        self.resources
            .add_builtin_extension_module(extension_module)
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
        opt_level: &str,
    ) -> Result<EmbeddedPythonBinaryData> {
        let resources = self.resources.package(logger, &self.python_exe)?;
        let mut extra_files = resources.extra_install_files()?;
        let linking_info = self.resolve_python_linking_info(logger, opt_level, &resources)?;
        let resources = EmbeddedResourcesBlobs::try_from(resources)?;

        if self.link_mode == LibpythonLinkMode::Dynamic {
            if let Some(p) = &self.distribution.libpython_shared_library {
                let manifest_path = Path::new(p.file_name().unwrap());
                let content = FileContent {
                    data: std::fs::read(&p)?,
                    executable: false,
                };

                extra_files.add_file(&manifest_path, &content)?;
            }
        }

        Ok(EmbeddedPythonBinaryData {
            config: self.config.clone(),
            linking_info,
            resources,
            extra_files,
            host: self.host_triple.clone(),
            target: self.target_triple.clone(),
        })
    }
}

#[cfg(test)]
pub mod tests {
    use {
        super::*, crate::py_packaging::standalone_distribution::ExtensionModuleFilter,
        crate::testutil::*,
    };

    pub fn get_standalone_executable_builder(
        logger: &slog::Logger,
    ) -> Result<StandalonePythonExecutableBuilder> {
        let distribution = get_default_distribution()?;

        // Link mode is static unless we're a dynamic distribution on Windows.
        let link_mode = if distribution.target_triple.contains("pc-windows")
            && distribution.python_symbol_visibility == "dllexport"
        {
            LibpythonLinkMode::Dynamic
        } else {
            LibpythonLinkMode::Static
        };

        let mut resources = PrePackagedResources::new(
            &PythonResourcesPolicy::InMemoryOnly,
            &distribution.cache_tag,
        );

        // We need to add minimal extension modules so builds actually work. If they are missing,
        // we'll get missing symbol errors during linking.
        for ext in
            distribution.filter_extension_modules(logger, &ExtensionModuleFilter::Minimal, None)?
        {
            resources.add_builtin_distribution_extension_module(&ext)?;
        }

        let config = EmbeddedPythonConfig::default();

        let python_exe = distribution.python_exe.clone();

        Ok(StandalonePythonExecutableBuilder {
            host_triple: env!("HOST").to_string(),
            target_triple: env!("HOST").to_string(),
            exe_name: "testapp".to_string(),
            distribution: distribution.clone(),
            link_mode,
            resources_policy: PythonResourcesPolicy::InMemoryOnly,
            resources,
            config,
            python_exe,
            extension_module_filter: ExtensionModuleFilter::Minimal,
            extension_module_variants: None,
        })
    }

    pub fn get_embedded(logger: &slog::Logger) -> Result<EmbeddedPythonBinaryData> {
        let exe = get_standalone_executable_builder(logger)?;
        exe.as_embedded_python_binary_data(&get_logger()?, "0")
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
