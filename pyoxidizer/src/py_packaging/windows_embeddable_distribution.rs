// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Functionality for Windows embeddable distributions. */

use {
    super::binary::{EmbeddedPythonBinaryData, PythonBinaryBuilder, PythonLibrary},
    super::bytecode::BytecodeCompiler,
    super::config::EmbeddedPythonConfig,
    super::distribution::{
        download_distribution, resolve_python_distribution_from_location, DistributionExtractLock,
        ExtensionModuleFilter, PythonDistribution, PythonDistributionLocation,
        IMPORTLIB_BOOTSTRAP_EXTERNAL_PY_37, IMPORTLIB_BOOTSTRAP_PY_37,
    },
    super::embedded_resource::EmbeddedPythonResourcesPrePackaged,
    super::libpython::{derive_importlib, ImportlibBytecode},
    super::resource::{BytecodeModule, ExtensionModuleData, ResourceData, SourceModule},
    super::standalone_distribution::ExtensionModule,
    crate::analyze::find_pe_dependencies_path,
    crate::python_distributions::GET_PIP_PY_19,
    anyhow::{anyhow, Context, Result},
    slog::warn,
    std::collections::{BTreeMap, HashMap},
    std::convert::TryInto,
    std::fmt::{Debug, Formatter},
    std::io::{BufRead, BufReader, Read},
    std::iter::FromIterator,
    std::path::{Path, PathBuf},
};

/// Obtain the crc32 of a filesystem path.
fn crc32_path(path: &Path) -> Result<u32> {
    let data = std::fs::read(path)?;

    Ok(crc::crc32::checksum_ieee(&data))
}

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

            for i in 0..zf.len() {
                let mut f = zf.by_index(i)?;

                if !f.is_file() {
                    continue;
                }

                let dest_path = extract_dir.join(f.sanitized_name());

                if dest_path.exists() && crc32_path(&dest_path)? != f.crc32() {
                    std::fs::remove_file(&dest_path)?;
                }

                if !dest_path.exists() {
                    let parent = dest_path
                        .parent()
                        .ok_or_else(|| anyhow!("could not resolve parent"))?;
                    std::fs::create_dir_all(parent)
                        .context(format!("creating parent directory {}", parent.display()))?;

                    let mut data = Vec::new();
                    f.read_to_end(&mut data)?;
                    std::fs::write(&dest_path, data)
                        .context(format!("writing {}", dest_path.display()))?;

                    // Assertion: we only use zip files for Windows embeddable distributions
                    // and don't need to care about the execute bit.
                }
            }
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
    fn python_exe_path(&self) -> &Path {
        &self.python_exe
    }

    fn python_major_minor_version(&self) -> String {
        unimplemented!()
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
        name: &str,
        _config: &EmbeddedPythonConfig,
        _extension_module_filter: &ExtensionModuleFilter,
        _preferred_extension_module_variants: Option<HashMap<String, String>>,
        _include_sources: bool,
        _include_resources: bool,
        _include_test: bool,
    ) -> Result<Box<dyn PythonBinaryBuilder>> {
        Ok(Box::new(WindowsEmbeddedablePythonExecutableBuilder {
            exe_name: name.to_string(),
            python_exe: self.python_exe.clone(),
            // TODO add distribution resources to this instance.
            resources: EmbeddedPythonResourcesPrePackaged::default(),
        }))
    }

    fn filter_extension_modules(
        &self,
        _logger: &slog::Logger,
        _filter: &ExtensionModuleFilter,
        _preferred_variants: Option<HashMap<String, String>>,
    ) -> Result<Vec<ExtensionModule>> {
        unimplemented!();
    }

    fn source_modules(&self) -> Result<Vec<SourceModule>> {
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

        let dest_dir = self
            .python_exe
            .parent()
            .ok_or(anyhow!("could not resolve parent directory"))?;

        let pip_exe_path = dest_dir.join("Scripts").join("pip.exe");

        if !pip_exe_path.exists() {
            warn!(logger, "pip not present; installing");
            let get_pip_py =
                download_distribution(&GET_PIP_PY_19.url, &GET_PIP_PY_19.sha256, dest_dir)?;

            let mut cmd = std::process::Command::new(&self.python_exe)
                .arg(format!("{}", get_pip_py.display()))
                .current_dir(dest_dir)
                .stdout(std::process::Stdio::piped())
                .spawn()?;
            {
                let stdout = cmd
                    .stdout
                    .as_mut()
                    .ok_or(anyhow!("could not read stdout"))?;
                let reader = BufReader::new(stdout);
                for line in reader.lines() {
                    warn!(logger, "{}", line?);
                }
            }
        }

        Ok(pip_exe_path)
    }

    fn resolve_distutils(
        &self,
        _logger: &slog::Logger,
        _dest_dir: &Path,
        _extra_python_paths: &[&Path],
    ) -> Result<HashMap<String, String>> {
        unimplemented!()
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
#[derive(Debug)]
pub struct WindowsEmbeddedablePythonExecutableBuilder {
    /// The name of the executable to build.
    exe_name: String,

    /// Path to Python executable that can be invoked at build time.
    python_exe: PathBuf,

    /// Python resources to be embedded in the binary.
    resources: EmbeddedPythonResourcesPrePackaged,
}

impl PythonBinaryBuilder for WindowsEmbeddedablePythonExecutableBuilder {
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
        self.resources.add_source_module(module)
    }

    fn add_bytecode_module(&mut self, module: &BytecodeModule) {
        self.resources.add_bytecode_module(module)
    }

    fn add_resource(&mut self, resource: &ResourceData) {
        self.resources.add_resource(resource)
    }

    fn add_extension_module(&mut self, _extension_module: &ExtensionModule) {
        unimplemented!()
    }

    fn add_extension_module_data(&mut self, _extension_module_data: &ExtensionModuleData) {
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

    fn resolve_python_library(
        &self,
        _logger: &slog::Logger,
        _host: &str,
        _target: &str,
        _opt_level: &str,
    ) -> Result<PythonLibrary> {
        unimplemented!()
    }

    fn as_embedded_python_binary_data(
        &self,
        _logger: &slog::Logger,
        _host: &str,
        _target: &str,
        _opt_level: &str,
    ) -> Result<EmbeddedPythonBinaryData> {
        unimplemented!()
    }
}

#[cfg(test)]
mod tests {
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
            .join(format!("python.{}", amd64_dist.sha256));

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
    fn test_as_python_executable_builder() -> Result<()> {
        let logger = get_logger()?;
        let dist = get_windows_embeddable_distribution()?;
        let config = EmbeddedPythonConfig::default();
        let extension_module_filter = ExtensionModuleFilter::All;

        let builder = dist.as_python_executable_builder(
            &logger,
            "foo",
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

        assert_eq!(
            pip_path,
            dist.python_exe
                .parent()
                .unwrap()
                .join("Scripts")
                .join("pip.exe")
        );

        Ok(())
    }
}
