// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Defining and manipulating binaries embedding Python.
*/

use {
    super::config::EmbeddedPythonConfig,
    super::embedded_resource::EmbeddedPythonResources,
    super::libpython::ImportlibBytecode,
    super::pyembed::{derive_python_config, write_default_python_config_rs},
    super::resource::{BytecodeModule, ExtensionModuleData, ResourceData, SourceModule},
    super::standalone_distribution::ExtensionModule,
    anyhow::Result,
    std::collections::BTreeMap,
    std::convert::TryFrom,
    std::fs::File,
    std::io::Write,
    std::path::{Path, PathBuf},
};

/// Describes a generic way to build a Python binary.
///
/// Binary here means an executable or library containing or linking to a
/// Python interpreter. It also includes embeddable resources within that
/// binary.
///
/// Concrete implementations can be turned into build artifacts or binaries
/// themselves.
pub trait PythonBinaryBuilder {
    /// Clone self into a Box'ed trait object.
    fn clone_box(&self) -> Box<dyn PythonBinaryBuilder>;

    /// The name of the binary.
    fn name(&self) -> String;

    /// Path to Python executable that can be used to derive info at build time.
    ///
    /// The produced binary is effectively a clone of the Python distribution behind the
    /// returned executable.
    fn python_exe_path(&self) -> &Path;

    /// Obtain source modules to be embedded in this instance.
    fn source_modules(&self) -> &BTreeMap<String, SourceModule>;

    /// Obtain bytecode modules to be embedded in this instance.
    fn bytecode_modules(&self) -> &BTreeMap<String, BytecodeModule>;

    /// Obtain resource data to be embedded in this instance.
    fn resources(&self) -> &BTreeMap<String, BTreeMap<String, Vec<u8>>>;

    /// Obtain extension modules to be embedded in this instance.
    fn extension_modules(&self) -> &BTreeMap<String, ExtensionModule>;

    /// Obtain extension modules to be embedded in this instance.
    fn extension_module_datas(&self) -> &BTreeMap<String, ExtensionModuleData>;

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

    /// Whether the binary requires the jemalloc library.
    fn requires_jemalloc(&self) -> bool;

    /// Obtain an `EmbeddedPythonBinaryData` instance from this one.
    fn as_embedded_python_binary_data(
        &self,
        logger: &slog::Logger,
        host: &str,
        target: &str,
        opt_level: &str,
    ) -> Result<EmbeddedPythonBinaryData>;
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

impl TryFrom<EmbeddedPythonResources> for EmbeddedResourcesBlobs {
    type Error = anyhow::Error;

    fn try_from(value: EmbeddedPythonResources) -> Result<Self, Self::Error> {
        let mut module_names = Vec::new();
        let mut modules = Vec::new();
        let mut resources = Vec::new();

        value.write_blobs(&mut module_names, &mut modules, &mut resources);

        Ok(Self {
            module_names,
            modules,
            resources,
        })
    }
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
