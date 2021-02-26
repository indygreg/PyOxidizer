// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Custom Python memory allocators.

This module holds code for customizing Python's memory allocators.

# Python Memory Allocators

The canonical documentation for Python's memory allocators is
https://docs.python.org/3/c-api/memory.html.

Important parts have been reproduced below for easy reference.

Python declares memory allocators via the `PyMemAllocatorEx` struct.
This holds pointers to functions which perform allocation, reallocation,
releasing, etc.

There are 3 _domains_ within the Python interpreter: raw, memory, and object.
The _raw_ domain is effectively the global allocator for Python. The
_memory_ and _object_ domains often wrap the _raw_ domain with custom logic,
such as arena allocation.

By default, the _raw_ domain uses malloc()/free(). The other domains
use _pymalloc_, which is an arena-based allocator backed by
malloc()/VirtualAlloc(). It is possible to customize the allocator used
by _pymalloc_ or to replace _pymalloc_ with your own `PyMemAllocatorEx`,
bypassing _pymalloc_ completely.

Here is the documentation for the various `PyMemAllocatorEx` members:

`void* malloc(void *ctx, size_t size)`
    Allocates n bytes and returns a pointer of type void* to the allocated
    memory, or NULL if the request fails.

    Requesting zero bytes returns a distinct non-NULL pointer if possible,
    as if PyMem_Malloc(1) had been called instead. The memory will not have
    been initialized in any way.

`void* PyMem_Calloc(size_t nelem, size_t elsize)`
    Allocates nelem elements each whose size in bytes is elsize and returns
    a pointer of type void* to the allocated memory, or NULL if the request
    fails. The memory is initialized to zeros.

    Requesting zero elements or elements of size zero bytes returns a
    distinct non-NULL pointer if possible, as if PyMem_RawCalloc(1, 1) had
    been called instead.

`void* PyMem_RawRealloc(void *p, size_t n)`
    Resizes the memory block pointed to by p to n bytes. The contents will be
    unchanged to the minimum of the old and the new sizes.

    If p is NULL, the call is equivalent to PyMem_RawMalloc(n); else if n is
    equal to zero, the memory block is resized but is not freed, and the
    returned pointer is non-NULL.

    Unless p is NULL, it must have been returned by a previous call to
    PyMem_RawMalloc(), PyMem_RawRealloc() or PyMem_RawCalloc().

`void PyMem_RawFree(void *p)`
    Frees the memory block pointed to by p, which must have been returned by
    a previous call to PyMem_RawMalloc(), PyMem_RawRealloc() or
    PyMem_RawCalloc(). Otherwise, or if PyMem_RawFree(p) has been called before,
    undefined behavior occurs.

    If p is NULL, no operation is performed.

(Documentation for the `PyMem_Raw*()` functions was used. However, the semantics
are the same regardless of which domain the `PyMemAllocatorEx` is installed
to.)

# Support for Custom Allocators

We support `jemalloc`, `mimalloc`, `snmalloc`, and Rust's global allocator as
custom Python allocators.

Rust's global allocator can independently also be set to one of the aforementioned
custom allocators via external Rust code.

Our `jemalloc`, `mimalloc`, and `snmalloc` Python allocator bindings speak
directly to the underlying C APIs provided by these allocators. By contrast,
going through the Rust global allocator introduces an abstraction layer. This
abstraction layer adds overhead (as we need to track allocation sizes to appease
Rust's allocator API). So even if Rust's global allocator is set to a custom
allocator, it is preferred to install the Python allocator because its bindings
to the allocator will be more efficient.

*/

use {
    libc::{c_void, size_t},
    python3_sys as pyffi,
    python_packaging::interpreter::MemoryAllocatorBackend,
    std::{alloc, collections::HashMap},
};

const MIN_ALIGN: usize = 16;

#[cfg(feature = "snmalloc-sys")]
const SNMALLOC_ALIGNMENT: usize = 8;

type RustAllocatorState = HashMap<*mut u8, alloc::Layout>;

/// Holds state for the Rust memory allocator.
///
/// Ideally we wouldn't need to track state. But Rust's dealloc() API
/// requires passing in a Layout that matches the allocation. This means
/// we need to track the Layout for each allocation. This data structure
/// facilitates that.
///
/// TODO HashMap isn't thread safe and the Python raw allocator doesn't
/// hold the GIL. So we need a thread safe map or a mutex guarding access.
pub(crate) struct RustAllocator {
    pub allocator: pyffi::PyMemAllocatorEx,
    pub arena: pyffi::PyObjectArenaAllocator,
    _state: Box<RustAllocatorState>,
}

extern "C" fn rust_malloc(ctx: *mut c_void, size: size_t) -> *mut c_void {
    let size = match size {
        0 => 1,
        val => val,
    };

    unsafe {
        let state = ctx as *mut RustAllocatorState;
        let layout = alloc::Layout::from_size_align_unchecked(size, MIN_ALIGN);
        let res = alloc::alloc(layout);

        (*state).insert(res, layout);

        //println!("allocated {} bytes to {:?}", size, res);
        res as *mut c_void
    }
}

#[cfg(feature = "jemalloc-sys")]
extern "C" fn jemalloc_malloc(_ctx: *mut c_void, size: size_t) -> *mut c_void {
    let size = match size {
        0 => 1,
        val => val,
    };

    unsafe { jemalloc_sys::mallocx(size, 0) }
}

#[cfg(feature = "libmimalloc-sys")]
extern "C" fn mimalloc_malloc(_ctx: *mut c_void, size: size_t) -> *mut c_void {
    let size = match size {
        0 => 1,
        val => val,
    };

    unsafe { libmimalloc_sys::mi_malloc(size) as *mut _ }
}

#[cfg(feature = "snmalloc-sys")]
extern "C" fn snmalloc_malloc(_ctx: *mut c_void, size: size_t) -> *mut c_void {
    let size = match size {
        0 => 1,
        val => val,
    };

    unsafe { snmalloc_sys::rust_alloc(SNMALLOC_ALIGNMENT, size) as *mut _ }
}

extern "C" fn rust_calloc(ctx: *mut c_void, nelem: size_t, elsize: size_t) -> *mut c_void {
    let size = match nelem * elsize {
        0 => 1,
        val => val,
    };

    unsafe {
        let state = ctx as *mut RustAllocatorState;
        let layout = alloc::Layout::from_size_align_unchecked(size, MIN_ALIGN);
        let res = alloc::alloc_zeroed(layout);

        (*state).insert(res, layout);

        //println!("zero allocated {} bytes to {:?}", size, res);

        res as *mut c_void
    }
}

#[cfg(feature = "jemalloc-sys")]
extern "C" fn jemalloc_calloc(_ctx: *mut c_void, nelem: size_t, elsize: size_t) -> *mut c_void {
    let size = match nelem * elsize {
        0 => 1,
        val => val,
    };

    unsafe { jemalloc_sys::mallocx(size, jemalloc_sys::MALLOCX_ZERO) }
}

#[cfg(feature = "libmimalloc-sys")]
extern "C" fn mimalloc_calloc(_ctx: *mut c_void, nelem: size_t, elsize: size_t) -> *mut c_void {
    let size = match nelem * elsize {
        0 => 1,
        val => val,
    };

    unsafe { libmimalloc_sys::mi_calloc(nelem, size) as *mut _ }
}

#[cfg(feature = "snmalloc-sys")]
extern "C" fn snmalloc_calloc(_ctx: *mut c_void, nelem: size_t, elsize: size_t) -> *mut c_void {
    let size = match nelem * elsize {
        0 => 1,
        val => val,
    };

    let ptr = unsafe { snmalloc_sys::rust_alloc(SNMALLOC_ALIGNMENT, size) };
    if ptr.is_null() {
        return ptr as *mut _;
    }

    // TODO should we use write_volatile() + memory fence to be sure?
    unsafe {
        std::ptr::write_bytes(ptr, 0, size);
    }

    ptr as *mut _
}

extern "C" fn rust_realloc(ctx: *mut c_void, ptr: *mut c_void, new_size: size_t) -> *mut c_void {
    if ptr.is_null() {
        return rust_malloc(ctx, new_size);
    }

    let new_size = match new_size {
        0 => 1,
        val => val,
    };

    unsafe {
        let state = ctx as *mut RustAllocatorState;
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

#[cfg(feature = "jemalloc-sys")]
extern "C" fn jemalloc_realloc(
    ctx: *mut c_void,
    ptr: *mut c_void,
    new_size: size_t,
) -> *mut c_void {
    if ptr.is_null() {
        return jemalloc_malloc(ctx, new_size);
    }

    let new_size = match new_size {
        0 => 1,
        val => val,
    };

    unsafe { jemalloc_sys::rallocx(ptr, new_size, 0) }
}

#[cfg(feature = "libmimalloc-sys")]
extern "C" fn mimalloc_realloc(
    ctx: *mut c_void,
    ptr: *mut c_void,
    new_size: size_t,
) -> *mut c_void {
    if ptr.is_null() {
        return mimalloc_malloc(ctx, new_size);
    }

    let new_size = match new_size {
        0 => 1,
        val => val,
    };

    unsafe { libmimalloc_sys::mi_realloc(ptr as *mut _, new_size) as *mut _ }
}

#[cfg(feature = "snmalloc-sys")]
extern "C" fn snmalloc_realloc(
    ctx: *mut c_void,
    ptr: *mut c_void,
    new_size: size_t,
) -> *mut c_void {
    if ptr.is_null() {
        return snmalloc_malloc(ctx, new_size);
    }
    let new_size = match new_size {
        0 => 1,
        val => val,
    };

    // TODO pass old_size properly.
    unsafe { snmalloc_sys::rust_realloc(ptr as *mut _, SNMALLOC_ALIGNMENT, 0, new_size) as *mut _ }
}

extern "C" fn rust_free(ctx: *mut c_void, ptr: *mut c_void) {
    if ptr.is_null() {
        return;
    }

    //println!("freeing {:?}", ptr as *mut u8);
    unsafe {
        let state = ctx as *mut RustAllocatorState;

        let key = ptr as *mut u8;
        let layout = (*state)
            .get(&key)
            .unwrap_or_else(|| panic!("could not find allocated memory record: {:?}", key));

        alloc::dealloc(key, *layout);
        (*state).remove(&key);
    }
}

#[cfg(feature = "jemalloc-sys")]
extern "C" fn jemalloc_free(_ctx: *mut c_void, ptr: *mut c_void) {
    if ptr.is_null() {
        return;
    }

    unsafe { jemalloc_sys::dallocx(ptr, 0) }
}

#[cfg(feature = "libmimalloc-sys")]
extern "C" fn mimalloc_free(_ctx: *mut c_void, ptr: *mut c_void) {
    if ptr.is_null() {
        return;
    }

    unsafe { libmimalloc_sys::mi_free(ptr as *mut _) }
}

#[cfg(feature = "snmalloc-sys")]
extern "C" fn snmalloc_free(_ctx: *mut c_void, ptr: *mut c_void) {
    if ptr.is_null() {
        return;
    }

    // TODO pass size properly.
    unsafe {
        snmalloc_sys::rust_dealloc(ptr as *mut _, SNMALLOC_ALIGNMENT, 0);
    }
}

extern "C" fn rust_arena_free(ctx: *mut c_void, ptr: *mut c_void, _size: size_t) {
    if ptr.is_null() {
        return;
    }

    unsafe {
        let state = ctx as *mut RustAllocatorState;

        let key = ptr as *mut u8;
        let layout = (*state)
            .get(&key)
            .unwrap_or_else(|| panic!("could not find allocated memory record: {:?}", key));

        alloc::dealloc(key, *layout);
        (*state).remove(&key);
    }
}

#[cfg(feature = "jemalloc-sys")]
extern "C" fn jemalloc_arena_free(_ctx: *mut c_void, ptr: *mut c_void, _size: size_t) {
    if ptr.is_null() {
        return;
    }

    unsafe { jemalloc_sys::dallocx(ptr, 0) }
}

#[cfg(feature = "libmimalloc-sys")]
extern "C" fn mimalloc_arena_free(_ctx: *mut c_void, ptr: *mut c_void, _size: size_t) {
    if ptr.is_null() {
        return;
    }

    unsafe { libmimalloc_sys::mi_free(ptr as *mut _) }
}

#[cfg(feature = "snmalloc-sys")]
extern "C" fn snmalloc_arena_free(_ctx: *mut c_void, ptr: *mut c_void, size: size_t) {
    if ptr.is_null() {
        return;
    }

    unsafe {
        snmalloc_sys::rust_dealloc(ptr as *mut _, SNMALLOC_ALIGNMENT, size);
    }
}

/// Represents a `PyMemAllocatorEx` that can be installed as a memory allocator.
pub(crate) enum PythonMemoryAllocator {
    /// Backed by a `PyMemAllocatorEx` struct.
    #[allow(dead_code)]
    Python(pyffi::PyMemAllocatorEx, pyffi::PyObjectArenaAllocator),

    /// Backed by a custom wrapper type.
    Rust(RustAllocator),
}

impl PythonMemoryAllocator {
    /// Construct an instance from a `MemoryAllocatorBackend`.
    ///
    /// Returns `None` if the backend shouldn't be defined.
    pub fn from_backend(backend: MemoryAllocatorBackend) -> Option<Self> {
        match backend {
            MemoryAllocatorBackend::Default => None,
            MemoryAllocatorBackend::Jemalloc => Some(Self::jemalloc()),
            MemoryAllocatorBackend::Mimalloc => Some(Self::mimalloc()),
            MemoryAllocatorBackend::Snmalloc => Some(Self::snmalloc()),
            MemoryAllocatorBackend::Rust => Some(Self::rust()),
        }
    }

    /// Construct a new instance using jemalloc.
    #[cfg(feature = "jemalloc-sys")]
    pub fn jemalloc() -> Self {
        Self::Python(
            pyffi::PyMemAllocatorEx {
                ctx: std::ptr::null_mut(),
                malloc: Some(jemalloc_malloc),
                calloc: Some(jemalloc_calloc),
                realloc: Some(jemalloc_realloc),
                free: Some(jemalloc_free),
            },
            pyffi::PyObjectArenaAllocator {
                ctx: std::ptr::null_mut(),
                alloc: Some(jemalloc_malloc),
                free: Some(jemalloc_arena_free),
            },
        )
    }

    #[cfg(not(feature = "jemalloc-sys"))]
    pub fn jemalloc() -> Self {
        panic!("jemalloc is not available in this build configuration");
    }

    /// Construct a new instance using mimalloc.
    #[cfg(feature = "libmimalloc-sys")]
    pub fn mimalloc() -> Self {
        Self::Python(
            pyffi::PyMemAllocatorEx {
                ctx: std::ptr::null_mut(),
                malloc: Some(mimalloc_malloc),
                calloc: Some(mimalloc_calloc),
                realloc: Some(mimalloc_realloc),
                free: Some(mimalloc_free),
            },
            pyffi::PyObjectArenaAllocator {
                ctx: std::ptr::null_mut(),
                alloc: Some(mimalloc_malloc),
                free: Some(mimalloc_arena_free),
            },
        )
    }

    #[cfg(not(feature = "libmimalloc-sys"))]
    pub fn mimalloc() -> Self {
        panic!("mimalloc is not available in this build configuration");
    }

    /// Construct a new instance using Rust's global allocator.
    pub fn rust() -> Self {
        // We need to allocate the HashMap on the heap so the pointer doesn't refer
        // to the stack. We rebox and add the Box to our struct so lifetimes are
        // managed.
        let alloc = Box::new(HashMap::<*mut u8, alloc::Layout>::new());
        let state = Box::into_raw(alloc);

        let allocator = pyffi::PyMemAllocatorEx {
            ctx: state as *mut c_void,
            malloc: Some(rust_malloc),
            calloc: Some(rust_calloc),
            realloc: Some(rust_realloc),
            free: Some(rust_free),
        };

        Self::Rust(RustAllocator {
            allocator,
            arena: pyffi::PyObjectArenaAllocator {
                ctx: state as *mut c_void,
                alloc: Some(rust_malloc),
                free: Some(rust_arena_free),
            },
            _state: unsafe { Box::from_raw(state) },
        })
    }

    /// Construct a new instance using snmalloc.
    #[cfg(feature = "snmalloc-sys")]
    pub fn snmalloc() -> Self {
        panic!("snmalloc is not yet fully implemented");

        Self::Python(
            pyffi::PyMemAllocatorEx {
                ctx: std::ptr::null_mut(),
                malloc: Some(snmalloc_malloc),
                calloc: Some(snmalloc_calloc),
                realloc: Some(snmalloc_realloc),
                free: Some(snmalloc_free),
            },
            pyffi::PyObjectArenaAllocator {
                ctx: std::ptr::null_mut(),
                alloc: Some(snmalloc_malloc),
                free: Some(snmalloc_arena_free),
            },
        )
    }

    #[cfg(not(feature = "snmalloc-sys"))]
    pub fn snmalloc() -> Self {
        panic!("snmalloc is not available in this build configuration");
    }

    /// Set this allocator to be the allocator for a certain "domain" in a Python interpreter.
    ///
    /// This should be called before `Py_Initialize*()`.
    pub fn set_allocator(&self, domain: pyffi::PyMemAllocatorDomain) {
        unsafe {
            pyffi::PyMem_SetAllocator(domain, self.as_memory_allocator() as *mut _);
        }
    }

    /// Set the arena allocator used by the `pymalloc` allocator.
    ///
    /// This only has an effect if the `pymalloc` allocator is registered to the
    /// `mem` or `object` allocator domains.
    #[allow(dead_code)]
    pub fn set_arena_allocator(&self) {
        // python3-sys has the size and ptr argument order to PyObjectArenaAllocator.free
        // swapped. So we can't set arena allocators until this is fixed.
        panic!("arena allocator not supported due to python3-sys bug");

        // unsafe { pyffi::PyObject_SetArenaAllocator(self.as_arena_allocator()) }
    }

    /// Obtain the pointer to the `PyMemAllocatorEx` for this allocator.
    fn as_memory_allocator(&self) -> *const pyffi::PyMemAllocatorEx {
        match self {
            PythonMemoryAllocator::Python(alloc, _) => alloc as *const _,
            PythonMemoryAllocator::Rust(alloc) => &alloc.allocator as *const _,
        }
    }

    #[allow(dead_code)]
    fn as_arena_allocator(&self) -> *mut pyffi::PyObjectArenaAllocator {
        match self {
            PythonMemoryAllocator::Python(_, arena) => arena as *const _ as *mut _,
            PythonMemoryAllocator::Rust(alloc) => &alloc.arena as *const _ as *mut _,
        }
    }
}
