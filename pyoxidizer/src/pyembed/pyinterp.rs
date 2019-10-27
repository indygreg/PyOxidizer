// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Manage an embedded Python interpreter.

use libc::c_char;
use python3_sys as pyffi;
use std::collections::BTreeSet;
use std::env;
use std::ffi::CString;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::ptr::null;

use cpython::exc::ValueError;
use cpython::{
    GILGuard, NoArgs, ObjectProtocol, PyClone, PyDict, PyErr, PyList, PyModule, PyObject, PyResult,
    PyString, Python, PythonObject, ToPyObject,
};

use super::config::{PythonConfig, PythonRawAllocator, PythonRunMode, TerminfoResolution};
use super::importer::PyInit__pyoxidizer_importer;
use super::osutils::resolve_terminfo_dirs;
#[cfg(feature = "jemalloc-sys")]
use super::pyalloc::make_raw_jemalloc_allocator;
use super::pyalloc::{make_raw_rust_memory_allocator, RawAllocator};
use super::pystr::{osstring_to_bytes, osstring_to_str, OwnedPyStr};

pub const PYOXIDIZER_IMPORTER_NAME: &[u8] = b"_pyoxidizer_importer\0";

const FROZEN_IMPORTLIB_NAME: &[u8] = b"_frozen_importlib\0";
const FROZEN_IMPORTLIB_EXTERNAL_NAME: &[u8] = b"_frozen_importlib_external\0";

/// Represents the results of executing Python code with exception handling.
#[derive(Debug)]
pub enum PythonRunResult {
    /// Code executed without raising an exception.
    Ok {},
    /// Code executed and raised an exception.
    Err {},
    /// Code executed and raised SystemExit with the specified exit code.
    Exit { code: i32 },
}

fn make_custom_frozen_modules(config: &PythonConfig) -> [pyffi::_frozen; 3] {
    [
        pyffi::_frozen {
            name: FROZEN_IMPORTLIB_NAME.as_ptr() as *const i8,
            code: config.frozen_importlib_data.as_ptr(),
            size: config.frozen_importlib_data.len() as i32,
        },
        pyffi::_frozen {
            name: FROZEN_IMPORTLIB_EXTERNAL_NAME.as_ptr() as *const i8,
            code: config.frozen_importlib_external_data.as_ptr(),
            size: config.frozen_importlib_external_data.len() as i32,
        },
        pyffi::_frozen {
            name: null(),
            code: null(),
            size: 0,
        },
    ]
}

#[cfg(windows)]
extern "C" {
    pub fn __acrt_iob_func(x: u32) -> *mut libc::FILE;
}

#[cfg(windows)]
fn stdin_to_file() -> *mut libc::FILE {
    // The stdin symbol is made available by importing <stdio.h>. On Windows,
    // stdin is defined in corecrt_wstdio.h as a `#define` that calls this
    // internal CRT function. There's no exported symbol to use. So we
    // emulate the behavior of the C code.
    //
    // Relying on an internal CRT symbol is probably wrong. But Microsoft
    // typically keeps backwards compatibility for undocumented functions
    // like this because people use them in the wild.
    //
    // An attempt was made to use fdopen(0) like we do on POSIX. However,
    // this causes a crash. The Microsoft C Runtime is already bending over
    // backwards to coerce its native HANDLEs into POSIX file descriptors.
    // Even if there are other ways to coerce a FILE* from a HANDLE
    // (_open_osfhandle() + _fdopen() might work), using the same function
    // that <stdio.h> uses to obtain a FILE* seems like the least risky thing
    // to do.
    unsafe { __acrt_iob_func(0) }
}

#[cfg(unix)]
fn stdin_to_file() -> *mut libc::FILE {
    unsafe { libc::fdopen(libc::STDIN_FILENO, &('r' as libc::c_char)) }
}

#[cfg(windows)]
fn stderr_to_file() -> *mut libc::FILE {
    unsafe { __acrt_iob_func(2) }
}

#[cfg(unix)]
fn stderr_to_file() -> *mut libc::FILE {
    unsafe { libc::fdopen(libc::STDERR_FILENO, &('w' as libc::c_char)) }
}

#[cfg(feature = "jemalloc-sys")]
fn raw_jemallocator() -> pyffi::PyMemAllocatorEx {
    make_raw_jemalloc_allocator()
}

#[cfg(not(feature = "jemalloc-sys"))]
fn raw_jemallocator() -> pyffi::PyMemAllocatorEx {
    panic!("jemalloc is not available in this build configuration");
}

/// Manages an embedded Python interpreter.
///
/// **Warning: Python interpreters have global state. There should only be a
/// single instance of this type per process.**
///
/// Instances must only be constructed through [`MainPythonInterpreter::new()`](#method.new).
///
/// This type and its various functionality is a glorified wrapper around the
/// Python C API. But there's a lot of added functionality on top of what the C
/// API provides.
///
/// Both the low-level `python3-sys` and higher-level `cpython` crates are used.
pub struct MainPythonInterpreter<'a> {
    pub config: PythonConfig,
    frozen_modules: [pyffi::_frozen; 3],
    init_run: bool,
    raw_allocator: Option<pyffi::PyMemAllocatorEx>,
    raw_rust_allocator: Option<RawAllocator>,
    gil: Option<GILGuard>,
    py: Option<Python<'a>>,
    program_name: Option<OwnedPyStr>,
}

impl<'a> MainPythonInterpreter<'a> {
    /// Construct a Python interpreter from a configuration.
    ///
    /// The Python interpreter is initialized as a side-effect. The GIL is held.
    pub fn new(config: PythonConfig) -> Result<MainPythonInterpreter<'a>, &'static str> {
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

        let (raw_allocator, raw_rust_allocator) = match config.raw_allocator {
            PythonRawAllocator::Jemalloc => (Some(raw_jemallocator()), None),
            PythonRawAllocator::Rust => (None, Some(make_raw_rust_memory_allocator())),
            PythonRawAllocator::System => (None, None),
        };

        let frozen_modules = make_custom_frozen_modules(&config);

        let mut res = MainPythonInterpreter {
            config,
            frozen_modules,
            init_run: false,
            raw_allocator,
            raw_rust_allocator,
            gil: None,
            py: None,
            program_name: None,
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
    fn init(&mut self) -> Result<Python, &'static str> {
        if self.init_run {
            return Ok(self.acquire_gil());
        }

        let config = &self.config;

        let exe = env::current_exe().or_else(|_| Err("could not obtain current exe"))?;
        let origin = exe
            .parent()
            .ok_or_else(|| "unable to get exe parent")?
            .display()
            .to_string();

        let sys_paths: Vec<String> = config
            .sys_paths
            .iter()
            .map(|path| path.replace("$ORIGIN", &origin))
            .collect();

        // TODO should we call PyMem::SetupDebugHooks() if enabled?
        if let Some(raw_allocator) = &self.raw_allocator {
            unsafe {
                let ptr = raw_allocator as *const _;
                pyffi::PyMem_SetAllocator(
                    pyffi::PyMemAllocatorDomain::PYMEM_DOMAIN_RAW,
                    ptr as *mut _,
                );
            }
        } else if let Some(raw_rust_allocator) = &self.raw_rust_allocator {
            unsafe {
                let ptr = &raw_rust_allocator.allocator as *const _;
                pyffi::PyMem_SetAllocator(
                    pyffi::PyMemAllocatorDomain::PYMEM_DOMAIN_RAW,
                    ptr as *mut _,
                );
            }
        }

        // Module state is a bit wonky.
        //
        // Our in-memory importer relies on a special module which holds references
        // to Python objects exposing module/resource data. This module is imported as
        // part of initializing the Python interpreter.
        //
        // This Python module object needs to hold references to the raw Python module
        // and resource data. Those references are defined by the InitModuleState struct.
        //
        // Unfortunately, we can't easily associate state with the interpreter before
        // calling Py_Initialize(). And the module initialization function receives no
        // arguments. Our solution is to update a global pointer to point at "our" state
        // then call Py_Initialize(). The module will be initialized as part of calling
        // Py_Initialize(). It will copy the contents at the pointer into the local
        // module state and the global pointer will be unused after that. The end result
        // is that we have no reliance on global variables outside of a short window
        // between now and when Py_Initialize() is called.
        //
        // We could potentially do away with this global variable by using a closure for
        // the initialization function. But this rabbit hole may involve gross hackery
        // like dynamic module names. It probably isn't worth it.

        // It is important for references in this struct to have a lifetime of at least
        // that of the interpreter.
        // TODO specify lifetimes so the compiler validates this for us.
        let module_state = super::importer::InitModuleState {
            register_filesystem_importer: self.config.filesystem_importer,
            sys_paths,
            py_modules_data: config.py_modules_data,
            py_resources_data: config.py_resources_data,
        };

        if config.use_custom_importlib {
            // Replace the frozen modules in the interpreter with our custom set
            // that knows how to import from memory.
            unsafe {
                pyffi::PyImport_FrozenModules = self.frozen_modules.as_ptr();
            }

            // Register our _pyoxidizer_importer extension which provides importing functionality.
            unsafe {
                // name char* needs to live as long as the interpreter is active.
                pyffi::PyImport_AppendInittab(
                    PYOXIDIZER_IMPORTER_NAME.as_ptr() as *const i8,
                    Some(PyInit__pyoxidizer_importer),
                );

                // Move pointer to our stack allocated instance. This pointer will be
                // accessed when creating the Python module object, which should be
                // done automatically as part of low-level interpreter initialization
                // when calling Py_Initialize() below.
                super::importer::NEXT_MODULE_STATE = &module_state;
            }
        }

        // TODO call PyImport_ExtendInitTab to avoid O(n) overhead.
        for e in &config.extra_extension_modules {
            let res = unsafe {
                pyffi::PyImport_AppendInittab(e.name.as_ptr() as *const i8, Some(e.init_func))
            };

            if res != 0 {
                return Err("unable to register extension module");
            }
        }

        let exe_str = exe.to_str().ok_or_else(|| "unable to convert exe to str")?;

        let home = OwnedPyStr::from_str(exe_str)?;

        unsafe {
            // Pointer needs to live for lifetime of interpreter.
            pyffi::Py_SetPythonHome(home.as_wchar_ptr());
        }

        let program_name = OwnedPyStr::from_str(exe_str)?;

        unsafe {
            pyffi::Py_SetProgramName(program_name.as_wchar_ptr());
        }

        // Value needs to live for lifetime of interpreter.
        self.program_name = Some(program_name);

        // If we don't call Py_SetPath(), Python has its own logic for initializing it.
        // We set it to an empty string because we don't want any paths by default. If
        // we do have defined paths, they will be set after Py_Initialize().
        unsafe {
            // Value is copied internally. So short lifetime is OK.
            let value = OwnedPyStr::from_str("")?;
            pyffi::Py_SetPath(value.as_wchar_ptr());
        }

        if let (Some(ref encoding), Some(ref errors)) =
            (&config.standard_io_encoding, &config.standard_io_errors)
        {
            let cencoding = CString::new(encoding.clone())
                .or_else(|_| Err("unable to convert encoding to C string"))?;
            let cerrors = CString::new(errors.clone())
                .or_else(|_| Err("unable to convert encoding error mode to C string"))?;

            let res = unsafe {
                pyffi::Py_SetStandardStreamEncoding(
                    cencoding.as_ptr() as *const i8,
                    cerrors.as_ptr() as *const i8,
                )
            };

            if res != 0 {
                return Err("unable to set standard stream encoding");
            }
        }

        unsafe {
            pyffi::Py_DontWriteBytecodeFlag = if config.dont_write_bytecode { 1 } else { 0 };
            pyffi::Py_IgnoreEnvironmentFlag = if config.ignore_python_env { 1 } else { 0 };
            pyffi::Py_NoSiteFlag = if config.import_site { 0 } else { 1 };
            pyffi::Py_NoUserSiteDirectory = if config.import_user_site { 0 } else { 1 };
            pyffi::Py_OptimizeFlag = config.opt_level;
            pyffi::Py_UnbufferedStdioFlag = if config.unbuffered_stdio { 1 } else { 0 };
        }

        /* Pre-initialization functions we could support:
         *
         * PyObject_SetArenaAllocator()
         * PySys_AddWarnOption()
         * PySys_AddXOption()
         * PySys_ResetWarnOptions()
         */

        unsafe {
            pyffi::Py_Initialize();
        }

        // We shouldn't be accessing this pointer after Py_Initialize(). And the
        // memory is stack allocated and doesn't outlive this frame. We don't want
        // to leave a stack pointer sitting around!
        unsafe {
            super::importer::NEXT_MODULE_STATE = std::ptr::null();
        }

        let py = unsafe { Python::assume_gil_acquired() };
        self.py = Some(py);
        self.init_run = true;

        // env::args() panics if arguments aren't valid Unicode. But invalid
        // Unicode arguments are possible and some applications may want to
        // support them.
        //
        // env::args_os() provides access to the raw OsString instances, which
        // will be derived from wchar_t on Windows and char* on POSIX. We can
        // convert these to Python str instances using a platform-specific
        // mechanism.
        let args_objs = env::args_os()
            .map(|os_arg| osstring_to_str(py, os_arg))
            .collect::<Result<Vec<PyObject>, &'static str>>()?;

        // This will steal the pointer to the elements and mem::forget them.
        let args = PyList::new(py, &args_objs);
        let argv = b"argv\0";

        let res = args.with_borrowed_ptr(py, |args_ptr| unsafe {
            pyffi::PySys_SetObject(argv.as_ptr() as *const i8, args_ptr)
        });

        match res {
            0 => (),
            _ => return Err("unable to set sys.argv"),
        }

        if config.argvb {
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
                _ => return Err("unable to set sys.argvb"),
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
            _ => return Err("unable to set sys.oxidized"),
        }

        if config.sys_frozen {
            let frozen = b"frozen\0";

            match py.True().with_borrowed_ptr(py, |py_true| unsafe {
                pyffi::PySys_SetObject(frozen.as_ptr() as *const i8, py_true)
            }) {
                0 => (),
                _ => return Err("unable to set sys.frozen"),
            }
        }

        if config.sys_meipass {
            let meipass = b"_MEIPASS\0";
            let value = PyString::new(py, &origin);

            match value.with_borrowed_ptr(py, |py_value| unsafe {
                pyffi::PySys_SetObject(meipass.as_ptr() as *const i8, py_value)
            }) {
                0 => (),
                _ => return Err("unable to set sys._MEIPASS"),
            }
        }

        Ok(py)
    }

    /// Ensure the Python GIL is released.
    pub fn release_gil(&mut self) {
        if self.py.is_some() {
            self.py = None;
            self.gil = None;
        }
    }

    /// Ensure the Python GIL is acquired, returning a handle on the interpreter.
    pub fn acquire_gil(&mut self) -> Python<'a> {
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

    /// Runs the interpreter with the default code execution settings.
    ///
    /// The crate was built with settings that configure what should be
    /// executed by default. Those settings will be loaded and executed.
    pub fn run(&mut self) -> PyResult<PyObject> {
        // clone() to avoid issues mixing mutable and immutable borrows of self.
        let run = self.config.run.clone();

        let py = self.acquire_gil();

        match run {
            PythonRunMode::None => Ok(py.None()),
            PythonRunMode::Repl => self.run_repl(),
            PythonRunMode::Module { module } => self.run_module_as_main(&module),
            PythonRunMode::Eval { code } => self.run_code(&code),
        }
    }

    /// Handle a raised SystemExit exception.
    ///
    /// This emulates the behavior in pythonrun.c:handle_system_exit() and
    /// _Py_HandleSystemExit() but without the call to exit(), which we don't want.
    fn handle_system_exit(&mut self, py: Python, err: PyErr) -> Result<i32, &'static str> {
        std::io::stdout()
            .flush()
            .or_else(|_| Err("failed to flush stdout"))?;

        let mut value = match err.pvalue {
            Some(ref instance) => {
                if instance.as_ptr() == py.None().as_ptr() {
                    return Ok(0);
                }

                instance.clone_ref(py)
            }
            None => {
                return Ok(0);
            }
        };

        if unsafe { pyffi::PyExceptionInstance_Check(value.as_ptr()) } != 0 {
            // The error code should be in the "code" attribute.
            if let Ok(code) = value.getattr(py, "code") {
                if code == py.None() {
                    return Ok(0);
                }

                // Else pretend exc_value.code is the new exception value to use
                // and fall through to below.
                value = code;
            }
        }

        if unsafe { pyffi::PyLong_Check(value.as_ptr()) } != 0 {
            return Ok(unsafe { pyffi::PyLong_AsLong(value.as_ptr()) as i32 });
        }

        let sys_module = py
            .import("sys")
            .or_else(|_| Err("unable to obtain sys module"))?;
        let stderr = sys_module.get(py, "stderr");

        // This is a cargo cult from the canonical implementation.
        unsafe { pyffi::PyErr_Clear() }

        match stderr {
            Ok(o) => unsafe {
                pyffi::PyFile_WriteObject(value.as_ptr(), o.as_ptr(), pyffi::Py_PRINT_RAW);
            },
            Err(_) => {
                unsafe {
                    pyffi::PyObject_Print(value.as_ptr(), stderr_to_file(), pyffi::Py_PRINT_RAW);
                }
                std::io::stderr()
                    .flush()
                    .or_else(|_| Err("failure to flush stderr"))?;
            }
        }

        unsafe {
            pyffi::PySys_WriteStderr(b"\n\0".as_ptr() as *const i8);
        }

        // This frees references to this exception, which may be necessary to avoid
        // badness.
        err.restore(py);
        unsafe {
            pyffi::PyErr_Clear();
        }

        Ok(1)
    }

    /// Runs the interpreter and handles any exception that was raised.
    pub fn run_and_handle_error(&mut self) -> PythonRunResult {
        // There are underdefined lifetime bugs at play here. There is no
        // explicit lifetime for the PyObject's returned. If we don't have
        // the local variable in scope, we can get into a situation where
        // drop() on self is called before the PyObject's drop(). This is
        // problematic because PyObject's drop() attempts to acquire the GIL.
        // If the interpreter is shut down, there is no GIL to acquire, and
        // we may segfault.
        // TODO look into setting lifetimes properly so the compiler can
        // prevent some issues.
        let res = self.run();
        let py = self.acquire_gil();

        match res {
            Ok(_) => PythonRunResult::Ok {},
            Err(err) => {
                // SystemExit is special in that PyErr_PrintEx() will call
                // exit() if it is seen. So, we handle it manually so we can
                // return an exit code instead of exiting.

                // TODO surely the cpython crate offers a better way to do this...
                err.restore(py);
                let matches =
                    unsafe { pyffi::PyErr_ExceptionMatches(pyffi::PyExc_SystemExit) } != 0;
                let err = cpython::PyErr::fetch(py);

                if matches {
                    return PythonRunResult::Exit {
                        code: match self.handle_system_exit(py, err) {
                            Ok(code) => code,
                            Err(msg) => {
                                eprintln!("{}", msg);
                                1
                            }
                        },
                    };
                }

                self.print_err(err);

                PythonRunResult::Err {}
            }
        }
    }

    /// Calls run() and resolves a suitable exit code.
    pub fn run_as_main(&mut self) -> i32 {
        match self.run_and_handle_error() {
            PythonRunResult::Ok {} => 0,
            PythonRunResult::Err {} => 1,
            PythonRunResult::Exit { code } => code,
        }
    }

    /// Runs a Python module as the __main__ module.
    ///
    /// Returns the execution result of the module code.
    ///
    /// The interpreter is automatically initialized if needed.
    pub fn run_module_as_main(&mut self, name: &str) -> PyResult<PyObject> {
        let py = self.acquire_gil();

        // This is modeled after runpy.py:_run_module_as_main().
        let main: PyModule = unsafe {
            PyObject::from_borrowed_ptr(
                py,
                pyffi::PyImport_AddModule("__main__\0".as_ptr() as *const c_char),
            )
            .cast_into(py)?
        };

        let main_dict = main.dict(py);

        let importlib_util = py.import("importlib.util")?;
        let spec = importlib_util.call(py, "find_spec", (name,), None)?;
        let loader = spec.getattr(py, "loader")?;
        let code = loader.call_method(py, "get_code", (name,), None)?;

        let origin = spec.getattr(py, "origin")?;
        let cached = spec.getattr(py, "cached")?;

        // TODO handle __package__.
        main_dict.set_item(py, "__name__", "__main__")?;
        main_dict.set_item(py, "__file__", origin)?;
        main_dict.set_item(py, "__cached__", cached)?;
        main_dict.set_item(py, "__doc__", py.None())?;
        main_dict.set_item(py, "__loader__", loader)?;
        main_dict.set_item(py, "__spec__", spec)?;

        unsafe {
            let globals = main_dict.as_object().as_ptr();
            let res = pyffi::PyEval_EvalCode(code.as_ptr(), globals, globals);

            if res.is_null() {
                let err = PyErr::fetch(py);
                err.print(py);
                Err(PyErr::fetch(py))
            } else {
                Ok(PyObject::from_owned_ptr(py, res))
            }
        }
    }

    /// Start and run a Python REPL.
    ///
    /// This emulates what CPython's main.c does.
    ///
    /// The interpreter is automatically initialized if needed.
    pub fn run_repl(&mut self) -> PyResult<PyObject> {
        let py = self.acquire_gil();

        unsafe {
            pyffi::Py_InspectFlag = 0;
        }

        // readline is optional. We don't care if it fails.
        if py.import("readline").is_ok() {}

        let sys = py.import("sys")?;

        if let Ok(hook) = sys.get(py, "__interactivehook__") {
            hook.call(py, NoArgs, None)?;
        }

        let stdin_filename = "<stdin>";
        let filename = CString::new(stdin_filename)
            .or_else(|_| Err(PyErr::new::<ValueError, _>(py, "could not create CString")))?;
        let mut cf = pyffi::PyCompilerFlags { cf_flags: 0 };

        // TODO use return value.
        unsafe {
            let stdin = stdin_to_file();
            pyffi::PyRun_AnyFileExFlags(stdin, filename.as_ptr() as *const c_char, 0, &mut cf)
        };

        Ok(py.None())
    }

    /// Runs Python code provided by a string.
    ///
    /// This is similar to what ``python -c <code>`` would do.
    ///
    /// The interpreter is automatically initialized if needed.
    pub fn run_code(&mut self, code: &str) -> PyResult<PyObject> {
        let py = self.acquire_gil();

        let code = CString::new(code).or_else(|_| {
            Err(PyErr::new::<ValueError, _>(
                py,
                "source code is not a valid C string",
            ))
        })?;

        unsafe {
            let main = pyffi::PyImport_AddModule("__main__\0".as_ptr() as *const _);

            if main.is_null() {
                return Err(PyErr::fetch(py));
            }

            let main_dict = pyffi::PyModule_GetDict(main);

            let res = pyffi::PyRun_StringFlags(
                code.as_ptr() as *const _,
                pyffi::Py_file_input,
                main_dict,
                main_dict,
                std::ptr::null_mut(),
            );

            if res.is_null() {
                Err(PyErr::fetch(py))
            } else {
                Ok(PyObject::from_owned_ptr(py, res))
            }
        }
    }

    /// Print a Python error.
    ///
    /// Under the hood this calls ``PyErr_PrintEx()``, which may call
    /// ``Py_Exit()`` and may write to stderr.
    pub fn print_err(&mut self, err: PyErr) {
        let py = self.acquire_gil();
        err.print(py);
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

impl<'a> Drop for MainPythonInterpreter<'a> {
    fn drop(&mut self) {
        if let Some(key) = &self.config.write_modules_directory_env {
            if let Ok(path) = env::var(key) {
                let path = PathBuf::from(path);
                let py = self.acquire_gil();

                if let Err(msg) = write_modules_to_directory(py, &path) {
                    eprintln!("error writing modules file: {}", msg);
                }
            }
        }

        let _ = unsafe { pyffi::Py_FinalizeEx() };
    }
}
