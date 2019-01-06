// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use libc::{c_void, size_t, wchar_t};
use pyffi;
use std::os::raw::c_char;
use std::ptr::null_mut;

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
