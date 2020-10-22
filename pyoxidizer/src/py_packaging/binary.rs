// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Defining and manipulating binaries embedding Python.
*/

use {
    super::config::EmbeddedPythonConfig,
    crate::app_packaging::resource::FileManifest,
    anyhow::Result,
    python_packaging::{
        policy::PythonPackagingPolicy,
        resource::{
            FileData, PythonExtensionModule, PythonModuleSource, PythonPackageDistributionResource,
            PythonPackageResource, PythonResource,
        },
        resource_collection::{PrePackagedResource, PythonResourceAddCollectionContext},
    },
    std::{
        collections::HashMap,
        fs::File,
        io::Write,
        path::{Path, PathBuf},
        sync::Arc,
    },
};

/// How a binary should link against libpython.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LibpythonLinkMode {
    /// Libpython will be statically linked into the binary.
    Static,
    /// The binary will dynamically link against libpython.
    Dynamic,
}

/// A callable that can influence PythonResourceAddCollectionContext.
pub type ResourceAddCollectionContextCallback<'a> = Box<
    dyn Fn(
            &PythonPackagingPolicy,
            &PythonResource,
            &mut PythonResourceAddCollectionContext,
        ) -> Result<()>
        + 'a,
>;

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
    fn clone_trait(&self) -> Arc<dyn PythonBinaryBuilder>;

    /// The name of the binary.
    fn name(&self) -> String;

    /// How the binary will link against libpython.
    fn libpython_link_mode(&self) -> LibpythonLinkMode;

    /// Obtain the cache tag to apply to Python bytecode modules.
    fn cache_tag(&self) -> &str;

    /// Obtain the `PythonPackagingPolicy` for the builder.
    fn python_packaging_policy(&self) -> &PythonPackagingPolicy;

    /// Path to Python executable that can be used to derive info at build time.
    ///
    /// The produced binary is effectively a clone of the Python distribution behind the
    /// returned executable.
    fn host_python_exe_path(&self) -> &Path;

    /// Path to Python executable that is native to the target architecture.
    // TODO this should not need to exist if we properly supported cross-compiling.
    fn target_python_exe_path(&self) -> &Path;

    /// The directory to install tcl/tk files into.
    fn tcl_files_path(&self) -> &Option<String>;

    /// Set the directory to install tcl/tk files into.
    fn set_tcl_files_path(&mut self, value: Option<String>);

    /// The value of the `windows_subsystem` Rust attribute for the generated Rust project.
    fn windows_subsystem(&self) -> &str;

    /// Set the value of the `windows_subsystem` Rust attribute for generated Rust projects.
    fn set_windows_subsystem(&mut self, value: &str) -> Result<()>;

    /// Obtain an iterator over all resource entries that will be embedded in the binary.
    ///
    /// This likely does not return extension modules that are statically linked
    /// into the binary. For those, see `builtin_extension_module_names()`.
    fn iter_resources<'a>(
        &'a self,
    ) -> Box<dyn Iterator<Item = (&'a String, &'a PrePackagedResource)> + 'a>;

    /// Runs `pip download` using the binary builder's settings.
    ///
    /// Returns resources discovered from the Python packages downloaded.
    fn pip_download(
        &self,
        logger: &slog::Logger,
        verbose: bool,
        args: &[String],
    ) -> Result<Vec<PythonResource>>;

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

    /// Add resources from the Python distribution to the builder.
    ///
    /// This method should likely be called soon after object construction
    /// in order to finish adding state from the Python distribution to the
    /// builder.
    ///
    /// The boundary between what distribution state should be initialized
    /// at binary construction time versus this method is not well-defined
    /// and is up to implementations. However, it is strongly recommended for
    /// the division to be handling of core/required interpreter state at
    /// construction time and all optional/standard library state in this
    /// method.
    ///
    /// `callback` defines an optional function which can be called between
    /// resource creation and adding that resource to the builder. This
    /// gives the caller an opportunity to influence how resources are added
    /// to the binary builder.
    fn add_distribution_resources(
        &mut self,
        callback: Option<ResourceAddCollectionContextCallback>,
    ) -> Result<()>;

    /// Add a `PythonModuleSource` to the resources collection.
    ///
    /// The location to load the resource from is optional. If specified, it
    /// will be used. If not, an appropriate location based on the resources
    /// policy will be chosen.
    fn add_python_module_source(
        &mut self,
        module: &PythonModuleSource,
        add_context: Option<PythonResourceAddCollectionContext>,
    ) -> Result<()>;

    /// Add a `PythonPackageResource` to the resources collection.
    ///
    /// The location to load the resource from is optional. If specified, it will
    /// be used. If not, an appropriate location based on the resources policy
    /// will be chosen.
    fn add_python_package_resource(
        &mut self,
        resource: &PythonPackageResource,
        add_context: Option<PythonResourceAddCollectionContext>,
    ) -> Result<()>;

    /// Add a `PythonPackageDistributionResource` to the resources collection.
    ///
    /// The location to load the resource from is optional. If specified, it will
    /// be used. If not, an appropriate location based on the resources policy
    /// will be chosen.
    fn add_python_package_distribution_resource(
        &mut self,
        resource: &PythonPackageDistributionResource,
        add_context: Option<PythonResourceAddCollectionContext>,
    ) -> Result<()>;

    /// Add a `PythonExtensionModule` to make available.
    ///
    /// The location to load the extension module from can be specified. However,
    /// different builders have different capabilities. And the location may be
    /// ignored in some cases. For example, when adding an extension module that
    /// is compiled into libpython itself, the location will always be inside
    /// libpython and it isn't possible to materialize the extension module as
    /// a standalone file.
    fn add_python_extension_module(
        &mut self,
        extension_module: &PythonExtensionModule,
        add_context: Option<PythonResourceAddCollectionContext>,
    ) -> Result<()>;

    /// Add a `FileData` to the resource collection.
    fn add_file_data(
        &mut self,
        file: &FileData,
        add_context: Option<PythonResourceAddCollectionContext>,
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

    /// Obtain an `EmbeddedPythonContext` instance from this one.
    fn to_embedded_python_context(
        &self,
        logger: &slog::Logger,
        opt_level: &str,
    ) -> Result<EmbeddedPythonContext>;
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

/// Holds filesystem paths to resources required to build a binary embedding Python.
pub struct EmbeddedPythonPaths {
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

/// Holds context necessary to embed Python in a binary.
pub struct EmbeddedPythonContext {
    /// The configuration for the embedded interpreter.
    pub config: EmbeddedPythonConfig,

    /// Information on how to link against Python.
    pub linking_info: PythonLinkingInfo,

    /// Newline delimited list of module names in resources.
    pub module_names: Vec<u8>,

    /// Python resources to embed in the binary.
    pub resources: Vec<u8>,

    /// Extra files to install next to produced binary.
    pub extra_files: FileManifest,

    /// Rust target triple for the host we are running on.
    pub host_triple: String,

    /// Rust target triple for the target we are building for.
    pub target_triple: String,
}

impl EmbeddedPythonContext {
    /// Write out files needed to link a binary.
    pub fn write_files(&self, dest_dir: &Path) -> Result<EmbeddedPythonPaths> {
        let module_names = dest_dir.join("py-module-names");
        let mut fh = File::create(&module_names)?;
        fh.write_all(&self.module_names)?;

        let embedded_resources = dest_dir.join("packed-resources");
        let mut fh = File::create(&embedded_resources)?;
        fh.write_all(&self.resources)?;

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

        let config_rs = dest_dir.join("default_python_config.rs");
        self.config
            .write_default_python_confis_rs(&config_rs, Some(&embedded_resources))?;

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

        Ok(EmbeddedPythonPaths {
            module_names,
            embedded_resources,
            libpython,
            libpyembeddedconfig,
            config_rs,
            cargo_metadata,
        })
    }
}
