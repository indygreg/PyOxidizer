// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Functionality for loading Windows DLLs from memory. */

use {
    memory_module_sys::{
        MemoryFreeLibrary, MemoryGetProcAddress, MemoryLoadLibraryEx, HCUSTOMMODULE,
    },
    std::ffi::{c_void, CStr},
    winapi::shared::basetsd::SIZE_T,
    winapi::shared::minwindef::{BOOL, DWORD, FARPROC, HINSTANCE__, LPVOID},
    winapi::shared::ntdef::LPCSTR,
    winapi::um::libloaderapi::{FreeLibrary, GetProcAddress, LoadLibraryA},
    winapi::um::memoryapi::{VirtualAlloc, VirtualFree},
};

pub(crate) unsafe fn load_library(data: &[u8]) -> *const c_void {
    MemoryLoadLibraryEx(
        data.as_ptr() as *const c_void,
        data.len(),
        default_alloc,
        default_free,
        default_load_library,
        default_get_proc_address,
        default_free_library,
        std::ptr::null_mut(),
    )
}

pub(crate) unsafe fn free_library(module: *const c_void) {
    MemoryFreeLibrary(module);
}

pub(crate) unsafe fn get_proc_address(module: *const c_void, name: &CStr) -> FARPROC {
    MemoryGetProcAddress(module, name.as_ptr())
}

#[no_mangle]
extern "C" fn default_alloc(
    address: LPVOID,
    size: SIZE_T,
    allocation_type: DWORD,
    protect: DWORD,
    _user_data: *mut c_void,
) -> LPVOID {
    unsafe { VirtualAlloc(address, size, allocation_type, protect) }
}

#[no_mangle]
extern "C" fn default_free(
    address: LPVOID,
    size: SIZE_T,
    free_type: DWORD,
    _user_data: *mut c_void,
) -> BOOL {
    unsafe { VirtualFree(address, size, free_type) }
}

#[no_mangle]
extern "C" fn default_load_library(filename: LPCSTR, _user_data: *mut c_void) -> HCUSTOMMODULE {
    let result = unsafe { LoadLibraryA(filename) };

    if result.is_null() {
        std::ptr::null() as HCUSTOMMODULE
    } else {
        result as HCUSTOMMODULE
    }
}

#[no_mangle]
extern "C" fn default_get_proc_address(
    module: HCUSTOMMODULE,
    name: LPCSTR,
    _user_data: *mut c_void,
) -> FARPROC {
    unsafe { GetProcAddress(module as *mut HINSTANCE__, name) }
}

#[no_mangle]
extern "C" fn default_free_library(module: HCUSTOMMODULE, _user_data: *mut c_void) {
    unsafe {
        FreeLibrary(module as *mut HINSTANCE__);
    }
}
