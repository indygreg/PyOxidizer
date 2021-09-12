// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Data structures for configuring a Python interpreter.

use {
    crate::NewInterpreterError,
    pyo3::ffi as pyffi,
    python_oxidized_importer::{PackedResourcesSource, PythonResourcesState},
    python_packaging::interpreter::{
        MemoryAllocatorBackend, MultiprocessingStartMethod, PythonInterpreterConfig,
        PythonInterpreterProfile, TerminfoResolution,
    },
    std::{
        convert::TryFrom,
        ffi::{CString, OsString},
        ops::Deref,
        path::PathBuf,
    },
};

/// Defines an extra extension module to load.
#[derive(Clone, Debug)]
pub struct ExtensionModule {
    /// Name of the extension module.
    pub name: CString,

    /// Extension module initialization function.
    pub init_func: unsafe extern "C" fn() -> *mut pyffi::PyObject,
}

/// Configure a Python interpreter.
///
/// This type defines the configuration of a Python interpreter. It is used
/// to initialize a Python interpreter embedded in the current process.
///
/// The type contains a reference to a `PythonInterpreterConfig` instance,
/// which is an abstraction over the low-level C structs that Python uses during
/// interpreter initialization.
///
/// The `PythonInterpreterConfig` has a single non-optional field: `profile`.
/// This defines the defaults for various fields of the `PyPreConfig` and
/// `PyConfig` instances that are initialized as part of interpreter
/// initialization. See
/// https://docs.python.org/3/c-api/init_config.html#isolated-configuration for
/// more.
///
/// During interpreter initialization, we produce a `PyPreConfig` and
/// `PyConfig` derived from this type. Config settings are applied in
/// layers. First, we use the `PythonInterpreterConfig.profile` to derive
/// a default instance given a profile. Next, we override fields if the
/// `PythonInterpreterConfig` has `Some(T)` value set. Finally, we populate
/// some fields if they are missing but required for the given configuration.
/// For example, when in *isolated* mode, we set `program_name` and `home`
/// unless an explicit value was provided in the `PythonInterpreterConfig`.
///
/// Generally speaking, the `PythonInterpreterConfig` exists to hold
/// configuration that is defined in the CPython initialization and
/// configuration API and `OxidizedPythonInterpreterConfig` exists to
/// hold higher-level configuration for features specific to this crate.
#[derive(Clone, Debug)]
pub struct OxidizedPythonInterpreterConfig<'a> {
    /// The path of the currently executing executable.
    ///
    /// If not set, [std::env::current_exe()] will be used.
    ///
    /// In all cases, the path will be canonicalized.
    pub exe: Option<PathBuf>,

    /// The filesystem path from which relative paths will be interpreted.
    pub origin: Option<PathBuf>,

    /// Low-level configuration of Python interpreter.
    pub interpreter_config: PythonInterpreterConfig,

    /// Memory allocator backend to use.
    pub allocator_backend: MemoryAllocatorBackend,

    /// Whether to install the custom allocator for the `raw` memory domain.
    ///
    /// See https://docs.python.org/3/c-api/memory.html for documentation on how Python
    /// memory allocator domains work.
    ///
    /// Has no effect if `allocator_backend` is `MemoryAllocatorBackend::Default`.
    pub allocator_raw: bool,

    /// Whether to install the custom allocator for the `mem` memory domain.
    ///
    /// See https://docs.python.org/3/c-api/memory.html for documentation on how Python
    /// memory allocator domains work.
    ///
    /// Has no effect if `allocator_backend` is `MemoryAllocatorBackend::Default`.
    pub allocator_mem: bool,

    /// Whether to install the custom allocator for the `obj` memory domain.
    ///
    /// See https://docs.python.org/3/c-api/memory.html for documentation on how Python
    /// memory allocator domains work.
    ///
    /// Has no effect if `allocator_backend` is `MemoryAllocatorBackend::Default`.
    pub allocator_obj: bool,

    /// Whether to install the custom allocator for the `pymalloc` arena allocator.
    ///
    /// See https://docs.python.org/3/c-api/memory.html for documentation on how Python
    /// memory allocation works.
    ///
    /// This setting requires the `pymalloc` allocator to be used for the `mem`
    /// or `obj` domains (`allocator_mem = false` and `allocator_obj = false` - this is
    /// the default behavior) and for a custom allocator backend to not be
    /// `MemoryAllocatorBackend::Default`.
    pub allocator_pymalloc_arena: bool,

    /// Whether to set up Python allocator debug hooks to detect memory bugs.
    ///
    /// This setting triggers the calling of `PyMem_SetupDebugHooks()` during
    /// interpreter initialization. It can be used with or without custom
    /// Python allocators.
    pub allocator_debug: bool,

    /// Whether to automatically set missing "path configuration" fields.
    ///
    /// If `true`, various path configuration
    /// (https://docs.python.org/3/c-api/init_config.html#path-configuration) fields
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
    pub set_missing_path_configuration: bool,

    /// Whether to install our custom meta path importer on interpreter init,
    /// and, if [`filesystem_importer`] is `true`, to add its ``path_hook``
    /// method to [`sys.path_hooks`] for `PathFinder`'s and [`pkgutil`]'s use.
    ///
    /// [`filesystem_importer`]: #structfield.filesystem_importer
    /// [`sys.path_hooks`]: https://docs.python.org/3/library/sys.html#sys.path_hooks
    /// [`pkgutil`]: https://docs.python.org/3/library/pkgutil.html
    pub oxidized_importer: bool,

    /// Whether to install the default `PathFinder` meta path finder and, if
    /// [`oxidized_importer`] is `true`, to add our custom meta path
    /// importer's ``path_hook`` method to [`sys.path_hooks`] for `PathFinder`'s
    /// and [`pkgutil`]'s use.
    ///
    /// [`oxidized_importer`]: #structfield.oxidized_importer
    /// [`sys.path_hooks`]: https://docs.python.org/3/library/sys.html#sys.path_hooks
    /// [`pkgutil`]: https://docs.python.org/3/library/pkgutil.html
    pub filesystem_importer: bool,

    /// References to packed resources data.
    ///
    /// The format of the data is defined by the ``python-packed-resources``
    /// crate. The data will be parsed as part of initializing the custom
    /// meta path importer during interpreter initialization when
    /// `oxidized_importer=true`. If `oxidized_importer=false`, this field
    /// is ignored.
    ///
    /// For `Path`-based sources, the special string `$ORIGIN` will be expanded
    /// to the directory of the current executable or the value of
    /// `self.origin` if set. Relative paths without `$ORIGIN` will be evaluated
    /// relative to the process's current working directory.
    pub packed_resources: Vec<PackedResourcesSource<'a>>,

    /// Extra extension modules to make available to the interpreter.
    ///
    /// The values will effectively be passed to ``PyImport_ExtendInitTab()``.
    pub extra_extension_modules: Option<Vec<ExtensionModule>>,

    /// Command line arguments to initialize `sys.argv` with.
    ///
    /// If `Some(T)`, interpreter initialization will set `PyConfig.argv`
    /// to a value derived from this value, overwriting an existing
    /// `.interpreter_config.argv` value, if set.
    ///
    /// `None` is evaluated to `Some(std::env::args_os().collect::<Vec<_>>()`
    /// if `.interpreter_config.argv` is `None` or `None` if
    /// `.interpreter_config.argv` is `Some(T)`.
    pub argv: Option<Vec<OsString>>,

    /// Whether to set sys.argvb with bytes versions of process arguments.
    ///
    /// On Windows, bytes will be UTF-16. On POSIX, bytes will be raw char*
    /// values passed to `int main()`.
    pub argvb: bool,

    /// Whether the main Python interpreter run routine will detect use of multiprocessing
    /// and run a multiprocessing worker process automatically.
    pub multiprocessing_auto_dispatch: bool,

    /// How to call `multiprocessing.set_start_method()` when `multiprocessing` is imported.
    pub multiprocessing_start_method: MultiprocessingStartMethod,

    /// Whether to set sys.frozen=True.
    ///
    /// Setting this will enable Python to emulate "frozen" binaries, such as
    /// those used by PyInstaller.
    pub sys_frozen: bool,

    /// Whether to set sys._MEIPASS to the directory of the executable.
    ///
    /// Setting this will enable Python to emulate PyInstaller's behavior
    /// of setting this attribute.
    pub sys_meipass: bool,

    /// How to resolve the `terminfo` database.
    pub terminfo_resolution: TerminfoResolution,

    /// Path to use to define the `TCL_LIBRARY` environment variable.
    ///
    /// This directory should contain an `init.tcl` file. It is commonly
    /// a directory named `tclX.Y`. e.g. `tcl8.6`.
    ///
    /// `$ORIGIN` in the path is expanded to the directory of the current
    /// executable.
    pub tcl_library: Option<PathBuf>,

    /// Environment variable holding the directory to write a loaded modules file.
    ///
    /// If this value is set and the environment it refers to is set,
    /// on interpreter shutdown, we will write a ``modules-<random>`` file to
    /// the directory specified containing a ``\n`` delimited list of modules
    /// loaded in ``sys.modules``.
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
        state.origin = config.origin().clone();

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
        let mut config = OxidizedPythonInterpreterConfig::default();
        config.origin = Some(PathBuf::from("/other/origin"));
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
