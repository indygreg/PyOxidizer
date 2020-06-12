// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Manage an embedded Python interpreter.

use {
    super::config::{MemoryAllocatorBackend, OxidizedPythonInterpreterConfig, TerminfoResolution},
    super::conversion::{osstr_to_pyobject, osstring_to_bytes},
    super::importer::{
        initialize_importer, PyInit_oxidized_importer, OXIDIZED_IMPORTER_NAME,
        OXIDIZED_IMPORTER_NAME_STR,
    },
    super::osutils::resolve_terminfo_dirs,
    super::pyalloc::{make_raw_rust_memory_allocator, RawAllocator},
    super::python_resources::PythonResourcesState,
    cpython::{
        GILGuard, NoArgs, ObjectProtocol, PyDict, PyErr, PyList, PyObject, PyString, Python,
        ToPyObject,
    },
    lazy_static::lazy_static,
    python3_sys as pyffi,
    std::collections::BTreeSet,
    std::convert::TryInto,
    std::env,
    std::ffi::CStr,
    std::fmt::{Display, Formatter},
    std::fs,
    std::io::Write,
    std::path::PathBuf,
};

#[cfg(feature = "jemalloc-sys")]
use super::pyalloc::make_raw_jemalloc_allocator;
use python3_sys::PyMemAllocatorEx;

lazy_static! {
    static ref GLOBAL_INTERPRETER_GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());
}

#[cfg(feature = "jemalloc-sys")]
fn raw_jemallocator() -> pyffi::PyMemAllocatorEx {
    make_raw_jemalloc_allocator()
}

#[cfg(not(feature = "jemalloc-sys"))]
fn raw_jemallocator() -> pyffi::PyMemAllocatorEx {
    panic!("jemalloc is not available in this build configuration");
}

/// Format a PyErr in a crude manner.
///
/// This is meant to be called during interpreter initialization. We can't
/// call PyErr_Print() because sys.stdout may not be available yet.
fn format_pyerr(py: Python, err: PyErr) -> Result<String, &'static str> {
    let type_repr = err
        .ptype
        .repr(py)
        .or_else(|_| Err("unable to get repr of error type"))?;

    if let Some(value) = &err.pvalue {
        let value_repr = value
            .repr(py)
            .or_else(|_| Err("unable to get repr of error value"))?;

        let value = format!(
            "{}: {}",
            type_repr.to_string_lossy(py),
            value_repr.to_string_lossy(py)
        );

        Ok(value)
    } else {
        Ok(type_repr.to_string_lossy(py).to_string())
    }
}

/// Represents an error encountered when creating an embedded Python interpreter.
#[derive(Debug)]
pub enum NewInterpreterError {
    Simple(&'static str),
    Dynamic(String),
}

impl From<&'static str> for NewInterpreterError {
    fn from(v: &'static str) -> Self {
        NewInterpreterError::Simple(v)
    }
}

impl Display for NewInterpreterError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match &self {
            NewInterpreterError::Simple(value) => value.fmt(f),
            NewInterpreterError::Dynamic(value) => value.fmt(f),
        }
    }
}

impl std::error::Error for NewInterpreterError {}

impl NewInterpreterError {
    pub fn new_from_pyerr(py: Python, err: PyErr, context: &str) -> Self {
        match format_pyerr(py, err) {
            Ok(value) => NewInterpreterError::Dynamic(format!("during {}: {}", context, value)),
            Err(msg) => NewInterpreterError::Dynamic(format!("during {}: {}", context, msg)),
        }
    }

    pub fn new_from_pystatus(status: &pyffi::PyStatus, context: &str) -> Self {
        if !status.func.is_null() && !status.err_msg.is_null() {
            let func = unsafe { CStr::from_ptr(status.func) };
            let msg = unsafe { CStr::from_ptr(status.err_msg) };

            NewInterpreterError::Dynamic(format!(
                "during {}: {}: {}",
                context,
                func.to_string_lossy(),
                msg.to_string_lossy()
            ))
        } else if !status.err_msg.is_null() {
            let msg = unsafe { CStr::from_ptr(status.err_msg) };

            NewInterpreterError::Dynamic(format!("during {}: {}", context, msg.to_string_lossy()))
        } else {
            NewInterpreterError::Dynamic(format!("during {}: could not format PyStatus", context))
        }
    }
}

enum InterpreterRawAllocator {
    Python(pyffi::PyMemAllocatorEx),
    Raw(RawAllocator),
}

impl InterpreterRawAllocator {
    fn as_ptr(&self) -> *const pyffi::PyMemAllocatorEx {
        match self {
            InterpreterRawAllocator::Python(alloc) => alloc as *const _,
            InterpreterRawAllocator::Raw(alloc) => &alloc.allocator as *const _,
        }
    }
}

impl From<pyffi::PyMemAllocatorEx> for InterpreterRawAllocator {
    fn from(allocator: PyMemAllocatorEx) -> Self {
        InterpreterRawAllocator::Python(allocator)
    }
}

impl From<RawAllocator> for InterpreterRawAllocator {
    fn from(allocator: RawAllocator) -> Self {
        InterpreterRawAllocator::Raw(allocator)
    }
}

#[derive(Debug, PartialEq)]
enum InterpreterState {
    NotStarted,
    Initializing,
    Initialized,
    Finalized,
}

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
    config: OxidizedPythonInterpreterConfig<'resources>,
    interpreter_state: InterpreterState,
    interpreter_guard: Option<std::sync::MutexGuard<'interpreter, ()>>,
    raw_allocator: Option<InterpreterRawAllocator>,
    gil: Option<GILGuard>,
    py: Option<Python<'python>>,
    /// Holds parsed resources state.
    ///
    /// The underling data backing this data structure is given an
    /// explicit lifetime, independent of the GIL. The lifetime should be
    /// that of this instance and no shorter.
    ///
    /// While this type doesn't access this field for any meaningful
    /// work, we need to hold on to a reference to the parsed resources
    /// data/state because the importer is storing a pointer to it. The
    /// reason it is storing a pointer and not a normal &ref is because
    /// the cpython bindings require that all class data elements be
    /// 'static. If we stored the PythonResourcesState as a normal Rust
    /// ref, we would require it be 'static. In reality, resources only
    /// need to live for the lifetime of the interpreter instance, which
    /// is shorter than 'static. So we cheat and store a pointer. And to
    /// ensure the memory behind that pointer isn't freed, we track it
    /// in this field. We also store the object in a box so it is on the
    /// heap and not dynamic.
    resources_state: Option<Box<PythonResourcesState<'resources, u8>>>,
}

impl<'python, 'interpreter, 'resources> MainPythonInterpreter<'python, 'interpreter, 'resources> {
    /// Construct a Python interpreter from a configuration.
    ///
    /// The Python interpreter is initialized as a side-effect. The GIL is held.
    pub fn new(
        config: OxidizedPythonInterpreterConfig<'resources>,
    ) -> Result<MainPythonInterpreter<'python, 'interpreter, 'resources>, NewInterpreterError> {
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
            interpreter_state: InterpreterState::NotStarted,
            raw_allocator: None,
            gil: None,
            py: None,
            resources_state: None,
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
        match &self.interpreter_state {
            InterpreterState::Initializing => {
                return Err(NewInterpreterError::Simple(
                    "interpreter in initializing state",
                ))
            }
            InterpreterState::Initialized => {
                return Ok(());
            }
            InterpreterState::NotStarted => {}
            InterpreterState::Finalized => {}
        }

        assert!(self.interpreter_guard.is_none());
        self.interpreter_guard = Some(GLOBAL_INTERPRETER_GUARD.lock().or_else(|_| {
            Err(NewInterpreterError::Simple(
                "unable to acquire global interpreter guard",
            ))
        })?);

        self.interpreter_state = InterpreterState::Initializing;

        let exe = env::current_exe()
            .or_else(|_| Err(NewInterpreterError::Simple("could not obtain current exe")))?;
        let origin = exe
            .parent()
            .ok_or_else(|| NewInterpreterError::Simple("unable to get exe parent"))?
            .to_path_buf();
        let origin_string = origin.display().to_string();

        set_pyimport_inittab(&self.config);

        // Pre-configure Python.
        let pre_config: pyffi::PyPreConfig = (&self.config.interpreter_config)
            .try_into()
            .or_else(|err| Err(NewInterpreterError::Dynamic(err)))?;

        unsafe {
            let status = pyffi::Py_PreInitialize(&pre_config);

            if pyffi::PyStatus_Exception(status) != 0 {
                return Err(NewInterpreterError::new_from_pystatus(
                    &status,
                    "Python pre-initialization",
                ));
            }
        };

        // Override the raw allocator if one is configured.
        if let Some(raw_allocator) = &self.config.raw_allocator {
            match raw_allocator.backend {
                MemoryAllocatorBackend::System => {}
                MemoryAllocatorBackend::Jemalloc => {
                    self.raw_allocator = Some(InterpreterRawAllocator::from(raw_jemallocator()));
                }
                MemoryAllocatorBackend::Rust => {
                    self.raw_allocator = Some(InterpreterRawAllocator::from(
                        make_raw_rust_memory_allocator(),
                    ));
                }
            }

            if let Some(allocator) = &self.raw_allocator {
                unsafe {
                    pyffi::PyMem_SetAllocator(
                        pyffi::PyMemAllocatorDomain::PYMEM_DOMAIN_RAW,
                        allocator.as_ptr() as *mut _,
                    );
                }
            }

            if raw_allocator.debug {
                unsafe {
                    pyffi::PyMem_SetupDebugHooks();
                }
            }
        }

        let mut py_config: pyffi::PyConfig = (&self.config)
            .try_into()
            .or_else(|err| Err(NewInterpreterError::Dynamic(err)))?;

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

        if self.config.oxidized_importer {
            self.resources_state = Some(Box::new(
                PythonResourcesState::new_from_env()
                    .or_else(|err| Err(NewInterpreterError::Simple(err)))?,
            ));

            if let Some(ref mut resources_state) = self.resources_state {
                resources_state
                    .load(self.config.packed_resources)
                    .or_else(|err| Err(NewInterpreterError::Simple(err)))?;

                let oxidized_importer = py.import(OXIDIZED_IMPORTER_NAME_STR).or_else(|err| {
                    Err(NewInterpreterError::new_from_pyerr(
                        py,
                        err,
                        "import of oxidized importer module",
                    ))
                })?;

                initialize_importer(py, &oxidized_importer, resources_state).or_else(|err| {
                    Err(NewInterpreterError::new_from_pyerr(
                        py,
                        err,
                        "initialization of oxidized importer",
                    ))
                })?;
            }
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
            let sys_module = py.import("sys").or_else(|err| {
                Err(NewInterpreterError::new_from_pyerr(
                    py,
                    err,
                    "obtaining sys module",
                ))
            })?;
            let meta_path = sys_module.get(py, "meta_path").or_else(|err| {
                Err(NewInterpreterError::new_from_pyerr(
                    py,
                    err,
                    "obtaining sys.meta_path",
                ))
            })?;
            meta_path
                .call_method(py, "pop", NoArgs, None)
                .or_else(|err| {
                    Err(NewInterpreterError::new_from_pyerr(
                        py,
                        err,
                        "sys.meta_path.pop()",
                    ))
                })?;
        }

        /* Pre-initialization functions we could support:
         *
         * PyObject_SetArenaAllocator()
         */

        self.py = Some(py);
        self.interpreter_state = InterpreterState::Initialized;

        // env::args() panics if arguments aren't valid Unicode. But invalid
        // Unicode arguments are possible and some applications may want to
        // support them.
        //
        // env::args_os() provides access to the raw OsString instances, which
        // will be derived from wchar_t on Windows and char* on POSIX. We can
        // convert these to Python str instances using a platform-specific
        // mechanism.
        let args_objs = env::args_os()
            .map(|os_arg| osstr_to_pyobject(py, &os_arg, None))
            .collect::<Result<Vec<PyObject>, &'static str>>()?;

        // This will steal the pointer to the elements and mem::forget them.
        let args = PyList::new(py, &args_objs);
        let argv = b"argv\0";

        let res = args.with_borrowed_ptr(py, |args_ptr| unsafe {
            pyffi::PySys_SetObject(argv.as_ptr() as *const i8, args_ptr)
        });

        match res {
            0 => (),
            _ => return Err(NewInterpreterError::Simple("unable to set sys.argv")),
        }

        if self.config.argvb {
            let args_objs: Vec<PyObject> = env::args_os()
                .map(|os_arg| osstring_to_bytes(py, os_arg))
                .collect();

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

        Ok(())
    }

    /// Ensure the Python GIL is released.
    pub fn release_gil(&mut self) {
        if self.py.is_some() {
            self.py = None;
            self.gil = None;
        }
    }

    /// Ensure the Python GIL is acquired, returning a handle on the interpreter.
    pub fn acquire_gil(&mut self) -> Result<Python<'python>, &'static str> {
        match self.interpreter_state {
            InterpreterState::NotStarted => {
                return Err("interpreter not initialized");
            }
            InterpreterState::Initializing => {
                return Err("interpreter not fully initialized");
            }
            InterpreterState::Initialized => {}
            InterpreterState::Finalized => {
                return Err("interpreter is finalized");
            }
        }

        Ok(match self.py {
            Some(py) => py,
            None => {
                let gil = GILGuard::acquire();
                let py = unsafe { Python::assume_gil_acquired() };

                self.gil = Some(gil);
                self.py = Some(py);

                py
            }
        })
    }

    /// Runs the Python interpreter in the context of a main() function.
    ///
    /// This will execute whatever is configured by
    /// `OxidizedPythonInterpreterConfig.run` and return an integer suitable
    /// for use as a process exit code.
    ///
    /// The `PythonRunMode::Eval`, `PythonRunMode::File`, and
    /// `PythonRunMode::Module`, and `PythonRunMode::Repl` run modes are
    /// evaluated via `Py_RunMain()`. `PythonRunMode::None` simply returns 0.
    ///
    /// `Py_RunMain` is the most robust mechanism to run code, files, or
    /// modules, as `Py_RunMain()` invokes the same APIs that `python` would.
    /// By contrast, the `run()`, `run_module_as_main()`, `run_code()`,
    /// `run_file()`, and `run_repl()` functions in the `python_eval` module
    /// reimplement functionality and may behave subtly different from what
    /// `python` would do. If you want the evaluation to behave like `python`,
    /// you should call this function.
    ///
    /// A downside to calling this function is that `Py_RunMain()` will finalize
    /// the interpreter and only gives you an exit code: there is no opportunity
    /// to inspect the return value or handle an uncaught exception. If you want
    /// to keep the interpreter alive or inspect the evaluation result, consider
    /// calling a function in the `python_eval` module.
    pub fn run_as_main(&mut self) -> i32 {
        if self.config.uses_py_runmain() {
            let res = unsafe { pyffi::Py_RunMain() };

            // Py_RunMain() finalizes the interpreter. So drop our refs and state.
            self.interpreter_guard = None;
            self.interpreter_state = InterpreterState::Finalized;
            self.resources_state = None;
            self.py = None;
            self.gil = None;

            res
        } else {
            0
        }
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
fn write_modules_to_directory(py: Python, path: &PathBuf) -> Result<(), &'static str> {
    // TODO this needs better error handling all over.

    fs::create_dir_all(path).or_else(|_| Err("could not create directory for modules"))?;

    let rand = uuid::Uuid::new_v4();

    let path = path.join(format!("modules-{}", rand.to_string()));

    let sys = py
        .import("sys")
        .or_else(|_| Err("could not obtain sys module"))?;
    let modules = sys
        .get(py, "modules")
        .or_else(|_| Err("could not obtain sys.modules"))?;

    let modules = modules
        .cast_as::<PyDict>(py)
        .or_else(|_| Err("sys.modules is not a dict"))?;

    let mut names = BTreeSet::new();
    for (key, _value) in modules.items(py) {
        names.insert(
            key.extract::<String>(py)
                .or_else(|_| Err("module name is not a str"))?,
        );
    }

    let mut f = fs::File::create(path).or_else(|_| Err("could not open file for writing"))?;

    for name in names {
        f.write_fmt(format_args!("{}\n", name))
            .or_else(|_| Err("could not write"))?;
    }

    Ok(())
}

impl<'python, 'interpreter, 'resources> Drop
    for MainPythonInterpreter<'python, 'interpreter, 'resources>
{
    fn drop(&mut self) {
        if let Some(key) = &self.config.write_modules_directory_env {
            if let Ok(path) = env::var(key) {
                let path = PathBuf::from(path);
                let py = self.acquire_gil().unwrap();

                if let Err(msg) = write_modules_to_directory(py, &path) {
                    eprintln!("error writing modules file: {}", msg);
                }
            }
        }

        let _ = unsafe { pyffi::Py_FinalizeEx() };
    }
}
