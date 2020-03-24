// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Functionality for loading Windows DLLs from memory.

Note that use of `module` in this file refers to a Windows `module`,
not a Python `module`.
*/

use {
    super::python_resources::PythonResourcesState,
    lazy_static::lazy_static,
    memory_module_sys::{
        MemoryFreeLibrary, MemoryGetProcAddress, MemoryLoadLibraryEx, HCUSTOMMODULE,
    },
    std::collections::HashMap,
    std::ffi::{c_void, CStr},
    std::sync::atomic::{AtomicUsize, Ordering},
    std::sync::Mutex,
    winapi::shared::basetsd::SIZE_T,
    winapi::shared::minwindef::{BOOL, DWORD, FARPROC, HINSTANCE__, LPVOID},
    winapi::shared::ntdef::LPCSTR,
    winapi::um::libloaderapi::{FreeLibrary, GetProcAddress, LoadLibraryA},
    winapi::um::memoryapi::{VirtualAlloc, VirtualFree},
};

/// Holds state for a module loaded from memory.
struct MemoryModule {
    /// Pointer to loaded module object.
    ptr: *const c_void,

    /// Number of references to this module.
    ///
    /// Used to track when to free this module.
    ///
    /// Count is increased when a module is loaded and decreased when unloaded.
    ref_count: AtomicUsize,
}

/// Holds state for modules loaded into memory.
struct MemoryModules {
    /// Index of loaded modules by name.
    modules: HashMap<String, MemoryModule>,

    /// Array of known memory modules.
    ///
    /// Used to quickly determine whether an address corresponds to a
    /// memory module.
    ///
    /// This data is redundant with the pointer in `MemoryModule`.
    /// But `GetProcAddress()` can be called several times as part of
    /// module loading and looking up values in an array is intuitively
    /// more efficient than iterating a HashMap's values.
    module_ptrs: Vec<*const c_void>,
}

unsafe impl Send for MemoryModules {}

lazy_static! {
    static ref MEMORY_MODULES: Mutex<MemoryModules> = {
        Mutex::new(MemoryModules {
            modules: HashMap::new(),
            module_ptrs: Vec::new(),
        })
    };
}

/// Load a library from memory, possibly retrieving missing libraries from resources state.
///
/// This is the primary interface to use for initiating the load of a library from memory.
/// It handles setting up user data and dispatching with the appropriate hooks set.
pub(crate) unsafe fn load_library_memory(
    resources_state: &PythonResourcesState<u8>,
    data: &[u8],
) -> *const c_void {
    MemoryLoadLibraryEx(
        data.as_ptr() as *const c_void,
        data.len(),
        default_alloc,
        default_free,
        custom_load_library,
        custom_get_proc_address,
        custom_free_library,
        resources_state as *const PythonResourcesState<u8> as *mut c_void,
    )
}

/// Free a library that was loaded from memory.
pub(crate) unsafe fn free_library_memory(module: *const c_void) {
    MemoryFreeLibrary(module);
}

/// Find the address of a symbol in a memory loaded module.
pub(crate) unsafe fn get_proc_address_memory(module: *const c_void, name: &CStr) -> FARPROC {
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

/// LoadLibraryA() implementation that looks in a `PythonResourcesState`.
///
/// If an existing module was already loaded from memory, we return a reference to it.
/// Otherwise we look for DLL data in memory and attempt to load from there.
/// Otherwise we fall back to LoadLibraryA().
#[no_mangle]
extern "C" fn custom_load_library(filename: LPCSTR, user_data: *mut c_void) -> HCUSTOMMODULE {
    assert!(!user_data.is_null());

    // Theoretically this is wrong. But we hopefully won't encounter DLL names
    // that aren't UTF-8!
    let name = unsafe { CStr::from_ptr(filename) }.to_string_lossy();

    // Return an already loaded memory module if we have one.
    // This is in a block so the lock on `MEMORY_MODULES` is released since there
    // is opportunity for deadlock via recursion below.
    {
        let memory_state = MEMORY_MODULES.lock().unwrap();
        if let Some(module) = memory_state.modules.get(name.as_ref()) {
            module.ref_count.fetch_add(1, Ordering::Acquire);

            return module.ptr;
        }
    }

    // Look for a loadable memory module in our resources data structure.
    let resources_state = unsafe {
        (user_data as *const PythonResourcesState<u8>)
            .as_ref()
            .unwrap()
    };

    if let Some(entry) = resources_state.resources.get(name.as_ref()) {
        if let Some(library_data) = &entry.in_memory_shared_library {
            let res = unsafe { load_library_memory(resources_state, library_data) };

            // If we loaded a module, store its state. Otherwise return its failure (NULL).
            if !res.is_null() {
                let mut memory_state = MEMORY_MODULES.lock().unwrap();

                memory_state.modules.insert(
                    name.to_string(),
                    MemoryModule {
                        ptr: res,
                        ref_count: AtomicUsize::new(1),
                    },
                );
                memory_state.module_ptrs.push(res);
            }

            return res;
        }
    }

    // No memory loaded module found. Fall back to system default.

    let result = unsafe { LoadLibraryA(filename) };

    if result.is_null() {
        std::ptr::null() as HCUSTOMMODULE
    } else {
        result as HCUSTOMMODULE
    }
}

/// Custom GetProcAddress() implementation that knows to look in memory-loaded modules.
#[no_mangle]
extern "C" fn custom_get_proc_address(
    module: HCUSTOMMODULE,
    name: LPCSTR,
    _user_data: *mut c_void,
) -> FARPROC {
    // Look for requested module in memory modules and proxy to `MemoryGetProcAddress`
    // if found.
    {
        let memory_state = MEMORY_MODULES.lock().unwrap();

        if memory_state.module_ptrs.contains(&module) {
            return unsafe { MemoryGetProcAddress(module, name) };
        }
    }

    unsafe { GetProcAddress(module as *mut HINSTANCE__, name) }
}

/// Custom FreeLibrary() implementation that knows to unlock memory-loaded modules.
#[no_mangle]
extern "C" fn custom_free_library(module: HCUSTOMMODULE, _user_data: *mut c_void) {
    let mut memory_state = MEMORY_MODULES.lock().unwrap();

    if let Some(index) = memory_state
        .module_ptrs
        .iter()
        .position(|ptr| ptr == &module)
    {
        memory_state.module_ptrs.remove(index);

        let mut free_module = None;

        for (name, module_state) in &memory_state.modules {
            if module_state.ptr == module {
                if module_state.ref_count.fetch_sub(1, Ordering::Acquire) == 1 {
                    free_module = Some(name.to_string());
                    break;
                }
            }
        }

        if let Some(free_module) = free_module {
            memory_state.modules.remove(&free_module);

            // Unlock to avoid potential for deadlock due to recursion.
            std::mem::drop(memory_state);
            return unsafe { MemoryFreeLibrary(module) };
        }
    }

    unsafe {
        FreeLibrary(module as *mut HINSTANCE__);
    }
}
