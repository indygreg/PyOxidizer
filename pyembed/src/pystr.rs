// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Bridge Rust and Python string types.

use {
    cpython::exc::UnicodeDecodeError,
    cpython::{PyErr, PyObject, PyResult, Python},
    python3_sys as pyffi,
    std::ffi::{CStr, OsStr, OsString},
    std::path::Path,
};

#[cfg(target_family = "unix")]
use std::ffi::CString;

#[cfg(target_family = "unix")]
use std::os::unix::ffi::OsStrExt;

#[cfg(target_family = "windows")]
use std::os::windows::prelude::OsStrExt;

#[cfg(target_family = "unix")]
const SURROGATEESCAPE: &[u8] = b"surrogateescape\0";

/// Convert an &OsStr to a PyObject, using an optional encoding.
///
/// The optional encoding is the name of a Python encoding. If not used,
/// the default system encoding will be used.
#[cfg(target_family = "unix")]
pub fn osstr_to_pyobject(
    py: Python,
    s: &OsStr,
    encoding: Option<&str>,
) -> Result<PyObject, &'static str> {
    // PyUnicode_DecodeLocaleAndSize says the input must have a trailing NULL.
    // So use a CString for that.
    let b = CString::new(s.as_bytes()).or_else(|_| Err("not a valid C string"))?;

    let raw_object = if let Some(encoding) = encoding {
        let encoding_cstring =
            CString::new(encoding.as_bytes()).or_else(|_| Err("encoding not a valid C string"))?;

        unsafe {
            pyffi::PyUnicode_Decode(
                b.as_ptr() as *const i8,
                b.to_bytes().len() as isize,
                encoding_cstring.as_ptr(),
                SURROGATEESCAPE.as_ptr() as *const i8,
            )
        }
    } else {
        unsafe {
            pyffi::PyUnicode_DecodeLocaleAndSize(
                b.as_ptr() as *const i8,
                b.to_bytes().len() as isize,
                SURROGATEESCAPE.as_ptr() as *const i8,
            )
        }
    };

    unsafe { Ok(PyObject::from_owned_ptr(py, raw_object)) }
}

#[cfg(target_family = "windows")]
pub fn osstr_to_pyobject(
    py: Python,
    s: &OsStr,
    _encoding: Option<&str>,
) -> Result<PyObject, &'static str> {
    // Windows OsString should be valid UTF-16. So we can ignore encoding.
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

pub fn path_to_pyobject(py: Python, path: &Path) -> PyResult<PyObject> {
    let encoding_ptr = unsafe { pyffi::Py_FileSystemDefaultEncoding };

    let encoding = if encoding_ptr.is_null() {
        None
    } else {
        Some(
            unsafe { CStr::from_ptr(encoding_ptr).to_str() }
                .or_else(|e| Err(PyErr::new::<UnicodeDecodeError, _>(py, e.to_string())))?,
        )
    };

    osstr_to_pyobject(py, path.as_os_str(), encoding)
        .or_else(|e| Err(PyErr::new::<UnicodeDecodeError, _>(py, e)))
}
