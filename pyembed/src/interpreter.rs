// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Manage an embedded Python interpreter.

use {
    crate::{
        config::{OxidizedPythonInterpreterConfig, ResolvedOxidizedPythonInterpreterConfig},
        conversion::osstring_to_bytes,
        error::NewInterpreterError,
        extension::{PyInit_oxidized_importer, OXIDIZED_IMPORTER_NAME, OXIDIZED_IMPORTER_NAME_STR},
        importer::{
            install_path_hook, remove_external_importers, replace_meta_path_importers,
            ImporterState,
        },
        osutils::resolve_terminfo_dirs,
        pyalloc::PythonMemoryAllocator,
        python_resources::PythonResourcesState,
    },
    cpython::{
        exc::RuntimeError, GILGuard, NoArgs, ObjectProtocol, PyDict, PyErr, PyList, PyResult,
        PyString, Python, PythonObject, ToPyObject,
    },
    once_cell::sync::Lazy,
    python3_sys as oldpyffi,
    python_packaging::interpreter::{MultiprocessingStartMethod, TerminfoResolution},
    std::{
        collections::BTreeSet,
        convert::{TryFrom, TryInto},
        env, fs,
        io::Write,
        path::{Path, PathBuf},
    },
};

static GLOBAL_INTERPRETER_GUARD: Lazy<std::sync::Mutex<()>> =
    Lazy::new(|| std::sync::Mutex::new(()));

/// Manages an embedded Python interpreter.
///
/// Python interpreters have global state and there can only be a single
/// instance of this type per process. There exists a global lock enforcing
/// this. Calling `new()` will block waiting for this lock. The lock is
/// released when the instance is dropped.
///
/// Instances must only be constructed through [`MainPythonInterpreter::new()`](#method.new).
///
/// This type and its various functionality is a glorified wrapper around the
/// Python C API. But there's a lot of added functionality on top of what the C
/// API provides.
///
/// Both the low-level `python3-sys` and higher-level `cpython` crates are used.
pub struct MainPythonInterpreter<'python, 'interpreter: 'python, 'resources: 'interpreter> {
    // It is possible to have a use-after-free if config is dropped before the
    // interpreter is finalized/dropped.
    config: ResolvedOxidizedPythonInterpreterConfig<'resources>,
    interpreter_guard: Option<std::sync::MutexGuard<'interpreter, ()>>,
    pub(crate) allocator: Option<PythonMemoryAllocator>,
    _gil: Option<GILGuard>,
    py: Option<Python<'python>>,
    /// File to write containing list of modules when the interpreter finalizes.
    write_modules_path: Option<PathBuf>,
}

impl<'python, 'interpreter, 'resources> MainPythonInterpreter<'python, 'interpreter, 'resources> {
    /// Construct a Python interpreter from a configuration.
    ///
    /// The Python interpreter is initialized as a side-effect. The GIL is held.
    pub fn new(
        config: OxidizedPythonInterpreterConfig<'resources>,
    ) -> Result<MainPythonInterpreter<'python, 'interpreter, 'resources>, NewInterpreterError> {
        let config: ResolvedOxidizedPythonInterpreterConfig<'resources> = config.try_into()?;

        match config.terminfo_resolution {
            TerminfoResolution::Dynamic => {
                if let Some(v) = resolve_terminfo_dirs() {
                    env::set_var("TERMINFO_DIRS", &v);
                }
            }
            TerminfoResolution::Static(ref v) => {
                env::set_var("TERMINFO_DIRS", v);
            }
            TerminfoResolution::None => {}
        }

        let mut res = MainPythonInterpreter {
            config,
            interpreter_guard: None,
            allocator: None,
            _gil: None,
            py: None,
            write_modules_path: None,
        };

        res.init()?;

        Ok(res)
    }

    /// Initialize the interpreter.
    ///
    /// This mutates global state in the Python interpreter according to the
    /// bound config and initializes the Python interpreter.
    ///
    /// After this is called, the embedded Python interpreter is ready to
    /// execute custom code.
    ///
    /// If called more than once, the function is a no-op from the perspective
    /// of interpreter initialization.
    ///
    /// Returns a Python instance which has the GIL acquired.
    fn init(&mut self) -> Result<(), NewInterpreterError> {
        assert!(self.interpreter_guard.is_none());
        self.interpreter_guard = Some(GLOBAL_INTERPRETER_GUARD.lock().map_err(|_| {
            NewInterpreterError::Simple("unable to acquire global interpreter guard")
        })?);

        let origin_string = self.config.origin().display().to_string();

        if let Some(tcl_library) = &self.config.tcl_library {
            std::env::set_var("TCL_LIBRARY", tcl_library);
        }

        set_pyimport_inittab(&self.config);

        // Pre-configure Python.
        let pre_config = oldpyffi::PyPreConfig::try_from(&self.config)?;

        unsafe {
            let status = oldpyffi::Py_PreInitialize(&pre_config);

            if oldpyffi::PyStatus_Exception(status) != 0 {
                return Err(NewInterpreterError::new_from_pystatus(
                    &status,
                    "Python pre-initialization",
                ));
            }
        };

        // Set the memory allocator domains if they are configured.
        self.allocator = PythonMemoryAllocator::from_backend(self.config.allocator_backend);

        if let Some(allocator) = &self.allocator {
            if self.config.allocator_raw {
                allocator.set_allocator(oldpyffi::PyMemAllocatorDomain::PYMEM_DOMAIN_RAW);
            }

            if self.config.allocator_mem {
                allocator.set_allocator(oldpyffi::PyMemAllocatorDomain::PYMEM_DOMAIN_MEM);
            }

            if self.config.allocator_obj {
                allocator.set_allocator(oldpyffi::PyMemAllocatorDomain::PYMEM_DOMAIN_OBJ);
            }

            if self.config.allocator_pymalloc_arena {
                if self.config.allocator_mem || self.config.allocator_obj {
                    return Err(NewInterpreterError::Simple("A custom pymalloc arena allocator cannot be used with custom `mem` or `obj` domain allocators"));
                }

                allocator.set_arena_allocator();
            }
        }

        // Debug hooks apply to all allocator domains and work with or without
        // custom domain allocators.
        if self.config.allocator_debug {
            unsafe {
                oldpyffi::PyMem_SetupDebugHooks();
            }
        }

        let mut py_config: oldpyffi::PyConfig = (&self.config).try_into()?;

        // Enable multi-phase initialization. This allows us to initialize
        // our custom importer before Python attempts any imports.
        py_config._init_main = 0;

        let status = unsafe { oldpyffi::Py_InitializeFromConfig(&py_config) };
        if unsafe { oldpyffi::PyStatus_Exception(status) } != 0 {
            return Err(NewInterpreterError::new_from_pystatus(
                &status,
                "initializing Python core",
            ));
        }

        // At this point, the core of Python is initialized.
        // importlib._bootstrap has been loaded. But not
        // importlib._bootstrap_external. This is where we work our magic to
        // inject our custom importer.

        let py = unsafe { Python::assume_gil_acquired() };
        self.py = Some(py);

        let oxidized_finder = if self.config.oxidized_importer {
            let resources_state = Box::new(PythonResourcesState::try_from(&self.config)?);

            let oxidized_importer = py.import(OXIDIZED_IMPORTER_NAME_STR).map_err(|err| {
                NewInterpreterError::new_from_pyerr(py, err, "import of oxidized importer module")
            })?;

            let cb = |importer_state: &mut ImporterState| match self
                .config
                .multiprocessing_start_method
            {
                MultiprocessingStartMethod::None => {}
                MultiprocessingStartMethod::Fork
                | MultiprocessingStartMethod::ForkServer
                | MultiprocessingStartMethod::Spawn => {
                    importer_state.set_multiprocessing_set_start_method(Some(
                        self.config.multiprocessing_start_method.to_string(),
                    ));
                }
                MultiprocessingStartMethod::Auto => {
                    // Windows uses "spawn" because "fork" isn't available.
                    // Everywhere else uses "fork." The default on macOS is "spawn." This
                    // is due to https://bugs.python.org/issue33725, which only affects
                    // Python framework builds. Our assumption is we aren't using a Python
                    // framework, so "spawn" is safe.
                    let method = if cfg!(target_family = "windows") {
                        "spawn"
                    } else {
                        "fork"
                    };

                    importer_state.set_multiprocessing_set_start_method(Some(method.to_string()));
                }
            };

            // Ownership of the resources state is transferred into the importer, where the Box
            // is summarily leaked. However, the importer tracks a pointer to the resources state
            // and will constitute the struct for dropping when it itself is dropped. We could
            // potentially encounter a use-after-free if the importer is used after self.config
            // is dropped. However, that would require self to be dropped. And if self is dropped,
            // there should no longer be a Python interpreter around. So it follows that the
            // importer state cannot be dropped after self.
            Some(
                replace_meta_path_importers(py, &oxidized_importer, resources_state, Some(cb))
                    .map_err(|err| {
                        NewInterpreterError::new_from_pyerr(
                            py,
                            err,
                            "initialization of oxidized importer",
                        )
                    })?,
            )
        } else {
            None
        };

        // Now proceed with the Python main initialization. This will initialize
        // importlib. And if the custom importlib bytecode was registered above,
        // our extension module will get imported and initialized.
        let status = unsafe { oldpyffi::_Py_InitializeMain() };

        if unsafe { oldpyffi::PyStatus_Exception(status) } != 0 {
            return Err(NewInterpreterError::new_from_pystatus(
                &status,
                "initializing Python main",
            ));
        }

        let sys_module = py
            .import("sys")
            .map_err(|err| NewInterpreterError::new_from_pyerr(py, err, "obtaining sys module"))?;

        // When the main initialization ran, it initialized the "external"
        // importer (importlib._bootstrap_external), mutating `sys.meta_path`
        // and `sys.path_hooks`.
        //
        // If enabled, OxidizedFinder should still be the initial entry on
        // `sys.meta_path`. And if it were capable, OxidizedFinder would have
        // serviced all imports so far.
        //
        // Here, we undo the mutations caused by initializing of the "external"
        // importers if we're not configured to perform filesystem importing.
        // Ideally there would be a field on PyConfig to prevent the initializing
        // of these importers. But there isn't. There is an `_install_importlib`
        // field. However, when disabled it disables a lot of "main" initialization
        // and isn't usable for us.

        if !self.config.filesystem_importer {
            remove_external_importers(py, &sys_module).map_err(|err| {
                NewInterpreterError::new_from_pyerr(py, err, "removing external importers")
            })?;
        }

        if let Some(finder) = &oxidized_finder {
            install_path_hook(py, &finder, &sys_module).map_err(|err| {
                NewInterpreterError::new_from_pyerr(
                    py,
                    err,
                    "installing OxidizedFinder in sys.path_hooks",
                )
            })?;
        }

        if self.config.argvb {
            let args_objs = self
                .config
                .resolve_sys_argvb()
                .iter()
                .map(|x| osstring_to_bytes(py, x.clone()))
                .collect::<Vec<_>>();

            let args = PyList::new(py, &args_objs);
            let argvb = b"argvb\0";

            let res = args.with_borrowed_ptr(py, |args_ptr| unsafe {
                oldpyffi::PySys_SetObject(argvb.as_ptr() as *const i8, args_ptr)
            });

            match res {
                0 => (),
                _ => return Err(NewInterpreterError::Simple("unable to set sys.argvb")),
            }
        }

        // As a convention, sys.oxidized is set to indicate we are running from
        // a self-contained application.
        let oxidized = b"oxidized\0";

        let res = py.True().with_borrowed_ptr(py, |py_true| unsafe {
            oldpyffi::PySys_SetObject(oxidized.as_ptr() as *const i8, py_true)
        });

        match res {
            0 => (),
            _ => return Err(NewInterpreterError::Simple("unable to set sys.oxidized")),
        }

        if self.config.sys_frozen {
            let frozen = b"frozen\0";

            match py.True().with_borrowed_ptr(py, |py_true| unsafe {
                oldpyffi::PySys_SetObject(frozen.as_ptr() as *const i8, py_true)
            }) {
                0 => (),
                _ => return Err(NewInterpreterError::Simple("unable to set sys.frozen")),
            }
        }

        if self.config.sys_meipass {
            let meipass = b"_MEIPASS\0";
            let value = PyString::new(py, &origin_string);

            match value.with_borrowed_ptr(py, |py_value| unsafe {
                oldpyffi::PySys_SetObject(meipass.as_ptr() as *const i8, py_value)
            }) {
                0 => (),
                _ => return Err(NewInterpreterError::Simple("unable to set sys._MEIPASS")),
            }
        }

        if let Some(key) = &self.config.write_modules_directory_env {
            if let Ok(path) = std::env::var(key) {
                let path = PathBuf::from(path);

                std::fs::create_dir_all(&path).map_err(|e| {
                    NewInterpreterError::Dynamic(format!(
                        "error creating directory for loaded modules files: {}",
                        e.to_string()
                    ))
                })?;

                // We use Python's uuid module to generate a filename. This avoids
                // a dependency on a Rust crate, which cuts down on dependency bloat.
                let uuid_mod = py.import("uuid").map_err(|e| {
                    NewInterpreterError::new_from_pyerr(py, e, "importing uuid module")
                })?;
                let uuid = uuid_mod.call(py, "uuid4", NoArgs, None).map_err(|e| {
                    NewInterpreterError::new_from_pyerr(py, e, "calling uuid.uuid()")
                })?;
                let uuid_str = uuid
                    .str(py)
                    .map_err(|e| {
                        NewInterpreterError::new_from_pyerr(py, e, "converting uuid to str")
                    })?
                    .to_string(py)
                    .map_err(|e| {
                        NewInterpreterError::new_from_pyerr(
                            py,
                            e,
                            "converting uuid str to Rust string",
                        )
                    })?
                    .to_string();

                self.write_modules_path = Some(path.join(format!("modules-{}", uuid_str)));
            }
        }

        Ok(())
    }

    /// Ensure the Python GIL is released.
    pub fn release_gil(&mut self) {
        self.py = None;
        self._gil = None;
    }

    /// Ensure the Python GIL is acquired, returning a handle on the interpreter.
    ///
    /// The returned value has a lifetime of the `MainPythonInterpreter`
    /// instance. This is because `MainPythonInterpreter.drop()` finalizes
    /// the interpreter. The borrow checker should refuse to compile code
    /// where the returned `Python` outlives `self`.
    pub fn acquire_gil(&mut self) -> Python<'_> {
        match self.py {
            Some(py) => py,
            None => {
                let gil = GILGuard::acquire();
                let py = unsafe { Python::assume_gil_acquired() };

                self._gil = Some(gil);
                self.py = Some(py);

                py
            }
        }
    }

    /// Runs `Py_RunMain()` and finalizes the interpreter.
    ///
    /// This will execute whatever is configured by the Python interpreter config
    /// and return an integer suitable for use as a process exit code.
    ///
    /// Calling this function will finalize the interpreter and only gives you an
    /// exit code: there is no opportunity to inspect the return value or handle
    /// an uncaught exception. If you want to keep the interpreter alive or inspect
    /// the evaluation result, consider calling a function on the interpreter handle
    /// that executes code.
    pub fn py_runmain(self) -> i32 {
        unsafe { oldpyffi::Py_RunMain() }
    }

    /// Run in "multiprocessing worker" mode.
    ///
    /// This should be called when `sys.argv[1] == "--multiprocessing-fork"`. It
    /// will parse arguments for the worker from `sys.argv` and call into the
    /// `multiprocessing` module to perform work.
    pub fn run_multiprocessing(&mut self) -> PyResult<i32> {
        // This code effectively reimplements multiprocessing.spawn.freeze_support(),
        // except entirely in the Rust domain. This function effectively verifies
        // `sys.argv[1] == "--multiprocessing-fork"` then parsed key=value arguments
        // from arguments that follow. The keys are well-defined and guaranteed to
        // be ASCII. The values are either ``None`` or an integer. This enables us
        // to parse the arguments purely from Rust.

        let argv = self.config.resolve_sys_argv().to_vec();

        if argv.len() < 2 {
            panic!("run_multiprocessing() called prematurely; sys.argv does not indicate multiprocessing mode");
        }

        let py = self.acquire_gil();
        let kwargs = PyDict::new(py);

        for arg in argv.iter().skip(2) {
            let arg = arg.to_string_lossy();

            let mut parts = arg.splitn(2, '=');

            let key = parts.next().ok_or_else(|| {
                PyErr::new::<RuntimeError, _>(py, "invalid multiprocessing argument")
            })?;
            let value = parts.next().ok_or_else(|| {
                PyErr::new::<RuntimeError, _>(py, "invalid multiprocessing argument")
            })?;

            let value = if value == "None" {
                py.None()
            } else {
                let v = value.parse::<isize>().map_err(|e| {
                    PyErr::new::<RuntimeError, _>(
                        py,
                        format!(
                            "unable to convert multiprocessing argument to integer: {}",
                            e
                        ),
                    )
                })?;

                v.into_py_object(py).into_object()
            };

            kwargs.set_item(py, key, value)?;
        }

        let spawn_module = py.import("multiprocessing.spawn")?;
        spawn_module.call(py, "spawn_main", NoArgs, Some(&kwargs))?;

        Ok(0)
    }

    /// Whether the Python interpreter is in "multiprocessing worker" mode.
    ///
    /// The `multiprocessing` module can work by spawning new processes
    /// with arguments `--multiprocessing-fork [key=value] ...`. This function
    /// detects if the current Python interpreter is configured for said execution.
    pub fn is_multiprocessing(&self) -> bool {
        let argv = self.config.resolve_sys_argv();

        argv.len() >= 2 && argv[1] == "--multiprocessing-fork"
    }

    /// Runs the Python interpreter.
    ///
    /// If multiprocessing dispatch is enabled, this will check if the
    /// current process invocation appears to be a spawned multiprocessing worker
    /// and dispatch to multiprocessing accordingly.
    ///
    /// Otherwise, this delegates to [Self::py_runmain].
    pub fn run(mut self) -> i32 {
        if self.config.multiprocessing_auto_dispatch && self.is_multiprocessing() {
            match self.run_multiprocessing() {
                Ok(code) => code,
                Err(e) => {
                    let py = self.acquire_gil();
                    e.print(py);
                    1
                }
            }
        } else {
            self.py_runmain()
        }
    }
}

static mut ORIGINAL_BUILTIN_EXTENSIONS: Option<Vec<oldpyffi::_inittab>> = None;
static mut REPLACED_BUILTIN_EXTENSIONS: Option<Vec<oldpyffi::_inittab>> = None;

/// Set PyImport_Inittab from config options.
///
/// CPython has buggy code around memory handling for PyImport_Inittab.
/// See https://github.com/python/cpython/pull/19746. So, we can't trust
/// the official APIs to do the correct thing if there are multiple
/// interpreters per process.
///
/// We maintain our own shadow copy of this array and synchronize it
/// to PyImport_Inittab during interpreter initialization so we don't
/// call the broken APIs.
fn set_pyimport_inittab(config: &OxidizedPythonInterpreterConfig) {
    // If this is our first time, copy the canonical source to our shadow
    // copy.
    unsafe {
        if ORIGINAL_BUILTIN_EXTENSIONS.is_none() {
            let mut entries: Vec<oldpyffi::_inittab> = Vec::new();

            for i in 0.. {
                let record = oldpyffi::PyImport_Inittab.offset(i);

                if (*record).name.is_null() {
                    break;
                }

                entries.push(*record);
            }

            ORIGINAL_BUILTIN_EXTENSIONS = Some(entries);
        }
    }

    // Now make a copy and add in new extensions.
    let mut extensions = unsafe { ORIGINAL_BUILTIN_EXTENSIONS.as_ref().unwrap().clone() };

    if config.oxidized_importer {
        let ptr = PyInit_oxidized_importer as *const ();
        extensions.push(oldpyffi::_inittab {
            name: OXIDIZED_IMPORTER_NAME.as_ptr() as *mut _,
            initfunc: Some(unsafe { std::mem::transmute::<*const (), extern "C" fn()>(ptr) }),
        });
    }

    // Add additional extension modules from the config.
    if let Some(extra_extension_modules) = &config.extra_extension_modules {
        for extension in extra_extension_modules {
            let ptr = extension.init_func as *const ();
            extensions.push(oldpyffi::_inittab {
                name: extension.name.as_ptr() as *mut _,
                initfunc: Some(unsafe { std::mem::transmute::<*const (), extern "C" fn()>(ptr) }),
            });
        }
    }

    // Add sentinel record with NULLs.
    extensions.push(oldpyffi::_inittab {
        name: std::ptr::null_mut(),
        initfunc: None,
    });

    // And finally replace the static in Python's code with our instance.
    unsafe {
        REPLACED_BUILTIN_EXTENSIONS = Some(extensions);
        oldpyffi::PyImport_Inittab = REPLACED_BUILTIN_EXTENSIONS.as_mut().unwrap().as_mut_ptr();
    }
}

/// Write loaded Python modules to a directory.
///
/// Given a Python interpreter and a path to a directory, this will create a
/// file in that directory named ``modules-<UUID>`` and write a ``\n`` delimited
/// list of loaded names from ``sys.modules`` into that file.
fn write_modules_to_path(py: Python, path: &Path) -> Result<(), &'static str> {
    // TODO this needs better error handling all over.

    let sys = py
        .import("sys")
        .map_err(|_| "could not obtain sys module")?;
    let modules = sys
        .get(py, "modules")
        .map_err(|_| "could not obtain sys.modules")?;

    let modules = modules
        .cast_as::<PyDict>(py)
        .map_err(|_| "sys.modules is not a dict")?;

    let mut names = BTreeSet::new();
    for (key, _value) in modules.items(py) {
        names.insert(
            key.extract::<String>(py)
                .map_err(|_| "module name is not a str")?,
        );
    }

    let mut f = fs::File::create(path).map_err(|_| "could not open file for writing")?;

    for name in names {
        f.write_fmt(format_args!("{}\n", name))
            .map_err(|_| "could not write")?;
    }

    Ok(())
}

impl<'python, 'interpreter, 'resources> Drop
    for MainPythonInterpreter<'python, 'interpreter, 'resources>
{
    fn drop(&mut self) {
        if let Some(path) = self.write_modules_path.clone() {
            if let Err(msg) = write_modules_to_path(self.acquire_gil(), &path) {
                eprintln!("error writing modules file: {}", msg);
            }
        }

        let _ = unsafe { oldpyffi::Py_FinalizeEx() };
    }
}
