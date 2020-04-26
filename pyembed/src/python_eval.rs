// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Functionality for evaluating Python code.

use {
    super::pystr::path_to_cstring,
    cpython::exc::{RuntimeError, SystemExit, ValueError},
    cpython::{PyErr, PyObject, PyResult, Python},
    python3_sys as pyffi,
    std::ffi::CString,
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
