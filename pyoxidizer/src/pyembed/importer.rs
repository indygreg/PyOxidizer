// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/* This module defines a Python meta path importer for importing from a self-contained binary. */

use std::collections::{HashMap, HashSet};
use std::io::Cursor;

use byteorder::{LittleEndian, ReadBytesExt};
use cpython::exc::{KeyError, ValueError};
use cpython::{
    py_class, py_class_impl, py_coerce_item, PyBool, PyErr, PyModule, PyObject, PyResult, PyString,
    Python, PythonObject, ToPyObject,
};
use python3_sys as pyffi;
use python3_sys::{PyBUF_READ, PyMemoryView_FromMemory};

use super::pyinterp::PYMODULES_NAME;

/// Parse modules blob data into a map of module name to module data.
fn parse_modules_blob(data: &'static [u8]) -> Result<HashMap<&str, &[u8]>, &'static str> {
    if data.len() < 4 {
        return Err("modules data too small");
    }

    let mut reader = Cursor::new(data);

    let count = reader.read_u32::<LittleEndian>().unwrap();
    let mut index = Vec::with_capacity(count as usize);
    let mut total_names_length = 0;

    let mut i = 0;
    while i < count {
        let name_length = reader.read_u32::<LittleEndian>().unwrap() as usize;
        let data_length = reader.read_u32::<LittleEndian>().unwrap() as usize;

        index.push((name_length, data_length));
        total_names_length += name_length;
        i += 1;
    }

    let mut res = HashMap::with_capacity(count as usize);
    let values_start_offset = reader.position() as usize + total_names_length;
    let mut values_current_offset: usize = 0;

    for (name_length, value_length) in index {
        let offset = reader.position() as usize;

        let name = unsafe { std::str::from_utf8_unchecked(&data[offset..offset + name_length]) };

        let value_offset = values_start_offset + values_current_offset;
        let value = &data[value_offset..value_offset + value_length];
        reader.set_position(offset as u64 + name_length as u64);
        values_current_offset += value_length;

        res.insert(name, value);
    }

    Ok(res)
}

#[allow(unused_doc_comments)]
/// Python type to facilitate access to in-memory modules data.
///
/// We /could/ use simple Python data structures (e.g. dict mapping
/// module names to data). However, if we pre-populated a Python dict,
/// we'd need to allocate PyObject instances for every value. This adds
/// overhead to startup. This type minimizes PyObject instantiation to
/// reduce overhead.
py_class!(class ModulesType |py| {
    data py_modules: HashMap<&'static str, &'static [u8]>;
    data pyc_modules: HashMap<&'static str, &'static [u8]>;
    data packages: HashSet<&'static str>;

    def get_source(&self, name: PyString) -> PyResult<PyObject> {
        let key = name.to_string(py)?;

        match self.py_modules(py).get(&*key) {
            Some(value) => {
                let py_value = unsafe {
                    let ptr = PyMemoryView_FromMemory(value.as_ptr() as * mut i8, value.len() as isize, PyBUF_READ);
                    PyObject::from_owned_ptr_opt(py, ptr)
                }.unwrap();

                Ok(py_value)
            },
            None => Err(PyErr::new::<KeyError, _>(py, "module not available"))
        }
    }

    def get_code(&self, name: PyString) -> PyResult<PyObject> {
        let key = name.to_string(py)?;

        match self.pyc_modules(py).get(&*key) {
            Some(value) => {
                let py_value = unsafe {
                    let ptr = PyMemoryView_FromMemory(value.as_ptr() as * mut i8, value.len() as isize, PyBUF_READ);
                    PyObject::from_owned_ptr_opt(py, ptr)
                }.unwrap();

                Ok(py_value)
            },
            None => Err(PyErr::new::<KeyError, _>(py, "module not available"))
        }
    }

    def has_module(&self, name: PyString) -> PyResult<PyBool> {
        let key = name.to_string(py)?;

        if self.py_modules(py).contains_key(&*key) {
            return Ok(true.to_py_object(py));
        }

        if self.pyc_modules(py).contains_key(&*key) {
            return Ok(true.to_py_object(py));
        }

        Ok(false.to_py_object(py))
    }

    def is_package(&self, name: PyString) -> PyResult<PyBool> {
        let key = name.to_string(py)?;

        Ok(if self.packages(py).contains(&*key) {
            true.to_py_object(py)
        } else {
            false.to_py_object(py)
        })
    }
});

fn populate_packages(packages: &mut HashSet<&'static str>, name: &'static str) {
    let mut search = name;

    while let Some(idx) = search.rfind('.') {
        packages.insert(&search[0..idx]);
        search = &search[0..idx];
    }
}

const DOC: &[u8] = b"Binary representation of Python modules\0";

/// Represents global module state to be passed at interpreter initialization time.
#[derive(Debug)]
pub struct InitModuleState {
    /// Raw data constituting Python module source code.
    pub py_data: &'static [u8],

    /// Raw data constituting Python module bytecode.
    pub pyc_data: &'static [u8],
}

/// Holds reference to next module state struct.
///
/// This module state will be copied into the module's state when the
/// Python module is initialized.
pub static mut NEXT_MODULE_STATE: *const InitModuleState = std::ptr::null();

/// State associated with each importer module instance.
///
/// We write per-module state to per-module instances of this struct so
/// we don't rely on global variables and so multiple importer modules can
/// exist without issue.
#[derive(Debug, Clone)]
struct ModuleState {
    /// Raw data constituting Python module source code.
    pub py_data: &'static [u8],

    /// Raw data constituting Python module bytecode.
    pub pyc_data: &'static [u8],
}

/// Obtain the module state for an instance of our importer module.
///
/// Creates a Python exception on failure.
///
/// Doesn't do type checking that the PyModule is of the appropriate type.
fn get_module_state<'a>(py: Python, m: &'a PyModule) -> Result<&'a mut ModuleState, PyErr> {
    let ptr = m.as_object().as_ptr();
    let state = unsafe { pyffi::PyModule_GetState(ptr) as *mut ModuleState };

    if state.is_null() {
        let err = PyErr::new::<ValueError, _>(py, "unable to retrieve module state");
        return Err(err);
    }

    Ok(unsafe { &mut *state })
}

static mut MODULE_DEF: pyffi::PyModuleDef = pyffi::PyModuleDef {
    m_base: pyffi::PyModuleDef_HEAD_INIT,
    m_name: PYMODULES_NAME.as_ptr() as *const _,
    m_doc: DOC.as_ptr() as *const _,
    m_size: std::mem::size_of::<ModuleState>() as isize,
    m_methods: 0 as *mut _,
    m_slots: 0 as *mut _,
    m_traverse: None,
    m_clear: None,
    m_free: None,
};

/// Initialize the Python module object.
///
/// This is called as part of the PyInit_* function to create the internal
/// module object for the interpreter.
///
/// This receives a handle to the current Python interpreter and just-created
/// Python module instance. It populates the internal module state and the
/// external, Python-facing module attributes.
///
/// Because this function accesses NEXT_MODULE_STATE, it should only be
/// called during interpreter initialization.
fn internal_init(py: Python, m: &PyModule) -> PyResult<()> {
    let mut state = get_module_state(py, m)?;

    unsafe {
        state.py_data = (*NEXT_MODULE_STATE).py_data;
        state.pyc_data = (*NEXT_MODULE_STATE).pyc_data;
    }

    let py_modules = match parse_modules_blob(state.py_data) {
        Ok(value) => value,
        Err(msg) => return Err(PyErr::new::<ValueError, _>(py, msg)),
    };

    let pyc_modules = match parse_modules_blob(state.pyc_data) {
        Ok(value) => value,
        Err(msg) => return Err(PyErr::new::<ValueError, _>(py, msg)),
    };

    // TODO consider baking set of packages into embedded data.
    let mut packages: HashSet<&'static str> = HashSet::with_capacity(pyc_modules.len());

    for key in py_modules.keys() {
        populate_packages(&mut packages, key);
    }

    for key in pyc_modules.keys() {
        populate_packages(&mut packages, key);
    }

    let modules = ModulesType::create_instance(py, py_modules, pyc_modules, packages)?;

    m.add(py, "MODULES", modules)?;

    Ok(())
}

/// Module initialization function.
///
/// This creates the Python module object.
///
/// We don't use the macros in the cpython crate because they are somewhat
/// opinionated about how things should work. e.g. they call
/// PyEval_InitThreads(), which is undesired. We want total control.
#[allow(non_snake_case)]
pub extern "C" fn PyInit__pymodules() -> *mut pyffi::PyObject {
    let py = unsafe { cpython::Python::assume_gil_acquired() };
    let module = unsafe { pyffi::PyModule_Create(&mut MODULE_DEF) };

    if module.is_null() {
        return module;
    }

    let module = match unsafe { PyObject::from_owned_ptr(py, module).cast_into::<PyModule>(py) } {
        Ok(m) => m,
        Err(e) => {
            PyErr::from(e).restore(py);
            return std::ptr::null_mut();
        }
    };

    match internal_init(py, &module) {
        Ok(()) => module.into_object().steal_ptr(),
        Err(e) => {
            e.restore(py);
            std::ptr::null_mut()
        }
    }
}
