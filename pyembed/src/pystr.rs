// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Bridge Rust and Python string types.

use libc::{c_void, size_t, wchar_t};
use python3_sys as pyffi;
use std::ffi::{CString, OsString};
use std::ptr::null_mut;

#[cfg(target_family = "unix")]
use std::os::unix::ffi::OsStrExt;
#[cfg(target_family = "windows")]
use std::os::windows::prelude::OsStrExt;

use cpython::{PyObject, Python};

#[derive(Debug)]
pub struct OwnedPyStr {
    data: *const wchar_t,
}

impl OwnedPyStr {
    pub fn as_wchar_ptr(&self) -> *const wchar_t {
        self.data
    }

    pub fn from_str(s: &str) -> Result<Self, &'static str> {
        // We need to convert to a C string so there is a terminal NULL
        // otherwise Py_DecodeLocale() can get confused.
        let cs = CString::new(s).or_else(|_| Err("source string has NULL bytes"))?;

        let size: *mut size_t = null_mut();
        let ptr = unsafe { pyffi::Py_DecodeLocale(cs.as_ptr(), size) };

        if ptr.is_null() {
            Err("could not convert str to Python string")
        } else {
            Ok(OwnedPyStr { data: ptr })
        }
    }
}

impl Drop for OwnedPyStr {
    fn drop(&mut self) {
        unsafe { pyffi::PyMem_RawFree(self.data as *mut c_void) }
    }
}

#[cfg(target_family = "unix")]
const SURROGATEESCAPE: &[u8] = b"surrogateescape\0";

#[cfg(target_family = "unix")]
pub fn osstring_to_str(py: Python, s: OsString) -> Result<PyObject, &'static str> {
    // PyUnicode_DecodeLocaleAndSize says the input must have a trailing NULL.
    // So use a CString for that.
    let b = CString::new(s.as_bytes()).or_else(|_| Err("not a valid C string"))?;
    unsafe {
        let o = pyffi::PyUnicode_DecodeLocaleAndSize(
            b.as_ptr() as *const i8,
            b.to_bytes().len() as isize,
            SURROGATEESCAPE.as_ptr() as *const i8,
        );

        Ok(PyObject::from_owned_ptr(py, o))
    }
}

#[cfg(target_family = "windows")]
pub fn osstring_to_str(py: Python, s: OsString) -> Result<PyObject, &'static str> {
    // Windows OsString should be valid UTF-16.
    let w: Vec<u16> = s.encode_wide().collect();
    unsafe {
        Ok(PyObject::from_owned_ptr(
            py,
            pyffi::PyUnicode_FromWideChar(w.as_ptr(), w.len() as isize),
        ))
    }
}

#[cfg(target_family = "unix")]
pub fn osstring_to_bytes(py: Python, s: OsString) -> PyObject {
    let b = s.as_bytes();
    unsafe {
        let o = pyffi::PyBytes_FromStringAndSize(b.as_ptr() as *const i8, b.len() as isize);
        PyObject::from_owned_ptr(py, o)
    }
}

#[cfg(target_family = "windows")]
pub fn osstring_to_bytes(py: Python, s: OsString) -> PyObject {
    let w: Vec<u16> = s.encode_wide().collect();
    unsafe {
        let o = pyffi::PyBytes_FromStringAndSize(w.as_ptr() as *const i8, w.len() as isize * 2);
        PyObject::from_owned_ptr(py, o)
    }
}
