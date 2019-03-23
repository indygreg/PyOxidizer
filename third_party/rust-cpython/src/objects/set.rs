// Copyright (c) 2018 Daniel Grunwald, Georges Racinet
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

use ffi;
use python::{Python, PythonObject};
use conversion::ToPyObject;
use objects::PyObject;
use err::{self, PyResult, PyErr};
use std::{mem, collections, hash, cmp, ptr};

/// Represents a Python `set`.
pub struct PySet(PyObject);

pyobject_newtype!(PySet, PySet_Check, PySet_Type);

impl PySet {
    /// Creates a new set from any iterable
    ///
    /// Corresponds to `set(iterable)` in Python.
    pub fn new<I>(py: Python, iterable: I) -> PyResult<PySet> where I: ToPyObject {
        iterable.with_borrowed_ptr(py, |iterable| unsafe {
            err::result_cast_from_owned_ptr(py, ffi::PySet_New(iterable))
        })
    }

    /// Creates an empty set
    ///
    /// Corresponds to `set()` in Python
    #[inline]
        pub fn empty(py: Python) -> PyResult<PySet> {
        unsafe {
            err::result_cast_from_owned_ptr(py,
                ffi::PySet_New(ptr::null_mut()))
        }
    }

    /// Empty an existing set of all values.
    #[inline]
    pub fn clear(&self, py: Python) -> PyResult<()> {
        unsafe { err::error_on_minusone(py,
                     ffi::PySet_Clear(self.0.as_ptr())
                 )}
    }

    /// Return the number of items in the set
    /// This is equivalent to Python `len(self)`
    #[inline]
    pub fn len(&self, _py: Python) -> usize {
        unsafe { ffi::PySet_Size(self.0.as_ptr()) as usize }
    }

    /// Determine if the set contains the specified value.
    /// This is equivalent to the Python expression `value in self`.
    pub fn contains<V>(&self, py: Python, value: V) -> PyResult<bool> where V: ToPyObject {
        value.with_borrowed_ptr(py, |key| unsafe {
            match ffi::PySet_Contains(self.0.as_ptr(), key) {
                1 => Ok(true),
                0 => Ok(false),
                _ => Err(PyErr::fetch(py))
            }
        })
    }

    /// Add a value.
    /// This is equivalent to the Python expression `self.add(value)`.
    pub fn add<V>(&self, py: Python, value: V) -> PyResult<()> where V: ToPyObject {
        value.with_borrowed_ptr(py, |value| unsafe {
            err::error_on_minusone(py,
                ffi::PySet_Add(self.0.as_ptr(), value))
        })
    }

    /// Discard a value
    /// This is equivalent to the Python expression `self.discard(value)`.
    pub fn discard<V>(&self, py: Python, value: V) -> PyResult<()> where V: ToPyObject {
        value.with_borrowed_ptr(py, |value| unsafe {
            err::error_on_minusone(py,
                ffi::PySet_Discard(self.0.as_ptr(), value))
        })
    }

    /// Pop a value
    /// This is equivalent to the Python expression `self.pop(value)`.
    /// We get KeyError if the set is empty
    pub fn pop(&self, py: Python) -> PyResult<PyObject> {
        let as_opt = unsafe {
            PyObject::from_borrowed_ptr_opt(py,
                ffi::PySet_Pop(self.0.as_ptr()))
        };
        match as_opt {
            None => Err(PyErr::fetch(py)),
            Some(obj) => Ok(obj)
        }
    }
}

impl <V, H> ToPyObject for collections::HashSet<V, H>
    where V: hash::Hash+cmp::Eq+ToPyObject,
          H: hash::BuildHasher
{
    type ObjectType = PySet;

    fn to_py_object(&self, py: Python) -> PySet {
        let set = PySet::empty(py).unwrap();
        for value in self {
            set.add(py, value).unwrap();
        };
        set
    }
}

impl <V> ToPyObject for collections::BTreeSet<V>
    where V: cmp::Eq+ToPyObject,
{
    type ObjectType = PySet;

    fn to_py_object(&self, py: Python) -> PySet {
        let set = PySet::empty(py).unwrap();
        for value in self {
            set.add(py, value).unwrap();
        };
        set
    }
}


#[cfg(test)]
mod test {
    use python::{Python, PythonObject};
    use conversion::ToPyObject;
    use objects::PySet;
    use std::collections::{HashSet, BTreeSet};

    #[test]
    fn test_len() {
        let gil = Python::acquire_gil();
        let py = gil.python();
        let mut v = HashSet::new();
        let set = v.to_py_object(py);
        assert_eq!(0, set.len(py));
        v.insert(7);
        let set2 = v.to_py_object(py);
        assert_eq!(1, set2.len(py));
    }

    #[test]
    fn test_contains() {
        let gil = Python::acquire_gil();
        let py = gil.python();
        let mut v = HashSet::new();
        v.insert(7);
        let set = v.to_py_object(py);
        assert!(true, set.contains(py, 7i32).unwrap());
        assert_eq!(false, set.contains(py, 8i32).unwrap());
    }

    #[test]
    fn test_clear() {
        let gil = Python::acquire_gil();
        let py = gil.python();
        let mut v = HashSet::new();
        v.insert(7);
        let set = v.to_py_object(py);
        set.clear(py).unwrap();
        assert_eq!(0, set.len(py));
        assert_eq!(false, set.contains(py, 7i32).unwrap());
    }

    #[test]
    fn test_add() {
        let gil = Python::acquire_gil();
        let py = gil.python();
        let mut v = HashSet::new();
        v.insert(7);
        let set = v.to_py_object(py);
        assert!(set.add(py, 42i32).is_ok());
        assert!(set.contains(py, 42i32).unwrap());
    }

    #[test]
    fn test_add_does_not_update_original_object() {
        let gil = Python::acquire_gil();
        let py = gil.python();
        let mut v = HashSet::new();
        v.insert(7);
        let set = v.to_py_object(py);
        assert!(set.add(py, 42i32).is_ok()); // change
        assert_eq!(None, v.get(&42i32)); // not updated
    }

    #[test]
    fn test_discard() {
        let gil = Python::acquire_gil();
        let py = gil.python();
        let mut v = HashSet::new();
        v.insert(7);
        let set = v.to_py_object(py);
        assert!(set.discard(py, 7i32).is_ok());
        assert_eq!(0, set.len(py));
        assert!(!set.contains(py, 7i32).unwrap());
    }

    #[test]
    fn test_discard_does_not_update_original_object() {
        let gil = Python::acquire_gil();
        let py = gil.python();
        let mut v = HashSet::new();
        v.insert(7);
        let set = v.to_py_object(py);
        assert!(set.discard(py, 7i32).is_ok()); // change
        assert!(v.contains(&7)); // not updated!
    }

    #[test]
    fn test_pop() {
        let gil = Python::acquire_gil();
        let py = gil.python();
        let mut v = HashSet::new();
        v.insert(7);
        let set = v.to_py_object(py);
        let popped = set.pop(py).unwrap();
        let as_int: i32 = popped.extract(py).unwrap();
        assert_eq!(as_int, 7);
        assert!(set.pop(py).is_err());
    }

    #[test]
    fn test_pop_does_not_update_original_object() {
        let gil = Python::acquire_gil();
        let py = gil.python();
        let mut v = HashSet::new();
        v.insert(7);
        let set = v.to_py_object(py);
        assert!(set.pop(py).is_ok()); // change
        assert!(v.contains(&7)); // not updated!
    }

    #[test]
    fn test_btree_set() {
        let gil = Python::acquire_gil();
        let py = gil.python();
        let mut v = BTreeSet::new();
        v.insert(7);
        v.insert(42);
        let set = v.to_py_object(py);
        assert!(set.contains(py, 7).unwrap());
        assert!(set.contains(py, 42).unwrap());
        assert!(!set.contains(py, 31).unwrap());
        // adding an element python side
        assert!(set.add(py, 31).is_ok());
        // original object not updated
        assert!(!v.contains(&31));
    }
}
