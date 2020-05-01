// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Functionality for evaluating Python code.

use {
    super::config::PythonRunMode,
    super::conversion::path_to_cstring,
    cpython::exc::{RuntimeError, SystemExit, ValueError},
    cpython::{
        NoArgs, ObjectProtocol, PyClone, PyErr, PyModule, PyObject, PyResult, Python, PythonObject,
    },
    libc::c_char,
    python3_sys as pyffi,
    std::ffi::CString,
    std::io::Write,
    std::path::Path,
};

/// Runs Python code provided by a string.
///
/// This is similar to what `python -c <code>` would do.
///
/// The interpreter is automatically initialized if needed.
///
/// A more robust mechanism to run Python code is by calling
/// `MainPythonInterpreter.run_as_main()` with
/// `OxidizedPythonInterpreterConfig.run = PythonRunMode::Eval`,
/// as this mode will run the actual code that `python -c` does,
/// not a reimplementation of it. See `run_as_main()`'s documentation
/// for more.
///
/// This function is geared towards running code similarly to
/// how `python -c` would. If all you want to do is evaluate
/// code, consider using `Python.eval()`. e.g.
/// `interpreter.acquire_gil().eval(...)`.
pub fn run_code(py: Python, code: &str) -> PyResult<PyObject> {
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

/// Runs Python code in a filesystem path.
///
/// This is similar to what `python <path>` would do.
///
/// A more robust mechanism to run a Python file is by calling
/// `MainPythonInterpreter.run_as_main()` with
/// `OxidizedPythonInterpreterConfig.run = PythonRunMode::File`,
/// as this mode will run the actual code that `python` does,
/// not a reimplementation of it. See `run_as_main()`'s documentation
/// for more.
pub fn run_file(py: Python, path: &Path) -> PyResult<PyObject> {
    let res = unsafe {
        // Python's APIs operate on a FILE*. So we need to coerce the
        // filename to a char*. Is there a better way to get a FILE* from
        // a HANDLE on Windows?
        let filename = path_to_cstring(path).or_else(|_| {
            Err(PyErr::new::<RuntimeError, _>(
                py,
                "cannot convert path to C string",
            ))
        })?;

        let fp = libc::fopen(filename.as_ptr(), "rb\0".as_ptr() as *const _);
        let mut cf = pyffi::PyCompilerFlags {
            cf_flags: 0,
            cf_feature_version: 0,
        };

        pyffi::PyRun_AnyFileExFlags(fp, filename.as_ptr(), 1, &mut cf)
    };

    if res == 0 {
        Ok(py.None())
    } else {
        Err(PyErr::new::<SystemExit, _>(py, 1))
    }
}

/// Runs a Python module as the __main__ module.
///
/// This is similar to what `python -m <module>` would do.
///
/// A more robust mechanism to run a Python file is by calling
/// `MainPythonInterpreter.run_as_main()` with
/// `OxidizedPythonInterpreterConfig.run = PythonRunMode::File`,
/// as this mode will run the actual code that `python` does,
/// not a reimplementation of it. See `run_as_main()`'s documentation
/// for more.
///
/// Returns the execution result of the module code.
pub fn run_module_as_main(py: Python, name: &str) -> PyResult<PyObject> {
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

/// Start and run a Python REPL.
///
/// This emulates what CPython's main.c does.
///
/// The interpreter is automatically initialized if needed.
///
/// A more robust mechanism to run a Python REPL is by calling
/// `MainPythonInterpreter.run_as_main()` with
/// `OxidizedPythonInterpreterConfig.run = PythonRunMode::Repl`,
/// as this mode will run the actual code that `python` does,
/// not a reimplementation of it. See `run_as_main()`'s documentation
/// for more.
pub fn run_repl(py: Python) -> PyResult<PyObject> {
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
    let mut cf = pyffi::PyCompilerFlags {
        cf_flags: 0,
        cf_feature_version: 0,
    };

    unsafe {
        let stdin = stdin_to_file();
        let res =
            pyffi::PyRun_AnyFileExFlags(stdin, filename.as_ptr() as *const c_char, 0, &mut cf);

        if res == 0 {
            Ok(py.None())
        } else {
            Err(PyErr::new::<SystemExit, _>(py, 1))
        }
    }
}

/// Runs Python code with the specified code execution settings.
///
/// This will execute whatever is configured by the passed
/// `PythonRunMode` and return a `PyObject` representing the value
/// returned by Python.
///
/// This function will use other `run_*` functions on this module to run
/// Python. Our functions may vary slightly from how `python -c`, `python -m`,
/// etc would do things. If you would like exact conformance with these
/// run modes, use `OxidizedPythonInterpreterConfig.run_as_main()` instead,
/// as that will evaluate using a Python API that does what `python` would do.
pub fn run(py: Python, run_mode: &PythonRunMode) -> PyResult<PyObject> {
    // Clone here because we call into &mut self functions and can't have
    // an immutable reference to &self.
    match run_mode {
        PythonRunMode::None => Ok(py.None()),
        PythonRunMode::Repl => run_repl(py),
        PythonRunMode::Module { module } => run_module_as_main(py, module),
        PythonRunMode::Eval { code } => run_code(py, code),
        PythonRunMode::File { path } => run_file(py, path),
    }
}

#[cfg(windows)]
fn stderr_to_file() -> *mut libc::FILE {
    unsafe { __acrt_iob_func(2) }
}

#[cfg(unix)]
fn stderr_to_file() -> *mut libc::FILE {
    unsafe { libc::fdopen(libc::STDERR_FILENO, &('w' as libc::c_char)) }
}

/// Handle a raised SystemExit exception.
///
/// This emulates the behavior in pythonrun.c:handle_system_exit() and
/// _Py_HandleSystemExit() but without the call to exit(), which we don't want.
pub(crate) fn handle_system_exit(py: Python, err: PyErr) -> Result<i32, &'static str> {
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

/// Runs the interpreter and handles any exception that was raised.
pub fn run_and_handle_error(py: Python, run_mode: &PythonRunMode) -> PythonRunResult {
    // There are underdefined lifetime bugs at play here. There is no
    // explicit lifetime for the PyObject's returned. If we don't have
    // the local variable in scope, we can get into a situation where
    // drop() on self is called before the PyObject's drop(). This is
    // problematic because PyObject's drop() attempts to acquire the GIL.
    // If the interpreter is shut down, there is no GIL to acquire, and
    // we may segfault.
    // TODO look into setting lifetimes properly so the compiler can
    // prevent some issues.
    let res = super::python_eval::run(py, run_mode);

    match res {
        Ok(_) => PythonRunResult::Ok {},
        Err(err) => {
            // SystemExit is special in that PyErr_PrintEx() will call
            // exit() if it is seen. So, we handle it manually so we can
            // return an exit code instead of exiting.

            // TODO surely the cpython crate offers a better way to do this...
            err.restore(py);
            let matches = unsafe { pyffi::PyErr_ExceptionMatches(pyffi::PyExc_SystemExit) } != 0;
            let err = cpython::PyErr::fetch(py);

            if matches {
                return PythonRunResult::Exit {
                    code: match super::python_eval::handle_system_exit(py, err) {
                        Ok(code) => code,
                        Err(msg) => {
                            eprintln!("{}", msg);
                            1
                        }
                    },
                };
            }

            err.print(py);

            PythonRunResult::Err {}
        }
    }
}
