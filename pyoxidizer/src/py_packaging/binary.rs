// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Defining and manipulating binaries embedding Python.
*/

use {
    super::config::EmbeddedPythonConfig,
    super::embedded_resource::EmbeddedPythonResources,
    super::pyembed::{derive_python_config, write_default_python_config_rs},
    super::standalone_distribution::DistributionExtensionModule,
    crate::app_packaging::resource::FileManifest,
    anyhow::Result,
    python_packaging::resource::{
        PythonExtensionModule, PythonModuleBytecodeFromSource, PythonModuleSource,
        PythonPackageDistributionResource, PythonPackageResource, PythonResource,
    },
    python_packaging::resource_collection::PythonResourcesPolicy,
    std::collections::{BTreeMap, HashMap},
    std::convert::TryFrom,
    std::fs::File,
    std::io::Write,
    std::path::{Path, PathBuf},
};

/// How a binary should link against libpython.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LibpythonLinkMode {
    /// Libpython will be statically linked into the binary.
    Static,
    /// The binary will dynamically link against libpython.
    Dynamic,
}

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

    /// How the binary will link against libpython.
    fn libpython_link_mode(&self) -> LibpythonLinkMode;

    /// Obtain the `PythonResourcesPolicy` for the builder.
    fn python_resources_policy(&self) -> &PythonResourcesPolicy;

    /// Path to Python executable that can be used to derive info at build time.
    ///
    /// The produced binary is effectively a clone of the Python distribution behind the
    /// returned executable.
    fn python_exe_path(&self) -> &Path;

    /// Obtain Python source modules imported from memory to be embedded in this instance.
    fn in_memory_module_sources(&self) -> BTreeMap<String, PythonModuleSource>;

    /// Obtain Python package resources data loaded from memory to be embedded in this instance.
    fn in_memory_package_resources(&self) -> BTreeMap<String, BTreeMap<String, Vec<u8>>>;

    /// Runs `pip install` using the binary builder's settings.
    ///
    /// Returns resources discovered as part of performing an install.
    fn pip_install(
        &self,
        logger: &slog::Logger,
        verbose: bool,
        install_args: &[String],
        extra_envs: &HashMap<String, String>,
    ) -> Result<Vec<PythonResource>>;

    /// Reads Python resources from the filesystem.
    fn read_package_root(
        &self,
        logger: &slog::Logger,
        path: &Path,
        packages: &[String],
    ) -> Result<Vec<PythonResource>>;

    /// Read Python resources from a populated virtualenv directory.
    fn read_virtualenv(&self, logger: &slog::Logger, path: &Path) -> Result<Vec<PythonResource>>;

    /// Runs `python setup.py install` using the binary builder's settings.
    ///
    /// Returns resources discovered as part of performing an install.
    fn setup_py_install(
        &self,
        logger: &slog::Logger,
        package_path: &Path,
        verbose: bool,
        extra_envs: &HashMap<String, String>,
        extra_global_arguments: &[String],
    ) -> Result<Vec<PythonResource>>;

    /// Add Python module source code to be imported from memory to the embedded resources.
    fn add_in_memory_module_source(&mut self, module: &PythonModuleSource) -> Result<()>;

    /// Add Python module source code to be imported from the filesystem relative to the produced binary.
    fn add_relative_path_module_source(
        &mut self,
        prefix: &str,
        module: &PythonModuleSource,
    ) -> Result<()>;

    /// Add Python module source code to a location as determined by the builder's resource policy.
    fn add_module_source(&mut self, module: &PythonModuleSource) -> Result<()> {
        match self.python_resources_policy().clone() {
            PythonResourcesPolicy::InMemoryOnly
            | PythonResourcesPolicy::PreferInMemoryFallbackFilesystemRelative(_) => {
                self.add_in_memory_module_source(module)
            }
            PythonResourcesPolicy::FilesystemRelativeOnly(ref prefix) => {
                self.add_relative_path_module_source(prefix, module)
            }
        }
    }

    /// Add a Python module bytecode to be imported from memory to the embedded resources.
    fn add_in_memory_module_bytecode(
        &mut self,
        module: &PythonModuleBytecodeFromSource,
    ) -> Result<()>;

    /// Add Python module bytecode to be imported from the filesystem relative to the produced binary.
    fn add_relative_path_module_bytecode(
        &mut self,
        prefix: &str,
        module: &PythonModuleBytecodeFromSource,
    ) -> Result<()>;

    /// Add Python module bytecode to a location as determined by the builder's resource policy.
    fn add_module_bytecode(&mut self, module: &PythonModuleBytecodeFromSource) -> Result<()> {
        match self.python_resources_policy().clone() {
            PythonResourcesPolicy::InMemoryOnly
            | PythonResourcesPolicy::PreferInMemoryFallbackFilesystemRelative(_) => {
                self.add_in_memory_module_bytecode(module)
            }
            PythonResourcesPolicy::FilesystemRelativeOnly(ref prefix) => {
                self.add_relative_path_module_bytecode(prefix, module)
            }
        }
    }

    /// Add resource data to the collection of embedded resource data.
    fn add_in_memory_package_resource(&mut self, resource: &PythonPackageResource) -> Result<()>;

    /// Add resource data to be loaded from the filesystem relative to the produced binary.
    fn add_relative_path_package_resource(
        &mut self,
        prefix: &str,
        resource: &PythonPackageResource,
    ) -> Result<()>;

    /// Add resource data to the collection of embedded resource data to a location as determined by the builder's resource policy.
    fn add_package_resource(&mut self, resource: &PythonPackageResource) -> Result<()> {
        match self.python_resources_policy().clone() {
            PythonResourcesPolicy::InMemoryOnly
            | PythonResourcesPolicy::PreferInMemoryFallbackFilesystemRelative(_) => {
                self.add_in_memory_package_resource(resource)
            }
            PythonResourcesPolicy::FilesystemRelativeOnly(ref prefix) => {
                self.add_relative_path_package_resource(prefix, resource)
            }
        }
    }

    /// Add a package distribution resource to be loaded from memory.
    fn add_in_memory_package_distribution_resource(
        &mut self,
        resource: &PythonPackageDistributionResource,
    ) -> Result<()>;

    /// Add a package distribution resource to be loaded from the filesystem relative to the produced binary.
    fn add_relative_path_package_distribution_resource(
        &mut self,
        prefix: &str,
        resource: &PythonPackageDistributionResource,
    ) -> Result<()>;

    /// Add a package distribution resource to a location determined by the builder's resource policy.
    fn add_package_distribution_resource(
        &mut self,
        resource: &PythonPackageDistributionResource,
    ) -> Result<()> {
        match self.python_resources_policy().clone() {
            PythonResourcesPolicy::InMemoryOnly
            | PythonResourcesPolicy::PreferInMemoryFallbackFilesystemRelative(_) => {
                self.add_in_memory_package_distribution_resource(resource)
            }
            PythonResourcesPolicy::FilesystemRelativeOnly(ref prefix) => {
                self.add_relative_path_package_distribution_resource(prefix, resource)
            }
        }
    }

    /// Add an extension module from a Python distribution to be linked into `libpython`.
    ///
    /// The extension module will be available for import using Python's special
    /// _builtin_ importer.
    fn add_builtin_distribution_extension_module(
        &mut self,
        extension_module: &DistributionExtensionModule,
    ) -> Result<()>;

    /// Add an extension module from a Python distribution to be loaded from memory.
    fn add_in_memory_distribution_extension_module(
        &mut self,
        extension_module: &DistributionExtensionModule,
    ) -> Result<()>;

    /// Add an extension module from a Python distribution to be loaded from a relative filesystem path.
    fn add_relative_path_distribution_extension_module(
        &mut self,
        prefix: &str,
        extension_module: &DistributionExtensionModule,
    ) -> Result<()>;

    /// Add an extension module from a Python distribution to be imported via whatever means the policy allows.
    fn add_distribution_extension_module(
        &mut self,
        extension_module: &DistributionExtensionModule,
    ) -> Result<()>;

    /// Add an extension module as defined by a dynamic library to be loaded from memory.
    fn add_in_memory_dynamic_extension_module(
        &mut self,
        extension_module: &PythonExtensionModule,
    ) -> Result<()>;

    /// Add an extension module as defined by a dynamic library to be loaded from a relative filesystem path.
    fn add_relative_path_dynamic_extension_module(
        &mut self,
        prefix: &str,
        extension_module: &PythonExtensionModule,
    ) -> Result<()>;

    /// Add an extension module as defined by a dynamic library.
    ///
    /// The extension module will be made available for import by the
    /// `PythonResourcesPolicy` attached to this builder. The extension module
    /// will either by loaded from memory or will be manifested as a file next
    /// to the produced binary installed in the policy's path prefix.
    fn add_dynamic_extension_module(
        &mut self,
        extension_module: &PythonExtensionModule,
    ) -> Result<()>;

    /// Add an extension module to be statically linked into the binary.
    fn add_static_extension_module(
        &mut self,
        extension_module_data: &PythonExtensionModule,
    ) -> Result<()>;

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
        opt_level: &str,
    ) -> Result<EmbeddedPythonBinaryData>;
}

/// Describes how to link a binary against Python.
pub struct PythonLinkingInfo {
    /// Path to a `pythonXY` library to link against.
    pub libpythonxy_filename: PathBuf,

    /// The contents of `libpythonxy_filename`.
    pub libpythonxy_data: Vec<u8>,

    /// Path to an existing `libpython` to link against. If present, this is
    /// the actual library containing Python symbols and `libpythonXY` is
    /// a placeholder.
    pub libpython_filename: Option<PathBuf>,

    /// Path to a library containing an alternate `config.c`.
    pub libpyembeddedconfig_filename: Option<PathBuf>,

    /// The contents of `libpyembeddedconfig_filename`.
    pub libpyembeddedconfig_data: Option<Vec<u8>>,

    /// Lines that need to be emitted from a Cargo build script.
    pub cargo_metadata: Vec<String>,
}

/// Represents serialized data embedded in binaries for loading Python resources.
pub struct EmbeddedResourcesBlobs {
    pub module_names: Vec<u8>,
    pub resources: Vec<u8>,
}

impl<'a> TryFrom<EmbeddedPythonResources<'a>> for EmbeddedResourcesBlobs {
    type Error = anyhow::Error;

    fn try_from(value: EmbeddedPythonResources) -> Result<Self, Self::Error> {
        let mut module_names = Vec::new();
        let mut resources = Vec::new();

        value.write_blobs(&mut module_names, &mut resources)?;

        Ok(Self {
            module_names,
            resources,
        })
    }
}

/// Holds filesystem paths to resources required to build a binary embedding Python.
pub struct EmbeddedPythonBinaryPaths {
    /// File containing a list of module names.
    pub module_names: PathBuf,

    /// File containing embedded resources data.
    pub embedded_resources: PathBuf,

    /// Path to library containing libpython.
    pub libpython: PathBuf,

    /// Path to a library containing an alternate compiled config.c file.
    pub libpyembeddedconfig: Option<PathBuf>,

    /// Path to `config.rs` derived from a `EmbeddedPythonConfig`.
    pub config_rs: PathBuf,

    /// Path to a file containing lines needed to be emitted by a Cargo build script.
    pub cargo_metadata: PathBuf,
}

/// Represents resources to embed Python in a binary.
pub struct EmbeddedPythonBinaryData {
    /// The configuration for the embedded interpreter.
    pub config: EmbeddedPythonConfig,

    /// Information on how to link against Python.
    pub linking_info: PythonLinkingInfo,

    /// Python resources to embed in the binary.
    pub resources: EmbeddedResourcesBlobs,

    /// Extra files to install next to produced binary.
    pub extra_files: FileManifest,

    /// Rust target triple for the host we are running on.
    pub host: String,

    /// Rust target triple for the target we are building for.
    pub target: String,
}

impl EmbeddedPythonBinaryData {
    /// Write out files needed to link a binary.
    pub fn write_files(&self, dest_dir: &Path) -> Result<EmbeddedPythonBinaryPaths> {
        let module_names = dest_dir.join("py-module-names");
        let mut fh = File::create(&module_names)?;
        fh.write_all(&self.resources.module_names)?;

        let embedded_resources = dest_dir.join("packed-resources");
        let mut fh = File::create(&embedded_resources)?;
        fh.write_all(&self.resources.resources)?;

        let libpython = dest_dir.join(&self.linking_info.libpythonxy_filename);
        let mut fh = File::create(&libpython)?;
        fh.write_all(&self.linking_info.libpythonxy_data)?;

        let libpyembeddedconfig = if let Some(data) = &self.linking_info.libpyembeddedconfig_data {
            let path = dest_dir.join(
                self.linking_info
                    .libpyembeddedconfig_filename
                    .as_ref()
                    .unwrap(),
            );
            let mut fh = File::create(&path)?;
            fh.write_all(data)?;
            Some(path)
        } else {
            None
        };

        let config_rs_data = derive_python_config(&self.config, &embedded_resources);
        let config_rs = dest_dir.join("default_python_config.rs");
        write_default_python_config_rs(&config_rs, &config_rs_data)?;

        let mut cargo_metadata_lines = Vec::new();
        cargo_metadata_lines.extend(self.linking_info.cargo_metadata.clone());

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
            module_names,
            embedded_resources,
            libpython,
            libpyembeddedconfig,
            config_rs,
            cargo_metadata,
        })
    }
}
