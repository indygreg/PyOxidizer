// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Custom Python memory allocators.
#[cfg(feature = "mimalloc")]
use {mimalloc::MiMalloc, std::ptr::null_mut,libmimalloc_sys as mimallocffi};

#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

use {
    libc::{c_void, size_t},
    python3_sys as pyffi,
    std::{alloc, collections::HashMap},
};

#[cfg(feature = "jemalloc-sys")]
use {jemalloc_sys as jemallocffi, std::ptr::null_mut};

const MIN_ALIGN: usize = 16;

type RawAllocatorState = HashMap<*mut u8, alloc::Layout>;

/// Holds state for the raw memory allocator.
///
/// Ideally we wouldn't need to track state. But Rust's dealloc() API
/// requires passing in a Layout that matches the allocation. This means
/// we need to track the Layout for each allocation. This data structure
/// facilitates that.
///
/// TODO HashMap isn't thread safe and the Python raw allocator doesn't
/// hold the GIL. So we need a thread safe map or a mutex guarding access.
pub struct RawAllocator {
    pub allocator: pyffi::PyMemAllocatorEx,
    _state: Box<RawAllocatorState>,
}

extern "C" fn raw_rust_malloc(ctx: *mut c_void, size: size_t) -> *mut c_void {
    // PyMem_RawMalloc()'s docs say: Requesting zero bytes returns a distinct
    // non-NULL pointer if possible, as if PyMem_RawMalloc(1) had been called
    // instead.
    let size = match size {
        0 => 1,
        val => val,
    };

    unsafe {
        let state = ctx as *mut RawAllocatorState;
        let layout = alloc::Layout::from_size_align_unchecked(size, MIN_ALIGN);
        let res = alloc::alloc(layout);

        (*state).insert(res, layout);

        //println!("allocated {} bytes to {:?}", size, res);
        res as *mut c_void
    }
}

extern "C" fn raw_rust_calloc(ctx: *mut c_void, nelem: size_t, elsize: size_t) -> *mut c_void {
    // PyMem_RawCalloc()'s docs say: Requesting zero elements or elements of
    // size zero bytes returns a distinct non-NULL pointer if possible, as if
    // PyMem_RawCalloc(1, 1) had been called instead.
    let size = match nelem * elsize {
        0 => 1,
        val => val,
    };

    unsafe {
        let state = ctx as *mut RawAllocatorState;
        let layout = alloc::Layout::from_size_align_unchecked(size, MIN_ALIGN);
        let res = alloc::alloc_zeroed(layout);

        (*state).insert(res, layout);

        //println!("zero allocated {} bytes to {:?}", size, res);

        res as *mut c_void
    }
}

extern "C" fn raw_rust_realloc(
    ctx: *mut c_void,
    ptr: *mut c_void,
    new_size: size_t,
) -> *mut c_void {
    //println!("reallocating {:?} to {} bytes", ptr as *mut u8, new_size);

    // PyMem_RawRealloc()'s docs say: If p is NULL, the call is equivalent to
    // PyMem_RawMalloc(n); else if n is equal to zero, the memory block is
    // resized but is not freed, and the returned pointer is non-NULL.
    if ptr.is_null() {
        return raw_rust_malloc(ctx, new_size);
    }

    let new_size = match new_size {
        0 => 1,
        val => val,
    };

    unsafe {
        let state = ctx as *mut RawAllocatorState;
        let layout = alloc::Layout::from_size_align_unchecked(new_size, MIN_ALIGN);

        let key = ptr as *mut u8;
        let old_layout = (*state)
            .remove(&key)
            .expect("original memory address not tracked");

        let res = alloc::realloc(ptr as *mut u8, old_layout, new_size);

        (*state).insert(res, layout);

        res as *mut c_void
    }
}

extern "C" fn raw_rust_free(ctx: *mut c_void, ptr: *mut c_void) {
    if ptr.is_null() {
        return;
    }

    //println!("freeing {:?}", ptr as *mut u8);
    unsafe {
        let state = ctx as *mut RawAllocatorState;

        let key = ptr as *mut u8;
        let layout = (*state)
            .get(&key)
            .unwrap_or_else(|| panic!("could not find allocated memory record: {:?}", key));

        alloc::dealloc(key, *layout);
        (*state).remove(&key);
    }
}

pub fn make_raw_rust_memory_allocator() -> RawAllocator {
    // We need to allocate the HashMap on the heap so the pointer doesn't refer
    // to the stack. We rebox and add the Box to our struct so lifetimes are
    // managed.
    let alloc = Box::new(HashMap::<*mut u8, alloc::Layout>::new());
    let state = Box::into_raw(alloc);

    let allocator = pyffi::PyMemAllocatorEx {
        ctx: state as *mut c_void,
        malloc: Some(raw_rust_malloc),
        calloc: Some(raw_rust_calloc),
        realloc: Some(raw_rust_realloc),
        free: Some(raw_rust_free),
    };

    RawAllocator {
        allocator,
        _state: unsafe { Box::from_raw(state) },
    }
}
// Now let's define a raw memory allocator that interfaces directly with mimalloc.
// This avoids the overhead of going through Rust's allocation layer.
#[cfg(feature = "mimalloc")]
extern "C" fn raw_mimalloc_malloc(_ctx: *mut c_void, size: size_t) -> *mut c_void {
    // PyMem_RawMalloc()'s docs say: Requesting zero bytes returns a distinct
    // non-NULL pointer if possible, as if PyMem_RawMalloc(1) had been called
    // instead.
    let size = match size {
        0 => 1,
        val => val,
    };
    // Allocate `size` bytes.Returns pointer to the allocated memory or null if out of memory.
    // Returns a unique pointer if called with `size` 0

    unsafe { mimallocffi::mi_malloc(size) }
}

#[cfg(feature = "mimalloc")]
extern "C" fn raw_mimalloc_calloc(_ctx: *mut c_void, nelem: size_t, elsize: size_t) -> *mut c_void {
    // PyMem_RawCalloc()'s docs say: Requesting zero elements or elements of
    // size zero bytes returns a distinct non-NULL pointer if possible, as if
    // PyMem_RawCalloc(1, 1) had been called instead.
    let size = match nelem * elsize {
        0 => 1,
        val => val,
    };
    // Allocate `count` items of `size` length each.
    // Returns `null` if `count * size` overflows or on out-of-memory.
    // All items are initialized to zero
    unsafe { mimallocffi::mi_calloc(nelem,size) }
}

#[cfg(feature = "mimalloc")]
extern "C" fn raw_mimalloc_realloc(
    _ctx: *mut c_void,
    ptr: *mut c_void,
    new_size: size_t,
) -> *mut c_void {
    // PyMem_RawRealloc()'s docs say: If p is NULL, the call is equivalent to
    // PyMem_RawMalloc(n); else if n is equal to zero, the memory block is
    // resized but is not freed, and the returned pointer is non-NULL.
    // Below should be automatic

    let new_size = match new_size {
        0 => 1,
        val => val,
    };
    // Re-allocate memory to `newsize` bytes.
    //
    // Return pointer to the allocated memory or null if out of memory. If null
    // is returned, the pointer `p` is not freed. Otherwise the original
    // pointer is either freed or returned as the reallocated result (in case
    // it fits in-place with the new size)
    //
    // If `p` is null, it behaves as [`mi_malloc`]. If `newsize` is larger than
    // the original `size` allocated for `p`, the bytes after `size` are
    // uninitialized

    unsafe { mimallocffi::mi_realloc(ptr, new_size) }
}

#[cfg(feature = "mimalloc")]
extern "C" fn raw_mimalloc_free(_ctx: *mut c_void, ptr: *mut c_void) {
    if ptr.is_null() {
        return;
    }
    // Free previously allocated memory
    // The pointer `p` must have been allocated before (or be null)
    unsafe { mimallocffi::mi_free(ptr) }
}

#[cfg(feature = "mimalloc")]
pub fn make_raw_mimalloc_allocator() -> pyffi::PyMemAllocatorEx {
    pyffi::PyMemAllocatorEx {
        ctx: null_mut(),
        malloc: Some(raw_mimalloc_malloc),
        calloc: Some(raw_mimalloc_calloc),
        realloc: Some(raw_mimalloc_realloc),
        free: Some(raw_mimalloc_free),
    }
}


// Now let's define a raw memory allocator that interfaces directly with jemalloc.
// This avoids the overhead of going through Rust's allocation layer.

#[cfg(feature = "jemalloc-sys")]
extern "C" fn raw_jemalloc_malloc(_ctx: *mut c_void, size: size_t) -> *mut c_void {
    // PyMem_RawMalloc()'s docs say: Requesting zero bytes returns a distinct
    // non-NULL pointer if possible, as if PyMem_RawMalloc(1) had been called
    // instead.
    let size = match size {
        0 => 1,
        val => val,
    };

    unsafe { jemallocffi::mallocx(size, 0) }
}

#[cfg(feature = "jemalloc-sys")]
extern "C" fn raw_jemalloc_calloc(_ctx: *mut c_void, nelem: size_t, elsize: size_t) -> *mut c_void {
    // PyMem_RawCalloc()'s docs say: Requesting zero elements or elements of
    // size zero bytes returns a distinct non-NULL pointer if possible, as if
    // PyMem_RawCalloc(1, 1) had been called instead.
    let size = match nelem * elsize {
        0 => 1,
        val => val,
    };

    unsafe { jemallocffi::mallocx(size, jemallocffi::MALLOCX_ZERO) }
}

#[cfg(feature = "jemalloc-sys")]
extern "C" fn raw_jemalloc_realloc(
    ctx: *mut c_void,
    ptr: *mut c_void,
    new_size: size_t,
) -> *mut c_void {
    // PyMem_RawRealloc()'s docs say: If p is NULL, the call is equivalent to
    // PyMem_RawMalloc(n); else if n is equal to zero, the memory block is
    // resized but is not freed, and the returned pointer is non-NULL.
    if ptr.is_null() {
        return raw_jemalloc_malloc(ctx, new_size);
    }

    let new_size = match new_size {
        0 => 1,
        val => val,
    };

    unsafe { jemallocffi::rallocx(ptr, new_size, 0) }
}

#[cfg(feature = "jemalloc-sys")]
extern "C" fn raw_jemalloc_free(_ctx: *mut c_void, ptr: *mut c_void) {
    if ptr.is_null() {
        return;
    }

    unsafe { jemallocffi::dallocx(ptr, 0) }
}

#[cfg(feature = "jemalloc-sys")]
pub fn make_raw_jemalloc_allocator() -> pyffi::PyMemAllocatorEx {
    pyffi::PyMemAllocatorEx {
        ctx: null_mut(),
        malloc: Some(raw_jemalloc_malloc),
        calloc: Some(raw_jemalloc_calloc),
        realloc: Some(raw_jemalloc_realloc),
        free: Some(raw_jemalloc_free),
    }
}
