// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Custom Python memory allocators.

This module holds code for customizing Python's memory allocators.

# Python Memory Allocators

The canonical documentation for Python's memory allocators is
<https://docs.python.org/3/c-api/memory.html>.

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
> Allocates n bytes and returns a pointer of type void* to the allocated
> memory, or NULL if the request fails.
>
> Requesting zero bytes returns a distinct non-NULL pointer if possible,
> as if PyMem_Malloc(1) had been called instead. The memory will not have
> been initialized in any way.

`void* PyMem_Calloc(size_t nelem, size_t elsize)`
> Allocates nelem elements each whose size in bytes is elsize and returns
> a pointer of type void* to the allocated memory, or NULL if the request
> fails. The memory is initialized to zeros.
>
> Requesting zero elements or elements of size zero bytes returns a
> distinct non-NULL pointer if possible, as if PyMem_RawCalloc(1, 1) had
> been called instead.

`void* PyMem_RawRealloc(void *p, size_t n)`
> Resizes the memory block pointed to by p to n bytes. The contents will be
> unchanged to the minimum of the old and the new sizes.
>
> If p is NULL, the call is equivalent to PyMem_RawMalloc(n); else if n is
> equal to zero, the memory block is resized but is not freed, and the
> returned pointer is non-NULL.
>
> Unless p is NULL, it must have been returned by a previous call to
> PyMem_RawMalloc(), PyMem_RawRealloc() or PyMem_RawCalloc().

`void PyMem_RawFree(void *p)`
> Frees the memory block pointed to by p, which must have been returned by
> a previous call to PyMem_RawMalloc(), PyMem_RawRealloc() or
> PyMem_RawCalloc(). Otherwise, or if PyMem_RawFree(p) has been called before,
> undefined behavior occurs.
>
> If p is NULL, no operation is performed.

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
    core::ffi::c_void,
    pyo3::ffi as pyffi,
    python_packaging::interpreter::MemoryAllocatorBackend,
    std::{
        alloc,
        collections::HashMap,
        ops::{Deref, DerefMut},
        sync::Mutex,
    },
};

const MIN_ALIGN: usize = 16;

/// Tracks allocations from an allocator.
///
/// Some allocators need to pass the original allocation size and alignment
/// to various functions. But this information isn't passed as part of Python's
/// allocator APIs.
///
/// This type exists to facilitate tracking the allocation metadata
/// out-of-band. Essentially, we create an instance of this on the heap
/// and store a pointer to it via the allocator "context" C structs.
///
/// The Python raw domain allocator doesn't hold the GIL. So operations
/// against this data structure called from the context of a raw domain
/// allocator must be thread safe.
///
/// Our current solution to this is a Mutex around the inner data structure.
/// Although this is inefficient: many calling functions perform multiple
/// container operations, requiring a lock for each one. It would be better
/// to have a RAII guard for scoped logical operation.
struct AllocationTracker {
    allocations: Mutex<HashMap<*mut c_void, alloc::Layout>>,
}

impl AllocationTracker {
    /// Construct a new instance.
    ///
    /// It is automatically boxed because it needs to live on the heap.
    fn new() -> Box<Self> {
        Box::new(Self {
            allocations: Mutex::new(HashMap::with_capacity(128)),
        })
    }

    /// Construct an instance from a pointer owned by someone else.
    fn from_owned_ptr(ptr: *mut c_void) -> BorrowedAllocationTracker {
        if ptr.is_null() {
            panic!("must not pass NULL pointer");
        }

        BorrowedAllocationTracker {
            inner: Some(unsafe { Box::from_raw(ptr as *mut AllocationTracker) }),
        }
    }

    /// Obtain an allocation record in this tracker.
    #[inline]
    fn get_allocation(&self, ptr: *mut c_void) -> Option<alloc::Layout> {
        self.allocations.lock().unwrap().get(&ptr).cloned()
    }

    /// Record an allocation in this tracker.
    ///
    /// An existing allocation for the specified memory address will be replaced.
    #[inline]
    fn insert_allocation(&mut self, ptr: *mut c_void, layout: alloc::Layout) {
        self.allocations.lock().unwrap().insert(ptr, layout);
    }

    /// Remove an allocation from this tracker.
    #[inline]
    fn remove_allocation(&mut self, ptr: *mut c_void) -> alloc::Layout {
        self.allocations
            .lock()
            .unwrap()
            .remove(&ptr)
            .expect("memory address not tracked")
    }
}

/// An `AllocationTracker` associated with a borrowed raw pointer.
///
/// Instances can be derefed to `AllocationTracker` and are "leaked"
/// when they are dropped.
struct BorrowedAllocationTracker {
    inner: Option<Box<AllocationTracker>>,
}

impl Deref for BorrowedAllocationTracker {
    type Target = AllocationTracker;

    fn deref(&self) -> &Self::Target {
        self.inner.as_ref().unwrap()
    }
}

impl DerefMut for BorrowedAllocationTracker {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner.as_mut().unwrap()
    }
}

impl Drop for BorrowedAllocationTracker {
    fn drop(&mut self) {
        Box::into_raw(self.inner.take().unwrap());
    }
}

/// Represents an interface to Rust's memory allocator.
pub(crate) struct TrackingAllocator {
    pub allocator: pyffi::PyMemAllocatorEx,
    pub arena: pyffi::PyObjectArenaAllocator,
    _state: Box<AllocationTracker>,
}

extern "C" fn rust_malloc(ctx: *mut c_void, size: usize) -> *mut c_void {
    let size = match size {
        0 => 1,
        val => val,
    };

    let mut tracker = AllocationTracker::from_owned_ptr(ctx);

    let layout = unsafe { alloc::Layout::from_size_align_unchecked(size, MIN_ALIGN) };
    let res = unsafe { alloc::alloc(layout) } as *mut _;

    tracker.insert_allocation(res, layout);

    res
}

#[cfg(feature = "jemalloc-sys")]
extern "C" fn jemalloc_malloc(_ctx: *mut c_void, size: usize) -> *mut c_void {
    let size = match size {
        0 => 1,
        val => val,
    };

    unsafe { jemalloc_sys::mallocx(size, 0) }
}

#[cfg(feature = "libmimalloc-sys")]
extern "C" fn mimalloc_malloc(_ctx: *mut c_void, size: usize) -> *mut c_void {
    let size = match size {
        0 => 1,
        val => val,
    };

    unsafe { libmimalloc_sys::mi_malloc(size) as *mut _ }
}

#[cfg(feature = "snmalloc-sys")]
extern "C" fn snmalloc_malloc(_ctx: *mut c_void, size: usize) -> *mut c_void {
    let size = match size {
        0 => 1,
        val => val,
    };

    unsafe { snmalloc_sys::sn_malloc(size) as *mut _ }
}

extern "C" fn rust_calloc(ctx: *mut c_void, nelem: usize, elsize: usize) -> *mut c_void {
    let size = match nelem * elsize {
        0 => 1,
        val => val,
    };

    let mut tracker = AllocationTracker::from_owned_ptr(ctx);

    let layout = unsafe { alloc::Layout::from_size_align_unchecked(size, MIN_ALIGN) };
    let res = unsafe { alloc::alloc_zeroed(layout) } as *mut _;

    tracker.insert_allocation(res, layout);

    res
}

#[cfg(feature = "jemalloc-sys")]
extern "C" fn jemalloc_calloc(_ctx: *mut c_void, nelem: usize, elsize: usize) -> *mut c_void {
    let size = match nelem * elsize {
        0 => 1,
        val => val,
    };

    unsafe { jemalloc_sys::mallocx(size, jemalloc_sys::MALLOCX_ZERO) }
}

#[cfg(feature = "libmimalloc-sys")]
extern "C" fn mimalloc_calloc(_ctx: *mut c_void, nelem: usize, elsize: usize) -> *mut c_void {
    let size = match nelem * elsize {
        0 => 1,
        val => val,
    };

    unsafe { libmimalloc_sys::mi_calloc(nelem, size) as *mut _ }
}

#[cfg(feature = "snmalloc-sys")]
extern "C" fn snmalloc_calloc(_ctx: *mut c_void, nelem: usize, elsize: usize) -> *mut c_void {
    let size = match nelem * elsize {
        0 => 1,
        val => val,
    };

    unsafe { snmalloc_sys::sn_calloc(nelem, size) as *mut _ }
}

extern "C" fn rust_realloc(ctx: *mut c_void, ptr: *mut c_void, new_size: usize) -> *mut c_void {
    if ptr.is_null() {
        return rust_malloc(ctx, new_size);
    }

    let new_size = match new_size {
        0 => 1,
        val => val,
    };

    let mut tracker = AllocationTracker::from_owned_ptr(ctx);

    let layout = unsafe { alloc::Layout::from_size_align_unchecked(new_size, MIN_ALIGN) };

    let old_layout = tracker.remove_allocation(ptr);

    let res = unsafe { alloc::realloc(ptr as *mut _, old_layout, new_size) } as *mut _;

    tracker.insert_allocation(res, layout);

    res
}

#[cfg(feature = "jemalloc-sys")]
extern "C" fn jemalloc_realloc(ctx: *mut c_void, ptr: *mut c_void, new_size: usize) -> *mut c_void {
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
extern "C" fn mimalloc_realloc(ctx: *mut c_void, ptr: *mut c_void, new_size: usize) -> *mut c_void {
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
extern "C" fn snmalloc_realloc(ctx: *mut c_void, ptr: *mut c_void, new_size: usize) -> *mut c_void {
    if ptr.is_null() {
        return snmalloc_malloc(ctx, new_size);
    }
    let new_size = match new_size {
        0 => 1,
        val => val,
    };

    unsafe { snmalloc_sys::sn_realloc(ptr as *mut _, new_size) as *mut _ }
}

extern "C" fn rust_free(ctx: *mut c_void, ptr: *mut c_void) {
    if ptr.is_null() {
        return;
    }

    let mut tracker = AllocationTracker::from_owned_ptr(ctx);

    let layout = tracker
        .get_allocation(ptr)
        .unwrap_or_else(|| panic!("could not find allocated memory record: {:?}", ptr));

    unsafe {
        alloc::dealloc(ptr as *mut _, layout);
    }

    tracker.remove_allocation(ptr);
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

    unsafe { snmalloc_sys::sn_free(ptr as *mut _) }
}

extern "C" fn rust_arena_free(ctx: *mut c_void, ptr: *mut c_void, _size: usize) {
    if ptr.is_null() {
        return;
    }

    let mut tracker = AllocationTracker::from_owned_ptr(ctx);

    let layout = tracker
        .get_allocation(ptr)
        .unwrap_or_else(|| panic!("could not find allocated memory record: {:?}", ptr));

    unsafe {
        alloc::dealloc(ptr as *mut _, layout);
    }

    tracker.remove_allocation(ptr);
}

#[cfg(feature = "jemalloc-sys")]
extern "C" fn jemalloc_arena_free(_ctx: *mut c_void, ptr: *mut c_void, _size: usize) {
    if ptr.is_null() {
        return;
    }

    unsafe { jemalloc_sys::dallocx(ptr, 0) }
}

#[cfg(feature = "libmimalloc-sys")]
extern "C" fn mimalloc_arena_free(_ctx: *mut c_void, ptr: *mut c_void, _size: usize) {
    if ptr.is_null() {
        return;
    }

    unsafe { libmimalloc_sys::mi_free(ptr as *mut _) }
}

#[cfg(feature = "snmalloc-sys")]
extern "C" fn snmalloc_arena_free(_ctx: *mut c_void, ptr: *mut c_void, _size: usize) {
    if ptr.is_null() {
        return;
    }

    unsafe { snmalloc_sys::sn_free(ptr as *mut _) }
}

/// Represents a `PyMemAllocatorEx` that can be installed as a memory allocator.
enum AllocatorInstance {
    /// Backed by a `PyMemAllocatorEx` struct.
    #[allow(dead_code)]
    Simple(pyffi::PyMemAllocatorEx, pyffi::PyObjectArenaAllocator),

    /// Backed by a custom wrapper type.
    Tracking(TrackingAllocator),
}

/// Represents a custom memory allocator that can be registered with Python.
pub struct PythonMemoryAllocator {
    /// The allocator being used (for identification purposes).
    backend: MemoryAllocatorBackend,

    /// Holds reference to data structures needed by the Python interpreter.
    instance: AllocatorInstance,
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
        Self {
            backend: MemoryAllocatorBackend::Jemalloc,
            instance: AllocatorInstance::Simple(
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
            ),
        }
    }

    #[cfg(not(feature = "jemalloc-sys"))]
    pub fn jemalloc() -> Self {
        panic!("jemalloc allocator requested but it isn't compiled into this build configuration; try `cargo build --features allocator-jemalloc`");
    }

    /// Construct a new instance using mimalloc.
    #[cfg(feature = "libmimalloc-sys")]
    pub fn mimalloc() -> Self {
        Self {
            backend: MemoryAllocatorBackend::Mimalloc,
            instance: AllocatorInstance::Simple(
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
            ),
        }
    }

    #[cfg(not(feature = "libmimalloc-sys"))]
    pub fn mimalloc() -> Self {
        panic!("mimalloc allocator requested but it isn't compiled into this build configuration; try `cargo build --features allocator-mimalloc`");
    }

    /// Construct a new instance using Rust's global allocator.
    pub fn rust() -> Self {
        // We temporarily convert the box to a raw pointer to workaround
        // borrow issues.
        let state = Box::into_raw(AllocationTracker::new());

        let allocator = pyffi::PyMemAllocatorEx {
            ctx: state as *mut c_void,
            malloc: Some(rust_malloc),
            calloc: Some(rust_calloc),
            realloc: Some(rust_realloc),
            free: Some(rust_free),
        };

        Self {
            backend: MemoryAllocatorBackend::Rust,
            instance: AllocatorInstance::Tracking(TrackingAllocator {
                allocator,
                arena: pyffi::PyObjectArenaAllocator {
                    ctx: state as *mut c_void,
                    alloc: Some(rust_malloc),
                    free: Some(rust_arena_free),
                },
                _state: unsafe { Box::from_raw(state) },
            }),
        }
    }

    /// Construct a new instance using snmalloc.
    #[cfg(feature = "snmalloc-sys")]
    pub fn snmalloc() -> Self {
        Self {
            backend: MemoryAllocatorBackend::Snmalloc,
            instance: AllocatorInstance::Simple(
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
            ),
        }
    }

    #[cfg(not(feature = "snmalloc-sys"))]
    pub fn snmalloc() -> Self {
        panic!("snmalloc allocator requested but it isn't compiled into this build configuration; try `cargo build --features allocator-snmalloc`");
    }

    /// Obtain the backend used for this instance.
    #[allow(unused)]
    pub fn backend(&self) -> MemoryAllocatorBackend {
        self.backend
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
        unsafe { pyffi::PyObject_SetArenaAllocator(self.as_arena_allocator()) }
    }

    /// Obtain the pointer to the `PyMemAllocatorEx` for this allocator.
    fn as_memory_allocator(&self) -> *const pyffi::PyMemAllocatorEx {
        match &self.instance {
            AllocatorInstance::Simple(alloc, _) => alloc as *const _,
            AllocatorInstance::Tracking(alloc) => &alloc.allocator as *const _,
        }
    }

    #[allow(dead_code)]
    fn as_arena_allocator(&self) -> *mut pyffi::PyObjectArenaAllocator {
        match &self.instance {
            AllocatorInstance::Simple(_, arena) => arena as *const _ as *mut _,
            AllocatorInstance::Tracking(alloc) => &alloc.arena as *const _ as *mut _,
        }
    }
}
