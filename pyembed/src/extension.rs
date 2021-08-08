// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Python extension initialization.

use {
    crate::{
        importer::{OxidizedFinder, OxidizedPathEntryFinder, OxidizedResourceReader},
        pkg_resources::{
            pkg_resources_find_distributions, register_pkg_resources_with_module,
            OxidizedPkgResourcesProvider,
        },
        python_resources::OxidizedResource,
        resource_scanning::find_resources_in_path,
    },
    cpython::{py_fn, ObjectProtocol, PythonObject},
    python3_sys as oldpyffi,
};

pub const OXIDIZED_IMPORTER_NAME_STR: &str = "oxidized_importer";
pub const OXIDIZED_IMPORTER_NAME: &[u8] = b"oxidized_importer\0";

const DOC: &[u8] = b"A highly-performant importer implemented in Rust\0";

static mut MODULE_DEF: oldpyffi::PyModuleDef = oldpyffi::PyModuleDef {
    m_base: oldpyffi::PyModuleDef_HEAD_INIT,
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
pub(crate) fn get_module_state<'a>(
    py: cpython::Python,
    m: &'a cpython::PyModule,
) -> Result<&'a mut ModuleState, cpython::PyErr> {
    let ptr = m.as_object().as_ptr();
    let state = unsafe { oldpyffi::PyModule_GetState(ptr) as *mut ModuleState };

    if state.is_null() {
        let err = cpython::PyErr::new::<cpython::exc::ValueError, _>(
            py,
            "unable to retrieve module state",
        );
        return Err(err);
    }

    Ok(unsafe { &mut *state })
}

/// Module initialization function.
///
/// This creates the Python module object.
///
/// We don't use the macros in the cpython crate because they are somewhat
/// opinionated about how things should work. e.g. they call
/// PyEval_InitThreads(), which is undesired. We want total control.
#[allow(non_snake_case)]
pub extern "C" fn PyInit_oxidized_importer() -> *mut oldpyffi::PyObject {
    let py = unsafe { cpython::Python::assume_gil_acquired() };

    let module = unsafe { oldpyffi::PyModule_Create(&mut MODULE_DEF) };

    if module.is_null() {
        return module;
    }

    let module = match unsafe {
        cpython::PyObject::from_owned_ptr(py, module).cast_into::<cpython::PyModule>(py)
    } {
        Ok(m) => m,
        Err(e) => {
            cpython::PyErr::from(e).restore(py);
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

/// Decodes source bytes into a str.
///
/// This is effectively a reimplementation of
/// importlib._bootstrap_external.decode_source().
fn decode_source(
    py: cpython::Python,
    io_module: &cpython::PyModule,
    source_bytes: cpython::PyObject,
) -> cpython::PyResult<cpython::PyObject> {
    // .py based module, so can't be instantiated until importing mechanism
    // is bootstrapped.
    let tokenize_module = py.import("tokenize")?;

    let buffer = io_module.call(py, "BytesIO", (&source_bytes,), None)?;
    let readline = buffer.getattr(py, "readline")?;
    let encoding = tokenize_module.call(py, "detect_encoding", (readline,), None)?;
    let newline_decoder = io_module.call(
        py,
        "IncrementalNewlineDecoder",
        (py.None(), py.True()),
        None,
    )?;
    let data = source_bytes.call_method(py, "decode", (encoding.get_item(py, 0)?,), None)?;
    newline_decoder.call_method(py, "decode", (data,), None)
}

fn register_pkg_resources(py: cpython::Python) -> cpython::PyResult<cpython::PyObject> {
    register_pkg_resources_with_module(py, py.import("pkg_resources")?.as_object())
}

/// Initialize the Python module object.
///
/// This is called as part of the PyInit_* function to create the internal
/// module object for the interpreter.
///
/// This receives a handle to the current Python interpreter and just-created
/// Python module instance. It populates the internal module state and registers
/// functions on the module object for usage by Python.
fn module_init(py: cpython::Python, m: &cpython::PyModule) -> cpython::PyResult<()> {
    // Enforce minimum Python version requirement.
    //
    // Some features likely work on older Python versions. But we can't
    // guarantee it. Let's prevent footguns.
    let sys_module = py.import("sys")?;
    let version_info = sys_module.get(py, "version_info")?;

    let major_version = version_info.getattr(py, "major")?.extract::<i32>(py)?;
    let minor_version = version_info.getattr(py, "minor")?.extract::<i32>(py)?;

    if major_version < 3 || minor_version < 8 {
        return Err(cpython::PyErr::new::<cpython::exc::ImportError, _>(
            py,
            "module requires Python 3.8+",
        ));
    }

    let mut state = get_module_state(py, m)?;

    state.initialized = false;

    m.add(
        py,
        "decode_source",
        py_fn!(
            py,
            decode_source(
                io_module: &cpython::PyModule,
                source_bytes: cpython::PyObject
            )
        ),
    )?;
    m.add(
        py,
        "find_resources_in_path",
        py_fn!(py, find_resources_in_path(path: cpython::PyObject)),
    )?;
    m.add(
        py,
        "register_pkg_resources",
        py_fn!(py, register_pkg_resources()),
    )?;
    m.add(
        py,
        "pkg_resources_find_distributions",
        py_fn!(
            py,
            pkg_resources_find_distributions(
                importer: cpython::PyObject,
                path_item: cpython::PyString,
                only: Option<bool> = false,
            )
        ),
    )?;

    m.add(
        py,
        "OxidizedDistribution",
        py.get_type::<crate::package_metadata::OxidizedDistribution>(),
    )?;
    m.add(py, "OxidizedFinder", py.get_type::<OxidizedFinder>())?;
    m.add(py, "OxidizedResource", py.get_type::<OxidizedResource>())?;
    m.add(
        py,
        "OxidizedResourceCollector",
        py.get_type::<crate::python_resource_collector::OxidizedResourceCollector>(),
    )?;
    m.add(
        py,
        "OxidizedResourceReader",
        py.get_type::<OxidizedResourceReader>(),
    )?;
    m.add(
        py,
        "OxidizedPathEntryFinder",
        py.get_type::<OxidizedPathEntryFinder>(),
    )?;
    m.add(
        py,
        "OxidizedPkgResourcesProvider",
        py.get_type::<OxidizedPkgResourcesProvider>(),
    )?;
    m.add(
        py,
        "PythonModuleSource",
        py.get_type::<crate::python_resource_types::PythonModuleSource>(),
    )?;
    m.add(
        py,
        "PythonModuleBytecode",
        py.get_type::<crate::python_resource_types::PythonModuleBytecode>(),
    )?;
    m.add(
        py,
        "PythonPackageResource",
        py.get_type::<crate::python_resource_types::PythonPackageResource>(),
    )?;
    m.add(
        py,
        "PythonPackageDistributionResource",
        py.get_type::<crate::python_resource_types::PythonPackageDistributionResource>(),
    )?;
    m.add(
        py,
        "PythonExtensionModule",
        py.get_type::<crate::python_resource_types::PythonExtensionModule>(),
    )?;

    Ok(())
}
