// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use libc::c_char;
use std::env;
use std::ffi::CString;
use std::path::PathBuf;
use std::ptr::null;

use cpython::{NoArgs, ObjectProtocol, PyErr, PyModule, PyObject, PyResult, PythonObject, Python, ToPyObject};
use pyffi;

use crate::data::*;
use crate::pyalloc::{make_raw_memory_allocator, RawAllocator};
use crate::pymodules_module::PyInit__pymodules;
use crate::pystr::OwnedPyStr;

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
    unsafe {
        __acrt_iob_func(0)
    }
}

#[cfg(unix)]
fn stdin_to_file() -> *mut libc::FILE {
    unsafe {
        libc::fdopen(libc::STDIN_FILENO, &('r' as libc::c_char));
    }
}

/// Represents an embedded Python interpreter.
///
/// Since the Python API has global state and methods of this mutate global
/// state, there should only be a single instance of this type at any time.
pub struct MainPythonInterpreter {
    pub config: PythonConfig,
    frozen_modules: [pyffi::_frozen; 3],
    init_run: bool,
    raw_allocator: Option<RawAllocator>,
}

impl MainPythonInterpreter {
    /// Construct an instance from a config.
    ///
    /// There are no significant side-effects from calling this.
    pub fn new(config: PythonConfig) -> MainPythonInterpreter {
        MainPythonInterpreter {
            config,
            frozen_modules: make_custom_frozen_modules(),
            init_run: false,
            raw_allocator: Some(make_raw_memory_allocator()),
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
    pub fn init(&mut self, py: Python) {
        // TODO return Result<> and don't panic.
        if self.init_run {
            return
        }

        let config = &self.config;

        if let Some(raw_allocator) = &self.raw_allocator {
            unsafe {
                let ptr = &raw_allocator.allocator as *const _;
                pyffi::PyMem_SetAllocator(pyffi::PyMemAllocatorDomain::PYMEM_DOMAIN_RAW, ptr as *mut _);

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
                pyffi::PyImport_AppendInittab(PYMODULES_NAME.as_ptr() as *const i8, Some(PyInit__pymodules));
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

        if let (Some(ref encoding), Some(ref errors)) = (&config.standard_io_encoding, &config.standard_io_errors) {
            let cencoding = CString::new(encoding.clone()).unwrap();
            let cerrors = CString::new(errors.clone()).unwrap();

            let res = unsafe {
                pyffi::Py_SetStandardStreamEncoding(cencoding.as_ptr() as *const i8, cerrors.as_ptr() as *const i8)
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

        // TODO support storing raw argv somewhere to work around
        // Python coercing it to Unicode on POSIX.

        unsafe {
            pyffi::Py_Initialize();
        }

        self.init_run = true;

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
    }

    pub fn run(&mut self) {
        let py = unsafe {
            Python::assume_gil_acquired()
        };

        self.init(py);

        py.eval("import re, sys; from black import main; main()", None, None).unwrap();

        //py.eval("print(\"hello, world\")", None, None).unwrap();
        //py.import("__main__").unwrap();
    }

    /// Runs a Python module as the __main__ module.
    ///
    /// Returns the execution result of the module code.
    pub fn run_module_as_main(&mut self, name: &str) -> PyResult<PyObject> {
        let py = unsafe {
            Python::assume_gil_acquired()
        };

        self.init(py);

        // This is modeled after runpy.py:_run_module_as_main().
        let main: PyModule = unsafe {
            PyObject::from_owned_ptr(py, pyffi::PyImport_AddModule("__main__\0".as_ptr() as *const c_char)).cast_into(py)?
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
        let py = unsafe {
            Python::assume_gil_acquired()
        };

        self.init(py);

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
            },
            Err(_) => (),
        };

        let stdin_filename = "<stdin>";
        let filename = CString::new(stdin_filename).expect("could not create CString");
        let mut cf = pyffi::PyCompilerFlags {
            cf_flags: 0,
        };

        // TODO use return value.
        unsafe {
            let stdin = stdin_to_file();
            pyffi::PyRun_AnyFileExFlags(stdin, filename.as_ptr() as *const c_char, 0, &mut cf)
        };

        Ok(py.None())
    }
}

impl Drop for MainPythonInterpreter {
    fn drop(&mut self) {
        let _ = unsafe {
            pyffi::Py_FinalizeEx()
        };
    }
}
