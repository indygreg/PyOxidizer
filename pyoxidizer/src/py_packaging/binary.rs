// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Defining and manipulating binaries embedding Python.
*/

use {
    super::config::PyembedPythonInterpreterConfig,
    anyhow::{anyhow, Context, Result},
    python_packaging::{
        policy::PythonPackagingPolicy,
        resource::{
            PythonExtensionModule, PythonModuleSource, PythonPackageDistributionResource,
            PythonPackageResource, PythonResource,
        },
        resource_collection::{
            CompiledResourcesCollection, PrePackagedResource, PythonResourceAddCollectionContext,
        },
    },
    std::{
        collections::HashMap,
        io::Write,
        path::{Path, PathBuf},
        sync::Arc,
    },
    tugger_file_manifest::{File, FileManifest},
    tugger_windows::VCRedistributablePlatform,
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

    /// Rust target triple the binary will run on.
    fn target_triple(&self) -> &str;

    /// Obtain run-time requirements for the Visual C++ Redistributable.
    ///
    /// If `None`, there is no dependency on `vcruntimeXXX.dll` files. If `Some`,
    /// the returned tuple declares the VC++ Redistributable major version string
    /// (e.g. `14`) and the VC++ Redistributable platform variant that is required.
    fn vc_runtime_requirements(&self) -> Option<(String, VCRedistributablePlatform)>;

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

    /// Add a `File` to the resource collection.
    fn add_file_data(
        &mut self,
        file: &File,
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

/// Holds context necessary to embed Python in a binary.
pub struct EmbeddedPythonContext<'a> {
    /// The configuration for the embedded interpreter.
    pub config: PyembedPythonInterpreterConfig,

    /// Information on how to link against Python.
    pub linking_info: PythonLinkingInfo,

    /// Python resources that need to be serialized to a file.
    pub pending_resources: Vec<(CompiledResourcesCollection<'a>, PathBuf)>,

    /// Extra files to install next to produced binary.
    pub extra_files: FileManifest,

    /// Rust target triple for the host we are running on.
    pub host_triple: String,

    /// Rust target triple for the target we are building for.
    pub target_triple: String,
}

impl<'a> EmbeddedPythonContext<'a> {
    /// Obtain the filesystem of the generated Rust source file containing the interpreter configuration.
    pub fn interpreter_config_rs_path(&self, dest_dir: impl AsRef<Path>) -> PathBuf {
        dest_dir.as_ref().join("default_python_config.rs")
    }

    /// Obtain path to a compiled library containing content of a compiled `config.c` file.
    ///
    /// This file contains global data structures for libpython defining extension modules.
    pub fn python_config_library_path(&self, dest_dir: impl AsRef<Path>) -> Option<PathBuf> {
        if let Some(filename) = &self.linking_info.libpyembeddedconfig_filename {
            Some(dest_dir.as_ref().join(filename))
        } else {
            None
        }
    }

    /// Resolve path to library containing libpython.
    pub fn libpython_path(&self, dest_dir: impl AsRef<Path>) -> PathBuf {
        dest_dir
            .as_ref()
            .join(&self.linking_info.libpythonxy_filename)
    }

    /// Resolve the filesystem path to the file containing cargo: lines.
    ///
    /// The `cargo:` lines will enabling linking with the appropriate libpython.
    pub fn cargo_metadata_path(&self, dest_dir: impl AsRef<Path>) -> PathBuf {
        dest_dir.as_ref().join("cargo_metadata.txt")
    }

    /// Obtain lines constituting cargo metadata.
    ///
    /// These should be printed from a build script. The printed lines enable
    /// linking with our libpython.
    pub fn cargo_metadata_lines(&self, dest_dir: impl AsRef<Path>) -> Vec<String> {
        let mut lines = self.linking_info.cargo_metadata.clone();

        // Tell Cargo where libpythonXY is located.
        lines.push(format!(
            "cargo:rustc-link-search=native={}",
            dest_dir.as_ref().display()
        ));

        // Give dependent crates the path to the default config file.
        lines.push(format!(
            "cargo:default-python-config-rs={}",
            self.interpreter_config_rs_path(dest_dir).display()
        ));

        lines
    }

    /// Ensure packed resources files are written.
    pub fn write_packed_resources(&self, dest_dir: impl AsRef<Path>) -> Result<()> {
        for (collection, path) in &self.pending_resources {
            let dest_path = dest_dir.as_ref().join(path);

            let mut writer = std::io::BufWriter::new(
                std::fs::File::create(&dest_path)
                    .with_context(|| format!("opening {} for writing", dest_path.display()))?,
            );
            collection
                .write_packed_resources(&mut writer)
                .context("writing packed resources")?;
        }

        Ok(())
    }

    /// Ensure files required by libpython are written.
    pub fn write_libpython(&self, dest_dir: impl AsRef<Path>) -> Result<()> {
        let mut fh = std::fs::File::create(self.libpython_path(&dest_dir))?;
        fh.write_all(&self.linking_info.libpythonxy_data)?;

        if let Some(data) = &self.linking_info.libpyembeddedconfig_data {
            let path = self.python_config_library_path(&dest_dir).ok_or_else(|| {
                anyhow!("embedded Python config library data defined without path")
            })?;
            let mut fh = std::fs::File::create(&path)?;
            fh.write_all(data)?;
        }

        Ok(())
    }

    /// Write the file containing the default interpreter configuration Rust struct.
    pub fn write_interpreter_config_rs(&self, dest_dir: impl AsRef<Path>) -> Result<()> {
        self.config
            .write_default_python_config_rs(self.interpreter_config_rs_path(&dest_dir))?;

        Ok(())
    }

    /// Write file containing cargo metadata lines.
    pub fn write_cargo_metadata(&self, dest_dir: impl AsRef<Path>) -> Result<()> {
        let mut fh = std::fs::File::create(self.cargo_metadata_path(&dest_dir))?;
        fh.write_all(self.cargo_metadata_lines(dest_dir).join("\n").as_bytes())?;

        Ok(())
    }

    /// Write out files needed to build a binary against our configuration.
    pub fn write_files(&self, dest_dir: &Path) -> Result<()> {
        self.write_packed_resources(&dest_dir)
            .context("writing packed resources")?;
        self.write_libpython(&dest_dir)
            .context("writing libpython")?;
        self.write_interpreter_config_rs(&dest_dir)
            .context("writing interpreter config Rust source")?;
        self.write_cargo_metadata(&dest_dir)
            .context("writing cargo metadata file")?;

        Ok(())
    }
}
