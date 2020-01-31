// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Defining and manipulating binaries embedding Python.
*/

use anyhow::Result;
use slog::warn;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempdir::TempDir;

use super::config::EmbeddedPythonConfig;
use super::embedded_resource::EmbeddedPythonResourcesPrePackaged;
use super::libpython::{link_libpython, ImportlibBytecode};
use super::pyembed::{derive_python_config, write_default_python_config_rs};
use super::resource::{BytecodeModule, ExtensionModuleData, ResourceData, SourceModule};
use super::standalone_distribution::{
    ExtensionModule, ExtensionModuleFilter, StandaloneDistribution,
};
use crate::py_packaging::distribution::PythonDistribution;

/// Describes a generic way to build a Python binary.
///
/// Binary here means an executable or library containing or linking to a
/// Python interpreter. It also includes embeddable resources within that
/// binary.
///
/// Concrete implementations can be turned into build artifacts or binaries
/// themselves.
pub trait PythonBinaryBuilder
where
    Self: Sized,
{
    /// The name of the binary.
    fn name(&self) -> String;

    /// Add a source module to the collection of embedded source modules.
    fn add_source_module(&mut self, module: &SourceModule);

    /// Add a bytecode module to the collection of embedded bytecode modules.
    fn add_bytecode_module(&mut self, module: &BytecodeModule);

    /// Add resource data to the collection of embedded resource data.
    fn add_resource(&mut self, resource: &ResourceData);

    /// Add an extension module to be embedded in the binary.
    fn add_extension_module(&mut self, extension_module: &ExtensionModule);

    /// Add an extension module to be embedded in the binary.
    fn add_extension_module_data(&mut self, extension_module_data: &ExtensionModuleData);

    /// Filter embedded resources against names in files.
    ///
    /// `files` is files to read names from.
    ///
    /// `glob_patterns` is file patterns of files to read names from.
    fn filter_resources_from_files(
        &mut self,
        logger: &slog::Logger,
        files: &[&Path],
        glob_patterns: &[&str],
    ) -> Result<()>;
}

/// A self-contained Python executable before it is compiled.
#[derive(Debug)]
pub struct PreBuiltPythonExecutable {
    /// The name of the executable to build.
    exe_name: String,

    /// The Python distribution being used to build this executable.
    pub distribution: Arc<StandaloneDistribution>,

    /// Python resources to be embedded in the binary.
    pub resources: EmbeddedPythonResourcesPrePackaged,

    /// Configuration of the embedded Python interpreter.
    pub config: EmbeddedPythonConfig,

    /// Path to python executable that can be invoked at build time.
    pub python_exe: PathBuf,

    /// Bytecode for importlib bootstrap modules.
    pub importlib_bytecode: ImportlibBytecode,
}

impl PythonBinaryBuilder for PreBuiltPythonExecutable {
    fn name(&self) -> String {
        self.exe_name.clone()
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
}

impl PreBuiltPythonExecutable {
    /// Create an instance from a Python distribution, using settings.
    #[allow(clippy::too_many_arguments)]
    pub fn from_python_distribution(
        logger: &slog::Logger,
        distribution: Arc<StandaloneDistribution>,
        name: &str,
        config: &EmbeddedPythonConfig,
        extension_module_filter: &ExtensionModuleFilter,
        preferred_extension_module_variants: Option<HashMap<String, String>>,
        include_sources: bool,
        include_resources: bool,
        include_test: bool,
    ) -> Result<Self> {
        let mut resources = EmbeddedPythonResourcesPrePackaged::from_distribution(
            logger,
            distribution.clone(),
            extension_module_filter,
            preferred_extension_module_variants,
            include_sources,
            include_resources,
            include_test,
        )?;

        // Always ensure minimal extension modules are present, otherwise we get
        // missing symbol errors at link time.
        for ext in
            distribution.filter_extension_modules(&logger, &ExtensionModuleFilter::Minimal, None)?
        {
            if !resources.extension_modules.contains_key(&ext.module) {
                resources.add_extension_module(&ext);
            }
        }

        let python_exe = distribution.python_exe.clone();
        let importlib_bytecode = distribution.resolve_importlib_bytecode()?;

        Ok(PreBuiltPythonExecutable {
            exe_name: name.to_string(),
            distribution,
            resources,
            config: config.clone(),
            python_exe,
            importlib_bytecode,
        })
    }

    /// Build a Python library suitable for linking.
    ///
    /// This will take the underlying distribution, resources, and
    /// configuration and produce a new executable binary.
    pub fn build_libpython(
        &self,
        logger: &slog::Logger,
        host: &str,
        target: &str,
        opt_level: &str,
    ) -> Result<PythonLibrary> {
        let resources = self.resources.package(logger, &self.python_exe)?;

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

        let mut cargo_metadata: Vec<String> = Vec::new();
        cargo_metadata.extend(library_info.cargo_metadata);

        let libpython_data = std::fs::read(&library_info.libpython_path)?;
        let libpyembeddedconfig_data = std::fs::read(&library_info.libpyembeddedconfig_path)?;

        Ok(PythonLibrary {
            libpython_filename: PathBuf::from(library_info.libpython_path.file_name().unwrap()),
            libpython_data,
            libpyembeddedconfig_filename: PathBuf::from(
                library_info.libpyembeddedconfig_path.file_name().unwrap(),
            ),
            libpyembeddedconfig_data,
            cargo_metadata,
        })
    }

    /// Generate data embedded in binaries representing Python resource data.
    pub fn build_embedded_blobs(&self, logger: &slog::Logger) -> Result<EmbeddedResourcesBlobs> {
        let embedded_resources = self.resources.package(logger, &self.python_exe)?;

        let mut module_names = Vec::new();
        let mut modules = Vec::new();
        let mut resources = Vec::new();

        embedded_resources.write_blobs(&mut module_names, &mut modules, &mut resources);

        Ok(EmbeddedResourcesBlobs {
            module_names,
            modules,
            resources,
        })
    }
}

/// A self-contained Python executable after it is built.
pub struct PythonLibrary {
    pub libpython_filename: PathBuf,
    pub libpython_data: Vec<u8>,
    pub libpyembeddedconfig_filename: PathBuf,
    pub libpyembeddedconfig_data: Vec<u8>,
    pub cargo_metadata: Vec<String>,
}

/// Represents serialized data embedded in binaries for loading Python resources.
pub struct EmbeddedResourcesBlobs {
    pub module_names: Vec<u8>,
    pub modules: Vec<u8>,
    pub resources: Vec<u8>,
}

/// Holds filesystem paths to resources required to build a binary embedding Python.
pub struct EmbeddedPythonBinaryPaths {
    pub importlib_bootstrap: PathBuf,
    pub importlib_bootstrap_external: PathBuf,
    pub module_names: PathBuf,
    pub py_modules: PathBuf,
    pub resources: PathBuf,
    pub libpython: PathBuf,
    pub libpyembeddedconfig: PathBuf,
    pub config_rs: PathBuf,
    pub cargo_metadata: PathBuf,
}

/// Represents resources to embed Python in a binary.
pub struct EmbeddedPythonBinaryData {
    pub config: EmbeddedPythonConfig,
    pub library: PythonLibrary,
    pub importlib: ImportlibBytecode,
    pub resources: EmbeddedResourcesBlobs,
    pub host: String,
    pub target: String,
}

impl EmbeddedPythonBinaryData {
    pub fn from_pre_built_python_executable(
        exe: &PreBuiltPythonExecutable,
        logger: &slog::Logger,
        host: &str,
        target: &str,
        opt_level: &str,
    ) -> Result<EmbeddedPythonBinaryData> {
        let library = exe.build_libpython(logger, host, target, opt_level)?;
        let resources = exe.build_embedded_blobs(logger)?;
        warn!(
            logger,
            "deriving custom importlib modules to support in-memory importing"
        );
        let importlib = exe.importlib_bytecode.clone();

        Ok(EmbeddedPythonBinaryData {
            config: exe.config.clone(),
            library,
            importlib,
            resources,
            host: host.to_string(),
            target: target.to_string(),
        })
    }

    /// Write out files needed to link a binary.
    pub fn write_files(&self, dest_dir: &Path) -> Result<EmbeddedPythonBinaryPaths> {
        let importlib_bootstrap = dest_dir.join("importlib_bootstrap");
        let mut fh = File::create(&importlib_bootstrap)?;
        fh.write_all(&self.importlib.bootstrap)?;

        let importlib_bootstrap_external = dest_dir.join("importlib_bootstrap_external");
        let mut fh = File::create(&importlib_bootstrap_external)?;
        fh.write_all(&self.importlib.bootstrap_external)?;

        let module_names = dest_dir.join("py-module-names");
        let mut fh = File::create(&module_names)?;
        fh.write_all(&self.resources.module_names)?;

        let py_modules = dest_dir.join("py-modules");
        let mut fh = File::create(&py_modules)?;
        fh.write_all(&self.resources.modules)?;

        let resources = dest_dir.join("python-resources");
        let mut fh = File::create(&resources)?;
        fh.write_all(&self.resources.resources)?;

        let libpython = dest_dir.join(&self.library.libpython_filename);
        let mut fh = File::create(&libpython)?;
        fh.write_all(&self.library.libpython_data)?;

        let libpyembeddedconfig = dest_dir.join(&self.library.libpyembeddedconfig_filename);
        let mut fh = File::create(&libpyembeddedconfig)?;
        fh.write_all(&self.library.libpyembeddedconfig_data)?;

        let config_rs_data = derive_python_config(
            &self.config,
            &importlib_bootstrap,
            &importlib_bootstrap_external,
            &py_modules,
            &resources,
        );
        let config_rs = dest_dir.join("default_python_config.rs");
        write_default_python_config_rs(&config_rs, &config_rs_data)?;

        let mut cargo_metadata_lines = Vec::new();
        cargo_metadata_lines.extend(self.library.cargo_metadata.clone());

        // Tell Cargo where libpythonXY is located.
        cargo_metadata_lines.push(format!(
            "cargo:rustc-link-search=native={}",
            dest_dir.display()
        ));

        // Give dependent crates the path to the default config file.
        cargo_metadata_lines.push(format!(
            "cargo:default-python-config-rs={}",
            config_rs.display()
        ));

        let cargo_metadata = dest_dir.join("cargo_metadata.txt");
        let mut fh = File::create(&cargo_metadata)?;
        fh.write_all(cargo_metadata_lines.join("\n").as_bytes())?;

        Ok(EmbeddedPythonBinaryPaths {
            importlib_bootstrap,
            importlib_bootstrap_external,
            module_names,
            py_modules,
            resources,
            libpython,
            libpyembeddedconfig,
            config_rs,
            cargo_metadata,
        })
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::py_packaging::standalone_distribution::ExtensionModuleFilter;
    use crate::testutil::*;

    pub fn get_prebuilt(logger: &slog::Logger) -> Result<PreBuiltPythonExecutable> {
        let distribution = get_default_distribution()?;
        let mut resources = EmbeddedPythonResourcesPrePackaged::default();

        // We need to add minimal extension modules so builds actually work. If they are missing,
        // we'll get missing symbol errors during linking.
        for ext in
            distribution.filter_extension_modules(logger, &ExtensionModuleFilter::Minimal, None)?
        {
            resources.add_extension_module(&ext);
        }

        let config = EmbeddedPythonConfig::default();

        let python_exe = distribution.python_exe.clone();
        let importlib_bytecode = distribution.resolve_importlib_bytecode()?;

        Ok(PreBuiltPythonExecutable {
            exe_name: "testapp".to_string(),
            distribution,
            resources,
            config,
            python_exe,
            importlib_bytecode,
        })
    }

    pub fn get_embedded(logger: &slog::Logger) -> Result<EmbeddedPythonBinaryData> {
        EmbeddedPythonBinaryData::from_pre_built_python_executable(
            &get_prebuilt(logger)?,
            &get_logger()?,
            env!("HOST"),
            env!("HOST"),
            "0",
        )
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
