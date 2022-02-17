// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Bridge Rust and Python string types.

use pyo3::{ffi as pyffi, prelude::*};

use std::ffi::OsString;
use std::os::raw::c_char;

#[cfg(target_family = "unix")]
use std::os::unix::ffi::OsStrExt;

#[cfg(target_family = "windows")]
use std::os::windows::prelude::OsStrExt;

#[cfg(unix)]
pub fn osstring_to_bytes(py: Python, s: OsString) -> &PyAny {
    let b = s.as_bytes();
    unsafe {
        let o = pyffi::PyBytes_FromStringAndSize(b.as_ptr() as *const c_char, b.len() as isize);
        PyObject::from_owned_ptr(py, o).into_ref(py)
    }
}

#[cfg(windows)]
pub fn osstring_to_bytes(py: Python, s: OsString) -> &PyAny {
    let w: Vec<u16> = s.encode_wide().collect();
    unsafe {
        let o = pyffi::PyBytes_FromStringAndSize(w.as_ptr() as *const c_char, w.len() as isize * 2);
        PyObject::from_owned_ptr(py, o).into_ref(py)
    }
}
