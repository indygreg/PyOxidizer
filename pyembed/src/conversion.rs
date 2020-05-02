// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Bridge Rust and Python string types.

use {
    cpython::buffer::PyBuffer,
    cpython::exc::UnicodeDecodeError,
    cpython::{PyDict, PyErr, PyObject, PyResult, Python},
    python3_sys as pyffi,
    std::collections::HashMap,
    std::ffi::{CStr, CString, OsStr},
    std::path::{Path, PathBuf},
};

#[cfg(not(library_mode = "extension"))]
use std::ffi::{NulError, OsString};

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

#[cfg(all(unix, not(library_mode = "extension")))]
pub fn osstring_to_bytes(py: Python, s: OsString) -> PyObject {
    let b = s.as_bytes();
    unsafe {
        let o = pyffi::PyBytes_FromStringAndSize(b.as_ptr() as *const i8, b.len() as isize);
        PyObject::from_owned_ptr(py, o)
    }
}

#[cfg(all(windows, not(library_mode = "extension")))]
pub fn osstring_to_bytes(py: Python, s: OsString) -> PyObject {
    let w: Vec<u16> = s.encode_wide().collect();
    unsafe {
        let o = pyffi::PyBytes_FromStringAndSize(w.as_ptr() as *const i8, w.len() as isize * 2);
        PyObject::from_owned_ptr(py, o)
    }
}

#[cfg(all(unix, not(library_mode = "extension")))]
pub fn path_to_cstring(path: &Path) -> Result<CString, NulError> {
    CString::new(path.as_os_str().as_bytes())
}

#[cfg(all(windows, not(library_mode = "extension")))]
pub fn path_to_cstring(path: &Path) -> Result<CString, NulError> {
    // This is not ideal...
    CString::new(path.to_string_lossy().as_bytes())
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

/// Convert a Rust Path to a pathlib.Path.
pub fn path_to_pathlib_path(py: Python, path: &Path) -> PyResult<PyObject> {
    let py_str = path_to_pyobject(py, path)?;

    let pathlib = py.import("pathlib")?;

    pathlib.call(py, "Path", (py_str,), None)
}

#[cfg(unix)]
pub fn pyobject_to_pathbuf(py: Python, value: PyObject) -> PyResult<PathBuf> {
    let os = py.import("os")?;

    let encoded = os
        .call(py, "fsencode", (value,), None)?
        .extract::<Vec<u8>>(py)?;
    let os_str = OsStr::from_bytes(&encoded);

    Ok(PathBuf::from(os_str))
}

#[cfg(windows)]
pub fn pyobject_to_pathbuf(py: Python, value: PyObject) -> PyResult<PathBuf> {
    let os = py.import("os")?;

    // This conversion is a bit wonky. First, the PyObject could be of various
    // types: str, bytes, or a path-like object. We normalize to a PyString
    // by round tripping through os.fsencode() and os.fsdecode(). The
    // os.fsencode() will take care of the type normalization for us and
    // os.fsdecode() gets us a PyString.
    let encoded = os.call(py, "fsencode", (value,), None)?;
    let normalized = os.call(py, "fsdecode", (encoded,), None)?;

    // We now have a Python str, which is a series of code points. The
    // ideal thing to do here would be to go to a wchar_t, then to OsStr,
    // then to PathBuf. As then the PathBuf would hold onto the native
    // wchar_t and allow lossless round-tripping. But we're lazy. So
    // we simply convert to a Rust string and feed that into PathBuf.
    // It should be close enough.
    let rust_normalized = normalized.extract::<String>(py)?;

    Ok(PathBuf::from(rust_normalized))
}

pub fn pyobject_to_pathbuf_optional(py: Python, value: PyObject) -> PyResult<Option<PathBuf>> {
    if value == py.None() {
        Ok(None)
    } else {
        Ok(Some(pyobject_to_pathbuf(py, value)?))
    }
}

/// Attempt to convert a PyObject to an owned Vec<u8>.
pub fn pyobject_to_owned_bytes(py: Python, value: &PyObject) -> PyResult<Vec<u8>> {
    let buffer = PyBuffer::get(py, value)?;

    let data = unsafe {
        std::slice::from_raw_parts::<u8>(buffer.buf_ptr() as *const _, buffer.len_bytes())
    };

    Ok(data.to_owned())
}

/// Attempt to convert a PyObject to owned Vec<u8>.
///
/// Returns Ok(None) if PyObject is None.
pub fn pyobject_to_owned_bytes_optional(py: Python, value: &PyObject) -> PyResult<Option<Vec<u8>>> {
    if value == &py.None() {
        Ok(None)
    } else {
        Ok(Some(pyobject_to_owned_bytes(py, value)?))
    }
}

pub fn pyobject_optional_resources_map_to_owned_bytes(
    py: Python,
    value: &PyObject,
) -> PyResult<Option<HashMap<String, Vec<u8>>>> {
    if value == &py.None() {
        Ok(None)
    } else {
        let source = value.cast_as::<PyDict>(py)?;
        let mut res = HashMap::with_capacity(source.len(py));

        for (k, v) in source.items(py) {
            res.insert(k.extract::<String>(py)?, pyobject_to_owned_bytes(py, &v)?);
        }

        Ok(Some(res))
    }
}

pub fn pyobject_optional_resources_map_to_pathbuf(
    py: Python,
    value: &PyObject,
) -> PyResult<Option<HashMap<String, PathBuf>>> {
    if value == &py.None() {
        Ok(None)
    } else {
        let source = value.cast_as::<PyDict>(py)?;
        let mut res = HashMap::with_capacity(source.len(py));

        for (k, v) in source.items(py) {
            res.insert(k.extract::<String>(py)?, pyobject_to_pathbuf(py, v)?);
        }

        Ok(Some(res))
    }
}
