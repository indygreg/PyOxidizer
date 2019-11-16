//! Work wih Python capsules
//!
use super::object::PyObject;
use err::{self, PyErr, PyResult};
use ffi::{PyCapsule_GetPointer, PyCapsule_Import, PyCapsule_New};
use libc::c_void;
use python::{Python, ToPythonPointer};
use std::ffi::{CStr, CString, NulError};
use std::mem;

/// Capsules are the preferred way to export/import C APIs between extension modules,
/// see [Providing a C API for an Extension Module](https://docs.python.org/3/extending/extending.html#using-capsules).
///
/// In particular, capsules can be very useful to start adding Rust extensions besides
/// existing traditional C ones, be it for gradual rewrites or to extend with new functionality.
/// They can also be used for interaction between independently compiled Rust extensions if needed.
///
/// Capsules can point to data, usually static arrays of constants and function pointers,
/// or to function pointers directly. These two cases have to be handled differently in Rust,
/// and the latter is possible only for architectures were data and function pointers have
/// the same sizes.
///
/// # Examples
/// ## Using a capsule defined in another extension module
/// This retrieves and use one of the simplest capsules in the Python standard library, found in
/// the `unicodedata` module. The C API enclosed in this capsule is the same for all Python
/// versions supported by this crate. This is not the case of all capsules from the standard
/// library. For instance the `struct` referenced by `datetime.datetime_CAPI` gets a new member
/// in version 3.7.
///
/// Note: this example is a lower-level version of the [`py_capsule!`] example. Only the
/// capsule retrieval actually differs.
/// ```
/// #[macro_use] extern crate cpython;
/// extern crate libc;
///
/// use cpython::{Python, PyCapsule};
/// use libc::{c_void, c_char, c_int};
/// use std::ffi::{CStr, CString};
/// use std::mem;
/// use std::ptr::null;
///
/// #[allow(non_camel_case_types)]
/// type Py_UCS4 = u32;
/// const UNICODE_NAME_MAXLEN: usize = 256;
///
/// #[repr(C)]
/// pub struct unicode_name_CAPI {
///     // the `ucd` signature arguments are actually optional (can be `NULL`) FFI PyObject
///     // pointers used to pass alternate (former) versions of Unicode data.
///     // We won't need to use them with an actual value in these examples, so it's enough to
///     // specify them as `*const c_void`, and it spares us a direct reference to the lower
///     // level Python FFI bindings.
///     size: c_int,
///     getname: unsafe extern "C" fn(
///         ucd: *const c_void,
///         code: Py_UCS4,
///         buffer: *const c_char,
///         buflen: c_int,
///         with_alias_and_seq: c_int,
///     ) -> c_int,
///     getcode: unsafe extern "C" fn(
///         ucd: *const c_void,
///         name: *const c_char,
///         namelen: c_int,
///         code: *const Py_UCS4,
///     ) -> c_int,
/// }
///
/// #[derive(Debug, PartialEq)]
/// pub enum UnicodeDataError {
///     InvalidCode,
///     UnknownName,
/// }
///
/// impl unicode_name_CAPI {
///     pub fn get_name(&self, code: Py_UCS4) -> Result<CString, UnicodeDataError> {
///         let mut buf: Vec<c_char> = Vec::with_capacity(UNICODE_NAME_MAXLEN);
///         let buf_ptr = buf.as_mut_ptr();
///         if unsafe {
///           ((*self).getname)(null(), code, buf_ptr, UNICODE_NAME_MAXLEN as c_int, 0)
///         } != 1 {
///             return Err(UnicodeDataError::InvalidCode);
///         }
///         mem::forget(buf);
///         Ok(unsafe { CString::from_raw(buf_ptr) })
///     }
///
///     pub fn get_code(&self, name: &CStr) -> Result<Py_UCS4, UnicodeDataError> {
///         let namelen = name.to_bytes().len() as c_int;
///         let mut code: [Py_UCS4; 1] = [0; 1];
///         if unsafe {
///             ((*self).getcode)(null(), name.as_ptr(), namelen, code.as_mut_ptr())
///         } != 1 {
///             return Err(UnicodeDataError::UnknownName);
///         }
///         Ok(code[0])
///     }
/// }
///
/// let gil = Python::acquire_gil();
/// let py = gil.python();
///
/// let capi: &unicode_name_CAPI = unsafe {
///     PyCapsule::import_data(
///         py,
///         CStr::from_bytes_with_nul_unchecked(b"unicodedata.ucnhash_CAPI\0"),
///     )
/// }
/// .unwrap();
///
/// assert_eq!(capi.get_name(32).unwrap().to_str(), Ok("SPACE"));
/// assert_eq!(capi.get_name(0), Err(UnicodeDataError::InvalidCode));
///
/// assert_eq!(
///     capi.get_code(CStr::from_bytes_with_nul(b"COMMA\0").unwrap()),
///     Ok(44)
/// );
/// assert_eq!(
///     capi.get_code(CStr::from_bytes_with_nul(b"\0").unwrap()),
///     Err(UnicodeDataError::UnknownName)
/// );
/// ```
///
/// ## Creating a capsule from Rust
/// In this example, we enclose some data and a function in a capsule, using an intermediate
/// `struct` as enclosing type, then retrieve them back and use them.
///
/// Warning: you definitely need to declare the data as `static`. If it's
/// only `const`, it's possible it would get cloned elsewhere, with the orginal
/// location being deallocated before it's actually used from another Python
/// extension.
///
///
/// ```
/// extern crate cpython;
/// extern crate libc;
///
/// use libc::{c_void, c_int};
/// use cpython::{PyCapsule, Python};
/// use std::ffi::{CStr, CString};
///
/// #[repr(C)]
/// struct CapsData {
///     value: c_int,
///     fun: fn(c_int, c_int) -> c_int,
/// }
///
/// fn add(a: c_int, b: c_int) -> c_int {
///     a + b
/// }
///
/// static DATA: CapsData = CapsData{value: 1, fun: add};
///
/// fn main() {
///     let gil = Python::acquire_gil();
///     let py = gil.python();
///     let caps = PyCapsule::new_data(py, &DATA, "somemod.capsdata").unwrap();
///
///     let retrieved: &CapsData = unsafe {caps.data_ref("somemod.capsdata")}.unwrap();
///     assert_eq!(retrieved.value, 1);
///     assert_eq!((retrieved.fun)(2 as c_int, 3 as c_int), 5);
/// }
/// ```
///
/// Of course, a more realistic example would be to store the capsule in a Python module,
/// allowing another extension (possibly foreign) to retrieve and use it.
/// Note that in that case, the capsule `name` must be full dotted name of the capsule object,
/// as we're doing here.
/// ```
/// # #[macro_use] extern crate cpython;
/// # extern crate libc;
/// # use libc::c_int;
/// # use cpython::PyCapsule;
/// # #[repr(C)]
/// # struct CapsData {
/// #     value: c_int,
/// #     fun: fn(c_int, c_int) -> c_int,
/// # }
/// # fn add(a: c_int, b: c_int) -> c_int {
/// #     a + b
/// # }
/// # static DATA: CapsData = CapsData{value: 1, fun: add};
/// py_module_initializer!(somemod, initsomemod, PyInit_somemod, |py, m| {
///   m.add(py, "__doc__", "A module holding a capsule")?;
///   m.add(py, "capsdata", PyCapsule::new_data(py, &DATA, "somemod.capsdata").unwrap())?;
///   Ok(())
/// });
/// ```
/// Another Rust extension could then declare `CapsData` and use `PyCapsule::import_data` to
/// fetch it back.
///
/// [`py_capsule!`]: macro.py_capsule.html
pub struct PyCapsule(PyObject);

pyobject_newtype!(PyCapsule, PyCapsule_CheckExact, PyCapsule_Type);

/// Macro to retrieve a Python capsule pointing to an array of data, with a layer of caching.
///
/// For more details on capsules, see [`PyCapsule`]
///
/// The caller has to define an appropriate `repr(C)` `struct` first, and put it in
/// scope (`use`) if needed along the macro invocation.
///
/// # Usage
///
/// ```ignore
///   py_capsule!(from some.python.module import capsulename as rustmodule for CapsuleStruct)
/// ```
///
/// where `CapsuleStruct` is the above mentioned `struct` defined by the caller.
///
/// The macro defines a Rust module named `rustmodule`, as specified by the caller.
/// This module provides a retrieval function with the following signature:
///
/// ```ignore
/// mod rustmodule {
///     pub unsafe fn retrieve<'a>(py: Python) -> PyResult<&'a CapsuleStruct> { ... }
/// }
/// ```
///
/// The `retrieve()` function is unsafe for the same reasons as [`PyCapsule::import_data`],
/// upon which it relies.
///
/// The newly defined module also contains a `RawPyObject` type suitable to represent C-level
/// Python objects. It can be used in `cpython` public API involving raw FFI pointers, such as
/// [`from_owned_ptr`].
///
/// # Examples
/// ## Using a capsule from the standard library
///
/// This retrieves and uses one of the simplest capsules in the Python standard library, found in
/// the `unicodedata` module. The C API enclosed in this capsule is the same for all Python
/// versions supported by this crate.
///
/// In this case, as with all capsules from the Python standard library, the capsule data
/// is an array (`static struct`) with constants and function pointers.
/// ```
/// #[macro_use] extern crate cpython;
/// extern crate libc;
///
/// use cpython::{Python, PyCapsule};
/// use libc::{c_char, c_int};
/// use std::ffi::{c_void, CStr, CString};
/// use std::mem;
/// use std::ptr::null;
///
/// #[allow(non_camel_case_types)]
/// type Py_UCS4 = u32;
/// const UNICODE_NAME_MAXLEN: usize = 256;
///
/// #[repr(C)]
/// pub struct unicode_name_CAPI {
///     // the `ucd` signature arguments are actually optional (can be `NULL`) FFI PyObject
///     // pointers used to pass alternate (former) versions of Unicode data.
///     // We won't need to use them with an actual value in these examples, so it's enough to
///     // specify them as `const c_void`, and it spares us a direct reference to the lower
///     // level Python FFI bindings.
///     size: c_int,
///     getname: unsafe extern "C" fn(
///         ucd: *const c_void,
///         code: Py_UCS4,
///         buffer: *const c_char,
///         buflen: c_int,
///         with_alias_and_seq: c_int,
///     ) -> c_int,
///     getcode: unsafe extern "C" fn(
///         ucd: *const c_void,
///         name: *const c_char,
///         namelen: c_int,
///         code: *const Py_UCS4,
///     ) -> c_int,
/// }
///
/// #[derive(Debug, PartialEq)]
/// pub enum UnicodeDataError {
///     InvalidCode,
///     UnknownName,
/// }
///
/// impl unicode_name_CAPI {
///     pub fn get_name(&self, code: Py_UCS4) -> Result<CString, UnicodeDataError> {
///         let mut buf: Vec<c_char> = Vec::with_capacity(UNICODE_NAME_MAXLEN);
///         let buf_ptr = buf.as_mut_ptr();
///         if unsafe {
///           ((*self).getname)(null(), code, buf_ptr, UNICODE_NAME_MAXLEN as c_int, 0)
///         } != 1 {
///             return Err(UnicodeDataError::InvalidCode);
///         }
///         mem::forget(buf);
///         Ok(unsafe { CString::from_raw(buf_ptr) })
///     }
///
///     pub fn get_code(&self, name: &CStr) -> Result<Py_UCS4, UnicodeDataError> {
///         let namelen = name.to_bytes().len() as c_int;
///         let mut code: [Py_UCS4; 1] = [0; 1];
///         if unsafe {
///             ((*self).getcode)(null(), name.as_ptr(), namelen, code.as_mut_ptr())
///         } != 1 {
///             return Err(UnicodeDataError::UnknownName);
///         }
///         Ok(code[0])
///     }
/// }
///
/// py_capsule!(from unicodedata import ucnhash_CAPI as capsmod for unicode_name_CAPI);
///
/// fn main() {
///     let gil = Python::acquire_gil();
///     let py = gil.python();
///
///     let capi = unsafe { capsmod::retrieve(py).unwrap() };
///     assert_eq!(capi.get_name(32).unwrap().to_str(), Ok("SPACE"));
///     assert_eq!(capi.get_name(0), Err(UnicodeDataError::InvalidCode));
///
///     assert_eq!(capi.get_code(CStr::from_bytes_with_nul(b"COMMA\0").unwrap()), Ok(44));
///     assert_eq!(capi.get_code(CStr::from_bytes_with_nul(b"\0").unwrap()),
///                Err(UnicodeDataError::UnknownName));
/// }
/// ```
///
/// ## With Python objects
///
/// In this example, we lend a Python object and receive a new one of which we take ownership.
///
/// ```
/// #[macro_use] extern crate cpython;
/// extern crate libc;
///
/// use cpython::{PyCapsule, PyObject, PyResult, Python};
/// use libc::c_void;
///
/// // In the struct, we still have to use c_void for C-level Python objects.
/// #[repr(C)]
/// pub struct spawn_CAPI {
///     spawnfrom: unsafe extern "C" fn(obj: *const c_void) -> *mut c_void,
/// }
///
/// py_capsule!(from some.mod import CAPI as capsmod for spawn_CAPI);
///
/// impl spawn_CAPI {
///    pub fn spawn_from(&self, py: Python, obj: PyObject) -> PyResult<PyObject> {
///        let raw = obj.as_ptr() as *const c_void;
///        Ok(unsafe {
///            PyObject::from_owned_ptr(
///                py,
///                ((*self).spawnfrom)(raw) as *mut capsmod::RawPyObject)
///        })
///    }
/// }
///
/// # fn main() {}  // just to avoid confusion with use due to insertion of main() in doctests
/// ```
///
/// [`PyCapsule`]: struct.PyCapsule.html
/// [`PyCapsule::import_data`]: struct.PyCapsule.html#method.import_data
#[macro_export]
macro_rules! py_capsule {
    (from $($capsmod:ident).+ import $capsname:ident as $rustmod:ident for $ruststruct: ident ) => (
        mod $rustmod {
            use super::*;
            use std::sync::Once;
            use $crate::PyClone;

            static mut CAPS_DATA: Option<$crate::PyResult<&$ruststruct>> = None;

            static INIT: Once = Once::new();

            pub type RawPyObject = $crate::_detail::ffi::PyObject;

            pub unsafe fn retrieve<'a>(py: $crate::Python) -> $crate::PyResult<&'a $ruststruct> {
                INIT.call_once(|| {
                    let caps_name =
                        std::ffi::CStr::from_bytes_with_nul_unchecked(
                            concat!($( stringify!($capsmod), "."),*,
                                    stringify!($capsname),
                                    "\0").as_bytes());
                    CAPS_DATA = Some($crate::PyCapsule::import_data(py, caps_name));
                });
                match CAPS_DATA {
                    Some(Ok(d)) => Ok(d),
                    Some(Err(ref e)) => Err(e.clone_ref(py)),
                    _ => panic!("Uninitialized"), // can't happen
                }
            }
        }
    )
}

/// Macro to retrieve a function pointer capsule.
///
/// This is not suitable for architectures where the sizes of function and data pointers
/// differ.
/// For general explanations about capsules, see [`PyCapsule`].
///
/// # Usage
///
/// ```ignore
///    py_capsule_fn!(from some.python.module import capsulename as rustmodule
///                       signature (args) -> ret_type)
/// ```
///
/// Similarly to [py_capsule!](macro_py_capsule), the macro defines
///
/// - a Rust module according to the name provided by the caller (here, `rustmodule`)
/// - a type alias for the given signature
/// - a retrieval function:
///
/// ```ignore
/// mod $rustmod {
///     pub type CapsuleFn = unsafe extern "C" (args) -> ret_type ;
///     pub unsafe fn retrieve<'a>(py: Python) -> PyResult<CapsuleFn) { ... }
/// }
/// ```
/// - a `RawPyObject` type suitable for signatures that involve Python C objects;
///   it can be used in `cpython` public API involving raw FFI pointers, such as
///   [`from_owned_ptr`].
///
/// The first call to `retrieve()` is cached for subsequent calls.
///
/// # Examples
/// ## Full example with primitive types
/// There is in the Python library no capsule enclosing a function pointer directly,
/// although the documentation presents it as a valid use-case. For this example, we'll
/// therefore have to create one, using the [`PyCapsule`] constructor, and to set it in an
/// existing  module (not to imply that a real extension should follow that example
/// and set capsules in modules they don't define!)
///
///
/// ```
/// #[macro_use] extern crate cpython;
/// extern crate libc;
/// use cpython::{PyCapsule, Python, FromPyObject};
/// use libc::{c_int, c_void};
///
/// extern "C" fn inc(a: c_int) -> c_int {
///     a + 1
/// }
///
/// /// for testing purposes, stores a capsule named `sys.capsfn`` pointing to `inc()`.
/// fn create_capsule() {
///     let gil = Python::acquire_gil();
///     let py = gil.python();
///     let pymod = py.import("sys").unwrap();
///     let caps = PyCapsule::new(py, inc as *const c_void, "sys.capsfn").unwrap();
///     pymod.add(py, "capsfn", caps).unwrap();
///  }
///
/// py_capsule_fn!(from sys import capsfn as capsmod signature (a: c_int) -> c_int);
///
/// // One could, e.g., reexport if needed:
/// pub use capsmod::CapsuleFn;
///
/// fn retrieve_use_capsule() {
///     let gil = Python::acquire_gil();
///     let py = gil.python();
///     let fun = capsmod::retrieve(py).unwrap();
///     assert_eq!( unsafe { fun(1) }, 2);
///
///     // let's demonstrate the (reexported) function type
///     let g: CapsuleFn = fun;
/// }
///
/// fn main() {
///     create_capsule();
///     retrieve_use_capsule();
///     // second call uses the cached function pointer
///     retrieve_use_capsule();
/// }
/// ```
///
/// ## With Python objects
///
/// In this example, we lend a Python object and receive a new one of which we take ownership.
///
/// ```
/// #[macro_use] extern crate cpython;
/// use cpython::{PyCapsule, PyObject, PyResult, Python};
///
/// py_capsule_fn!(from some.mod import capsfn as capsmod
///     signature (raw: *mut RawPyObject) -> *mut RawPyObject);
///
/// fn retrieve_use_capsule(py: Python, obj: PyObject) -> PyResult<PyObject> {
///     let fun = capsmod::retrieve(py)?;
///     let raw = obj.as_ptr();
///     Ok(unsafe { PyObject::from_owned_ptr(py, fun(raw)) })
/// }
///
/// # fn main() {} // avoid problems with injection of declarations with Rust 1.25
///
/// ```
///
/// [`PyCapsule`]: struct.PyCapsule.html
/// [`from_owned_ptr`]: struct.PyObject.html#method.from_owned_ptr`
#[macro_export]
macro_rules! py_capsule_fn {
    (from $($capsmod:ident).+ import $capsname:ident as $rustmod:ident signature $( $sig: tt)* ) => (
        mod $rustmod {
            use super::*;
            use std::sync::Once;
            use $crate::PyClone;

            pub type CapsuleFn = unsafe extern "C" fn $( $sig )*;
            pub type RawPyObject = $crate::_detail::ffi::PyObject;

            static mut CAPS_FN: Option<$crate::PyResult<CapsuleFn>> = None;

            static INIT: Once = Once::new();

            fn import(py: $crate::Python) -> $crate::PyResult<CapsuleFn> {
                unsafe {
                    let caps_name =
                        std::ffi::CStr::from_bytes_with_nul_unchecked(
                            concat!($( stringify!($capsmod), "."),*,
                                    stringify!($capsname),
                                    "\0").as_bytes());
                    Ok(::std::mem::transmute($crate::PyCapsule::import(py, caps_name)?))
                }
            }

            pub fn retrieve(py: $crate::Python) -> $crate::PyResult<CapsuleFn> {
                unsafe {
                    INIT.call_once(|| { CAPS_FN = Some(import(py)) });
                    match CAPS_FN.as_ref().unwrap() {
                        &Ok(f) => Ok(f),
                        &Err(ref e) => Err(e.clone_ref(py)),
                    }
                }
            }
        }
    )
}

impl PyCapsule {
    /// Retrieve the contents of a capsule pointing to some data as a reference.
    ///
    /// The retrieved data would typically be an array of static data and/or function pointers.
    /// This method doesn't work for standalone function pointers.
    ///
    /// # Safety
    /// This method is unsafe, because
    /// - nothing guarantees that the `T` type is appropriate for the data referenced by the capsule
    ///   pointer
    /// - the returned lifetime doesn't guarantee either to cover the actual lifetime of the data
    ///   (although capsule data is usually static)
    pub unsafe fn import_data<'a, T>(py: Python, name: &CStr) -> PyResult<&'a T> {
        Ok(&*(Self::import(py, name)? as *const T))
    }

    /// Retrieves the contents of a capsule as a void pointer by its name.
    ///
    /// This is suitable in particular for later conversion as a function pointer
    /// with `mem::transmute`, for architectures where data and function pointers have
    /// the same size (see details about this in the
    /// [documentation](https://doc.rust-lang.org/std/mem/fn.transmute.html#examples)
    /// of the Rust standard library).
    pub fn import(py: Python, name: &CStr) -> PyResult<*const c_void> {
        let caps_ptr = unsafe { PyCapsule_Import(name.as_ptr(), 0) };
        if caps_ptr.is_null() {
            return Err(PyErr::fetch(py));
        }
        Ok(caps_ptr)
    }

    /// Convenience method to create a capsule for some data
    ///
    /// The encapsuled data may be an array of functions, but it can't be itself a
    /// function directly.
    ///
    /// May panic when running out of memory.
    ///
    pub fn new_data<T, N>(py: Python, data: &'static T, name: N) -> Result<Self, NulError>
    where
        N: Into<Vec<u8>>,
    {
        Self::new(py, data as *const T as *const c_void, name)
    }

    /// Creates a new capsule from a raw void pointer
    ///
    /// This is suitable in particular to store a function pointer in a capsule. These
    /// can be obtained simply by a simple cast:
    ///
    /// ```
    /// extern crate libc;
    /// use libc::c_void;
    ///
    /// extern "C" fn inc(a: i32) -> i32 {
    ///     a + 1
    /// }
    ///
    /// fn main() {
    ///     let ptr = inc as *const c_void;
    /// }
    /// ```
    ///
    /// # Errors
    /// This method returns `NulError` if `name` contains a 0 byte (see also `CString::new`)
    pub fn new<N>(py: Python, pointer: *const c_void, name: N) -> Result<Self, NulError>
    where
        N: Into<Vec<u8>>,
    {
        let name = CString::new(name)?;
        let caps = unsafe {
            Ok(err::cast_from_owned_ptr_or_panic(
                py,
                PyCapsule_New(pointer as *mut c_void, name.as_ptr(), None),
            ))
        };
        // it is required that the capsule name outlives the call as a char*
        // TODO implement a proper PyCapsule_Destructor to release it properly
        mem::forget(name);
        caps
    }

    /// Returns a reference to the capsule data.
    ///
    /// The name must match exactly the one given at capsule creation time (see `new_data`) and
    /// is converted to a C string under the hood. If that's too much overhead, consider using
    /// `data_ref_cstr()` or caching strategies.
    ///
    /// This is unsafe, because
    /// - nothing guarantees that the `T` type is appropriate for the data referenced by the capsule
    ///   pointer
    /// - the returned lifetime doesn't guarantee either to cover the actual lifetime of the data
    ///   (although capsule data is usually static)
    ///
    /// # Errors
    /// This method returns `NulError` if `name` contains a 0 byte (see also `CString::new`)
    pub unsafe fn data_ref<'a, T, N>(&self, name: N) -> Result<&'a T, NulError>
    where
        N: Into<Vec<u8>>,
    {
        Ok(self.data_ref_cstr(&CString::new(name)?))
    }

    /// Returns a reference to the capsule data.
    ///
    /// This is identical to `data_ref`, except for the name passing. This allows to use
    /// lower level constructs without overhead, such as `CStr::from_bytes_with_nul_unchecked`
    /// or the `cstr!` macro of `rust-cpython`
    pub unsafe fn data_ref_cstr<'a, T>(&self, name: &CStr) -> &'a T {
        &*(PyCapsule_GetPointer(self.as_ptr(), name.as_ptr()) as *const T)
    }
}
