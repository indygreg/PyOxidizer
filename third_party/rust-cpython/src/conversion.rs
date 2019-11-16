// Copyright (c) 2015 Daniel Grunwald
//
// Permission is hereby granted, free of charge, to any person obtaining a copy of this
// software and associated documentation files (the "Software"), to deal in the Software
// without restriction, including without limitation the rights to use, copy, modify, merge,
// publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons
// to whom the Software is furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all copies or
// substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED,
// INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR
// PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE
// FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR
// OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.

use std;
use ffi;
use python::{Python, PythonObject, PythonObjectWithCheckedDowncast, PyDrop, PyClone};
use objects::PyObject;
use err::PyResult;

/// Conversion trait that allows various objects to be converted into Python objects.
/// 
/// Note: The associated type `ObjectType` is used so that some Rust types
/// convert to a more precise type of Python object.
/// For example, `[T]::to_py_object()` will result in a `PyList`.
/// You can always calls `val.to_py_object(py).into_py_object()` in order to obtain `PyObject`
/// (the second into_py_object() call via the PythonObject trait corresponds to the upcast from `PyList` to `PyObject`).
pub trait ToPyObject {
    type ObjectType : PythonObject;

    /// Converts self into a Python object.
    fn to_py_object(&self, py: Python) -> Self::ObjectType;

    /// Converts self into a Python object.
    ///
    /// May be more efficient than `to_py_object` in some cases because
    /// it can move out of the input object.
    #[inline]
    fn into_py_object(self, py: Python) -> Self::ObjectType
      where Self: Sized
    {
        self.to_py_object(py)
    }

    /// Converts self into a Python object and calls the specified closure
    /// on the native FFI pointer underlying the Python object.
    ///
    /// May be more efficient than `to_py_object` because it does not need
    /// to touch any reference counts when the input object already is a Python object.
    #[inline]
    fn with_borrowed_ptr<F, R>(&self, py: Python, f: F) -> R
        where F: FnOnce(*mut ffi::PyObject) -> R
    {
        let obj = self.to_py_object(py).into_object();
        let res = f(obj.as_ptr());
        obj.release_ref(py);
        res
    }

    // FFI functions that accept a borrowed reference will use:
    //   input.with_borrowed_ptr(|obj| ffi::Call(obj)
    // 1) input is &PyObject
    //   -> with_borrowed_ptr() just forwards to the closure
    // 2) input is PyObject
    //   -> with_borrowed_ptr() just forwards to the closure
    // 3) input is &str, int, ...
    //   -> to_py_object() allocates new Python object; FFI call happens; release_ref() calls Py_DECREF()

    // FFI functions that steal a reference will use:
    //   let input = input.into_py_object()?; ffi::Call(input.steal_ptr())
    // 1) input is &PyObject
    //   -> into_py_object() calls Py_INCREF
    // 2) input is PyObject
    //   -> into_py_object() is no-op
    // 3) input is &str, int, ...
    //   -> into_py_object() allocates new Python object
}

py_impl_to_py_object_for_python_object!(PyObject);

/// FromPyObject is implemented by various types that can be extracted from a Python object.
///
/// Normal usage is through the `PyObject::extract` helper method:
/// ```let obj: PyObject = ...;
/// let value = obj.extract::<TargetType>(py)?;
/// ```
///
/// Each target type for this conversion supports a different Python objects as input.
/// Calls with an unsupported input object will result in an exception (usually a `TypeError`).
/// 
/// This trait is also used by the `py_fn!` and `py_class!` and `py_argparse!` macros
/// in order to translate from Python objects to the expected Rust parameter types.
/// For example, the parameter `x` in `def method(self, x: i32)` will use
/// `impl FromPyObject for i32` to convert the input Python object into a Rust `i32`.
/// When these macros are used with reference parameters (`x: &str`), the trait
/// `RefFromPyObject` is used instead.
pub trait FromPyObject<'s> : Sized {
    /// Extracts `Self` from the source `PyObject`.
    fn extract(py: Python, obj: &'s PyObject) -> PyResult<Self>;
}


py_impl_from_py_object_for_python_object!(PyObject);


/// RefFromPyObject is implemented by various types that can be extracted
/// as a reference from a Python object.
/// Depending on the input object, the reference may point into memory owned
/// by the Python interpreter; or into a temporary object.
///
/// ```let obj: PyObject = ...;
/// let sum_of_bytes = <[u8] as RefFromPyObject>::with_extracted(py, obj,
///     |data: &[u8]| data.iter().sum()
/// );
/// ```
/// A lambda has to be used because the slice may refer to temporary object
/// that exists only during the `with_extracted` call.
///
/// Each target type for this conversion supports a different Python objects as input.
/// Calls with an unsupported input object will result in an exception (usually a `TypeError`).
/// 
/// This trait is also used by the `py_fn!` and `py_class!` and `py_argparse!` macros
/// in order to translate from Python objects to the expected Rust parameter types.
/// For example, the parameter `x` in `def method(self, x: &[u8])` will use
/// `impl RefFromPyObject for [u8]` to convert the input Python object into a Rust `&[u8]`.
/// When these macros are used with non-reference parameters (`x: i32`), the trait
/// `FromPyObject` is used instead.
pub trait RefFromPyObject {
    fn with_extracted<F, R>(py: Python, obj: &PyObject, f: F) -> PyResult<R>
        where F: FnOnce(&Self) -> R;
}

impl <T: ?Sized> RefFromPyObject for T
    where for<'a> &'a T: FromPyObject<'a>
{
    #[inline]
    fn with_extracted<F, R>(py: Python, obj: &PyObject, f: F) -> PyResult<R>
        where F: FnOnce(&Self) -> R
    {
        match FromPyObject::extract(py, obj) {
            Ok(val) => Ok(f(val)),
            Err(e) => Err(e)
        }
    }
}

/*
impl <'prepared, T> ExtractPyObject<'prepared> for T
where T: PythonObjectWithCheckedDowncast
{
    type Prepared = PyObject;

    #[inline]
    fn prepare_extract(py: Python, obj: &PyObject) -> PyResult<Self::Prepared> {
        Ok(obj.clone_ref(py))
    }

    #[inline]
    fn extract(py: Python, obj: &'prepared Self::Prepared) -> PyResult<T> {
        Ok(obj.clone_ref(py).cast_into(py)?)
    }
}
*/

/// `ToPyObject` for references: calls to_py_object() on the underlying `T`.
impl <'a, T: ?Sized> ToPyObject for &'a T where T: ToPyObject {
    type ObjectType = T::ObjectType;

    #[inline]
    fn to_py_object(&self, py: Python) -> T::ObjectType {
        <T as ToPyObject>::to_py_object(*self, py)
    }

    #[inline]
    fn into_py_object(self, py: Python) -> T::ObjectType {
        <T as ToPyObject>::to_py_object(self, py)
    }

    #[inline]
    fn with_borrowed_ptr<F, R>(&self, py: Python, f: F) -> R
        where F: FnOnce(*mut ffi::PyObject) -> R
    {
        <T as ToPyObject>::with_borrowed_ptr(*self, py, f)
    }
}

/// `Option::Some<T>` is converted like `T`.
/// `Option::None` is converted to Python `None`.
impl <T> ToPyObject for Option<T> where T: ToPyObject {
    type ObjectType = PyObject;

    fn to_py_object(&self, py: Python) -> PyObject {
        match *self {
            Some(ref val) => val.to_py_object(py).into_object(),
            None => py.None()
        }
    }

    fn into_py_object(self, py: Python) -> PyObject {
        match self {
            Some(val) => val.into_py_object(py).into_object(),
            None => py.None()
        }
    }
}

/// If the python value is None, returns `Option::None`.
/// Otherwise, converts the python value to `T` and returns `Some(T)`.
impl <'s, T> FromPyObject<'s> for Option<T> where T: FromPyObject<'s> {
    fn extract(py: Python, obj: &'s PyObject) -> PyResult<Self> {
        if obj.as_ptr() == unsafe { ffi::Py_None() } {
            Ok(None)
        } else {
            match T::extract(py, obj) {
                Ok(v) => Ok(Some(v)),
                Err(e) => Err(e)
            }
        }
    }
}

/*
impl <'prepared, T> ExtractPyObject<'prepared> for Option<T>
where T: ExtractPyObject<'prepared>
{
    type Prepared = Option<T::Prepared>;

    fn prepare_extract(py: Python, obj: &PyObject) -> PyResult<Self::Prepared> {
        if obj.as_ptr() == unsafe { ffi::Py_None() } {
            Ok(None)
        } else {
            Ok(Some(T::prepare_extract(py, obj)?))
        }
    }

    fn extract(py: Python, obj: &'prepared Self::Prepared) -> PyResult<Option<T>> {
        match *obj {
            Some(ref inner) => {
                match T::extract(py, inner) {
                    Ok(v) => Ok(Some(v)),
                    Err(e) => Err(e)
                }
            },
            None => Ok(None)
        }
    }
}
*/

