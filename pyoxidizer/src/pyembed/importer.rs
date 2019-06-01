// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/* This module defines a Python meta path importer for importing from a self-contained binary. */

use std::collections::{HashMap, HashSet};
use std::ffi::CStr;
use std::io::Cursor;

use byteorder::{LittleEndian, ReadBytesExt};
use cpython::exc::{KeyError, ValueError};
use cpython::{
    py_class, py_class_impl, py_coerce_item, py_fn, PyBool, PyErr, PyList, PyModule, PyObject,
    PyResult, PyString, Python, PythonObject, ToPyObject,
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

/// Represents which importer to use for known modules.
#[derive(Debug)]
enum KnownModuleFlavor {
    Builtin,
    Frozen,
    InMemory,
}

type KnownModules = HashMap<&'static str, KnownModuleFlavor>;

/// State associated with each importer module instance.
///
/// We write per-module state to per-module instances of this struct so
/// we don't rely on global variables and so multiple importer modules can
/// exist without issue.
#[derive(Debug)]
struct ModuleState {
    /// Raw data constituting Python module source code.
    py_data: &'static [u8],

    /// Raw data constituting Python module bytecode.
    pyc_data: &'static [u8],

    /// Handle on the BuiltinImporter meta path importer.
    builtin_importer: Option<PyObject>,

    /// Handle on the FrozenImporter meta path importer.
    frozen_importer: Option<PyObject>,

    /// Stores mapping of module name to module type.
    ///
    /// This facilitates dispatching to an importer with a single lookup instead
    /// of iterating over all importers.
    known_modules: Option<Box<KnownModules>>,
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

/// Garbage collection function for our importer module.
pub extern "C" fn pymodules_clear(m: *mut pyffi::PyObject) -> libc::c_int {
    let state = unsafe { pyffi::PyModule_GetState(m) as *mut ModuleState };

    if state.is_null() {
        return 0;
    }

    // py_data and pyc_data are simple Rust refs. Don't need to do anything special.

    // We need to destroy references to PyObject in state otherwise we will leak them.
    unsafe {
        (*state).builtin_importer = None;
        (*state).frozen_importer = None;
    }

    // Need to drop Rust values that are owned by this instance.
    unsafe {
        (*state).known_modules = None;
    }

    0
}

static mut MODULE_DEF: pyffi::PyModuleDef = pyffi::PyModuleDef {
    m_base: pyffi::PyModuleDef_HEAD_INIT,
    m_name: PYMODULES_NAME.as_ptr() as *const _,
    m_doc: DOC.as_ptr() as *const _,
    m_size: std::mem::size_of::<ModuleState>() as isize,
    m_methods: 0 as *mut _,
    m_slots: 0 as *mut _,
    m_traverse: None,
    m_clear: Some(pymodules_clear),
    m_free: None,
};

/// Initialize the Python module object.
///
/// This is called as part of the PyInit_* function to create the internal
/// module object for the interpreter.
///
/// This receives a handle to the current Python interpreter and just-created
/// Python module instance. It populates the internal module state and registers
/// a _setup() on the module object for usage by Python.
///
/// Because this function accesses NEXT_MODULE_STATE, it should only be
/// called during interpreter initialization.
fn module_init(py: Python, m: &PyModule) -> PyResult<()> {
    let mut state = get_module_state(py, m)?;

    state.builtin_importer = None;
    state.frozen_importer = None;
    state.known_modules = None;

    unsafe {
        state.py_data = (*NEXT_MODULE_STATE).py_data;
        state.pyc_data = (*NEXT_MODULE_STATE).pyc_data;
    }

    m.add(
        py,
        "_setup",
        py_fn!(py, module_setup(m: PyModule, sys_module: PyModule)),
    )?;

    Ok(())
}

/// Called after module import/initialization to configure the importing mechanism.
///
/// This does the heavy work of configuring the importing mechanism.
///
/// This function should only be called once as part of
/// _frozen_importlib_external._install_external_importers().
fn module_setup(py: Python, m: PyModule, sys_module: PyModule) -> PyResult<PyObject> {
    let mut state = get_module_state(py, &m)?;

    let meta_path = sys_module.get(py, "meta_path")?;

    // We should be executing as part of
    // _frozen_importlib_external._install_external_importers().
    // _frozen_importlib._install() should have already been called and set up
    // sys.meta_path with [BuiltinImporter, FrozenImporter]. Those should be the
    // only meta path importers present.

    let meta_path = meta_path.cast_as::<PyList>(py)?;

    if meta_path.len(py) != 2 {
        return Err(PyErr::new::<ValueError, _>(
            py,
            "sys.meta_path does not contain 2 values",
        ));
    }

    let builtin_importer = meta_path.get_item(py, 0);
    let frozen_importer = meta_path.get_item(py, 1);

    state.builtin_importer = Some(builtin_importer);
    state.frozen_importer = Some(frozen_importer);

    let py_modules = match parse_modules_blob(state.py_data) {
        Ok(value) => value,
        Err(msg) => return Err(PyErr::new::<ValueError, _>(py, msg)),
    };

    let pyc_modules = match parse_modules_blob(state.pyc_data) {
        Ok(value) => value,
        Err(msg) => return Err(PyErr::new::<ValueError, _>(py, msg)),
    };

    // Populate our known module lookup table with entries from builtins, frozens, and
    // finally us. Last write wins and has the same effect as registering our
    // meta path importer first. This should be safe. If nothing else, it allows
    // some builtins to be overwritten by .py implemented modules.
    let mut known_modules = Box::new(KnownModules::with_capacity(pyc_modules.len() + 10));

    for i in 0.. {
        let record = unsafe { pyffi::PyImport_Inittab.offset(i) };

        if unsafe { *record }.name.is_null() {
            break;
        }

        let name = unsafe { CStr::from_ptr((*record).name as _) };
        let name_str = match name.to_str() {
            Ok(v) => v,
            Err(_) => {
                return Err(PyErr::new::<ValueError, _>(
                    py,
                    "unable to parse PyImport_Inittab",
                ));
            }
        };

        known_modules.insert(name_str, KnownModuleFlavor::Builtin);
    }

    for i in 0.. {
        let record = unsafe { pyffi::PyImport_FrozenModules.offset(i) };

        if unsafe { *record }.name.is_null() {
            break;
        }

        let name = unsafe { CStr::from_ptr((*record).name as _) };
        let name_str = match name.to_str() {
            Ok(v) => v,
            Err(_) => {
                return Err(PyErr::new::<ValueError, _>(
                    py,
                    "unable to parse PyImport_FrozenModules",
                ));
            }
        };

        known_modules.insert(name_str, KnownModuleFlavor::Frozen);
    }

    // TODO consider baking set of packages into embedded data.
    let mut packages: HashSet<&'static str> = HashSet::with_capacity(pyc_modules.len());

    for key in py_modules.keys() {
        known_modules.insert(key, KnownModuleFlavor::InMemory);
        populate_packages(&mut packages, key);
    }

    for key in pyc_modules.keys() {
        known_modules.insert(key, KnownModuleFlavor::InMemory);
        populate_packages(&mut packages, key);
    }

    let modules = ModulesType::create_instance(py, py_modules, pyc_modules, packages)?;

    m.add(py, "MODULES", modules)?;

    state.known_modules = Some(known_modules);

    Ok(py.None())
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

    match module_init(py, &module) {
        Ok(()) => module.into_object().steal_ptr(),
        Err(e) => {
            e.restore(py);
            std::ptr::null_mut()
        }
    }
}
