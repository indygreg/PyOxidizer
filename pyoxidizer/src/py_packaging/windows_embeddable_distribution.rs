// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Functionality for Windows embeddable distributions. */

use {
    super::binary::{EmbeddedPythonBinaryData, PythonBinaryBuilder, PythonLinkingInfo},
    super::bytecode::BytecodeCompiler,
    super::config::EmbeddedPythonConfig,
    super::distribution::{
        extract_zip, resolve_python_distribution_from_location, DistributionExtractLock,
        ExtensionModuleFilter, PythonDistribution, PythonDistributionLocation,
        PythonModuleSuffixes, IMPORTLIB_BOOTSTRAP_EXTERNAL_PY_37, IMPORTLIB_BOOTSTRAP_PY_37,
    },
    super::embedded_resource::EmbeddedPythonResourcesPrePackaged,
    super::libpython::{derive_importlib, ImportlibBytecode},
    super::packaging_tool::bootstrap_packaging_tools,
    super::resource::{
        BytecodeModuleSource, ExtensionModuleData, PythonModuleSource, PythonResource, ResourceData,
    },
    super::resources_policy::PythonResourcesPolicy,
    super::standalone_distribution::DistributionExtensionModule,
    crate::analyze::find_pe_dependencies_path,
    crate::app_packaging::resource::FileManifest,
    anyhow::{anyhow, Result},
    slog::warn,
    std::collections::{BTreeMap, HashMap},
    std::convert::TryInto,
    std::fmt::{Debug, Formatter},
    std::iter::FromIterator,
    std::path::{Path, PathBuf},
    tempdir::TempDir,
};

/// Represents a Python extension module on Windows that is standalone.
///
/// The extension module is effectively a DLL.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowsEmbeddableDistributionExtensionModule {
    /// Python module name.
    pub name: String,

    /// Filesystem path to .pyd file.
    pub path: PathBuf,

    /// Paths to DLL dependencies within the embeddable Python distribution.
    ///
    /// These are extra files that need to be distributed in order for the
    /// extension module to load.
    pub distribution_dll_dependencies: Vec<PathBuf>,
}

/// Represents a bytecode module in a Windows embeddable distribution.
#[derive(Clone, PartialEq)]
pub struct WindowsEmbeddableDistributionBytecodeModule {
    /// Python module name.
    pub name: String,

    /// Whether the module is a package.
    pub is_package: bool,

    /// Bytecode code data (without pyc header).
    pub code: Vec<u8>,
}

impl Debug for WindowsEmbeddableDistributionBytecodeModule {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(
            f,
            "WindowsEmbeddableDistributionBytecodeModule {{ name: {}, is_package: {}, code: {} bytes }}",
            self.name, self.is_package, self.code.len()
        )
    }
}

/// A pre-built Python distribution for Windows.
///
/// This type represents the zip file Python distributions distributed by the
/// official Python project. The zip files contain a python.exe, pythonXY.dll,
/// a pythonXY.zip containing the standard library, .pyd files for extension
/// modules, and various .dll dependencies.
#[derive(Clone, Debug)]
pub struct WindowsEmbeddableDistribution {
    /// Path to python executable.
    pub python_exe: PathBuf,

    /// Path to pythonXY dll.
    pub python_dll: PathBuf,

    /// Path to pythonXY.zip containing standard library.
    pub python_zip: PathBuf,

    /// Extra DLLs from this distribution that are required to run Python.
    ///
    /// This is likely just a reference to a `vcruntimeXXX.dll`.
    pub extra_distribution_dlls: Vec<PathBuf>,

    /// Path to license file.
    pub license_path: PathBuf,

    /// Extension modules in the distribution.
    ///
    /// These exist as standalone .pyd files.
    pub extension_modules: BTreeMap<String, WindowsEmbeddableDistributionExtensionModule>,

    /// Bytecode modules present in the distribution.
    pub bytecode_modules: BTreeMap<String, WindowsEmbeddableDistributionBytecodeModule>,
}

impl WindowsEmbeddableDistribution {
    pub fn from_location(
        logger: &slog::Logger,
        location: &PythonDistributionLocation,
        distributions_dir: &Path,
    ) -> Result<Self> {
        let (archive_path, extract_path) =
            resolve_python_distribution_from_location(logger, location, distributions_dir)?;

        Self::from_zip_file(&archive_path, &extract_path)
    }

    /// Obtain an instance by extracting a zip file to a directory.
    pub fn from_zip_file(path: &Path, extract_dir: &Path) -> Result<Self> {
        let zip_data = std::fs::read(path)?;
        let cursor = std::io::Cursor::new(zip_data);

        let mut zf = zip::ZipArchive::new(cursor)?;

        {
            let _lock = DistributionExtractLock::new(extract_dir)?;
            extract_zip(extract_dir, &mut zf)?;
        }

        Self::from_directory(extract_dir)
    }

    /// Obtain an instance from an extracted zip file on the filesystem.
    pub fn from_directory(path: &Path) -> Result<Self> {
        let filenames: Vec<String> = std::fs::read_dir(path)?
            .map(|f| Ok(f?.file_name().to_string_lossy().to_string()))
            .collect::<Result<Vec<String>>>()?;

        let zips = filenames
            .iter()
            .filter(|f| f.ends_with(".zip"))
            .collect::<Vec<&String>>();

        if zips.len() != 1 {
            return Err(anyhow!("unexpected number of .zip files"));
        }

        let zip_file = zips[0];

        if !zip_file.starts_with("python") {
            return Err(anyhow!("unexpected zip file name; expected pythonXY.zip"));
        }

        let xy_version = &zip_file[6..zip_file.find('.').unwrap()];

        if xy_version != "37" {
            return Err(anyhow!(
                "Only Python 3.7 Windows embeddable distributions are currently supported"
            ));
        }

        let python_exe = path.join("python.exe");
        if !python_exe.exists() {
            return Err(anyhow!("{} does not exist", python_exe.display()));
        }

        let python_dll = path.join(format!("python{}.dll", xy_version));
        if !python_dll.exists() {
            return Err(anyhow!("{} does not exist", python_dll.display()));
        }

        let extra_distribution_dlls = find_pe_dependencies_path(&python_dll)?
            .iter()
            .filter_map(|dll| {
                if let Some(dll) = dll_in_list(dll, &filenames) {
                    Some(path.join(dll))
                } else {
                    None
                }
            })
            .collect();

        let license_path = path.join("LICENSE.txt");

        // Extension modules are .pyd files, which are actually DLLs.
        let extension_modules = BTreeMap::from_iter(
            filenames
                .iter()
                .filter(|f| f.ends_with(".pyd"))
                .map(
                    |f| -> Result<(String, WindowsEmbeddableDistributionExtensionModule)> {
                        let name = &f[0..f.len() - 4];
                        let pyd_path = path.join(f);

                        let distribution_dll_dependencies = find_pe_dependencies_path(&pyd_path)?
                            .iter()
                            .filter_map(|dll| {
                                if let Some(dll) = dll_in_list(dll, &filenames) {
                                    Some(path.join(dll))
                                } else {
                                    None
                                }
                            })
                            .collect();

                        Ok((
                            name.to_string(),
                            WindowsEmbeddableDistributionExtensionModule {
                                name: name.to_string(),
                                path: pyd_path,
                                distribution_dll_dependencies,
                            },
                        ))
                    },
                )
                .filter_map(Result::ok),
        );

        let python_zip = path.join(format!("python{}.zip", xy_version));
        if !python_zip.exists() {
            return Err(anyhow!("{} does not exist", python_zip.display()));
        }

        let bytecode_modules = read_stdlib_zip(&python_zip)?;

        Ok(WindowsEmbeddableDistribution {
            python_exe,
            python_dll,
            python_zip,
            extra_distribution_dlls,
            license_path,
            extension_modules,
            bytecode_modules,
        })
    }
}

impl PythonDistribution for WindowsEmbeddableDistribution {
    fn clone_box(&self) -> Box<dyn PythonDistribution> {
        Box::new(self.clone())
    }

    fn python_exe_path(&self) -> &Path {
        &self.python_exe
    }

    fn python_major_minor_version(&self) -> String {
        unimplemented!()
    }

    fn python_module_suffixes(&self) -> Result<PythonModuleSuffixes> {
        PythonModuleSuffixes::resolve_from_python_exe(&self.python_exe)
    }

    fn create_bytecode_compiler(&self) -> Result<BytecodeCompiler> {
        BytecodeCompiler::new(&self.python_exe)
    }

    fn resolve_importlib_bytecode(&self) -> Result<ImportlibBytecode> {
        let mut compiler = self.create_bytecode_compiler()?;

        derive_importlib(
            IMPORTLIB_BOOTSTRAP_PY_37,
            IMPORTLIB_BOOTSTRAP_EXTERNAL_PY_37,
            &mut compiler,
        )
    }

    fn as_python_executable_builder(
        &self,
        _logger: &slog::Logger,
        host_triple: &str,
        target_triple: &str,
        name: &str,
        resources_policy: &PythonResourcesPolicy,
        config: &EmbeddedPythonConfig,
        _extension_module_filter: &ExtensionModuleFilter,
        _preferred_extension_module_variants: Option<HashMap<String, String>>,
        _include_sources: bool,
        _include_resources: bool,
        _include_test: bool,
    ) -> Result<Box<dyn PythonBinaryBuilder>> {
        Ok(Box::new(WindowsEmbeddedablePythonExecutableBuilder {
            host_triple: host_triple.to_string(),
            target_triple: target_triple.to_string(),
            exe_name: name.to_string(),
            python_exe: self.python_exe.clone(),
            python_dll: self.python_dll.clone(),
            // TODO add distribution resources to this instance.
            resources: EmbeddedPythonResourcesPrePackaged::new(resources_policy),
            config: config.clone(),
            importlib_bytecode: self.resolve_importlib_bytecode()?,
        }))
    }

    fn filter_extension_modules(
        &self,
        _logger: &slog::Logger,
        _filter: &ExtensionModuleFilter,
        _preferred_variants: Option<HashMap<String, String>>,
    ) -> Result<Vec<DistributionExtensionModule>> {
        unimplemented!();
    }

    fn source_modules(&self) -> Result<Vec<PythonModuleSource>> {
        // Windows embeddable distributions don't have source modules.
        Ok(Vec::new())
    }

    fn resource_datas(&self) -> Result<Vec<ResourceData>> {
        // There are some resources in the zip file. But we haven't implemented
        // parsing for them.
        Ok(Vec::new())
    }

    fn ensure_pip(&self, logger: &slog::Logger) -> Result<PathBuf> {
        // Windows embeddable distributions don't contain pip or ensurepip. So we
        // download a deterministic version of get-pip.py and run it to install pip.

        let dist_dir = self
            .python_exe
            .parent()
            .ok_or_else(|| anyhow!("could not resolve parent directory"))?;
        let dist_parent_dir = dist_dir
            .parent()
            .ok_or_else(|| anyhow!("could not resolve parent directory"))?;

        let pip_exe_path = dist_dir.join("pip.exe");

        if !pip_exe_path.exists() {
            warn!(logger, "pip not present; installing");
            bootstrap_packaging_tools(
                logger,
                &self.python_exe,
                dist_parent_dir,
                // Install executables and packages in the distribution itself because
                // the default locations of `Scripts` and `Lib/site-packages` aren't picked
                // up by the distribution by default.
                dist_dir,
                dist_dir,
            )?;
        }

        Ok(pip_exe_path)
    }

    fn resolve_distutils(
        &self,
        _logger: &slog::Logger,
        _dest_dir: &Path,
        _extra_python_paths: &[&Path],
    ) -> Result<HashMap<String, String>> {
        // This method is meant to install a custom version of distutils.
        // Since we don't need to hack distutils to target the Windows embeddable
        // distributions, no hacking is necessary.
        Ok(HashMap::new())
    }

    fn filter_compatible_python_resources(
        &self,
        _logger: &slog::Logger,
        resources: &[PythonResource],
        _target_triple: &str,
    ) -> Result<Vec<PythonResource>> {
        Ok(resources.to_vec())
    }
}

/// Looks for a DLL in a file names list without case sensitivity.
///
/// PE may list a DLL using UPPERCASE but its filename in the distribution
/// may be lowercase.
fn dll_in_list(dll: &str, files: &[String]) -> Option<String> {
    let dll = dll.to_lowercase();

    if files.contains(&dll) {
        Some(dll)
    } else {
        None
    }
}

fn read_stdlib_zip(
    path: &Path,
) -> Result<BTreeMap<String, WindowsEmbeddableDistributionBytecodeModule>> {
    let zip_fh = std::fs::File::open(&path)?;
    let reader = std::io::BufReader::new(zip_fh);
    let mut zf = zip::ZipArchive::new(reader)?;

    let mut res = BTreeMap::new();

    for i in 0..zf.len() {
        let mut f = zf.by_index(i)?;

        if !f.is_file() {
            return Err(anyhow!("zip archive member {} is not a file", f.name()));
        }

        // TODO collect or record existence of non-bytecode files?
        if !f.name().ends_with(".pyc") {
            continue;
        }

        if f.size() <= 16 {
            return Err(anyhow!("zip archive member {} is too small", f.name()));
        }

        let read_size = (f.size() - 16).try_into()?;
        let code = podio::ReadPodExt::read_exact(&mut f, read_size)?;

        let is_package = f.name().ends_with("__init__.pyc");

        let name = f.name().to_string();
        // Strip .pyc.
        let name = name[0..name.len() - 4].to_string();
        // Normalize path separator to module name.
        let name = name.replace("/", ".");

        let name = if name.ends_with(".__init__") {
            name[0..name.len() - ".__init__".len()].to_string()
        } else {
            name
        };

        res.insert(
            name.clone(),
            WindowsEmbeddableDistributionBytecodeModule {
                name,
                is_package,
                code,
            },
        );
    }

    Ok(res)
}

/// A `PythonBinaryBuilder` used by `WindowsEmbeddableDistribution`.
///
/// Instances can derive artifacts needed to build executables using
/// `WindowsEmbeddableDistribution` instances.
#[derive(Clone, Debug)]
pub struct WindowsEmbeddedablePythonExecutableBuilder {
    /// Rust target triple we are running from.
    host_triple: String,

    /// Rust target triple we are building for.
    target_triple: String,

    /// The name of the executable to build.
    exe_name: String,

    /// Path to Python executable that can be invoked at build time.
    python_exe: PathBuf,

    /// Path to pythonXY dll.
    python_dll: PathBuf,

    /// Python resources to be embedded in the binary.
    resources: EmbeddedPythonResourcesPrePackaged,

    /// Configuration for embedded Python interpreter.
    config: EmbeddedPythonConfig,

    /// Compiled bytecode for importlib bootstrap modules.
    importlib_bytecode: ImportlibBytecode,
}

impl WindowsEmbeddedablePythonExecutableBuilder {
    /// Resolve a `pythonXY.lib` suitable for linking against.
    ///
    /// Windows embeddable distributions link against an existing python DLL
    /// when the cpython/python3-sys crates are built. But the `pyembed` crate
    /// has a `links` entry against `pythonXY` (that's a literal `XY`, not a
    /// placeholder for the actual version). That means we need to generate a
    /// `pythonXY.lib` to placate the linker. The function generate content for
    /// such a file.
    pub fn resolve_pythonxy_lib(&self, logger: &slog::Logger, opt_level: &str) -> Result<Vec<u8>> {
        warn!(logger, "compiling fake pythonXY.lib");

        let temp_dir = TempDir::new("pyoxidizer-build-libpython")?;

        let empty_source = temp_dir.path().join("empty.c");
        std::fs::File::create(&empty_source)?;

        cc::Build::new()
            .out_dir(temp_dir.path())
            .host(&self.host_triple)
            .target(&self.target_triple)
            .opt_level_str(opt_level)
            .file(&empty_source)
            .cargo_metadata(false)
            .compile("pythonXY");

        let output_path = temp_dir.path().join("pythonXY.lib");

        Ok(std::fs::read(&output_path)?)
    }

    /// Derive a `PythonLinkingInfo` for the current builder.
    pub fn as_python_linking_info(
        &self,
        logger: &slog::Logger,
        opt_level: &str,
    ) -> Result<PythonLinkingInfo> {
        let libpython_dir = self
            .python_dll
            .parent()
            .ok_or_else(|| anyhow!("unable to resolve parent directory of Python DLL"))?;

        let cargo_metadata = vec![
            "cargo:rustc-link-lib=static=pythonXY".to_string(),
            format!("cargo:rustc-link-search=native={}", libpython_dir.display()),
        ];

        Ok(PythonLinkingInfo {
            libpythonxy_filename: PathBuf::from("pythonXY.lib"),
            libpythonxy_data: self.resolve_pythonxy_lib(logger, opt_level)?,
            libpython_filename: Some(self.python_dll.clone()),
            libpyembeddedconfig_filename: None,
            libpyembeddedconfig_data: None,
            cargo_metadata,
        })
    }
}

impl PythonBinaryBuilder for WindowsEmbeddedablePythonExecutableBuilder {
    fn clone_box(&self) -> Box<dyn PythonBinaryBuilder> {
        Box::new(self.clone())
    }

    fn name(&self) -> String {
        self.exe_name.clone()
    }

    fn python_resources_policy(&self) -> &PythonResourcesPolicy {
        unimplemented!();
    }

    fn python_exe_path(&self) -> &Path {
        &self.python_exe
    }

    fn in_memory_module_sources(&self) -> BTreeMap<String, PythonModuleSource> {
        self.resources.get_in_memory_module_sources()
    }

    fn in_memory_module_bytecodes(&self) -> BTreeMap<String, BytecodeModuleSource> {
        self.resources.get_in_memory_module_bytecodes()
    }

    fn in_memory_package_resources(&self) -> BTreeMap<String, BTreeMap<String, Vec<u8>>> {
        self.resources.get_in_memory_package_resources()
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

    fn add_in_memory_module_bytecode(&mut self, module: &BytecodeModuleSource) -> Result<()> {
        self.resources.add_in_memory_module_bytecode(module)
    }

    fn add_relative_path_module_bytecode(
        &mut self,
        prefix: &str,
        module: &BytecodeModuleSource,
    ) -> Result<()> {
        self.resources
            .add_relative_path_module_bytecode(module, prefix)
    }

    fn add_in_memory_package_resource(&mut self, resource: &ResourceData) -> Result<()> {
        self.resources.add_in_memory_package_resource(resource)
    }

    fn add_relative_path_package_resource(
        &mut self,
        prefix: &str,
        resource: &ResourceData,
    ) -> Result<()> {
        self.resources
            .add_relative_path_package_resource(prefix, resource)
    }

    fn add_builtin_distribution_extension_module(
        &mut self,
        _extension_module: &DistributionExtensionModule,
    ) -> Result<()> {
        unimplemented!()
    }

    fn add_in_memory_distribution_extension_module(
        &mut self,
        _extension_module: &DistributionExtensionModule,
    ) -> Result<()> {
        unimplemented!();
    }

    fn add_relative_path_distribution_extension_module(
        &mut self,
        _prefix: &str,
        _extension_module: &DistributionExtensionModule,
    ) -> Result<()> {
        unimplemented!();
    }

    fn add_distribution_extension_module(
        &mut self,
        _extension_module: &DistributionExtensionModule,
    ) -> Result<()> {
        unimplemented!();
    }

    fn add_in_memory_dynamic_extension_module(
        &mut self,
        _extension_module: &ExtensionModuleData,
    ) -> Result<()> {
        unimplemented!();
    }

    fn add_relative_path_dynamic_extension_module(
        &mut self,
        _prefix: &str,
        _extension_module: &ExtensionModuleData,
    ) -> Result<()> {
        unimplemented!();
    }

    fn add_dynamic_extension_module(
        &mut self,
        _extension_module: &ExtensionModuleData,
    ) -> Result<()> {
        unimplemented!()
    }

    fn add_static_extension_module(
        &mut self,
        _extension_module_data: &ExtensionModuleData,
    ) -> Result<()> {
        unimplemented!()
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
        // jemalloc not supported on Windows.
        false
    }

    fn as_embedded_python_binary_data(
        &self,
        logger: &slog::Logger,
        opt_level: &str,
    ) -> Result<EmbeddedPythonBinaryData> {
        let resources = self
            .resources
            .package(logger, &self.python_exe)?
            .try_into()?;

        let extra_files = FileManifest::default();

        let linking_info = self.as_python_linking_info(logger, opt_level)?;

        Ok(EmbeddedPythonBinaryData {
            config: self.config.clone(),
            linking_info,
            importlib: self.importlib_bytecode.clone(),
            resources,
            extra_files,
            host: self.host_triple.clone(),
            target: self.target_triple.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    use crate::py_packaging::packaging_tool::pip_install;
    use {
        super::*, crate::python_distributions::CPYTHON_WINDOWS_EMBEDDABLE_BY_TRIPLE,
        crate::testutil::*,
    };

    #[test]
    fn test_windows_embeddable_distribution() -> Result<()> {
        let logger = get_logger()?;
        let temp_dir = tempdir::TempDir::new("pyoxidizer-test")?;

        let amd64_dist = CPYTHON_WINDOWS_EMBEDDABLE_BY_TRIPLE
            .get("x86_64-pc-windows-msvc")
            .unwrap();

        let dist = WindowsEmbeddableDistribution::from_location(
            &logger,
            &PythonDistributionLocation::Url {
                url: amd64_dist.url.clone(),
                sha256: amd64_dist.sha256.clone(),
            },
            temp_dir.path(),
        )?;

        let extract_dir = temp_dir
            .path()
            .join(format!("python.{}", &amd64_dist.sha256[0..12]));

        assert_eq!(dist.python_exe, extract_dir.join("python.exe"));
        assert_eq!(
            dist.extension_modules.get("_ssl"),
            Some(&WindowsEmbeddableDistributionExtensionModule {
                name: "_ssl".to_string(),
                path: extract_dir.join("_ssl.pyd"),
                distribution_dll_dependencies: vec![
                    extract_dir.join("vcruntime140.dll"),
                    extract_dir.join("libcrypto-1_1.dll"),
                    extract_dir.join("libssl-1_1.dll"),
                    extract_dir.join("python37.dll")
                ],
            })
        );
        assert_eq!(dist.source_modules()?, Vec::new());
        assert!(dist.bytecode_modules.contains_key("distutils"));
        let distutils = dist.bytecode_modules.get("distutils").unwrap();
        assert_eq!(distutils.name, "distutils".to_string());
        assert!(distutils.is_package);
        assert!(!distutils.code.is_empty());
        assert_eq!(dist.resource_datas()?, Vec::new());

        Ok(())
    }

    #[test]
    #[cfg(windows)]
    fn test_as_python_executable_builder() -> Result<()> {
        let logger = get_logger()?;
        let dist = get_windows_embeddable_distribution()?;
        let config = EmbeddedPythonConfig::default();
        let extension_module_filter = ExtensionModuleFilter::All;

        let builder = dist.as_python_executable_builder(
            &logger,
            env!("HOST"),
            env!("HOST"),
            "foo",
            &PythonResourcesPolicy::InMemoryOnly,
            &config,
            &extension_module_filter,
            None,
            true,
            true,
            true,
        )?;

        assert_eq!(builder.name(), "foo".to_string());
        assert_eq!(builder.python_exe_path(), &dist.python_exe);
        assert!(!builder.requires_jemalloc());

        Ok(())
    }

    #[test]
    #[cfg(windows)]
    fn test_resolve_importlib_bytecode() -> Result<()> {
        let dist = get_windows_embeddable_distribution()?;

        dist.resolve_importlib_bytecode()?;

        Ok(())
    }

    #[test]
    #[cfg(windows)]
    fn test_ensure_pip() -> Result<()> {
        let logger = get_logger()?;
        let dist = get_windows_embeddable_distribution()?;

        let pip_path = dist.ensure_pip(&logger)?;

        assert_eq!(pip_path, dist.python_exe.parent().unwrap().join("pip.exe"));
        assert!(pip_path.exists());

        Ok(())
    }

    #[test]
    #[cfg(windows)]
    fn test_install_black() -> Result<()> {
        let logger = get_logger()?;
        let dist = get_windows_embeddable_distribution()?;

        dist.ensure_pip(&logger)?;

        let resources = pip_install(
            &logger,
            &dist,
            env!("HOST"),
            false,
            &["black==19.10b0".to_string()],
            &HashMap::new(),
        )?;

        assert!(resources.iter().any(|r| r.full_name() == "appdirs"));
        assert!(resources.iter().any(|r| r.full_name() == "black"));

        Ok(())
    }
}
