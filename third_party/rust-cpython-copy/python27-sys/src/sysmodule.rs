use libc::{c_int, c_char};
use object::PyObject;

#[cfg_attr(windows, link(name="pythonXY"))] extern "C" {
    pub fn PySys_GetObject(arg1: *mut c_char) -> *mut PyObject;
    pub fn PySys_SetObject(arg1: *mut c_char, arg2: *mut PyObject) -> c_int;
    pub fn PySys_SetArgv(arg1: c_int, arg2: *mut *mut c_char) -> ();
    pub fn PySys_SetArgvEx(arg1: c_int, arg2: *mut *mut c_char, arg3: c_int) -> ();
    pub fn PySys_SetPath(arg1: *mut c_char) -> ();
    pub fn PySys_WriteStdout(format: *const c_char, ...) -> ();
    pub fn PySys_WriteStderr(format: *const c_char, ...) -> ();
    pub fn PySys_ResetWarnOptions() -> ();
    pub fn PySys_AddWarnOption(arg1: *mut c_char) -> ();
    pub fn PySys_HasWarnOptions() -> c_int;
}
