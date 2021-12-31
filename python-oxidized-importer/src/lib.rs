// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! oxidized_importer Python extension.

mod conversion;
#[allow(clippy::needless_option_as_deref)]
mod importer;
#[cfg(windows)]
mod memory_dll;
mod package_metadata;
#[allow(clippy::needless_option_as_deref)]
mod path_entry_finder;
mod pkg_resources;
#[allow(clippy::needless_option_as_deref)]
mod python_resource_collector;
mod python_resource_types;
mod python_resources;
mod resource_reader;
mod resource_scanning;
#[cfg(feature = "zipimport")]
#[allow(clippy::needless_option_as_deref)]
mod zip_import;

pub use crate::{
    importer::{
        install_path_hook, remove_external_importers, replace_meta_path_importers, ImporterState,
        OxidizedFinder,
    },
    python_resource_collector::PyTempDir,
    python_resources::{PackedResourcesSource, PythonResourcesState},
};

#[cfg(feature = "zipimport")]
pub use crate::zip_import::{OxidizedZipFinder, ZipIndex};

use {
    crate::{
        path_entry_finder::OxidizedPathEntryFinder,
        pkg_resources::{register_pkg_resources_with_module, OxidizedPkgResourcesProvider},
        python_resources::OxidizedResource,
        resource_reader::OxidizedResourceReader,
    },
    pyo3::{
        exceptions::{PyImportError, PyValueError},
        ffi as pyffi,
        prelude::*,
        AsPyPointer, FromPyPointer,
    },
};

/// Name of Python extension module.
pub const OXIDIZED_IMPORTER_NAME_STR: &str = "oxidized_importer";

/// Null terminated [OXIDIZED_IMPORTER_NAME_STR].
pub const OXIDIZED_IMPORTER_NAME: &[u8] = b"oxidized_importer\0";

const DOC: &[u8] = b"A highly-performant importer implemented in Rust\0";

static mut MODULE_DEF: pyffi::PyModuleDef = pyffi::PyModuleDef {
    m_base: pyffi::PyModuleDef_HEAD_INIT,
    m_name: OXIDIZED_IMPORTER_NAME.as_ptr() as *const _,
    m_doc: DOC.as_ptr() as *const _,
    m_size: std::mem::size_of::<ModuleState>() as isize,
    m_methods: 0 as *mut _,
    m_slots: 0 as *mut _,
    m_traverse: None,
    m_clear: None,
    m_free: None,
};

/// State associated with each importer module instance.
///
/// We write per-module state to per-module instances of this struct so
/// we don't rely on global variables and so multiple importer modules can
/// exist without issue.
#[derive(Debug)]
pub(crate) struct ModuleState {
    /// Whether the module has been initialized.
    pub(crate) initialized: bool,
}

/// Obtain the module state for an instance of our importer module.
///
/// Creates a Python exception on failure.
///
/// Doesn't do type checking that the PyModule is of the appropriate type.
pub(crate) fn get_module_state(m: &PyModule) -> Result<&mut ModuleState, PyErr> {
    let ptr = m.as_ptr();
    let state = unsafe { pyffi::PyModule_GetState(ptr) as *mut ModuleState };

    if state.is_null() {
        return Err(PyValueError::new_err("unable to retrieve module state"));
    }

    Ok(unsafe { &mut *state })
}

/// Module initialization function.
///
/// This creates the Python module object.
///
/// We don't use the macros in the pyo3 crate because they are somewhat
/// opinionated about how things should work. e.g. they call
/// PyEval_InitThreads(), which is undesired. We want total control.
#[allow(non_snake_case)]
#[no_mangle]
pub extern "C" fn PyInit_oxidized_importer() -> *mut pyffi::PyObject {
    let py = unsafe { Python::assume_gil_acquired() };

    let module = unsafe { pyffi::PyModule_Create(&mut MODULE_DEF) } as *mut pyffi::PyObject;

    if module.is_null() {
        return module;
    }

    let module = match unsafe { PyModule::from_owned_ptr_or_err(py, module) } {
        Ok(m) => m,
        Err(e) => {
            e.restore(py);
            return std::ptr::null_mut();
        }
    };

    match module_init(py, module) {
        Ok(()) => module.into_ptr(),
        Err(e) => {
            e.restore(py);
            std::ptr::null_mut()
        }
    }
}

/// Decodes source bytes into a str.
///
/// This is effectively a reimplementation of
/// importlib._bootstrap_external.decode_source().
#[pyfunction]
pub(crate) fn decode_source<'p>(
    py: Python,
    io_module: &'p PyModule,
    source_bytes: &PyAny,
) -> PyResult<&'p PyAny> {
    // .py based module, so can't be instantiated until importing mechanism
    // is bootstrapped.
    let tokenize_module = py.import("tokenize")?;

    let buffer = io_module.getattr("BytesIO")?.call((source_bytes,), None)?;
    let readline = buffer.getattr("readline")?;
    let encoding = tokenize_module
        .getattr("detect_encoding")?
        .call((readline,), None)?;
    let newline_decoder = io_module
        .getattr("IncrementalNewlineDecoder")?
        .call((py.None(), true), None)?;
    let data = source_bytes.call_method("decode", (encoding.get_item(0)?,), None)?;
    newline_decoder.call_method("decode", (data,), None)
}

#[pyfunction]
fn register_pkg_resources(py: Python) -> PyResult<()> {
    register_pkg_resources_with_module(py, py.import("pkg_resources")?)
}

/// Initialize the Python module object.
///
/// This is called as part of the PyInit_* function to create the internal
/// module object for the interpreter.
///
/// This receives a handle to the current Python interpreter and just-created
/// Python module instance. It populates the internal module state and registers
/// functions on the module object for usage by Python.
fn module_init(py: Python, m: &PyModule) -> PyResult<()> {
    // Enforce minimum Python version requirement.
    //
    // Some features likely work on older Python versions. But we can't
    // guarantee it. Let's prevent footguns.
    if py.version_info() < (3, 8) {
        return Err(PyImportError::new_err("module requires Python 3.8+"));
    }

    let mut state = get_module_state(m)?;

    state.initialized = false;

    crate::pkg_resources::init_module(m)?;
    crate::resource_scanning::init_module(m)?;

    m.add_function(wrap_pyfunction!(decode_source, m)?)?;
    m.add_function(wrap_pyfunction!(register_pkg_resources, m)?)?;

    m.add_class::<crate::package_metadata::OxidizedDistribution>()?;
    m.add_class::<OxidizedFinder>()?;
    m.add_class::<OxidizedResource>()?;
    m.add_class::<crate::python_resource_collector::OxidizedResourceCollector>()?;
    m.add_class::<OxidizedResourceReader>()?;
    m.add_class::<OxidizedPathEntryFinder>()?;
    m.add_class::<OxidizedPkgResourcesProvider>()?;
    m.add_class::<crate::python_resource_types::PythonModuleSource>()?;
    m.add_class::<crate::python_resource_types::PythonModuleBytecode>()?;
    m.add_class::<crate::python_resource_types::PythonPackageResource>()?;
    m.add_class::<crate::python_resource_types::PythonPackageDistributionResource>()?;
    m.add_class::<crate::python_resource_types::PythonExtensionModule>()?;

    init_zipimport(m)?;

    Ok(())
}

#[cfg(feature = "zipimport")]
fn init_zipimport(m: &PyModule) -> PyResult<()> {
    m.add_class::<crate::zip_import::OxidizedZipFinder>()?;

    Ok(())
}

#[cfg(not(feature = "zipimport"))]
fn init_zipimport(_m: &PyModule) -> PyResult<()> {
    Ok(())
}
