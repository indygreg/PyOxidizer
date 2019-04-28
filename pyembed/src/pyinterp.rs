// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use libc::c_char;
use std::env;
use std::ffi::CString;
use std::path::PathBuf;
use std::ptr::null;

use cpython::{
    GILGuard, NoArgs, ObjectProtocol, PyErr, PyList, PyModule, PyObject, PyResult, Python,
    PythonObject, ToPyObject,
};
use pyffi;

use crate::data::*;
use crate::pyalloc::{make_raw_memory_allocator, RawAllocator};
use crate::pymodules_module::PyInit__pymodules;
use crate::pystr::{osstring_to_bytes, osstring_to_str, OwnedPyStr};

const PYMODULES_NAME: &'static [u8] = b"_pymodules\0";

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
    /// Filesystem paths to add to sys.path.
    ///
    /// ``.`` will resolve to the path of the application at run-time.
    pub sys_paths: Vec<PathBuf>,
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
}

impl PythonConfig {
    /// Obtain the PythonConfig with the settings compiled into the binary.
    pub fn default() -> PythonConfig {
        PythonConfig {
            exe: env::current_exe().unwrap(),
            program_name: PROGRAM_NAME.to_string(),
            standard_io_encoding: STANDARD_IO_ENCODING,
            standard_io_errors: STANDARD_IO_ERRORS,
            opt_level: OPT_LEVEL,
            use_custom_importlib: true,
            sys_paths: vec![],
            import_site: !NO_SITE,
            import_user_site: !NO_USER_SITE_DIRECTORY,
            ignore_python_env: IGNORE_ENVIRONMENT,
            dont_write_bytecode: DONT_WRITE_BYTECODE,
            unbuffered_stdio: UNBUFFERED_STDIO,
            argvb: false,
        }
    }
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
        MainPythonInterpreter {
            config,
            frozen_modules: make_custom_frozen_modules(),
            init_run: false,
            raw_allocator: Some(make_raw_memory_allocator()),
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
            },
            None => { },
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
    /// If called more than once, is a no-op.
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

        /*
        // TODO expand "." to the exe's path.
        let paths: Vec<&str> = config.sys_paths.iter().map(|p| p.to_str().unwrap()).collect();
        // TODO use ; on Windows.
        // TODO OwnedPyStr::from("") appears to fail?
        let paths = paths.join(":");
        let path = OwnedPyStr::from(paths.as_str());
        unsafe {
            pyffi::Py_SetPath(path.into());
        }
        */

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

    pub fn run(&mut self) -> PyResult<PyObject> {
        self.init();

        match RUN_MODE {
            0 => {
                self.run_repl()
            },
            1 => {
                let name = RUN_MODULE_NAME.expect("RUN_MODULE_NAME should be defined");
                self.run_module_as_main(name)
            },
            2 => {
                let code = RUN_CODE.expect("RUN_CODE should be defined");
                self.run_code(code)
            }
            val => panic!("unhandled run mode: {}", val),
        }
    }

    /// Runs a Python module as the __main__ module.
    ///
    /// Returns the execution result of the module code.
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

    pub fn run_code(&mut self, code: &str) -> PyResult<PyObject> {
        let py = self.init();

        let code = CString::new(code).unwrap();

        unsafe {
            let main = pyffi::PyImport_AddModule("__main__\0".as_ptr() as *const _);

            if main.is_null() {
                return Err(PyErr::fetch(py));
            }

            let main_dict = pyffi::PyModule_GetDict(main);

            let res = pyffi::PyRun_StringFlags(code.as_ptr() as *const _, pyffi::Py_file_input, main_dict, main_dict, 0 as *mut _);

            if res.is_null() {
                Err(PyErr::fetch(py))
            } else {
                Ok(PyObject::from_owned_ptr(py, res))
            }
        }
    }

    pub fn print_err(&mut self, err: PyErr) {
        let py = self.acquire_gil();
        err.print(py);
    }
}

impl<'a> Drop for MainPythonInterpreter<'a> {
    fn drop(&mut self) {
        let _ = unsafe { pyffi::Py_FinalizeEx() };
    }
}
