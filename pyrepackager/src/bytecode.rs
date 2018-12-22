// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use cpython::{Python, PyBytes, PyErr, PyObject};
use libc::c_char;
use pyffi::{Py_CompileStringExFlags, Py_file_input, Py_MARSHAL_VERSION, PyMarshal_WriteObjectToString};
use std::ffi::CString;

/// Compile Python source to bytecode in-process.
///
/// This can be used to produce data for a frozen module.
pub fn compile_bytecode(source: &Vec<u8>, filename: &str) -> Vec<u8> {
    // Need to convert to CString to ensure trailing NULL is present.
    let source = CString::new(source.clone()).unwrap();
    let filename = CString::new(filename).unwrap();

    // TODO we could probably eliminate no-auto-initialize and do away with this.
    cpython::prepare_freethreaded_python();

    let gil = Python::acquire_gil();
    let py = gil.python();

    // We can pick up a different Python version from what the distribution is
    // running. This will result in "bad" bytecode being generated. Check for
    // that.
    // TODO we should validate against the parsed distribution instead of
    // hard-coding the version number.
    if pyffi::Py_MARSHAL_VERSION != 4 {
        panic!("unrecognized marshal version {}; did build.rs link against Python 3.7?", pyffi::Py_MARSHAL_VERSION);
    }

    let mut flags = pyffi::PyCompilerFlags {
        cf_flags: 0,
    };

    let code = unsafe {
        let flags_ptr = &mut flags;
        Py_CompileStringExFlags(source.as_ptr() as *const c_char, filename.as_ptr() as *const c_char, Py_file_input, flags_ptr, 0)
    };

    if PyErr::occurred(py) {
        let err = PyErr::fetch(py);
        err.print(py);
        panic!("Python error when compiling {}", filename.to_str().unwrap());
    }

    if code.is_null() {
        panic!("code is null without Python error. Huh?");
    }

    let marshalled = unsafe {
        PyMarshal_WriteObjectToString(code, Py_MARSHAL_VERSION)
    };

    let marshalled = unsafe {
        PyObject::from_owned_ptr(py, marshalled)
    };

    let data = marshalled.cast_as::<PyBytes>(py).unwrap().data(py);

    return data.to_vec();
}
