// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Functionality for loading Windows DLLs from memory. */

use {
    memory_module_sys::{MemoryFreeLibrary, MemoryGetProcAddress, MemoryLoadLibrary},
    std::ffi::{c_void, CStr},
    winapi::shared::minwindef::FARPROC,
};

pub(crate) unsafe fn load_library(data: &[u8]) -> *const c_void {
    MemoryLoadLibrary(data.as_ptr() as *const c_void, data.len())
}

pub(crate) unsafe fn free_library(module: *const c_void) -> c_void {
    MemoryFreeLibrary(module)
}

pub(crate) unsafe fn get_proc_address(module: *const c_void, name: &CStr) -> FARPROC {
    MemoryGetProcAddress(module, name.as_ptr())
}
