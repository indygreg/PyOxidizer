// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Functionality for standalone Python distributions. */

use {
    super::{
        binary::{LibpythonLinkMode, PythonBinaryBuilder},
        config::{default_memory_allocator, PyembedPythonInterpreterConfig},
        distribution::{
            resolve_python_distribution_from_location, AppleSdkInfo, BinaryLibpythonLinkMode,
            DistributionExtractLock, PythonDistribution, PythonDistributionLocation,
        },
        distutils::prepare_hacked_distutils,
        standalone_builder::StandalonePythonExecutableBuilder,
    },
    crate::environment::{Environment, LINUX_TARGET_TRIPLES, MACOS_TARGET_TRIPLES},
    anyhow::{anyhow, Context, Result},
    duct::cmd,
    log::{info, warn},
    once_cell::sync::Lazy,
    path_dedot::ParseDot,
    python_packaging::{
        bytecode::{BytecodeCompiler, PythonBytecodeCompiler},
        filesystem_scanning::{find_python_resources, walk_tree_files},
        interpreter::{PythonInterpreterConfig, PythonInterpreterProfile, TerminfoResolution},
        licensing::{ComponentFlavor, LicenseFlavor, LicensedComponent},
        location::ConcreteResourceLocation,
        module_util::{is_package_from_path, PythonModuleSuffixes},
        policy::PythonPackagingPolicy,
        resource::{
            LibraryDependency, PythonExtensionModule, PythonExtensionModuleVariants,
            PythonModuleSource, PythonPackageResource, PythonResource,
        },
    },
    serde::Deserialize,
    simple_file_manifest::{FileData, FileEntry},
    std::{
        collections::{hash_map::RandomState, BTreeMap, HashMap},
        io::{BufRead, BufReader, Read},
        path::{Path, PathBuf},
        sync::Arc,
    },
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

/// Distribution extensions with known problems on Linux.
///
/// These will never be packaged.
pub static BROKEN_EXTENSIONS_LINUX: Lazy<Vec<String>> = Lazy::new(|| {
    vec![
        // Linking issues.
        "_crypt".to_string(),
        // Linking issues.
        "nis".to_string(),
    ]
});

/// Distribution extensions with known problems on macOS.
///
/// These will never be packaged.
pub static BROKEN_EXTENSIONS_MACOS: Lazy<Vec<String>> = Lazy::new(|| {
    vec![
        // curses and readline have linking issues.
        "curses".to_string(),
        "_curses_panel".to_string(),
        "readline".to_string(),
    ]
});

/// Python modules that we shouldn't generate bytecode for by default.
///
/// These are Python modules in the standard library that don't have valid bytecode.
pub static NO_BYTECODE_MODULES: Lazy<Vec<&'static str>> = Lazy::new(|| {
    vec![
        "lib2to3.tests.data.bom",
        "lib2to3.tests.data.crlf",
        "lib2to3.tests.data.different_encoding",
        "lib2to3.tests.data.false_encoding",
        "lib2to3.tests.data.py2_test_grammar",
        "lib2to3.tests.data.py3_test_grammar",
        "test.bad_coding",
        "test.badsyntax_3131",
        "test.badsyntax_future3",
        "test.badsyntax_future4",
        "test.badsyntax_future5",
        "test.badsyntax_future6",
        "test.badsyntax_future7",
        "test.badsyntax_future8",
        "test.badsyntax_future9",
        "test.badsyntax_future10",
        "test.badsyntax_pep3120",
    ]
});

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
                .map(|p| FileData::Path(python_path.join(p))),
            static_filename: self
                .path_static
                .as_ref()
                .map(|f| PathBuf::from(PathBuf::from(f).file_name().unwrap())),
            dynamic_library: self
                .path_dynamic
                .clone()
                .map(|p| FileData::Path(python_path.join(p))),
            dynamic_filename: self
                .path_dynamic
                .as_ref()
                .map(|f| PathBuf::from(PathBuf::from(f).file_name().unwrap())),
            framework: self.framework.unwrap_or(false),
            system: self.system.unwrap_or(false),
        }
    }
}

#[allow(unused)]
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

#[allow(unused)]
#[derive(Debug, Deserialize)]
struct PythonBuildCoreInfo {
    objs: Vec<String>,
    links: Vec<LinkEntry>,
    shared_lib: Option<String>,
    static_lib: Option<String>,
}

#[allow(unused)]
#[derive(Debug, Deserialize)]
struct PythonBuildInfo {
    core: PythonBuildCoreInfo,
    extensions: BTreeMap<String, Vec<PythonBuildExtensionInfo>>,
    inittab_object: String,
    inittab_source: String,
    inittab_cflags: Vec<String>,
    object_file_format: String,
}

#[allow(unused)]
#[derive(Debug, Deserialize)]
struct PythonJsonMain {
    version: String,
    target_triple: String,
    optimizations: String,
    python_tag: String,
    python_abi_tag: Option<String>,
    python_config_vars: HashMap<String, String>,
    python_platform_tag: String,
    python_implementation_cache_tag: String,
    python_implementation_hex_version: u64,
    python_implementation_name: String,
    python_implementation_version: Vec<String>,
    python_version: String,
    python_major_minor_version: String,
    python_paths: HashMap<String, String>,
    python_paths_abstract: HashMap<String, String>,
    python_exe: String,
    python_stdlib_test_packages: Vec<String>,
    python_suffixes: HashMap<String, Vec<String>>,
    python_bytecode_magic_number: String,
    python_symbol_visibility: String,
    python_extension_module_loading: Vec<String>,
    apple_sdk_canonical_name: Option<String>,
    apple_sdk_platform: Option<String>,
    apple_sdk_version: Option<String>,
    apple_sdk_deployment_target: Option<String>,
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

            if version != "7" {
                return Err(anyhow!(
                    "expected version 7 standalone distribution; found version {}",
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

fn parse_python_major_minor_version(version: &str) -> String {
    let mut at_least_minor_version = String::from(version);
    if !version.contains('.') {
        at_least_minor_version.push_str(".0");
    }
    at_least_minor_version
        .split('.')
        .take(2)
        .collect::<Vec<_>>()
        .join(".")
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

    let windows_layout = p.join("Scripts").exists();

    let bin_dir = if windows_layout {
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

    let unix_lib_dir = p.join("lib").join(format!(
        "python{}",
        parse_python_major_minor_version(python_version)
    ));

    let stdlib = if unix_lib_dir.exists() {
        unix_lib_dir
    } else if windows_layout {
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

pub fn invoke_python(python_paths: &PythonPaths, args: &[&str]) {
    let site_packages_s = python_paths.site_packages.display().to_string();

    if site_packages_s.starts_with("\\\\?\\") {
        panic!("Unexpected Windows UNC path in site-packages path");
    }

    info!("setting PYTHONPATH {}", site_packages_s);

    let mut envs: HashMap<String, String, RandomState> = std::env::vars().collect();
    envs.insert("PYTHONPATH".to_string(), site_packages_s);

    info!(
        "running {} {}",
        python_paths.python_exe.display(),
        args.join(" ")
    );

    let command = cmd(&python_paths.python_exe, args)
        .full_env(&envs)
        .stderr_to_stdout()
        .reader()
        .unwrap_or_else(|_| {
            panic!(
                "failed to run {} {}",
                python_paths.python_exe.display(),
                args.join(" ")
            )
        });
    {
        let reader = BufReader::new(&command);
        for line in reader.lines() {
            match line {
                Ok(line) => {
                    warn!("{}", line);
                }
                Err(err) => {
                    warn!("Error when reading output: {:?}", err);
                }
            }
        }
    }
}

/// Describes how libpython is linked in a standalone distribution.
#[derive(Clone, Debug, PartialEq, Eq)]
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

    /// Python implementation name.
    pub python_implementation: String,

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

    /// Python packages in the standard library providing tests.
    stdlib_test_packages: Vec<String>,

    /// How libpython is linked in this distribution.
    link_mode: StandaloneDistributionLinkMode,

    /// Symbol visibility for Python symbols.
    pub python_symbol_visibility: String,

    /// Capabilities of distribution to load extension modules.
    extension_module_loading: Vec<String>,

    /// Apple SDK build/targeting settings.
    apple_sdk_info: Option<AppleSdkInfo>,

    /// Holds license information for the core distribution.
    pub core_license: Option<LicensedComponent>,

    /// SPDX license shortnames that apply to this distribution.
    ///
    /// Licenses only cover the core distribution. Licenses for libraries
    /// required by extensions are stored next to the extension's linking
    /// info.
    pub licenses: Option<Vec<String>>,

    /// Path to file holding license text for this distribution.
    pub license_path: Option<PathBuf>,

    /// Path to Tcl library files.
    tcl_library_path: Option<PathBuf>,

    /// Directories under `tcl_library_path` containing tcl files.
    tcl_library_paths: Option<Vec<String>>,

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
    pub libraries: BTreeMap<String, PathBuf>,

    pub py_modules: BTreeMap<String, PathBuf>,

    /// Non-module Python resource files.
    ///
    /// Keys are package names. Values are maps of resource name to data for the resource
    /// within that package.
    pub resources: BTreeMap<String, BTreeMap<String, PathBuf>>,

    /// Path to copy of hacked dist to use for packaging rules venvs
    pub venv_base: PathBuf,

    /// Path to object file defining _PyImport_Inittab.
    pub inittab_object: PathBuf,

    /// Compiler flags to use to build object containing _PyImport_Inittab.
    pub inittab_cflags: Vec<String>,

    /// Tag to apply to bytecode files.
    ///
    /// e.g. `cpython-39`.
    pub cache_tag: String,

    /// Suffixes for Python module types.
    module_suffixes: PythonModuleSuffixes,

    /// List of strings denoting C Runtime requirements.
    pub crt_features: Vec<String>,

    /// Configuration variables used by Python.
    config_vars: HashMap<String, String>,
}

impl StandaloneDistribution {
    pub fn from_location(
        location: &PythonDistributionLocation,
        distributions_dir: &Path,
    ) -> Result<Self> {
        let (archive_path, extract_path) =
            resolve_python_distribution_from_location(location, distributions_dir)?;

        Self::from_tar_zst_file(&archive_path, &extract_path)
    }

    /// Create an instance from a .tar.zst file.
    ///
    /// The distribution will be extracted to ``extract_dir`` if necessary.
    pub fn from_tar_zst_file(path: &Path, extract_dir: &Path) -> Result<Self> {
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

        Self::from_tar_zst(reader, extract_dir).context("reading tar.zst distribution data")
    }

    /// Extract and analyze a standalone distribution from a zstd compressed tar stream.
    pub fn from_tar_zst<R: Read>(source: R, extract_dir: &Path) -> Result<Self> {
        let dctx = zstd::stream::Decoder::new(source)?;

        Self::from_tar(dctx, extract_dir).context("reading tar distribution data")
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

                    // The mtimes in the archive may be 0 / UNIX epoch. This shouldn't
                    // matter. However, pip will sometimes attempt to produce a zip file of
                    // its own content and Python's zip code won't handle times before 1980,
                    // which is later than UNIX epoch. This can lead to pip blowing up at
                    // run-time. We work around this by not adjusting the mtime when
                    // extracting the archive. This effectively makes the mtime "now."
                    entry.set_preserve_mtime(false);

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
                            .with_context(|| "dedotting symlinked source")?
                            .to_path_buf();

                        let mut source = dest
                            .parent()
                            .ok_or_else(|| anyhow!("unable to resolve parent"))?
                            .to_path_buf();
                        source.extend(link_name.unwrap().components());
                        let source = source
                            .parse_dot()
                            .with_context(|| "dedotting symlink destination")?
                            .to_path_buf();

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
        let mut libraries = BTreeMap::new();
        let frozen_c: Vec<u8> = Vec::new();
        let mut py_modules: BTreeMap<String, PathBuf> = BTreeMap::new();
        let mut resources: BTreeMap<String, BTreeMap<String, PathBuf>> = BTreeMap::new();

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

        // Derive the distribution's license from a license file, if present.
        let core_license = if let Some(ref python_license_path) = pi.license_path {
            let license_path = python_path.join(python_license_path);
            let license_text = std::fs::read_to_string(&license_path).with_context(|| {
                format!("unable to read Python license {}", license_path.display())
            })?;

            let expression = pi.licenses.clone().unwrap().join(" OR ");

            let mut component = LicensedComponent::new_spdx(
                ComponentFlavor::PythonDistribution(pi.python_implementation_name.clone()),
                &expression,
            )?;
            component.add_license_text(license_text);

            Some(component)
        } else {
            None
        };

        // Collect object files for libpython.
        for obj in &pi.build_info.core.objs {
            let rel_path = PathBuf::from(obj);
            let full_path = python_path.join(obj);

            objs_core.insert(rel_path, full_path);
        }

        for entry in &pi.build_info.core.links {
            let depends = entry.to_library_dependency(&python_path);

            if let Some(p) = &depends.static_library {
                if let Some(p) = p.backing_path() {
                    libraries.insert(depends.name.clone(), p.to_path_buf());
                }
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
                let extension_file_suffix = if let Some(p) = &entry.shared_lib {
                    if let Some(idx) = p.rfind('.') {
                        p[idx..].to_string()
                    } else {
                        "".to_string()
                    }
                } else {
                    "".to_string()
                };

                let object_file_data = entry
                    .objs
                    .iter()
                    .map(|p| FileData::Path(python_path.join(p)))
                    .collect();
                let mut links = Vec::new();

                for link in &entry.links {
                    let depends = link.to_library_dependency(&python_path);

                    if let Some(p) = &depends.static_library {
                        if let Some(p) = p.backing_path() {
                            libraries.insert(depends.name.clone(), p.to_path_buf());
                        }
                    }

                    links.push(depends);
                }

                let component_flavor =
                    ComponentFlavor::PythonStandardLibraryExtensionModule(module.clone());

                let mut license = if entry.license_public_domain.unwrap_or(false) {
                    LicensedComponent::new(component_flavor, LicenseFlavor::PublicDomain)
                } else if let Some(licenses) = &entry.licenses {
                    let expression = licenses.join(" OR ");
                    LicensedComponent::new_spdx(component_flavor, &expression)?
                } else if let Some(core) = &core_license {
                    LicensedComponent::new_spdx(
                        component_flavor,
                        core.spdx_expression()
                            .ok_or_else(|| anyhow!("could not resolve SPDX license for core"))?
                            .as_ref(),
                    )?
                } else {
                    LicensedComponent::new(component_flavor, LicenseFlavor::None)
                };

                if let Some(license_paths) = &entry.license_paths {
                    for path in license_paths {
                        let path = python_path.join(path);
                        let text = std::fs::read_to_string(&path)
                            .with_context(|| format!("reading {}", path.display()))?;

                        license.add_license_text(text);
                    }
                }

                ems.push(PythonExtensionModule {
                    name: module.clone(),
                    init_fn: Some(entry.init_fn.clone()),
                    extension_file_suffix,
                    shared_library: entry
                        .shared_lib
                        .as_ref()
                        .map(|path| FileData::Path(python_path.join(path))),
                    object_file_data,
                    is_package: false,
                    link_libraries: links,
                    is_stdlib: true,
                    builtin_default: entry.in_core,
                    required: entry.required,
                    variant: Some(entry.variant.clone()),
                    license: Some(license),
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
            false,
            true,
        )? {
            match entry? {
                PythonResource::PackageResource(resource) => {
                    if !resources.contains_key(&resource.leaf_package) {
                        resources.insert(resource.leaf_package.clone(), BTreeMap::new());
                    }

                    resources.get_mut(&resource.leaf_package).unwrap().insert(
                        resource.relative_name.clone(),
                        match &resource.data {
                            FileData::Path(path) => path.to_path_buf(),
                            FileData::Memory(_) => {
                                return Err(anyhow!(
                                    "should not have received in-memory resource data"
                                ))
                            }
                        },
                    );
                }
                PythonResource::ModuleSource(source) => match &source.source {
                    FileData::Path(path) => {
                        py_modules.insert(source.name.clone(), path.to_path_buf());
                    }
                    FileData::Memory(_) => {
                        return Err(anyhow!("should not have received in-memory source data"))
                    }
                },

                PythonResource::ModuleBytecodeRequest(_) => {}
                PythonResource::ModuleBytecode(_) => {}
                PythonResource::PackageDistributionResource(_) => {}
                PythonResource::ExtensionModule(_) => {}
                PythonResource::EggFile(_) => {}
                PythonResource::PathExtension(_) => {}
                PythonResource::File(_) => {}
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

        let apple_sdk_info = if let Some(canonical_name) = pi.apple_sdk_canonical_name {
            let platform = pi
                .apple_sdk_platform
                .ok_or_else(|| anyhow!("apple_sdk_platform not defined"))?;
            let version = pi
                .apple_sdk_version
                .ok_or_else(|| anyhow!("apple_sdk_version not defined"))?;
            let deployment_target = pi
                .apple_sdk_deployment_target
                .ok_or_else(|| anyhow!("apple_sdk_deployment_target not defined"))?;

            Some(AppleSdkInfo {
                canonical_name,
                platform,
                version,
                deployment_target,
            })
        } else {
            None
        };

        let inittab_object = python_path.join(pi.build_info.inittab_object);

        Ok(Self {
            base_dir: dist_dir.to_path_buf(),
            target_triple: pi.target_triple,
            python_implementation: pi.python_implementation_name,
            python_tag: pi.python_tag,
            python_abi_tag: pi.python_abi_tag,
            python_platform_tag: pi.python_platform_tag,
            version: pi.python_version.clone(),
            python_exe: python_exe_path(dist_dir)?,
            stdlib_path,
            stdlib_test_packages: pi.python_stdlib_test_packages,
            link_mode,
            python_symbol_visibility: pi.python_symbol_visibility,
            extension_module_loading: pi.python_extension_module_loading,
            apple_sdk_info,
            core_license,
            licenses: pi.licenses.clone(),
            license_path: pi.license_path.as_ref().map(PathBuf::from),
            tcl_library_path: pi
                .tcl_library_path
                .as_ref()
                .map(|path| dist_dir.join("python").join(path)),
            tcl_library_paths: pi.tcl_library_paths.clone(),
            extension_modules,
            frozen_c,
            includes,
            links_core,
            libraries,
            objs_core,
            libpython_shared_library,
            py_modules,
            resources,
            venv_base,
            inittab_object,
            inittab_cflags: pi.build_info.inittab_cflags,
            cache_tag: pi.python_implementation_cache_tag,
            module_suffixes,
            crt_features: pi.crt_features,
            config_vars: pi.python_config_vars,
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

    /// Whether the distribution is capable of loading filed-based Python extension modules.
    pub fn is_extension_module_file_loadable(&self) -> bool {
        self.extension_module_loading
            .contains(&"shared-library".to_string())
    }
}

impl PythonDistribution for StandaloneDistribution {
    fn clone_trait(&self) -> Arc<dyn PythonDistribution> {
        Arc::new(self.clone())
    }

    fn target_triple(&self) -> &str {
        &self.target_triple
    }

    fn compatible_host_triples(&self) -> Vec<String> {
        let mut res = vec![self.target_triple.clone()];

        res.extend(
            match self.target_triple() {
                "aarch64-unknown-linux-gnu" => vec![],
                // musl libc linked distributions run on GNU Linux.
                "aarch64-unknown-linux-musl" => vec!["aarch64-unknown-linux-gnu"],
                "x86_64-unknown-linux-gnu" => vec![],
                // musl libc linked distributions run on GNU Linux.
                "x86_64-unknown-linux-musl" => vec!["x86_64-unknown-linux-gnu"],
                "aarch64-apple-darwin" => vec![],
                "x86_64-apple-darwin" => vec![],
                // 32-bit Windows GNU on 32-bit Windows MSVC and 64-bit Windows.
                "i686-pc-windows-gnu" => vec![
                    "i686-pc-windows-msvc",
                    "x86_64-pc-windows-gnu",
                    "x86_64-pc-windows-msvc",
                ],
                // 32-bit Windows MSVC runs on 32-bit Windows MSVC and 64-bit Windows.
                "i686-pc-windows-msvc" => vec![
                    "i686-pc-windows-gnu",
                    "x86_64-pc-windows-gnu",
                    "x86_64-pc-windows-msvc",
                ],
                // 64-bit Windows GNU/MSVC runs on the other.
                "x86_64-pc-windows-gnu" => vec!["x86_64-pc-windows-msvc"],
                "x86_64-pc-windows-msvc" => vec!["x86_64-pc-windows-gnu"],
                _ => vec![],
            }
            .iter()
            .map(|x| x.to_string()),
        );

        res
    }

    fn python_exe_path(&self) -> &Path {
        &self.python_exe
    }

    fn python_version(&self) -> &str {
        &self.version
    }

    fn python_major_minor_version(&self) -> String {
        parse_python_major_minor_version(&self.version)
    }

    fn python_implementation(&self) -> &str {
        &self.python_implementation
    }

    fn python_implementation_short(&self) -> &str {
        // TODO capture this in distribution metadata
        match self.python_implementation.as_str() {
            "cpython" => "cp",
            "python" => "py",
            "pypy" => "pp",
            "ironpython" => "ip",
            "jython" => "jy",
            s => panic!("unsupported Python implementation: {}", s),
        }
    }

    fn python_tag(&self) -> &str {
        &self.python_tag
    }

    fn python_abi_tag(&self) -> Option<&str> {
        match &self.python_abi_tag {
            Some(tag) => {
                if tag.is_empty() {
                    None
                } else {
                    Some(tag)
                }
            }
            None => None,
        }
    }

    fn python_platform_tag(&self) -> &str {
        &self.python_platform_tag
    }

    fn python_platform_compatibility_tag(&self) -> &str {
        // TODO capture this in distribution metadata.
        if !self.is_extension_module_file_loadable() {
            return "none";
        }

        match self.python_platform_tag.as_str() {
            "linux-aarch64" => "manylinux2014_aarch64",
            "linux-x86_64" => "manylinux2014_x86_64",
            "linux-i686" => "manylinux2014_i686",
            "macosx-10.9-x86_64" => "macosx_10_9_x86_64",
            "macosx-11.0-arm64" => "macosx_11_0_arm64",
            "win-amd64" => "win_amd64",
            "win32" => "win32",
            p => panic!("unsupported Python platform: {}", p),
        }
    }

    fn cache_tag(&self) -> &str {
        &self.cache_tag
    }

    fn python_module_suffixes(&self) -> Result<PythonModuleSuffixes> {
        Ok(self.module_suffixes.clone())
    }

    fn python_config_vars(&self) -> &HashMap<String, String> {
        &self.config_vars
    }

    fn stdlib_test_packages(&self) -> Vec<String> {
        self.stdlib_test_packages.clone()
    }

    fn apple_sdk_info(&self) -> Option<&AppleSdkInfo> {
        self.apple_sdk_info.as_ref()
    }

    fn create_bytecode_compiler(
        &self,
        env: &Environment,
    ) -> Result<Box<dyn PythonBytecodeCompiler>> {
        let temp_dir = env.temporary_directory("pyoxidizer-bytecode-compiler")?;

        Ok(Box::new(BytecodeCompiler::new(
            &self.python_exe,
            temp_dir.path(),
        )?))
    }

    fn create_packaging_policy(&self) -> Result<PythonPackagingPolicy> {
        let mut policy = PythonPackagingPolicy::default();

        // In-memory shared library loading is brittle. Disable this configuration
        // even if supported because it leads to pain.
        if self.supports_in_memory_shared_library_loading() {
            policy.set_resources_location(ConcreteResourceLocation::InMemory);
            policy.set_resources_location_fallback(Some(ConcreteResourceLocation::RelativePath(
                "lib".to_string(),
            )));
        }

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

        for name in NO_BYTECODE_MODULES.iter() {
            policy.register_no_bytecode_module(name);
        }

        Ok(policy)
    }

    fn create_python_interpreter_config(&self) -> Result<PyembedPythonInterpreterConfig> {
        let embedded_default = PyembedPythonInterpreterConfig::default();

        Ok(PyembedPythonInterpreterConfig {
            config: PythonInterpreterConfig {
                profile: PythonInterpreterProfile::Isolated,
                ..embedded_default.config
            },
            allocator_backend: default_memory_allocator(self.target_triple()),
            allocator_raw: true,
            oxidized_importer: true,
            filesystem_importer: false,
            terminfo_resolution: TerminfoResolution::Dynamic,
            ..embedded_default
        })
    }

    fn as_python_executable_builder(
        &self,
        host_triple: &str,
        target_triple: &str,
        name: &str,
        libpython_link_mode: BinaryLibpythonLinkMode,
        policy: &PythonPackagingPolicy,
        config: &PyembedPythonInterpreterConfig,
        host_distribution: Option<Arc<dyn PythonDistribution>>,
    ) -> Result<Box<dyn PythonBinaryBuilder>> {
        // TODO can we avoid these clones?
        let target_distribution = Arc::new(self.clone());
        let host_distribution: Arc<dyn PythonDistribution> =
            host_distribution.unwrap_or_else(|| Arc::new(self.clone()));

        let builder = StandalonePythonExecutableBuilder::from_distribution(
            host_distribution,
            target_distribution,
            host_triple.to_string(),
            target_triple.to_string(),
            name.to_string(),
            libpython_link_mode,
            policy.clone(),
            config.clone(),
        )?;

        Ok(builder as Box<dyn PythonBinaryBuilder>)
    }

    fn python_resources<'a>(&self) -> Vec<PythonResource<'a>> {
        let extension_modules = self
            .extension_modules
            .iter()
            .flat_map(|(_, exts)| exts.iter().map(|e| PythonResource::from(e.to_owned())));

        let module_sources = self.py_modules.iter().map(|(name, path)| {
            PythonResource::from(PythonModuleSource {
                name: name.clone(),
                source: FileData::Path(path.clone()),
                is_package: is_package_from_path(path),
                cache_tag: self.cache_tag.clone(),
                is_stdlib: true,
                is_test: self.is_stdlib_test_package(name),
            })
        });

        let resource_datas = self.resources.iter().flat_map(|(package, inner)| {
            inner.iter().map(move |(name, path)| {
                PythonResource::from(PythonPackageResource {
                    leaf_package: package.clone(),
                    relative_name: name.clone(),
                    data: FileData::Path(path.clone()),
                    is_stdlib: true,
                    is_test: self.is_stdlib_test_package(package),
                })
            })
        });

        extension_modules
            .chain(module_sources)
            .chain(resource_datas)
            .collect::<Vec<PythonResource<'a>>>()
    }

    /// Ensure pip is available to run in the distribution.
    fn ensure_pip(&self) -> Result<PathBuf> {
        let dist_prefix = self.base_dir.join("python").join("install");
        let python_paths = resolve_python_paths(&dist_prefix, &self.version);

        let pip_path = python_paths.bin_dir.join(PIP_EXE_BASENAME);

        if !pip_path.exists() {
            warn!("{} doesnt exist", pip_path.display().to_string());
            invoke_python(&python_paths, &["-m", "ensurepip"]);
        }

        Ok(pip_path)
    }

    fn resolve_distutils(
        &self,
        libpython_link_mode: LibpythonLinkMode,
        dest_dir: &Path,
        extra_python_paths: &[&Path],
    ) -> Result<HashMap<String, String>> {
        let mut res = match libpython_link_mode {
            // We need to patch distutils if the distribution is statically linked.
            LibpythonLinkMode::Static => prepare_hacked_distutils(
                &self.stdlib_path.join("distutils"),
                dest_dir,
                extra_python_paths,
            ),
            LibpythonLinkMode::Dynamic => Ok(HashMap::new()),
        }?;

        // Modern versions of setuptools vendor their own copy of distutils
        // and use it by default. If we hacked distutils above, we need to ensure
        // that hacked copy is used. Even if we don't hack distutils, there is an
        // unknown change in behavior in a release after setuptools 63.2.0 causing
        // extension module building to fail due to missing Python.h. In older
        // versions the CFLAGS has an -I with the path to our standalone
        // distribution. But in modern versions it uses the `/install/include/pythonX.Y`
        // path from sysconfig with the proper prefixing. This bug was exposed when
        // we attempted to upgrade PBS distributions from 20220802 to 20221002.
        // We'll need to fix this before Python 3.12, which drops distutils from the
        // stdlib.
        //
        // The actual value of the environment variable doesn't matter as long as it
        // isn't "local". However, the setuptools docs suggest using "stdlib."
        res.insert("SETUPTOOLS_USE_DISTUTILS".to_string(), "stdlib".to_string());

        Ok(res)
    }

    /// Determines whether dynamically linked extension modules can be loaded from memory.
    fn supports_in_memory_shared_library_loading(&self) -> bool {
        // Loading from memory is only supported on Windows where symbols are
        // declspec(dllexport) and the distribution is capable of loading
        // shared library extensions.
        self.target_triple.contains("pc-windows")
            && self.python_symbol_visibility == "dllexport"
            && self
                .extension_module_loading
                .contains(&"shared-library".to_string())
    }

    fn tcl_files(&self) -> Result<Vec<(PathBuf, FileEntry)>> {
        let mut res = vec![];

        if let Some(root) = &self.tcl_library_path {
            if let Some(paths) = &self.tcl_library_paths {
                for subdir in paths {
                    for entry in walkdir::WalkDir::new(root.join(subdir))
                        .sort_by(|a, b| a.file_name().cmp(b.file_name()))
                        .into_iter()
                    {
                        let entry = entry?;

                        let path = entry.path();

                        if path.is_dir() {
                            continue;
                        }

                        let rel_path = path.strip_prefix(root)?;

                        res.push((rel_path.to_path_buf(), FileEntry::try_from(path)?));
                    }
                }
            }
        }

        Ok(res)
    }

    fn tcl_library_path_directory(&self) -> Option<String> {
        // TODO this should probably be exposed from the JSON metadata.
        Some("tcl8.6".to_string())
    }
}

#[cfg(test)]
pub mod tests {
    use {
        super::*,
        crate::testutil::*,
        python_packaging::{
            bytecode::CompileMode, policy::ExtensionModuleFilter,
            resource::BytecodeOptimizationLevel,
        },
        std::collections::BTreeSet,
    };

    #[test]
    fn test_stdlib_annotations() -> Result<()> {
        let distribution = get_default_distribution(None)?;

        for resource in distribution.python_resources() {
            match resource {
                PythonResource::ModuleSource(module) => {
                    assert!(module.is_stdlib);

                    if module.name.starts_with("test") {
                        assert!(module.is_test);
                    }
                }
                PythonResource::PackageResource(r) => {
                    assert!(r.is_stdlib);
                    if r.leaf_package.starts_with("test") {
                        assert!(r.is_test);
                    }
                }
                _ => (),
            }
        }

        Ok(())
    }

    #[test]
    fn test_tcl_files() -> Result<()> {
        for dist in get_all_standalone_distributions()? {
            let tcl_files = dist.tcl_files()?;

            if dist.target_triple().contains("pc-windows")
                && !dist.is_extension_module_file_loadable()
            {
                assert!(tcl_files.is_empty());
            } else {
                assert!(!tcl_files.is_empty());
            }
        }

        Ok(())
    }

    #[test]
    fn test_extension_module_copyleft_filtering() -> Result<()> {
        for dist in get_all_standalone_distributions()? {
            let mut policy = dist.create_packaging_policy()?;
            policy.set_extension_module_filter(ExtensionModuleFilter::All);

            let all_extensions = policy
                .resolve_python_extension_modules(
                    dist.extension_modules.values(),
                    &dist.target_triple,
                )?
                .into_iter()
                .map(|e| (e.name, e.variant))
                .collect::<BTreeSet<_>>();

            policy.set_extension_module_filter(ExtensionModuleFilter::NoCopyleft);

            let no_copyleft_extensions = policy
                .resolve_python_extension_modules(
                    dist.extension_modules.values(),
                    &dist.target_triple,
                )?
                .into_iter()
                .map(|e| (e.name, e.variant))
                .collect::<BTreeSet<_>>();

            let dropped = all_extensions
                .difference(&no_copyleft_extensions)
                .cloned()
                .collect::<Vec<_>>();

            let added = no_copyleft_extensions
                .difference(&all_extensions)
                .cloned()
                .collect::<Vec<_>>();

            // 3.10 distributions stopped shipping GPL licensed extensions.
            let (linux_dropped, linux_added) =
                if ["3.8", "3.9"].contains(&dist.python_major_minor_version().as_str()) {
                    (
                        vec![
                            ("_gdbm".to_string(), Some("default".to_string())),
                            ("readline".to_string(), Some("default".to_string())),
                        ],
                        vec![("readline".to_string(), Some("libedit".to_string()))],
                    )
                } else {
                    (vec![], vec![])
                };

            let (wanted_dropped, wanted_added) = match (
                dist.python_major_minor_version().as_str(),
                dist.target_triple(),
            ) {
                (_, "aarch64-unknown-linux-gnu") => (linux_dropped.clone(), linux_added.clone()),
                (_, "x86_64-unknown-linux-gnu") => (linux_dropped.clone(), linux_added.clone()),
                (_, "x86_64_v2-unknown-linux-gnu") => (linux_dropped.clone(), linux_added.clone()),
                (_, "x86_64_v3-unknown-linux-gnu") => (linux_dropped.clone(), linux_added.clone()),
                (_, "x86_64-unknown-linux-musl") => (linux_dropped.clone(), linux_added.clone()),
                (_, "x86_64_v2-unknown-linux-musl") => (linux_dropped.clone(), linux_added.clone()),
                (_, "x86_64_v3-unknown-linux-musl") => (linux_dropped.clone(), linux_added.clone()),
                (_, "i686-pc-windows-msvc") => (vec![], vec![]),
                (_, "x86_64-pc-windows-msvc") => (vec![], vec![]),
                (_, "aarch64-apple-darwin") => (vec![], vec![]),
                (_, "x86_64-apple-darwin") => (vec![], vec![]),
                _ => (vec![], vec![]),
            };

            assert_eq!(
                dropped,
                wanted_dropped,
                "dropped matches for {} {}",
                dist.python_major_minor_version(),
                dist.target_triple(),
            );
            assert_eq!(
                added,
                wanted_added,
                "added matches for {} {}",
                dist.python_major_minor_version(),
                dist.target_triple()
            );
        }

        Ok(())
    }

    #[test]
    fn compile_syntax_error() -> Result<()> {
        let env = get_env()?;
        let dist = get_default_distribution(None)?;

        let temp_dir = env.temporary_directory("pyoxidizer-test")?;

        let mut compiler = BytecodeCompiler::new(dist.python_exe_path(), temp_dir.path())?;
        let res = compiler.compile(
            b"invalid syntax",
            "foo.py",
            BytecodeOptimizationLevel::Zero,
            CompileMode::Bytecode,
        );
        assert!(res.is_err());
        let err = res.err().unwrap();
        assert!(err
            .to_string()
            .starts_with("compiling error: invalid syntax"));

        temp_dir.close()?;

        Ok(())
    }

    #[test]
    fn apple_sdk_info() -> Result<()> {
        for dist in get_all_standalone_distributions()? {
            if dist.target_triple().contains("-apple-") {
                assert!(dist.apple_sdk_info().is_some());
            } else {
                assert!(dist.apple_sdk_info().is_none());
            }
        }

        Ok(())
    }

    #[test]
    fn test_parse_python_major_minor_version() {
        let version_expectations = [
            ("3.7.1", "3.7"),
            ("3.10.1", "3.10"),
            ("1.2.3.4.5", "1.2"),
            ("1", "1.0"),
        ];
        for (version, expected) in version_expectations {
            assert_eq!(parse_python_major_minor_version(version), expected);
        }
    }

    #[test]
    fn test_resolve_python_paths_site_packages() -> Result<()> {
        let python_paths = resolve_python_paths(Path::new("/test/dir"), "3.10.4");
        assert_eq!(
            python_paths
                .site_packages
                .to_str()
                .unwrap()
                .replace('\\', "/"),
            "/test/dir/lib/python3.10/site-packages"
        );
        let python_paths = resolve_python_paths(Path::new("/test/dir"), "3.9.1");
        assert_eq!(
            python_paths
                .site_packages
                .to_str()
                .unwrap()
                .replace('\\', "/"),
            "/test/dir/lib/python3.9/site-packages"
        );
        Ok(())
    }
}
