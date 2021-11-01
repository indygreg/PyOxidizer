// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Defining and manipulating binaries embedding Python.
*/

use {
    crate::{
        environment::Environment,
        py_packaging::{config::PyembedPythonInterpreterConfig, distribution::AppleSdkInfo},
    },
    anyhow::{anyhow, Context, Result},
    pyo3_build_config::{
        BuildFlags, InterpreterConfig as PyO3InterpreterConfig, PythonImplementation, PythonVersion,
    },
    python_packaging::{
        policy::PythonPackagingPolicy,
        resource::{
            PythonExtensionModule, PythonModuleSource, PythonPackageDistributionResource,
            PythonPackageResource, PythonResource,
        },
        resource_collection::{
            AddResourceAction, CompiledResourcesCollection, PrePackagedResource,
            PythonResourceAddCollectionContext,
        },
    },
    std::{
        collections::HashMap,
        path::{Path, PathBuf},
        sync::Arc,
    },
    tugger_file_manifest::{File, FileManifest},
    tugger_windows::VcRedistributablePlatform,
};

include!("../pyembed-license.rs");

/// How a binary should link against libpython.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LibpythonLinkMode {
    /// Libpython will be statically linked into the binary.
    Static,
    /// The binary will dynamically link against libpython.
    Dynamic,
}

/// Determines how packed resources are loaded by the generated binary.
///
/// This effectively controls how resources file are written to disk
/// and what `pyembed::PackedResourcesSource` will get serialized in the
/// configuration.
#[derive(Clone, Debug, PartialEq)]
pub enum PackedResourcesLoadMode {
    /// Packed resources will not be loaded.
    None,

    /// Resources data will be embedded in the binary.
    ///
    /// The data will be referenced via an `include_bytes!()` and the
    /// stored path controls the name of the file that will be materialized
    /// in the artifacts directory.
    EmbeddedInBinary(String),

    /// Resources data will be serialized to a file relative to the built binary.
    ///
    /// The configuration will reference the file via a relative path using
    /// `$ORIGIN` expansion. Memory mapped I/O will be used to read the file.
    BinaryRelativePathMemoryMapped(String),
}

impl ToString for PackedResourcesLoadMode {
    fn to_string(&self) -> String {
        match self {
            Self::None => "none".to_string(),
            Self::EmbeddedInBinary(filename) => format!("embedded:{}", filename),
            Self::BinaryRelativePathMemoryMapped(path) => {
                format!("binary-relative-memory-mapped:{}", path)
            }
        }
    }
}

impl TryFrom<&str> for PackedResourcesLoadMode {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        if value == "none" {
            Ok(Self::None)
        } else {
            let parts = value.splitn(2, ':').collect::<Vec<_>>();
            if parts.len() != 2 {
                Err(
                    "resources load mode value not recognized; must have form `type:value`"
                        .to_string(),
                )
            } else {
                let prefix = parts[0];
                let value = parts[1];

                match prefix {
                    "embedded" => {
                        Ok(Self::EmbeddedInBinary(value.to_string()))
                    }
                    "binary-relative-memory-mapped" => {
                        Ok(Self::BinaryRelativePathMemoryMapped(value.to_string()))
                    }
                    _ => Err(format!("{} is not a valid prefix; must be 'embedded' or 'binary-relative-memory-mapped'", prefix))
                }
            }
        }
    }
}

/// Describes how Windows Runtime DLLs (e.g. vcruntime140.dll) should be handled during builds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WindowsRuntimeDllsMode {
    /// Never attempt to install Windows Runtime DLLs.
    ///
    /// A binary will be generated with no runtime DLLs next to it.
    Never,

    /// Install Windows Runtime DLLs if they can be located. Do nothing if not.
    WhenPresent,

    /// Always install Windows Runtime DLLs and fail if they can't be found.
    Always,
}

impl ToString for WindowsRuntimeDllsMode {
    fn to_string(&self) -> String {
        match self {
            Self::Never => "never",
            Self::WhenPresent => "when-present",
            Self::Always => "always",
        }
        .to_string()
    }
}

impl TryFrom<&str> for WindowsRuntimeDllsMode {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "never" => Ok(Self::Never),
            "when-present" => Ok(Self::WhenPresent),
            "always" => Ok(Self::Always),
            _ => Err(format!("{} is not a valid mode; must be 'never'", value)),
        }
    }
}

/// Describes extra behavior for a linker invocation.
#[derive(Clone, Debug, PartialEq)]
pub enum LinkingAnnotation {
    /// Link an Apple framework library of the given name.
    LinkFramework(String),

    /// Link a library of the given name.
    LinkLibrary(String),

    /// Link a static library of the given name.
    LinkLibraryStatic(String),

    /// A search path for libraries.
    Search(PathBuf),

    /// A search path for native libraries.
    SearchNative(PathBuf),
}

impl LinkingAnnotation {
    /// Convert the instance to a `cargo:*` string representing this annotation.
    pub fn to_cargo_annotation(&self) -> String {
        match self {
            Self::LinkFramework(framework) => {
                format!("cargo:rustc-link-lib=framework={}", framework)
            }
            Self::LinkLibrary(lib) => format!("cargo:rustc-link-lib={}", lib),
            Self::LinkLibraryStatic(lib) => format!("cargo:rustc-link-lib=static={}", lib),
            Self::Search(path) => format!("cargo:rustc-link-search={}", path.display()),
            Self::SearchNative(path) => {
                format!("cargo:rustc-link-search=native={}", path.display())
            }
        }
    }
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
    fn vc_runtime_requirements(&self) -> Option<(String, VcRedistributablePlatform)>;

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

    /// Apple SDK build/targeting information.
    fn apple_sdk_info(&self) -> Option<&AppleSdkInfo>;

    /// Obtain how Windows runtime DLLs will be handled during builds.
    ///
    /// See the enum's documentation for behavior.
    ///
    /// This setting is ignored for binaries that don't need the Windows runtime
    /// DLLs.
    fn windows_runtime_dlls_mode(&self) -> &WindowsRuntimeDllsMode;

    /// Set the value for `windows_runtime_dlls_mode()`.
    fn set_windows_runtime_dlls_mode(&mut self, value: WindowsRuntimeDllsMode);

    /// The directory to install tcl/tk files into.
    fn tcl_files_path(&self) -> &Option<String>;

    /// Set the directory to install tcl/tk files into.
    fn set_tcl_files_path(&mut self, value: Option<String>);

    /// The value of the `windows_subsystem` Rust attribute for the generated Rust project.
    fn windows_subsystem(&self) -> &str;

    /// Set the value of the `windows_subsystem` Rust attribute for generated Rust projects.
    fn set_windows_subsystem(&mut self, value: &str) -> Result<()>;

    /// How packed Python resources will be loaded by the binary.
    fn packed_resources_load_mode(&self) -> &PackedResourcesLoadMode;

    /// Set how packed Python resources will be loaded by the binary.
    fn set_packed_resources_load_mode(&mut self, load_mode: PackedResourcesLoadMode);

    /// Obtain an iterator over all resource entries that will be embedded in the binary.
    ///
    /// This likely does not return extension modules that are statically linked
    /// into the binary. For those, see `builtin_extension_module_names()`.
    fn iter_resources<'a>(
        &'a self,
    ) -> Box<dyn Iterator<Item = (&'a String, &'a PrePackagedResource)> + 'a>;

    /// Resolve license metadata from an iterable of `PythonResource` and store that data.
    ///
    /// The resolved license data can later be used to ensure packages conform
    /// to license restrictions. This method can safely be called on resources
    /// that aren't added to the instance / resource collector: it simply
    /// registers the license metadata so it can be consulted later.
    fn index_package_license_info_from_resources<'a>(
        &mut self,
        resources: &[PythonResource<'a>],
    ) -> Result<()>;

    /// Runs `pip download` using the binary builder's settings.
    ///
    /// Returns resources discovered from the Python packages downloaded.
    fn pip_download(
        &mut self,
        logger: &slog::Logger,
        verbose: bool,
        args: &[String],
    ) -> Result<Vec<PythonResource>>;

    /// Runs `pip install` using the binary builder's settings.
    ///
    /// Returns resources discovered as part of performing an install.
    fn pip_install(
        &mut self,
        logger: &slog::Logger,
        verbose: bool,
        install_args: &[String],
        extra_envs: &HashMap<String, String>,
    ) -> Result<Vec<PythonResource>>;

    /// Reads Python resources from the filesystem.
    fn read_package_root(
        &mut self,
        logger: &slog::Logger,
        path: &Path,
        packages: &[String],
    ) -> Result<Vec<PythonResource>>;

    /// Read Python resources from a populated virtualenv directory.
    fn read_virtualenv(
        &mut self,
        logger: &slog::Logger,
        path: &Path,
    ) -> Result<Vec<PythonResource>>;

    /// Runs `python setup.py install` using the binary builder's settings.
    ///
    /// Returns resources discovered as part of performing an install.
    fn setup_py_install(
        &mut self,
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
    ) -> Result<Vec<AddResourceAction>>;

    /// Add a `PythonModuleSource` to the resources collection.
    ///
    /// The location to load the resource from is optional. If specified, it
    /// will be used. If not, an appropriate location based on the resources
    /// policy will be chosen.
    fn add_python_module_source(
        &mut self,
        module: &PythonModuleSource,
        add_context: Option<PythonResourceAddCollectionContext>,
    ) -> Result<Vec<AddResourceAction>>;

    /// Add a `PythonPackageResource` to the resources collection.
    ///
    /// The location to load the resource from is optional. If specified, it will
    /// be used. If not, an appropriate location based on the resources policy
    /// will be chosen.
    fn add_python_package_resource(
        &mut self,
        resource: &PythonPackageResource,
        add_context: Option<PythonResourceAddCollectionContext>,
    ) -> Result<Vec<AddResourceAction>>;

    /// Add a `PythonPackageDistributionResource` to the resources collection.
    ///
    /// The location to load the resource from is optional. If specified, it will
    /// be used. If not, an appropriate location based on the resources policy
    /// will be chosen.
    fn add_python_package_distribution_resource(
        &mut self,
        resource: &PythonPackageDistributionResource,
        add_context: Option<PythonResourceAddCollectionContext>,
    ) -> Result<Vec<AddResourceAction>>;

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
    ) -> Result<Vec<AddResourceAction>>;

    /// Add a `File` to the resource collection.
    fn add_file_data(
        &mut self,
        file: &File,
        add_context: Option<PythonResourceAddCollectionContext>,
    ) -> Result<Vec<AddResourceAction>>;

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

    /// Whether the binary requires the Mimalloc library.
    fn requires_mimalloc(&self) -> bool;

    /// Whether the binary requires the Snmalloc library.
    fn requires_snmalloc(&self) -> bool;

    /// Obtain an `EmbeddedPythonContext` instance from this one.
    fn to_embedded_python_context(
        &self,
        logger: &slog::Logger,
        env: &Environment,
        opt_level: &str,
    ) -> Result<EmbeddedPythonContext>;
}

/// Represents a linkable target defining a Python implementation.
pub trait LinkablePython {
    /// Write any files that need to exist to support linking.
    ///
    /// Files will be written to the directory specified.
    fn write_files(&self, dest_dir: &Path, target_triple: &str) -> Result<()>;

    /// Obtain linker annotations needed to link this libpython.
    ///
    /// `dest_dir` will be the directory where any files written by `write_files()` will
    /// be located.
    ///
    /// `alias` denotes whether to alias the library name to `pythonXY`.
    fn linking_annotations(&self, dest_dir: &Path, alias: bool) -> Result<Vec<LinkingAnnotation>>;
}

/// Link against a shared library on the filesystem.
#[derive(Clone, Debug)]
pub struct LinkSharedLibraryPath {
    /// Path to dynamic library to link.
    pub library_path: PathBuf,

    /// Additional linking annotations.
    pub linking_annotations: Vec<LinkingAnnotation>,
}

impl LinkSharedLibraryPath {
    /// Resolve the name of the library.
    fn library_name(&self) -> Result<String> {
        let filename = self
            .library_path
            .file_name()
            .ok_or_else(|| anyhow!("unable to resolve shared library file name"))?
            .to_string_lossy();

        if filename.ends_with(".dll") {
            Ok(filename.trim_end_matches(".dll").to_string())
        } else if filename.ends_with(".dylib") {
            Ok(filename
                .trim_end_matches(".dylib")
                .trim_start_matches("lib")
                .to_string())
        } else if filename.ends_with(".so") {
            Ok(filename
                .trim_end_matches(".so")
                .trim_start_matches("lib")
                .to_string())
        } else {
            Err(anyhow!(
                "unhandled libpython shared library filename: {}",
                filename
            ))
        }
    }
}

impl LinkablePython for LinkSharedLibraryPath {
    fn write_files(&self, _dest_dir: &Path, _target_triple: &str) -> Result<()> {
        Ok(())
    }

    fn linking_annotations(&self, _dest_dir: &Path, alias: bool) -> Result<Vec<LinkingAnnotation>> {
        let lib_dir = self
            .library_path
            .parent()
            .ok_or_else(|| anyhow!("could not derive parent directory of library path"))?;

        let mut annotations = vec![
            LinkingAnnotation::LinkLibrary(if alias {
                format!("pythonXY:{}", self.library_name()?)
            } else {
                self.library_name()?
            }),
            LinkingAnnotation::SearchNative(lib_dir.to_path_buf()),
        ];

        annotations.extend(self.linking_annotations.iter().cloned());

        Ok(annotations)
    }
}

/// Link against a custom built static library with tracked library data.
#[derive(Clone, Debug)]
pub struct LinkStaticLibraryData {
    /// libpython static library content.
    pub library_data: Vec<u8>,

    /// Additional linker directives to link this static library.
    pub linking_annotations: Vec<LinkingAnnotation>,
}

impl LinkStaticLibraryData {
    fn library_name(&self) -> &'static str {
        "python3"
    }

    fn library_path(&self, dest_dir: impl AsRef<Path>, target_triple: &str) -> PathBuf {
        dest_dir
            .as_ref()
            .join(if target_triple.contains("-windows-") {
                format!("{}.lib", self.library_name())
            } else {
                format!("lib{}.a", self.library_name())
            })
    }
}

impl LinkablePython for LinkStaticLibraryData {
    fn write_files(&self, dest_dir: &Path, target_triple: &str) -> Result<()> {
        let lib_path = self.library_path(dest_dir, target_triple);

        std::fs::write(&lib_path, &self.library_data)
            .with_context(|| format!("writing {}", lib_path.display()))?;

        Ok(())
    }

    fn linking_annotations(&self, dest_dir: &Path, alias: bool) -> Result<Vec<LinkingAnnotation>> {
        let mut annotations = vec![
            LinkingAnnotation::LinkLibraryStatic(if alias {
                format!("pythonXY:{}", self.library_name())
            } else {
                self.library_name().to_string()
            }),
            LinkingAnnotation::SearchNative(dest_dir.to_path_buf()),
        ];

        annotations.extend(self.linking_annotations.iter().cloned());

        Ok(annotations)
    }
}

/// Describes how to link a `libpython`.
pub enum LibpythonLinkSettings {
    /// Link against an existing shared library.
    ExistingDynamic(LinkSharedLibraryPath),
    /// Link against a custom static library.
    StaticData(LinkStaticLibraryData),
}

impl LinkablePython for LibpythonLinkSettings {
    fn write_files(&self, dest_dir: &Path, target_triple: &str) -> Result<()> {
        match self {
            Self::ExistingDynamic(l) => l.write_files(dest_dir, target_triple),
            Self::StaticData(l) => l.write_files(dest_dir, target_triple),
        }
    }

    fn linking_annotations(&self, dest_dir: &Path, alias: bool) -> Result<Vec<LinkingAnnotation>> {
        match self {
            Self::ExistingDynamic(l) => l.linking_annotations(dest_dir, alias),
            Self::StaticData(l) => l.linking_annotations(dest_dir, alias),
        }
    }
}

impl From<LinkSharedLibraryPath> for LibpythonLinkSettings {
    fn from(l: LinkSharedLibraryPath) -> Self {
        Self::ExistingDynamic(l)
    }
}

impl From<LinkStaticLibraryData> for LibpythonLinkSettings {
    fn from(l: LinkStaticLibraryData) -> Self {
        Self::StaticData(l)
    }
}

/// Filename of artifact containing the default PythonInterpreterConfig.
pub const DEFAULT_PYTHON_CONFIG_FILENAME: &str = "default_python_config.rs";

/// Holds context necessary to embed Python in a binary.
pub struct EmbeddedPythonContext<'a> {
    /// The configuration for the embedded interpreter.
    pub config: PyembedPythonInterpreterConfig,

    /// Information on how to link against Python.
    pub link_settings: LibpythonLinkSettings,

    /// Python resources that need to be serialized to a file.
    pub pending_resources: Vec<(CompiledResourcesCollection<'a>, PathBuf)>,

    /// Extra files to install next to produced binary.
    pub extra_files: FileManifest,

    /// Rust target triple for the host we are running on.
    pub host_triple: String,

    /// Rust target triple for the target we are building for.
    pub target_triple: String,

    /// Name of the Python implementation.
    pub python_implementation: PythonImplementation,

    /// Python interpreter version.
    pub python_version: PythonVersion,

    /// Path to a `python` executable that runs on the host/build machine.
    pub python_exe_host: PathBuf,

    /// Python build flags.
    ///
    /// To pass to PyO3.
    pub python_build_flags: BuildFlags,
}

impl<'a> EmbeddedPythonContext<'a> {
    /// Obtain the filesystem of the generated Rust source file containing the interpreter configuration.
    pub fn interpreter_config_rs_path(&self, dest_dir: impl AsRef<Path>) -> PathBuf {
        dest_dir.as_ref().join(DEFAULT_PYTHON_CONFIG_FILENAME)
    }

    /// Resolve the filesystem path to the PyO3 configuration file.
    pub fn pyo3_config_path(&self, dest_dir: impl AsRef<Path>) -> PathBuf {
        dest_dir.as_ref().join("pyo3-build-config-file.txt")
    }

    /// Resolve a [PyO3InterpreterConfig] for this instance.
    pub fn pyo3_interpreter_config(
        &self,
        dest_dir: impl AsRef<Path>,
    ) -> Result<PyO3InterpreterConfig> {
        Ok(PyO3InterpreterConfig {
            implementation: self.python_implementation,
            version: self.python_version,
            // Irrelevant since we control link settings below.
            shared: matches!(
                &self.link_settings,
                LibpythonLinkSettings::ExistingDynamic(_)
            ),
            // pyembed requires the full Python API.
            abi3: false,
            // We define linking info via explicit build script lines.
            lib_name: None,
            lib_dir: None,
            executable: Some(self.python_exe_host.to_string_lossy().to_string()),
            // TODO set from Python distribution metadata.
            pointer_width: Some(if self.target_triple.starts_with("i686-") {
                32
            } else {
                64
            }),
            build_flags: BuildFlags(self.python_build_flags.0.clone()),
            suppress_build_script_link_lines: true,
            extra_build_script_lines: self
                .link_settings
                .linking_annotations(dest_dir.as_ref(), self.target_triple.contains("-windows-"))?
                .iter()
                .map(|la| la.to_cargo_annotation())
                .collect::<Vec<_>>(),
        })
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
        self.link_settings
            .write_files(dest_dir.as_ref(), &self.target_triple)
    }

    /// Write the file containing the default interpreter configuration Rust struct.
    pub fn write_interpreter_config_rs(&self, dest_dir: impl AsRef<Path>) -> Result<()> {
        self.config
            .write_default_python_config_rs(self.interpreter_config_rs_path(&dest_dir))?;

        Ok(())
    }

    /// Write the PyO3 configuration file.
    pub fn write_pyo3_config(&self, dest_dir: impl AsRef<Path>) -> Result<()> {
        let dest_dir = dest_dir.as_ref();
        let dest_path = self.pyo3_config_path(dest_dir);

        // Serialize our contents to a vec first.
        let mut new_file_contents = vec![];
        self.pyo3_interpreter_config(dest_dir)?
            .to_writer(&mut new_file_contents)
            .map_err(|e| anyhow!("error serializing PyO3 config file: {}", e))?;

        if let Ok(existing_file_contents) = std::fs::read(&dest_path) {
            if new_file_contents == existing_file_contents {
                // Contents are identical. If we wrote the file out again, it would
                // cause various pyo3 crates to be recompiled, so we skip the write
                // and return early.
                return Ok(());
            }
        }

        // Write contents out.
        std::fs::write(dest_path, new_file_contents)
            .map_err(|e| anyhow!("error writing PyO3 config file: {}", e))?;

        Ok(())
    }

    /// Write out files needed to build a binary against our configuration.
    pub fn write_files(&self, dest_dir: &Path) -> Result<()> {
        self.write_packed_resources(&dest_dir)
            .context("write_packed_resources()")?;
        self.write_libpython(&dest_dir)
            .context("write_libpython()")?;
        self.write_interpreter_config_rs(&dest_dir)
            .context("write_interpreter_config_rs()")?;
        self.write_pyo3_config(&dest_dir)
            .context("write_pyo3_config()")?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resources_load_mode_serialization() {
        assert_eq!(
            PackedResourcesLoadMode::None.to_string(),
            "none".to_string()
        );
        assert_eq!(
            PackedResourcesLoadMode::EmbeddedInBinary("resources".into()).to_string(),
            "embedded:resources".to_string()
        );
        assert_eq!(
            PackedResourcesLoadMode::BinaryRelativePathMemoryMapped("relative-resources".into())
                .to_string(),
            "binary-relative-memory-mapped:relative-resources".to_string()
        );
    }

    #[test]
    fn test_resources_load_mode_parsing() -> Result<()> {
        assert_eq!(
            PackedResourcesLoadMode::try_from("none").unwrap(),
            PackedResourcesLoadMode::None
        );
        assert_eq!(
            PackedResourcesLoadMode::try_from("embedded:resources").unwrap(),
            PackedResourcesLoadMode::EmbeddedInBinary("resources".into())
        );
        assert_eq!(
            PackedResourcesLoadMode::try_from("binary-relative-memory-mapped:relative").unwrap(),
            PackedResourcesLoadMode::BinaryRelativePathMemoryMapped("relative".into())
        );

        Ok(())
    }

    #[test]
    fn test_dynamic_library_name() -> Result<()> {
        assert_eq!(
            LinkSharedLibraryPath {
                library_path: "libpython3.9.so".into(),
                linking_annotations: vec![],
            }
            .library_name()?,
            "python3.9"
        );

        assert_eq!(
            LinkSharedLibraryPath {
                library_path: "libpython3.9.dylib".into(),
                linking_annotations: vec![],
            }
            .library_name()?,
            "python3.9"
        );

        assert_eq!(
            LinkSharedLibraryPath {
                library_path: "python3.dll".into(),
                linking_annotations: vec![],
            }
            .library_name()?,
            "python3"
        );

        Ok(())
    }
}
