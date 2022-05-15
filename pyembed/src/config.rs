// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Data structures for configuring a Python interpreter.

use {
    crate::NewInterpreterError,
    oxidized_importer::{PackedResourcesSource, PythonResourcesState},
    pyo3::ffi as pyffi,
    python_packaging::interpreter::{
        MemoryAllocatorBackend, MultiprocessingStartMethod, PythonInterpreterConfig,
        PythonInterpreterProfile, TerminfoResolution,
    },
    std::{
        ffi::{CString, OsString},
        ops::Deref,
        path::PathBuf,
    },
};

#[cfg(feature = "serialization")]
use serde::{Deserialize, Serialize};

/// Defines a Python extension module and its initialization function.
///
/// Essentially represents a module name and pointer to its initialization
/// function.
#[derive(Clone, Debug)]
pub struct ExtensionModule {
    /// Name of the extension module.
    pub name: CString,

    /// Extension module initialization function.
    pub init_func: unsafe extern "C" fn() -> *mut pyffi::PyObject,
}

/// Configuration for a Python interpreter.
///
/// This type is used to create a [crate::MainPythonInterpreter], which manages
/// a Python interpreter running in the current process.
///
/// This type wraps a [PythonInterpreterConfig], which is an abstraction over
/// the low-level C structs (`PyPreConfig` and `PyConfig`) used as part of
/// Python's C initialization API. In addition to this data structure, the
/// fields on this type facilitate control of additional features provided by
/// this crate.
///
/// The [PythonInterpreterConfig] has a single non-optional field:
/// [PythonInterpreterConfig::profile]. This defines the defaults for various
/// fields of the `PyPreConfig` and `PyConfig` C structs. See
/// <https://docs.python.org/3/c-api/init_config.html#isolated-configuration> for
/// more.
///
/// When this type is converted to `PyPreConfig` and `PyConfig`, instances
/// of these C structs are created from the specified profile. e.g. by calling
/// `PyPreConfig_InitPythonConfig()`, `PyPreConfig_InitIsolatedConfig`,
/// `PyConfig_InitPythonConfig`, and `PyConfig_InitIsolatedConfig`. Then
/// for each field in `PyPreConfig` and `PyConfig`, if a corresponding field
/// on [PythonInterpreterConfig] is [Some], then the `PyPreConfig` or
/// `PyConfig` field will be updated accordingly.
///
/// During interpreter initialization, [Self::resolve()] is called to
/// resolve/finalize any missing values and convert the instance into a
/// [ResolvedOxidizedPythonInterpreterConfig]. It is this type that is
/// used to produce a `PyPreConfig` and `PyConfig`, which are used to
/// initialize the Python interpreter.
///
/// Some fields on this type are redundant or conflict with those on
/// [PythonInterpreterConfig]. Read the documentation of each field to
/// understand how they interact. Since [PythonInterpreterConfig] is defined
/// in a different crate, its docs are not aware of the existence of
/// this crate/type.
///
/// This struct implements `Deserialize` and `Serialize` and therefore can be
/// serialized to any format supported by the `serde` crate. This feature is
/// used by `pyoxy` to allow YAML-based configuration of Python interpreters.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serialization", derive(Deserialize, Serialize))]
#[cfg_attr(feature = "serialization", serde(default))]
pub struct OxidizedPythonInterpreterConfig<'a> {
    /// The path of the currently executing executable.
    ///
    /// This value will always be [Some] on [ResolvedOxidizedPythonInterpreterConfig]
    /// instances.
    ///
    /// Default value: [None].
    ///
    /// [Self::resolve()] behavior: sets to [std::env::current_exe()] if not set.
    /// Will canonicalize the final path, which may entail filesystem I/O.
    pub exe: Option<PathBuf>,

    /// The filesystem path from which relative paths will be interpreted.
    ///
    /// This value will always be [Some] on [ResolvedOxidizedPythonInterpreterConfig]
    /// instances.
    ///
    /// Default value: [None].
    ///
    /// [Self::resolve()] behavior: sets to [Self::exe.parent()] if not set.
    pub origin: Option<PathBuf>,

    /// Low-level configuration of Python interpreter.
    ///
    /// Default value: [PythonInterpreterConfig::default()] with
    /// [PythonInterpreterConfig::profile] always set to [PythonInterpreterProfile::Python].
    ///
    /// [Self::resolve()] behavior: most fields are copied verbatim.
    /// [PythonInterpreterConfig::module_search_paths] entries have the special token
    /// `$ORIGIN` expanded to the resolved value of [Self::origin].
    pub interpreter_config: PythonInterpreterConfig,

    /// Memory allocator backend to use.
    ///
    /// Default value: [MemoryAllocatorBackend::Default].
    ///
    /// Interpreter initialization behavior: after `Py_PreInitialize()` is called,
    /// [crate::pyalloc::PythonMemoryAllocator::from_backend()] is called. If this
    /// resolves to a [crate::pyalloc::PythonMemoryAllocator], that allocator will
    /// be installed as per [Self::allocator_raw], [Self::allocator_mem],
    /// [Self::allocator_obj], and [Self::allocator_pymalloc_arena]. If a custom
    /// allocator backend is defined but all the `allocator_*` flags are [false],
    /// the allocator won't be used.
    pub allocator_backend: MemoryAllocatorBackend,

    /// Whether to install the custom allocator for the `raw` memory domain.
    ///
    /// See <https://docs.python.org/3/c-api/memory.html> for documentation on how Python
    /// memory allocator domains work.
    ///
    /// Default value: [true]
    ///
    /// Interpreter initialization behavior: controls whether [Self::allocator_backend]
    /// is used for the `raw` memory domain.
    ///
    /// Has no effect if [Self::allocator_backend] is [MemoryAllocatorBackend::Default].
    pub allocator_raw: bool,

    /// Whether to install the custom allocator for the `mem` memory domain.
    ///
    /// See <https://docs.python.org/3/c-api/memory.html> for documentation on how Python
    /// memory allocator domains work.
    ///
    /// Default value: [false]
    ///
    /// Interpreter initialization behavior: controls whether [Self::allocator_backend]
    /// is used for the `mem` memory domain.
    ///
    /// Has no effect if [Self::allocator_backend] is [MemoryAllocatorBackend::Default].
    pub allocator_mem: bool,

    /// Whether to install the custom allocator for the `obj` memory domain.
    ///
    /// See <https://docs.python.org/3/c-api/memory.html> for documentation on how Python
    /// memory allocator domains work.
    ///
    /// Default value: [false]
    ///
    /// Interpreter initialization behavior: controls whether [Self::allocator_backend]
    /// is used for the `obj` memory domain.
    ///
    /// Has no effect if [Self::allocator_backend] is [MemoryAllocatorBackend::Default].
    pub allocator_obj: bool,

    /// Whether to install the custom allocator for the `pymalloc` arena allocator.
    ///
    /// See <https://docs.python.org/3/c-api/memory.html> for documentation on how Python
    /// memory allocation works.
    ///
    /// Default value: [false]
    ///
    /// Interpreter initialization behavior: controls whether [Self::allocator_backend]
    /// is used for the `pymalloc` arena allocator.
    ///
    /// This setting requires the `pymalloc` allocator to be used for the `mem`
    /// or `obj` domains (`allocator_mem = false` and `allocator_obj = false` - this is
    /// the default behavior) and for [Self::allocator_backend] to not be
    /// [MemoryAllocatorBackend::Default].
    pub allocator_pymalloc_arena: bool,

    /// Whether to set up Python allocator debug hooks to detect memory bugs.
    ///
    /// Default value: [false]
    ///
    /// Interpreter initialization behavior: triggers the calling of
    /// `PyMem_SetupDebugHooks()` after custom allocators are installed.
    ///
    /// This setting can be used with or without custom memory allocators
    /// (see other `allocator_*` fields).
    pub allocator_debug: bool,

    /// Whether to automatically set missing "path configuration" fields.
    ///
    /// If `true`, various path configuration
    /// (<https://docs.python.org/3/c-api/init_config.html#path-configuration>) fields
    /// will be set automatically if their corresponding `.interpreter_config`
    /// fields are `None`. For example, `program_name` will be set to the current
    /// executable and `home` will be set to the executable's directory.
    ///
    /// If this is `false`, the default path configuration built into libpython
    /// is used.
    ///
    /// Setting this to `false` likely enables isolated interpreters to be used
    /// with "external" Python installs. If this is `true`, the default isolated
    /// configuration expects files like the Python standard library to be installed
    /// relative to the current executable. You will need to either ensure these
    /// files are present, define `packed_resources`, and/or set
    /// `.interpreter_config.module_search_paths` to ensure the interpreter can find
    /// the Python standard library, otherwise the interpreter will fail to start.
    ///
    /// Without this set or corresponding `.interpreter_config` fields set, you
    /// may also get run-time errors like
    /// `Could not find platform independent libraries <prefix>` or
    /// `Consider setting $PYTHONHOME to <prefix>[:<exec_prefix>]`. If you see
    /// these errors, it means the automatic path config resolutions built into
    /// libpython didn't work because the run-time layout didn't match the
    /// build-time configuration.
    ///
    /// Default value: [true]
    pub set_missing_path_configuration: bool,

    /// Whether to install `oxidized_importer` during interpreter initialization.
    ///
    /// If [true], `oxidized_importer` will be imported during interpreter
    /// initialization and an instance of `oxidized_importer.OxidizedFinder`
    /// will be installed on `sys.meta_path` as the first element.
    ///
    /// If [Self::packed_resources] are defined, they will be loaded into the
    /// `OxidizedFinder`.
    ///
    /// If [Self::filesystem_importer] is [true], its *path hook* will be
    /// registered on [`sys.path_hooks`] so `PathFinder` (the standard filesystem
    /// based importer) and [`pkgutil`] can use it.
    ///
    /// Default value: [false]
    ///
    /// Interpreter initialization behavior: See above.
    ///
    /// [`sys.path_hooks`]: https://docs.python.org/3/library/sys.html#sys.path_hooks
    /// [`pkgutil`]: https://docs.python.org/3/library/pkgutil.html
    pub oxidized_importer: bool,

    /// Whether to install the path-based finder.
    ///
    /// Controls whether to install the Python standard library `PathFinder` meta
    /// path finder (this is the meta path finder that loads Python modules and
    /// resources from the filesystem).
    ///
    /// Also controls whether to add `OxidizedFinder`'s path hook to
    /// [`sys.path_hooks`].
    ///
    /// Due to lack of control over low-level Python interpreter initialization,
    /// the standard library `PathFinder` will be registered on `sys.meta_path`
    /// and `sys.path_hooks` for a brief moment when the interpreter is initialized.
    /// If `sys.path` contains valid entries that would be serviced by this finder
    /// and `oxidized_importer` isn't able to service imports, it is possible for the
    /// path-based finder to be used to import some Python modules needed to initialize
    /// the Python interpreter. In many cases, this behavior is harmless. In all cases,
    /// the path-based importer is removed after Python interpreter initialization, so
    /// future imports won't be serviced by this path-based importer if it is disabled
    /// by this flag.
    ///
    /// Default value: [true]
    ///
    /// Interpreter initialization behavior: If false, path-based finders are removed
    /// from `sys.meta_path` and `sys.path_hooks` is cleared.
    ///
    /// [`sys.path_hooks`]: https://docs.python.org/3/library/sys.html#sys.path_hooks
    pub filesystem_importer: bool,

    /// References to packed resources data.
    ///
    /// The format of the data is defined by the ``python-packed-resources``
    /// crate. The data will be parsed as part of initializing the custom
    /// meta path importer during interpreter initialization when
    /// `oxidized_importer=true`. If `oxidized_importer=false`, this field
    /// is ignored.
    ///
    /// If paths are relative, that will be evaluated relative to the process's
    /// current working directory following the operating system's standard
    /// path expansion behavior.
    ///
    /// Default value: `vec![]`
    ///
    /// [Self::resolve()] behavior: [PackedResourcesSource::MemoryMappedPath] members
    /// have the special string `$ORIGIN` expanded to the string value that
    /// [Self::origin] resolves to.
    ///
    /// This field is ignored during serialization.
    #[cfg_attr(feature = "serialization", serde(skip))]
    pub packed_resources: Vec<PackedResourcesSource<'a>>,

    /// Extra extension modules to make available to the interpreter.
    ///
    /// The values will effectively be passed to ``PyImport_ExtendInitTab()``.
    ///
    /// Default value: [None]
    ///
    /// Interpreter initialization behavior: `PyImport_Inittab` will be extended
    /// with entries from this list. This makes the extensions available as
    /// built-in extension modules.
    ///
    /// This field is ignored during serialization.
    #[cfg_attr(feature = "serialization", serde(skip))]
    pub extra_extension_modules: Option<Vec<ExtensionModule>>,

    /// Command line arguments to initialize `sys.argv` with.
    ///
    /// Default value: [None]
    ///
    /// [Self::resolve()] behavior: [Some] value is used if set. Otherwise
    /// [PythonInterpreterConfig::argv] is used if set. Otherwise
    /// [std::env::args_os()] is called.
    ///
    /// Interpreter initialization behavior: the resolved [Some] value is used
    /// to populate `PyConfig.argv`.
    pub argv: Option<Vec<OsString>>,

    /// Whether to set `sys.argvb` with bytes versions of process arguments.
    ///
    /// On Windows, bytes will be UTF-16. On POSIX, bytes will be raw char*
    /// values passed to `int main()`.
    ///
    /// Enabling this feature will give Python applications access to the raw
    /// `bytes` values of raw argument data passed into the executable. The single
    /// or double width bytes nature of the data is preserved.
    ///
    /// Unlike `sys.argv` which may chomp off leading argument depending on the
    /// Python execution mode, `sys.argvb` has all the arguments used to initialize
    /// the process. i.e. the first argument is always the executable.
    ///
    /// Default value: [false]
    ///
    /// Interpreter initialization behavior: `sys.argvb` will be set to a
    /// `list[bytes]`. `sys.argv` and `sys.argvb` should have the same number
    /// of elements.
    pub argvb: bool,

    /// Automatically detect and run in `multiprocessing` mode.
    ///
    /// If set, [crate::MainPythonInterpreter::run()] will detect when the invoked
    /// interpreter looks like it is supposed to be a `multiprocessing` worker and
    /// will automatically call into the `multiprocessing` module instead of running
    /// the configured code.
    ///
    /// Enabling this has the same effect as calling `multiprocessing.freeze_support()`
    /// in your application code's `__main__` and replaces the need to do so.
    ///
    /// Default value: [true]
    pub multiprocessing_auto_dispatch: bool,

    /// Controls how to call `multiprocessing.set_start_method()`.
    ///
    /// Default value: [MultiprocessingStartMethod::Auto]
    ///
    /// Interpreter initialization behavior: if [Self::oxidized_importer] is [true],
    /// the `OxidizedImporter` will be taught to call `multiprocessing.set_start_method()`
    /// when `multiprocessing` is imported. If [false], this value has no effect.
    pub multiprocessing_start_method: MultiprocessingStartMethod,

    /// Whether to set sys.frozen=True.
    ///
    /// Setting this will enable Python to emulate "frozen" binaries, such as
    /// those used by PyInstaller.
    ///
    /// Default value: [false]
    ///
    /// Interpreter initialization behavior: If [true], `sys.frozen = True`.
    /// If [false], `sys.frozen` is not defined.
    pub sys_frozen: bool,

    /// Whether to set sys._MEIPASS to the directory of the executable.
    ///
    /// Setting this will enable Python to emulate PyInstaller's behavior
    /// of setting this attribute. This could potentially help with self-contained
    /// application compatibility by masquerading as PyInstaller and causing code
    /// to activate *PyInstaller mode*.
    ///
    /// Default value: [false]
    ///
    /// Interpreter initialization behavior: If [true], `sys._MEIPASS` will
    /// be set to a `str` holding the value of [Self::origin]. If [false],
    /// `sys._MEIPASS` will not be defined.
    pub sys_meipass: bool,

    /// How to resolve the `terminfo` database.
    ///
    /// Default value: [TerminfoResolution::Dynamic]
    ///
    /// Interpreter initialization behavior: the `TERMINFO_DIRS` environment
    /// variable may be set for this process depending on what [TerminfoResolution]
    /// instructs to do.
    ///
    /// `terminfo` is not used on Windows and this setting is ignored on that
    /// platform.
    pub terminfo_resolution: TerminfoResolution,

    /// Path to use to define the `TCL_LIBRARY` environment variable.
    ///
    /// This directory should contain an `init.tcl` file. It is commonly
    /// a directory named `tclX.Y`. e.g. `tcl8.6`.
    ///
    /// Default value: [None]
    ///
    /// [Self::resolve()] behavior: the token `$ORIGIN` is expanded to the
    /// resolved value of [Self::origin].
    ///
    /// Interpreter initialization behavior: if set, the `TCL_LIBRARY` environment
    /// variable will be set for the current process.
    pub tcl_library: Option<PathBuf>,

    /// Environment variable holding the directory to write a loaded modules file.
    ///
    /// If this value is set and the environment it refers to is set,
    /// on interpreter shutdown, we will write a `modules-<random>` file to
    /// the directory specified containing a `\n` delimited list of modules
    /// loaded in `sys.modules`.
    ///
    /// This setting is useful to record which modules are loaded during the execution
    /// of a Python interpreter.
    ///
    /// Default value: [None]
    pub write_modules_directory_env: Option<String>,
}

impl<'a> Default for OxidizedPythonInterpreterConfig<'a> {
    fn default() -> Self {
        Self {
            exe: None,
            origin: None,
            interpreter_config: PythonInterpreterConfig {
                profile: PythonInterpreterProfile::Python,
                ..PythonInterpreterConfig::default()
            },
            allocator_backend: MemoryAllocatorBackend::Default,
            // We set to true by default so any installed custom backend
            // takes effect.
            allocator_raw: true,
            allocator_mem: false,
            allocator_obj: false,
            allocator_pymalloc_arena: false,
            allocator_debug: false,
            set_missing_path_configuration: true,
            oxidized_importer: false,
            filesystem_importer: true,
            packed_resources: vec![],
            extra_extension_modules: None,
            argv: None,
            argvb: false,
            multiprocessing_auto_dispatch: true,
            multiprocessing_start_method: MultiprocessingStartMethod::Auto,
            sys_frozen: false,
            sys_meipass: false,
            terminfo_resolution: TerminfoResolution::Dynamic,
            tcl_library: None,
            write_modules_directory_env: None,
        }
    }
}

impl<'a> OxidizedPythonInterpreterConfig<'a> {
    /// Create a new type with all values resolved.
    pub fn resolve(
        self,
    ) -> Result<ResolvedOxidizedPythonInterpreterConfig<'a>, NewInterpreterError> {
        let argv = if let Some(args) = self.argv {
            Some(args)
        } else if self.interpreter_config.argv.is_some() {
            None
        } else {
            Some(std::env::args_os().collect::<Vec<_>>())
        };

        let exe = if let Some(exe) = self.exe {
            exe
        } else {
            std::env::current_exe()
                .map_err(|_| NewInterpreterError::Simple("could not obtain current executable"))?
        };

        // We always canonicalize the current executable because we use path
        // comparisons in the path hooks importer to assess whether a given sys.path
        // entry is this executable.
        let exe = dunce::canonicalize(exe)
            .map_err(|_| NewInterpreterError::Simple("could not obtain current executable path"))?;

        let origin = if let Some(origin) = self.origin {
            origin
        } else {
            exe.parent()
                .ok_or(NewInterpreterError::Simple(
                    "unable to obtain current executable parent directory",
                ))?
                .to_path_buf()
        };

        let origin_string = origin.display().to_string();

        let packed_resources = self
            .packed_resources
            .into_iter()
            .map(|entry| match entry {
                PackedResourcesSource::Memory(_) => entry,
                PackedResourcesSource::MemoryMappedPath(p) => {
                    PackedResourcesSource::MemoryMappedPath(PathBuf::from(
                        p.display().to_string().replace("$ORIGIN", &origin_string),
                    ))
                }
            })
            .collect::<Vec<_>>();

        let module_search_paths = self
            .interpreter_config
            .module_search_paths
            .as_ref()
            .map(|x| {
                x.iter()
                    .map(|p| {
                        PathBuf::from(p.display().to_string().replace("$ORIGIN", &origin_string))
                    })
                    .collect::<Vec<_>>()
            });

        let tcl_library = self
            .tcl_library
            .as_ref()
            .map(|x| PathBuf::from(x.display().to_string().replace("$ORIGIN", &origin_string)));

        Ok(ResolvedOxidizedPythonInterpreterConfig {
            inner: Self {
                exe: Some(exe),
                origin: Some(origin),
                interpreter_config: PythonInterpreterConfig {
                    module_search_paths,
                    ..self.interpreter_config
                },
                argv,
                packed_resources,
                tcl_library,
                ..self
            },
        })
    }
}

/// An `OxidizedPythonInterpreterConfig` that has fields resolved.
pub struct ResolvedOxidizedPythonInterpreterConfig<'a> {
    inner: OxidizedPythonInterpreterConfig<'a>,
}

impl<'a> Deref for ResolvedOxidizedPythonInterpreterConfig<'a> {
    type Target = OxidizedPythonInterpreterConfig<'a>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'a> TryFrom<OxidizedPythonInterpreterConfig<'a>>
    for ResolvedOxidizedPythonInterpreterConfig<'a>
{
    type Error = NewInterpreterError;

    fn try_from(value: OxidizedPythonInterpreterConfig<'a>) -> Result<Self, Self::Error> {
        value.resolve()
    }
}

impl<'a> ResolvedOxidizedPythonInterpreterConfig<'a> {
    /// Obtain the value for the current executable.
    pub fn exe(&self) -> &PathBuf {
        self.inner.exe.as_ref().expect("exe should have a value")
    }

    /// Obtain the path for $ORIGIN.
    pub fn origin(&self) -> &PathBuf {
        self.inner
            .origin
            .as_ref()
            .expect("origin should have a value")
    }

    /// Resolve the effective value of `sys.argv`.
    pub fn resolve_sys_argv(&self) -> &[OsString] {
        if let Some(args) = &self.inner.argv {
            args
        } else if let Some(args) = &self.inner.interpreter_config.argv {
            args
        } else {
            panic!("1 of .argv or .interpreter_config.argv should be set")
        }
    }

    /// Resolve the value to use for `sys.argvb`.
    pub fn resolve_sys_argvb(&self) -> Vec<OsString> {
        if let Some(args) = &self.inner.interpreter_config.argv {
            args.clone()
        } else if let Some(args) = &self.inner.argv {
            args.clone()
        } else {
            std::env::args_os().collect::<Vec<_>>()
        }
    }
}

impl<'a, 'config: 'a> TryFrom<&ResolvedOxidizedPythonInterpreterConfig<'config>>
    for PythonResourcesState<'a, u8>
{
    type Error = NewInterpreterError;

    fn try_from(
        config: &ResolvedOxidizedPythonInterpreterConfig<'config>,
    ) -> Result<Self, Self::Error> {
        let mut state = Self::default();
        state.set_current_exe(config.exe().to_path_buf());
        state.set_origin(config.origin().to_path_buf());

        for source in &config.packed_resources {
            match source {
                PackedResourcesSource::Memory(data) => {
                    state
                        .index_data(data)
                        .map_err(NewInterpreterError::Simple)?;
                }
                PackedResourcesSource::MemoryMappedPath(path) => {
                    state
                        .index_path_memory_mapped(path)
                        .map_err(NewInterpreterError::Dynamic)?;
                }
            }
        }

        state
            .index_interpreter_builtins()
            .map_err(NewInterpreterError::Simple)?;

        Ok(state)
    }
}

#[cfg(test)]
mod tests {
    use {super::*, anyhow::Result};

    #[test]
    fn test_packed_resources_implicit_origin() -> Result<()> {
        let mut config = OxidizedPythonInterpreterConfig::default();
        config
            .packed_resources
            .push(PackedResourcesSource::MemoryMappedPath(PathBuf::from(
                "$ORIGIN/lib/packed-resources",
            )));

        let resolved = config.resolve()?;

        assert_eq!(
            resolved.packed_resources,
            vec![PackedResourcesSource::MemoryMappedPath(
                resolved.origin().join("lib/packed-resources")
            )]
        );

        Ok(())
    }

    #[test]
    fn test_packed_resources_explicit_origin() -> Result<()> {
        let mut config = OxidizedPythonInterpreterConfig {
            origin: Some(PathBuf::from("/other/origin")),
            ..Default::default()
        };

        config
            .packed_resources
            .push(PackedResourcesSource::MemoryMappedPath(PathBuf::from(
                "$ORIGIN/lib/packed-resources",
            )));

        let resolved = config.resolve()?;

        assert_eq!(
            resolved.packed_resources,
            vec![PackedResourcesSource::MemoryMappedPath(PathBuf::from(
                "/other/origin/lib/packed-resources"
            ))]
        );

        Ok(())
    }
}
