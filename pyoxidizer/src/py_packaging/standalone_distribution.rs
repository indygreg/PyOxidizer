// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Functionality for standalone Python distributions. */

use {
    super::binary::{LibpythonLinkMode, PythonBinaryBuilder},
    super::config::EmbeddedPythonConfig,
    super::distribution::{
        is_stdlib_test_package, resolve_python_distribution_from_location, BinaryLibpythonLinkMode,
        DistributionExtractLock, PythonDistribution, PythonDistributionLocation,
    },
    super::distutils::prepare_hacked_distutils,
    super::standalone_builder::StandalonePythonExecutableBuilder,
    crate::environment::{LINUX_TARGET_TRIPLES, MACOS_TARGET_TRIPLES},
    anyhow::{anyhow, Context, Result},
    copy_dir::copy_dir,
    lazy_static::lazy_static,
    path_dedot::ParseDot,
    python_packaging::bytecode::{BytecodeCompiler, PythonBytecodeCompiler},
    python_packaging::filesystem_scanning::{find_python_resources, walk_tree_files},
    python_packaging::module_util::{is_package_from_path, PythonModuleSuffixes},
    python_packaging::policy::PythonPackagingPolicy,
    python_packaging::resource::{
        DataLocation, LibraryDependency, PythonExtensionModule, PythonExtensionModuleVariants,
        PythonModuleSource, PythonPackageResource, PythonResource,
    },
    serde::{Deserialize, Serialize},
    slog::{info, warn},
    std::collections::{BTreeMap, HashMap},
    std::io::{BufRead, BufReader, Read},
    std::path::{Path, PathBuf},
    std::sync::Arc,
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
    /// Convert the instance to a `LibraryDependency`.
    fn to_library_dependency(&self, python_path: &Path) -> LibraryDependency {
        LibraryDependency {
            name: self.name.clone(),
            static_library: self
                .path_static
                .clone()
                .map(|p| DataLocation::Path(python_path.join(p))),
            dynamic_library: self
                .path_dynamic
                .clone()
                .map(|p| DataLocation::Path(python_path.join(p))),
            framework: self.framework.unwrap_or(false),
            system: self.system.unwrap_or(false),
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
    pub python_symbol_visibility: String,

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
    pub links_core: Vec<LibraryDependency>,

    /// Filesystem location of pythonXY shared library for this distribution.
    ///
    /// Only set if `link_mode` is `StandaloneDistributionLinkMode::Dynamic`.
    pub libpython_shared_library: Option<PathBuf>,

    /// Extension modules available to this distribution.
    pub extension_modules: BTreeMap<String, PythonExtensionModuleVariants>,

    pub frozen_c: Vec<u8>,

    /// Include files for Python.
    ///
    /// Keys are relative paths. Values are filesystem paths.
    pub includes: BTreeMap<String, PathBuf>,

    /// Static libraries available for linking.
    ///
    /// Keys are library names, without the "lib" prefix or file extension.
    /// Values are filesystem paths where library is located.
    pub libraries: BTreeMap<String, DataLocation>,

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
    #[allow(clippy::unnecessary_unwrap)]
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

                let mut symlinks = vec![];

                for entry in tf.entries()? {
                    let mut entry =
                        entry.map_err(|e| anyhow!("failed to iterate over archive: {}", e))?;

                    // Windows doesn't support symlinks without special permissions.
                    // So we track symlinks explicitly and copy files post extract if
                    // running on that platform.
                    let link_name = entry.link_name().unwrap_or(None);

                    if link_name.is_some() && cfg!(target_family = "windows") {
                        // The entry's path is the file to write, relative to the archive's
                        // root. We need to expand to an absolute path to facilitate copying.

                        // The link name is the file to symlink to, or the file we're copying.
                        // This path is relative to the entry path. So we need join with the
                        // entry's directory and canonicalize. There is also a security issue
                        // at play: archives could contain bogus symlinks pointing outside the
                        // archive. So we detect this, just in case.

                        let mut dest = absolute_path.clone();
                        dest.extend(entry.path()?.components());
                        let dest = dest
                            .parse_dot()
                            .with_context(|| "dedotting symlinked source")?;

                        let mut source = dest
                            .parent()
                            .ok_or_else(|| anyhow!("unable to resolve parent"))?
                            .to_path_buf();
                        source.extend(link_name.unwrap().components());
                        let source = source
                            .parse_dot()
                            .with_context(|| "dedotting symlink destination")?;

                        if !source.starts_with(&absolute_path) {
                            return Err(anyhow!("malicious symlink detected in archive"));
                        }

                        symlinks.push((source, dest));
                    } else {
                        entry
                            .unpack_in(&absolute_path)
                            .with_context(|| "unable to extract tar member")?;
                    }
                }

                for (source, dest) in symlinks {
                    std::fs::copy(&source, &dest).with_context(|| {
                        format!(
                            "copying symlinked file {} -> {}",
                            source.display(),
                            dest.display(),
                        )
                    })?;
                }

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
        let mut links_core: Vec<LibraryDependency> = Vec::new();
        let mut extension_modules: BTreeMap<String, PythonExtensionModuleVariants> =
            BTreeMap::new();
        let mut includes: BTreeMap<String, PathBuf> = BTreeMap::new();
        let mut libraries: BTreeMap<String, DataLocation> = BTreeMap::new();
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
            let depends = entry.to_library_dependency(&python_path);

            if let Some(p) = &depends.static_library {
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
            let mut ems = PythonExtensionModuleVariants::default();

            for entry in variants.iter() {
                let object_file_data = entry
                    .objs
                    .iter()
                    .map(|p| DataLocation::Path(python_path.join(p)))
                    .collect();
                let mut links = Vec::new();

                for link in &entry.links {
                    let depends = link.to_library_dependency(&python_path);

                    if let Some(p) = &depends.static_library {
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

                ems.push(PythonExtensionModule {
                    name: module.clone(),
                    init_fn: Some(entry.init_fn.clone()),
                    extension_file_suffix: "".to_string(),
                    shared_library: if let Some(path) = &entry.shared_lib {
                        Some(DataLocation::Path(python_path.join(path)))
                    } else {
                        None
                    },
                    object_file_data,
                    is_package: false,
                    link_libraries: links,
                    is_stdlib: true,
                    builtin_default: entry.in_core,
                    required: entry.required,
                    variant: Some(entry.variant.clone()),
                    licenses: entry.licenses.clone(),
                    license_texts: if let Some(licenses) = &entry.license_paths {
                        Some(
                            licenses
                                .iter()
                                .map(|p| DataLocation::Path(python_path.join(p)))
                                .collect(),
                        )
                    } else {
                        None
                    },
                    license_public_domain: entry.license_public_domain,
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

    /// Determines support for building a libpython from this distribution.
    ///
    /// Returns a tuple of bools indicating whether this distribution can
    /// build a static libpython and a dynamically linked libpython.
    pub fn libpython_link_support(&self) -> (bool, bool) {
        if self.target_triple.contains("pc-windows") {
            // On Windows, support for libpython linkage is determined
            // by presence of a shared library in the distribution. This
            // isn't entirely semantically correct. Since we use `dllexport`
            // for all symbols in standalone distributions, it may
            // theoretically be possible to produce both a static and dynamic
            // libpython from the same object files. But since the
            // static and dynamic distributions are built so differently, we
            // don't want to take any chances and we force each distribution
            // to its own domain.
            (
                self.libpython_shared_library.is_none(),
                self.libpython_shared_library.is_some(),
            )
        } else if self.target_triple.contains("linux-musl") {
            // Musl binaries don't support dynamic linking.
            (true, false)
        } else {
            // Elsewhere we can choose which link mode to use.
            (true, true)
        }
    }

    /// Determines whether dynamically linked extension modules can be loaded from memory.
    pub fn supports_in_memory_dynamically_linked_extension_loading(&self) -> bool {
        // Loading from memory is only supported on Windows where symbols are
        // declspec(dllexport) and the distribution is capable of loading
        // shared library extensions.
        self.target_triple.contains("pc-windows")
            && self.python_symbol_visibility == "dllexport"
            && self
                .extension_module_loading
                .contains(&"shared-library".to_string())
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

    fn create_bytecode_compiler(&self) -> Result<Box<dyn PythonBytecodeCompiler>> {
        Ok(Box::new(BytecodeCompiler::new(&self.python_exe)?))
    }

    fn create_packaging_policy(&self) -> Result<PythonPackagingPolicy> {
        let mut policy = PythonPackagingPolicy::default();

        for triple in LINUX_TARGET_TRIPLES.iter() {
            for ext in BROKEN_EXTENSIONS_LINUX.iter() {
                policy.register_broken_extension(triple, ext);
            }
        }

        for triple in MACOS_TARGET_TRIPLES.iter() {
            for ext in BROKEN_EXTENSIONS_MACOS.iter() {
                policy.register_broken_extension(triple, ext);
            }
        }

        Ok(policy)
    }

    fn as_python_executable_builder(
        &self,
        _logger: &slog::Logger,
        host_triple: &str,
        target_triple: &str,
        name: &str,
        libpython_link_mode: BinaryLibpythonLinkMode,
        policy: &PythonPackagingPolicy,
        config: &EmbeddedPythonConfig,
    ) -> Result<Box<dyn PythonBinaryBuilder>> {
        StandalonePythonExecutableBuilder::from_distribution(
            // TODO can we avoid this clone?
            Arc::new(Box::new(self.clone())),
            host_triple.to_string(),
            target_triple.to_string(),
            name.to_string(),
            libpython_link_mode,
            policy.clone(),
            config.clone(),
        )
    }

    fn iter_extension_modules<'a>(
        &'a self,
    ) -> Box<dyn Iterator<Item = &'a PythonExtensionModule> + 'a> {
        Box::new(
            self.extension_modules
                .iter()
                .map(|(_, exts)| exts.iter())
                .flatten(),
        )
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
                    is_stdlib: true,
                    is_test: is_stdlib_test_package(name),
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
                    is_stdlib: true,
                    is_test: is_stdlib_test_package(&package),
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

#[cfg(test)]
pub mod tests {
    use {super::*, crate::testutil::*};

    #[test]
    fn test_stdlib_annotations() -> Result<()> {
        let distribution = get_default_distribution()?;

        for module in distribution.source_modules()? {
            assert!(module.is_stdlib);

            if module.name.starts_with("test") {
                assert!(module.is_test);
            }
        }

        for resource in distribution.resource_datas()? {
            assert!(resource.is_stdlib);
            if resource.leaf_package.starts_with("test") {
                assert!(resource.is_test);
            }
        }

        Ok(())
    }
}
