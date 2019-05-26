// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use libc::c_char;
use python3_sys as pyffi;
use std::collections::BTreeSet;
use std::env;
use std::ffi::CString;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::ptr::null;

use cpython::{
    GILGuard, NoArgs, ObjectProtocol, PyClone, PyDict, PyErr, PyList, PyModule, PyObject, PyResult,
    PyString, PyTuple, Python, PythonObject, ToPyObject,
};

use super::data::*;
use super::pyalloc::{make_raw_memory_allocator, RawAllocator};
use super::pymodules_module::PyInit__pymodules;
use super::pystr::{osstring_to_bytes, osstring_to_str, OwnedPyStr};

pub const PYMODULES_NAME: &'static [u8] = b"_pymodules\0";

const FROZEN_IMPORTLIB_NAME: &'static [u8] = b"_frozen_importlib\0";
const FROZEN_IMPORTLIB_EXTERNAL_NAME: &'static [u8] = b"_frozen_importlib_external\0";

/// Holds the configuration of an embedded Python interpreter.
pub struct PythonConfig {
    /// Path to the current executable.
    pub exe: PathBuf,
    /// Name of the current program to tell to Python.
    pub program_name: String,
    /// Name of encoding for stdio handles.
    pub standard_io_encoding: Option<String>,
    /// Name of encoding error mode for stdio handles.
    pub standard_io_errors: Option<String>,
    /// Python optimization level.
    pub opt_level: i32,
    /// Whether to load our custom frozen importlib bootstrap modules.
    pub use_custom_importlib: bool,
    /// Whether to load the filesystem-based sys.meta_path finder.
    pub filesystem_importer: bool,
    /// Filesystem paths to add to sys.path.
    ///
    /// ``$ORIGIN`` will resolve to the directory of the application at
    /// run-time.
    pub sys_paths: Vec<String>,
    /// Whether to load the site.py module at initialization time.
    pub import_site: bool,
    /// Whether to load a user-specific site module at initialization time.
    pub import_user_site: bool,
    /// Whether to ignore various PYTHON* environment variables.
    pub ignore_python_env: bool,
    /// Whether to suppress writing of ``.pyc`` files when importing ``.py``
    /// files from the filesystem. This is typically irrelevant since modules
    /// are imported from memory.
    pub dont_write_bytecode: bool,
    /// Whether stdout and stderr streams should be unbuffered.
    pub unbuffered_stdio: bool,
    /// Whether to set sys.argvb with bytes versions of process arguments.
    ///
    /// On Windows, bytes will be UTF-16. On POSIX, bytes will be raw char*
    /// values passed to `int main()`.
    pub argvb: bool,
    /// Whether to use Rust's global memory allocator for the Python raw
    /// memory domain.
    pub rust_allocator_raw: bool,
    /// Environment variable holding the directory to write a loaded modules file.
    ///
    /// If this value is set and the environment it refers to is set,
    /// on interpreter shutdown, we will write a ``modules-<random>`` file to
    /// the directory specified containing a ``\n`` delimited list of modules
    /// loaded in ``sys.modules``.
    pub write_modules_directory_env: Option<String>,
}

impl PythonConfig {
    /// Obtain the PythonConfig with the settings compiled into the binary.
    pub fn default() -> PythonConfig {
        let standard_io_encoding = match STANDARD_IO_ENCODING {
            Some(value) => Some(String::from(value)),
            None => None,
        };

        let standard_io_errors = match STANDARD_IO_ERRORS {
            Some(value) => Some(String::from(value)),
            None => None,
        };

        let write_modules_directory_env = match WRITE_MODULES_DIRECTORY_ENV {
            Some(path) => Some(String::from(path)),
            None => None,
        };

        let sys_paths = SYS_PATHS
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<String>>();

        PythonConfig {
            exe: env::current_exe().unwrap(),
            program_name: PROGRAM_NAME.to_string(),
            standard_io_encoding,
            standard_io_errors,
            opt_level: OPT_LEVEL,
            use_custom_importlib: true,
            filesystem_importer: FILESYSTEM_IMPORTER,
            sys_paths,
            import_site: !NO_SITE,
            import_user_site: !NO_USER_SITE_DIRECTORY,
            ignore_python_env: IGNORE_ENVIRONMENT,
            dont_write_bytecode: DONT_WRITE_BYTECODE,
            unbuffered_stdio: UNBUFFERED_STDIO,
            argvb: false,
            rust_allocator_raw: RUST_ALLOCATOR_RAW,
            write_modules_directory_env,
        }
    }
}

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

fn make_custom_frozen_modules() -> [pyffi::_frozen; 3] {
    [
        pyffi::_frozen {
            name: FROZEN_IMPORTLIB_NAME.as_ptr() as *const i8,
            code: FROZEN_IMPORTLIB_DATA.as_ptr(),
            size: FROZEN_IMPORTLIB_DATA.len() as i32,
        },
        pyffi::_frozen {
            name: FROZEN_IMPORTLIB_EXTERNAL_NAME.as_ptr() as *const i8,
            code: FROZEN_IMPORTLIB_EXTERNAL_DATA.as_ptr(),
            size: FROZEN_IMPORTLIB_EXTERNAL_DATA.len() as i32,
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
    pub fn __acrt_iob_func(x: libc::uint32_t) -> *mut libc::FILE;
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
    unsafe { __acrt_iob_func(libc::STDERR_FILENO) }
}

#[cfg(unix)]
fn stderr_to_file() -> *mut libc::FILE {
    unsafe { libc::fdopen(libc::STDERR_FILENO, &('w' as libc::c_char)) }
}

/// Represents an embedded Python interpreter.
///
/// Since the Python API has global state and methods of this mutate global
/// state, there should only be a single instance of this type at any time.
pub struct MainPythonInterpreter<'a> {
    pub config: PythonConfig,
    frozen_modules: [pyffi::_frozen; 3],
    init_run: bool,
    raw_allocator: Option<RawAllocator>,
    gil: Option<GILGuard>,
    py: Option<Python<'a>>,
}

impl<'a> MainPythonInterpreter<'a> {
    /// Construct an instance from a config.
    ///
    /// There are no significant side-effects from calling this.
    pub fn new(config: PythonConfig) -> MainPythonInterpreter<'a> {
        let raw_allocator = if config.rust_allocator_raw {
            Some(make_raw_memory_allocator())
        } else {
            None
        };

        MainPythonInterpreter {
            config,
            frozen_modules: make_custom_frozen_modules(),
            init_run: false,
            raw_allocator,
            gil: None,
            py: None,
        }
    }

    /// Ensure the Python GIL is released.
    pub fn release_gil(&mut self) {
        match self.py {
            Some(_) => {
                self.py = None;
                self.gil = None;
            }
            None => {}
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
    pub fn init(&mut self) -> Python {
        // TODO return Result<> and don't panic.
        if self.init_run {
            return self.acquire_gil();
        }

        let config = &self.config;

        if let Some(raw_allocator) = &self.raw_allocator {
            unsafe {
                let ptr = &raw_allocator.allocator as *const _;
                pyffi::PyMem_SetAllocator(
                    pyffi::PyMemAllocatorDomain::PYMEM_DOMAIN_RAW,
                    ptr as *mut _,
                );

                // TODO call this if memory debugging enabled.
                //pyffi::PyMem_SetupDebugHooks();
            }
        }

        // Module state is a bit wonky.
        //
        // Our in-memory importer relies on a special module which holds references
        // to Python objects exposing module/resource data. This module is imported as
        // part of initializing the Python interpreter.
        //
        // This Python module object needs to hold references to the raw Python module
        // and resource data. Those references are defined by the ModuleState struct.
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
        let module_state = super::pymodules_module::ModuleState {
            py_data: PY_MODULES_DATA,
            pyc_data: PYC_MODULES_DATA,
        };

        if config.use_custom_importlib {
            // Replace the frozen modules in the interpreter with our custom set
            // that knows how to import from memory.
            unsafe {
                pyffi::PyImport_FrozenModules = self.frozen_modules.as_ptr();
            }

            // Register our _pymodules extension which exposes modules data.
            unsafe {
                // name char* needs to live as long as the interpreter is active.
                pyffi::PyImport_AppendInittab(
                    PYMODULES_NAME.as_ptr() as *const i8,
                    Some(PyInit__pymodules),
                );

                // Move pointer to our stack allocated instance. This pointer will be
                // accessed when creating the Python module object, which should be
                // done automatically as part of low-level interpreter initialization
                // when calling Py_Initialize() below.
                super::pymodules_module::NEXT_MODULE_STATE = &module_state;
            }
        }

        let home = OwnedPyStr::from(config.exe.to_str().unwrap());

        unsafe {
            // Pointer needs to live for lifetime of interpreter.
            pyffi::Py_SetPythonHome(home.into());
        }

        let program_name = OwnedPyStr::from(config.program_name.as_str());

        unsafe {
            // Pointer needs to live for lifetime of interpreter.
            pyffi::Py_SetProgramName(program_name.into());
        }

        if let (Some(ref encoding), Some(ref errors)) =
            (&config.standard_io_encoding, &config.standard_io_errors)
        {
            let cencoding = CString::new(encoding.clone()).unwrap();
            let cerrors = CString::new(errors.clone()).unwrap();

            let res = unsafe {
                pyffi::Py_SetStandardStreamEncoding(
                    cencoding.as_ptr() as *const i8,
                    cerrors.as_ptr() as *const i8,
                )
            };

            if res != 0 {
                panic!("unable to set standard stream encoding");
            }
        }

        unsafe {
            pyffi::Py_DontWriteBytecodeFlag = match config.dont_write_bytecode {
                true => 1,
                false => 0,
            };
        }

        unsafe {
            pyffi::Py_IgnoreEnvironmentFlag = match config.ignore_python_env {
                true => 1,
                false => 0,
            };
        }

        unsafe {
            pyffi::Py_NoSiteFlag = match config.import_site {
                true => 0,
                false => 1,
            };
        }

        unsafe {
            pyffi::Py_NoUserSiteDirectory = match config.import_user_site {
                true => 0,
                false => 1,
            };
        }

        unsafe {
            pyffi::Py_OptimizeFlag = config.opt_level;
        }

        unsafe {
            pyffi::Py_UnbufferedStdioFlag = match config.unbuffered_stdio {
                true => 1,
                false => 0,
            };
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
            super::pymodules_module::NEXT_MODULE_STATE = std::ptr::null();
        }

        let py = unsafe { Python::assume_gil_acquired() };
        self.py = Some(py);
        self.init_run = true;

        let sys_module = py.import("sys").expect("unable to import sys");

        // Our hacked _frozen_importlib_external module doesn't register
        // the filesystem importers. This is because a) we don't need it to
        // since interpreter init can be satisfied by in-memory modules
        // b) having that code read config settings during interpreter startup
        // would be challenging. So, we handle installation of filesystem
        // importers here, if desired.

        // This is what importlib._bootstrap_external usally does:
        // supported_loaders = _get_supported_file_loaders()
        // sys.path_hooks.extend([FileFinder.path_hook(*supported_loaders)])
        // sys.meta_path.append(PathFinder)
        if self.config.filesystem_importer {
            let frozen = py
                .import("_frozen_importlib_external")
                .expect("unable to import _frozen_importlib_external");

            let loaders = frozen
                .call(py, "_get_supported_file_loaders", NoArgs, None)
                .expect("error calling _get_supported_file_loaders()");
            let loaders_list = loaders
                .cast_as::<PyList>(py)
                .expect("unable to cast loaders to list");
            let loaders_vec: Vec<PyObject> = loaders_list.iter(py).collect();
            let loaders_tuple = PyTuple::new(py, loaders_vec.as_slice());

            let file_finder = frozen
                .get(py, "FileFinder")
                .expect("unable to get FileFinder");
            let path_hook = file_finder
                .call_method(py, "path_hook", loaders_tuple, None)
                .expect("unable to construct path hook");

            let path_hooks = sys_module
                .get(py, "path_hooks")
                .expect("unable to get sys.path_hooks");
            path_hooks
                .call_method(py, "append", (path_hook,), None)
                .expect("unable to append sys.path_hooks");

            let path_finder = frozen
                .get(py, "PathFinder")
                .expect("unable to get PathFinder");
            let meta_path = sys_module
                .get(py, "meta_path")
                .expect("unable to get sys.meta_path");
            meta_path
                .call_method(py, "append", (path_finder,), None)
                .expect("unable to append to sys.meta_path");
        }

        // Ideally we should be calling Py_SetPath() before Py_Initialize(). But we
        // tried to do this and only ran into problems due to string conversions,
        // unwanted side-effects. Updating fields after initialization should have
        // the same effect.

        // Always clear out sys.path.
        let sys_path = sys_module.get(py, "path").expect("unable to get sys.path");
        sys_path
            .call_method(py, "clear", NoArgs, None)
            .expect("unable to call sys.path.clear()");

        // And repopulate it with entries from the config.
        for path in &config.sys_paths {
            let path = path.replace(
                "$ORIGIN",
                config.exe.parent().unwrap().display().to_string().as_str(),
            );
            let py_path = PyString::new(py, path.as_str());

            sys_path
                .call_method(py, "append", (py_path,), None)
                .expect("could not append sys.path");
        }

        // env::args() panics if arguments aren't valid Unicode. But invalid
        // Unicode arguments are possible and some applications may want to
        // support them.
        //
        // env::args_os() provides access to the raw OsString instances, which
        // will be derived from wchar_t on Windows and char* on POSIX. We can
        // convert these to Python str instances using a platform-specific
        // mechanism.
        let args_objs: Vec<PyObject> = env::args_os()
            .map(|os_arg| osstring_to_str(py, os_arg))
            .collect();

        // This will steal the pointer to the elements and mem::forget them.
        let args = PyList::new(py, &args_objs);
        let argv = b"argv\0";

        let res = args.with_borrowed_ptr(py, |args_ptr| unsafe {
            pyffi::PySys_SetObject(argv.as_ptr() as *const i8, args_ptr)
        });

        match res {
            0 => (),
            _ => panic!("unable to set sys.argv"),
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
                _ => panic!("unable to set sys.argvb"),
            }
        }

        // As a convention, sys.frozen is set to indicate we are running from
        // a self-contained application.
        let frozen = b"_pymodules\0";

        let res = py.True().with_borrowed_ptr(py, |py_true| unsafe {
            pyffi::PySys_SetObject(frozen.as_ptr() as *const i8, py_true)
        });

        match res {
            0 => (),
            _ => panic!("unable to set sys.frozen"),
        }

        py
    }

    /// Runs the interpreter with the default code execution settings.
    ///
    /// The crate was built with settings that configure what should be
    /// executed by default. Those settings will be loaded and executed.
    pub fn run(&mut self) -> PyResult<PyObject> {
        self.init();

        match RUN_MODE {
            0 => self.run_repl(),
            1 => {
                let name = RUN_MODULE_NAME.expect("RUN_MODULE_NAME should be defined");
                self.run_module_as_main(name)
            }
            2 => {
                let code = RUN_CODE.expect("RUN_CODE should be defined");
                self.run_code(code)
            }
            val => panic!("unhandled run mode: {}", val),
        }
    }

    /// Handle a raised SystemExit exception.
    ///
    /// This emulates the behavior in pythonrun.c:handle_system_exit() and
    /// _Py_HandleSystemExit() but without the call to exit(), which we don't want.
    fn handle_system_exit(&mut self, py: Python, err: PyErr) -> i32 {
        std::io::stdout().flush().expect("failed to flush stdout");

        let mut value = match err.pvalue {
            Some(ref instance) => {
                if instance.as_ptr() == py.None().as_ptr() {
                    return 0;
                }

                instance.clone_ref(py)
            }
            None => {
                return 0;
            }
        };

        if unsafe { pyffi::PyExceptionInstance_Check(value.as_ptr()) } != 0 {
            // The error code should be in the "code" attribute.
            if let Ok(code) = value.getattr(py, "code") {
                if code == py.None() {
                    return 0;
                }

                // Else pretend exc_value.code is the new exception value to use
                // and fall through to below.
                value = code;
            }
        }

        if unsafe { pyffi::PyLong_Check(value.as_ptr()) } != 0 {
            return unsafe { pyffi::PyLong_AsLong(value.as_ptr()) as i32 };
        }

        let sys_module = py.import("sys").expect("unable to obtain sys module");
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
                std::io::stderr().flush().expect("failure to flush stderr");
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

        return 1;
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
                        code: self.handle_system_exit(py, err),
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
        let py = self.init();

        // This is modeled after runpy.py:_run_module_as_main().
        let main: PyModule = unsafe {
            PyObject::from_owned_ptr(
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
        let py = self.init();

        unsafe {
            pyffi::Py_InspectFlag = 0;
        }

        match py.import("readline") {
            Ok(_) => (),
            Err(_) => (),
        };

        let sys = py.import("sys")?;

        match sys.get(py, "__interactivehook__") {
            Ok(hook) => {
                hook.call(py, NoArgs, None)?;
            }
            Err(_) => (),
        };

        let stdin_filename = "<stdin>";
        let filename = CString::new(stdin_filename).expect("could not create CString");
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
        let py = self.init();

        let code = CString::new(code).unwrap();

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
                0 as *mut _,
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
fn write_modules_to_directory(py: Python, path: &PathBuf) {
    // TODO this needs better error handling all over.

    fs::create_dir_all(path).expect("could not create directory for modules");

    let rand = uuid::Uuid::new_v4();

    let path = path.join(format!("modules-{}", rand.to_string()));

    let sys = py.import("sys").expect("could not obtain sys module");
    let modules = sys
        .get(py, "modules")
        .expect("could not obtain sys.modules");

    let modules = modules
        .cast_as::<PyDict>(py)
        .expect("sys.modules is not a dict");

    let mut names = BTreeSet::new();
    for (key, _value) in modules.items(py) {
        names.insert(key.extract::<String>(py).expect("module name is not a str"));
    }

    let mut f = fs::File::create(path).expect("could not open file for writing");

    for name in names {
        f.write_fmt(format_args!("{}\n", name))
            .expect("could not write");
    }
}

impl<'a> Drop for MainPythonInterpreter<'a> {
    fn drop(&mut self) {
        if let Some(key) = &self.config.write_modules_directory_env {
            match env::var(key) {
                Ok(path) => {
                    let path = PathBuf::from(path);
                    let py = self.acquire_gil();
                    write_modules_to_directory(py, &path);
                }
                Err(_) => {}
            }
        }

        let _ = unsafe { pyffi::Py_FinalizeEx() };
    }
}
