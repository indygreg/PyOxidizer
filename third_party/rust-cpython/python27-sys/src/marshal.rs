use libc::{c_char, c_int, c_long, FILE};
use object::PyObject;
use pyport::Py_ssize_t;

// 1 -> 2 in df88846ebca9186514e86bc2067242233ade4608 (Python 2.5)
pub const Py_MARSHAL_VERSION: c_int = 2;

#[cfg_attr(windows, link(name="pythonXY"))] extern "C" {
    pub fn PyMarshal_WriteLongToFile(arg1: c_long,
                                     arg2: *mut FILE,
                                     arg3: c_int);
    pub fn PyMarshal_WriteObjectToFile(arg1: *mut PyObject,
                                       arg2: *mut FILE,
                                       arg3: c_int);
    pub fn PyMarshal_WriteObjectToString(arg1: *mut PyObject,
                                         arg2: c_int) -> *mut PyObject;
    pub fn PyMarshal_ReadObjectFromString(arg1: *const c_char,
                                          arg2: Py_ssize_t) -> *mut PyObject;
}

#[cfg(not(Py_LIMITED_API))]
#[cfg_attr(windows, link(name="pythonXY"))] extern "C" {
    pub fn PyMarshal_ReadLongFromFile(arg1: *mut FILE) -> c_long;
    pub fn PyMarshal_ReadShortFromFile(arg1: *mut FILE) -> c_int;
    pub fn PyMarshal_ReadObjectFromFile(arg1: *mut FILE) -> *mut PyObject;
    pub fn PyMarshal_ReadLastObjectFromFile(arg1: *mut FILE) -> *mut PyObject;
}
