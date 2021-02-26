// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Manage an embedded Python interpreter.

use {
    crate::{
        config::{OxidizedPythonInterpreterConfig, ResolvedOxidizedPythonInterpreterConfig},
        conversion::osstring_to_bytes,
        error::NewInterpreterError,
        importer::{
            replace_meta_path_importers, PyInit_oxidized_importer, OXIDIZED_IMPORTER_NAME,
            OXIDIZED_IMPORTER_NAME_STR,
        },
        osutils::resolve_terminfo_dirs,
        pyalloc::PythonMemoryAllocator,
        python_resources::PythonResourcesState,
    },
    cpython::{GILGuard, NoArgs, ObjectProtocol, PyDict, PyList, PyString, Python, ToPyObject},
    once_cell::sync::Lazy,
    python3_sys as pyffi,
    python_packaging::interpreter::TerminfoResolution,
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
    gil: Option<GILGuard>,
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
            gil: None,
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
        let pre_config = pyffi::PyPreConfig::try_from(&self.config)?;

        unsafe {
            let status = pyffi::Py_PreInitialize(&pre_config);

            if pyffi::PyStatus_Exception(status) != 0 {
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
                allocator.set_allocator(pyffi::PyMemAllocatorDomain::PYMEM_DOMAIN_RAW);
            }

            if self.config.allocator_mem {
                allocator.set_allocator(pyffi::PyMemAllocatorDomain::PYMEM_DOMAIN_MEM);
            }

            if self.config.allocator_obj {
                allocator.set_allocator(pyffi::PyMemAllocatorDomain::PYMEM_DOMAIN_OBJ);
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
                pyffi::PyMem_SetupDebugHooks();
            }
        }

        let mut py_config: pyffi::PyConfig = (&self.config).try_into()?;

        // Enable multi-phase initialization. This allows us to initialize
        // our custom importer before Python attempts any imports.
        py_config._init_main = 0;

        let status = unsafe { pyffi::Py_InitializeFromConfig(&py_config) };
        if unsafe { pyffi::PyStatus_Exception(status) } != 0 {
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

        if self.config.oxidized_importer {
            let resources_state = Box::new(PythonResourcesState::try_from(&self.config)?);

            let oxidized_importer = py.import(OXIDIZED_IMPORTER_NAME_STR).map_err(|err| {
                NewInterpreterError::new_from_pyerr(py, err, "import of oxidized importer module")
            })?;

            // Ownership of the resources state is transferred into the importer, where the Box
            // is summarily leaked. However, the importer tracks a pointer to the resources state
            // and will constitute the struct for dropping when it itself is dropped. We could
            // potentially encounter a use-after-free if the importer is used after self.config
            // is dropped. However, that would require self to be dropped. And if self is dropped,
            // there should no longer be a Python interpreter around. So it follows that the
            // importer state cannot be dropped after self.
            replace_meta_path_importers(py, &oxidized_importer, resources_state).map_err(
                |err| {
                    NewInterpreterError::new_from_pyerr(
                        py,
                        err,
                        "initialization of oxidized importer",
                    )
                },
            )?;
        }

        // Now proceed with the Python main initialization. This will initialize
        // importlib. And if the custom importlib bytecode was registered above,
        // our extension module will get imported and initialized.
        let status = unsafe { pyffi::_Py_InitializeMain() };

        if unsafe { pyffi::PyStatus_Exception(status) } != 0 {
            return Err(NewInterpreterError::new_from_pystatus(
                &status,
                "initializing Python main",
            ));
        }

        // When the main initialization ran, it initialized the "external"
        // importer (importlib._bootstrap_external). Our meta path importer
        // should have been registered first and would have been used for
        // all imports, if configured for such.
        //
        // Here, we remove the filesystem importer if we aren't configured
        // to use it. Ideally there would be a field on PyConfig to disable
        // just the external importer. But there isn't. The only field
        // controls both internal and external bootstrap modules and when
        // set it will disable a lot of "main" initialization.
        if !self.config.filesystem_importer {
            let sys_module = py.import("sys").map_err(|err| {
                NewInterpreterError::new_from_pyerr(py, err, "obtaining sys module")
            })?;
            let meta_path = sys_module.get(py, "meta_path").map_err(|err| {
                NewInterpreterError::new_from_pyerr(py, err, "obtaining sys.meta_path")
            })?;
            meta_path
                .call_method(py, "pop", NoArgs, None)
                .map_err(|err| {
                    NewInterpreterError::new_from_pyerr(py, err, "sys.meta_path.pop()")
                })?;
        }

        /* Pre-initialization functions we could support:
         *
         * PyObject_SetArenaAllocator()
         */

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
                pyffi::PySys_SetObject(argvb.as_ptr() as *const i8, args_ptr)
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
            pyffi::PySys_SetObject(oxidized.as_ptr() as *const i8, py_true)
        });

        match res {
            0 => (),
            _ => return Err(NewInterpreterError::Simple("unable to set sys.oxidized")),
        }

        if self.config.sys_frozen {
            let frozen = b"frozen\0";

            match py.True().with_borrowed_ptr(py, |py_true| unsafe {
                pyffi::PySys_SetObject(frozen.as_ptr() as *const i8, py_true)
            }) {
                0 => (),
                _ => return Err(NewInterpreterError::Simple("unable to set sys.frozen")),
            }
        }

        if self.config.sys_meipass {
            let meipass = b"_MEIPASS\0";
            let value = PyString::new(py, &origin_string);

            match value.with_borrowed_ptr(py, |py_value| unsafe {
                pyffi::PySys_SetObject(meipass.as_ptr() as *const i8, py_value)
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

                self.write_modules_path =
                    Some(PathBuf::from(path).join(format!("modules-{}", uuid_str)));
            }
        }

        Ok(())
    }

    /// Ensure the Python GIL is released.
    pub fn release_gil(&mut self) {
        self.py = None;
        self.gil = None;
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

                self.gil = Some(gil);
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
        unsafe { pyffi::Py_RunMain() }
    }
}

static mut ORIGINAL_BUILTIN_EXTENSIONS: Option<Vec<pyffi::_inittab>> = None;
static mut REPLACED_BUILTIN_EXTENSIONS: Option<Box<Vec<pyffi::_inittab>>> = None;

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
            let mut entries: Vec<pyffi::_inittab> = Vec::new();

            for i in 0.. {
                let record = pyffi::PyImport_Inittab.offset(i);

                if (*record).name.is_null() {
                    break;
                }

                entries.push(*record);
            }

            ORIGINAL_BUILTIN_EXTENSIONS = Some(entries);
        }
    }

    // Now make a copy and add in new extensions.
    let mut extensions = Box::new(unsafe { ORIGINAL_BUILTIN_EXTENSIONS.as_ref().unwrap().clone() });

    if config.oxidized_importer {
        let ptr = PyInit_oxidized_importer as *const ();
        extensions.push(pyffi::_inittab {
            name: OXIDIZED_IMPORTER_NAME.as_ptr() as *mut _,
            initfunc: Some(unsafe { std::mem::transmute::<*const (), extern "C" fn()>(ptr) }),
        });
    }

    // Add additional extension modules from the config.
    if let Some(extra_extension_modules) = &config.extra_extension_modules {
        for extension in extra_extension_modules {
            let ptr = extension.init_func as *const ();
            extensions.push(pyffi::_inittab {
                name: extension.name.as_ptr() as *mut _,
                initfunc: Some(unsafe { std::mem::transmute::<*const (), extern "C" fn()>(ptr) }),
            });
        }
    }

    // Add sentinel record with NULLs.
    extensions.push(pyffi::_inittab {
        name: std::ptr::null_mut(),
        initfunc: None,
    });

    // And finally replace the static in Python's code with our instance.
    unsafe {
        REPLACED_BUILTIN_EXTENSIONS = Some(extensions);
        pyffi::PyImport_Inittab = REPLACED_BUILTIN_EXTENSIONS.as_mut().unwrap().as_mut_ptr();
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

        let _ = unsafe { pyffi::Py_FinalizeEx() };
    }
}
