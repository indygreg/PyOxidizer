// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use libc::{c_void, size_t, wchar_t};
use pyffi;
use std::ffi::{CString, OsString};
use std::os::raw::c_char;
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

impl Drop for OwnedPyStr {
    fn drop(&mut self) {
        unsafe { pyffi::PyMem_RawFree(self.data as *mut c_void) }
    }
}

impl<'a> From<&'a str> for OwnedPyStr {
    fn from(s: &str) -> Self {
        let size: *mut size_t = null_mut();

        let ptr = unsafe { pyffi::Py_DecodeLocale(s.as_ptr() as *const c_char, size) };

        if ptr.is_null() {
            panic!("could not convert str to Python string");
        }

        OwnedPyStr { data: ptr }
    }
}

impl Into<*const wchar_t> for OwnedPyStr {
    fn into(self) -> *const wchar_t {
        self.data
    }
}

const SURROGATEESCAPE: &'static [u8] = b"surrogateescape\0";

#[cfg(target_family = "unix")]
pub fn osstring_to_str(py: Python, s: OsString) -> PyObject {
    // PyUnicode_DecodeLocaleAndSize says the input must have a trailing NULL.
    // So use a CString for that.
    let b = CString::new(s.as_bytes()).expect("valid C string");
    unsafe {
        let o = pyffi::PyUnicode_DecodeLocaleAndSize(
            b.as_ptr() as *const i8,
            b.to_bytes().len() as isize,
            SURROGATEESCAPE.as_ptr() as *const i8,
        );

        PyObject::from_owned_ptr(py, o)
    }
}

#[cfg(target_family = "windows")]
pub fn osstring_to_str(py: Python, s: OsString) -> PyObject {
    // Windows OsString should be valid UTF-16.
    let w = s.to_wide();
    unsafe { PyObject::from_owned_ptr(py, pyffi::PyUnicode_FromWideChar(w.as_ptr(), w.len())) }
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
    let w = s.to_wide();
    unsafe {
        let o = pyffi::PyBytes_FromStringAndSize(w.as_ptr() as *const i8, w.len() * 2);
        PyObject::from_owned_ptr(py, o)
    }
}
