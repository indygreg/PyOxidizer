// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Functionality for a Python importer.

This module defines a Python meta path importer and associated functionality
for importing Python modules from memory.
*/

use {
    super::pyinterp::PYOXIDIZER_IMPORTER_NAME,
    super::python_resources::{PythonImporterState, PythonModuleLocation},
    cpython::exc::{FileNotFoundError, ImportError, RuntimeError, ValueError},
    cpython::{
        py_class, py_class_impl, py_coerce_item, py_fn, NoArgs, ObjectProtocol, PyClone, PyDict,
        PyErr, PyList, PyModule, PyObject, PyResult, PyString, PyTuple, Python, PythonObject,
        ToPyObject,
    },
    python3_sys as pyffi,
    python3_sys::{PyBUF_READ, PyMemoryView_FromMemory},
    std::cell::RefCell,
    std::collections::HashMap,
    std::sync::Arc,
};

/// Obtain a Python memoryview referencing a memory slice.
///
/// New memoryview allows Python to access the underlying memory without
/// copying it.
#[inline]
fn get_memory_view(py: Python, data: &Option<&'static [u8]>) -> Option<PyObject> {
    if let Some(data) = data {
        let ptr =
            unsafe { PyMemoryView_FromMemory(data.as_ptr() as _, data.len() as _, PyBUF_READ) };
        unsafe { PyObject::from_owned_ptr_opt(py, ptr) }
    } else {
        None
    }
}

#[allow(unused_doc_comments, clippy::too_many_arguments)]
/// Python type to import modules.
///
/// This type implements the importlib.abc.MetaPathFinder interface for
/// finding/loading modules. It supports loading various flavors of modules,
/// allowing it to be the only registered sys.meta_path importer.
py_class!(class PyOxidizerFinder |py| {
    data imp_module: PyModule;
    data marshal_loads: PyObject;
    data builtin_importer: PyObject;
    data frozen_importer: PyObject;
    data call_with_frames_removed: PyObject;
    data module_spec_type: PyObject;
    data decode_source: PyObject;
    data exec_fn: PyObject;
    data importer_state: PythonImporterState;
    data resource_readers: RefCell<Box<HashMap<String, PyObject>>>;

    // Start of importlib.abc.MetaPathFinder interface.

    def find_spec(&self, fullname: &PyString, path: &PyObject, target: Option<PyObject> = None) -> PyResult<PyObject> {
        let key = fullname.to_string(py)?;

        if let Some(location) = self.importer_state(py).modules.get(&*key) {
            match location {
                PythonModuleLocation::Builtin => {
                    // BuiltinImporter.find_spec() always returns None if `path` is defined.
                    // And it doesn't use `target`. So don't proxy these values.
                    self.builtin_importer(py).call_method(py, "find_spec", (fullname,), None)
                }
                PythonModuleLocation::Frozen => {
                    self.frozen_importer(py).call_method(py, "find_spec", (fullname, path, target), None)
                }
                PythonModuleLocation::InMemory { .. } => {
                    let is_package = self.importer_state(py).packages.contains(&*key);

                    // TODO consider setting origin and has_location so __file__ will be
                    // populated.

                    let kwargs = PyDict::new(py);
                    kwargs.set_item(py, "is_package", is_package)?;

                    self.module_spec_type(py).call(py, (fullname, self), Some(&kwargs))
                }
            }
        } else {
            Ok(py.None())
        }
    }

    def find_module(&self, fullname: &PyObject, path: &PyObject) -> PyResult<PyObject> {
        let finder = self.as_object();
        let find_spec = finder.getattr(py, "find_spec")?;
        let spec = find_spec.call(py, (fullname, path), None)?;

        if spec == py.None() {
            Ok(py.None())
        } else {
            spec.getattr(py, "loader")
        }
    }

    def invalidate_caches(&self) -> PyResult<PyObject> {
        Ok(py.None())
    }

    // End of importlib.abc.MetaPathFinder interface.

    // Start of importlib.abc.Loader interface.

    def create_module(&self, _spec: &PyObject) -> PyResult<PyObject> {
        Ok(py.None())
    }

    def exec_module(&self, module: &PyObject) -> PyResult<PyObject> {
        let name = module.getattr(py, "__name__")?;
        let key = name.extract::<String>(py)?;

        if let Some(location) = self.importer_state(py).modules.get(&*key) {
            match location {
                PythonModuleLocation::Builtin => {
                    self.builtin_importer(py).call_method(py, "exec_module", (module,), None)
                },
                PythonModuleLocation::Frozen => {
                    self.frozen_importer(py).call_method(py, "exec_module", (module,), None)
                },
                PythonModuleLocation::InMemory { bytecode, .. } => {
                    match get_memory_view(py, bytecode) {
                        Some(value) => {
                            let code = self.marshal_loads(py).call(py, (value,), None)?;
                            let exec_fn = self.exec_fn(py);
                            let dict = module.getattr(py, "__dict__")?;

                            self.call_with_frames_removed(py).call(py, (exec_fn, code, dict), None)
                        },
                        None => {
                            Err(PyErr::new::<ImportError, _>(py, ("cannot find code in memory", name)))
                        }
                    }
                },
            }
        } else {
            // Raising here might make more sense, as exec_module() shouldn't
            // be called on the Loader that didn't create the module.
            Ok(py.None())
        }
    }

    // End of importlib.abc.Loader interface.

    // Start of importlib.abc.InspectLoader interface.

    def get_code(&self, fullname: &PyString) -> PyResult<PyObject> {
        let key = fullname.to_string(py)?;

        if let Some(location) = self.importer_state(py).modules.get(&*key) {
            match location {
                PythonModuleLocation::Frozen => {
                    let imp_module = self.imp_module(py);

                    imp_module.call(py, "get_frozen_object", (fullname,), None)
                },
                PythonModuleLocation::InMemory { bytecode, .. } => {
                    match get_memory_view(py, bytecode) {
                        Some(value) => {
                            self.marshal_loads(py).call(py, (value,), None)
                        }
                        None => {
                            Err(PyErr::new::<ImportError, _>(py, ("cannot find code in memory", fullname)))
                        }
                    }
                },
                PythonModuleLocation::Builtin => {
                    Ok(py.None())
                }
            }
        } else {
            Ok(py.None())
        }
    }

    def get_source(&self, fullname: &PyString) -> PyResult<PyObject> {
        let key = fullname.to_string(py)?;

        if let Some(location) = self.importer_state(py).modules.get(&*key) {
            if let PythonModuleLocation::InMemory { source, .. } = location {
                match get_memory_view(py, source) {
                    Some(value) => {
                        // decode_source (from importlib._bootstrap_external)
                        // can't handle memoryview. So we take the memory hit and
                        // cast to bytes.
                        let b = value.call_method(py, "tobytes", NoArgs, None)?;
                        self.decode_source(py).call(py, (b,), None)
                    },
                    None => {
                        Err(PyErr::new::<ImportError, _>(py, ("source not available", fullname)))
                    }
                }
            } else {
                Ok(py.None())
            }
        } else {
            Ok(py.None())
        }
    }

    // End of importlib.abc.InspectLoader interface.

    // Support obtaining ResourceReader instances.
    def get_resource_reader(&self, fullname: &PyString) -> PyResult<PyObject> {
        let key = fullname.to_string(py)?;

        // This should not happen since code below should not be recursive into this
        // function.
        let mut resource_readers = match self.resource_readers(py).try_borrow_mut() {
            Ok(v) => v,
            Err(_) => {
                return Err(PyErr::new::<RuntimeError, _>(py, "resource reader already borrowed"));
            }
        };

        // Return an existing instance if we have one.
        if let Some(reader) = resource_readers.get(&*key) {
            return Ok(reader.clone_ref(py));
        }

        // Only create a reader if the name is a package.
        if self.importer_state(py).packages.contains(&*key) {

            // Not all packages have known resources.
            let resources = match self.importer_state(py).package_resources.get(&*key) {
                Some(v) => v.clone(),
                None => {
                    let h: Box<HashMap<&'static str, &'static [u8]>> = Box::new(HashMap::new());
                    Arc::new(h)
                }
            };

            let reader = PyOxidizerResourceReader::create_instance(py, resources)?.into_object();
            resource_readers.insert(key.to_string(), reader.clone_ref(py));

            Ok(reader)
        } else {
            Ok(py.None())
        }
    }
});

#[allow(unused_doc_comments)]
/// Implements in-memory reading of resource data.
///
/// Implements importlib.abc.ResourceReader.
py_class!(class PyOxidizerResourceReader |py| {
    data resources: Arc<Box<HashMap<&'static str, &'static [u8]>>>;

    /// Returns an opened, file-like object for binary reading of the resource.
    ///
    /// If the resource cannot be found, FileNotFoundError is raised.
    def open_resource(&self, resource: &PyString) -> PyResult<PyObject> {
        let key = resource.to_string(py)?;

        if let Some(data) = self.resources(py).get(&*key) {
            match get_memory_view(py, &Some(data)) {
                Some(mv) => {
                    let io_module = py.import("io")?;
                    let bytes_io = io_module.get(py, "BytesIO")?;

                    bytes_io.call(py, (mv,), None)
                }
                None => Err(PyErr::fetch(py))
            }
        } else {
            Err(PyErr::new::<FileNotFoundError, _>(py, "resource not found"))
        }
    }

    /// Returns the file system path to the resource.
    ///
    /// If the resource does not concretely exist on the file system, raise
    /// FileNotFoundError.
    def resource_path(&self, _resource: &PyString) -> PyResult<PyObject> {
        Err(PyErr::new::<FileNotFoundError, _>(py, "in-memory resources do not have filesystem paths"))
    }

    /// Returns True if the named name is considered a resource. FileNotFoundError
    /// is raised if name does not exist.
    def is_resource(&self, name: &PyString) -> PyResult<PyObject> {
        let key = name.to_string(py)?;

        if self.resources(py).contains_key(&*key) {
            Ok(py.True().as_object().clone_ref(py))
        } else {
            Err(PyErr::new::<FileNotFoundError, _>(py, "resource not found"))
        }
    }

    /// Returns an iterable of strings over the contents of the package.
    ///
    /// Do note that it is not required that all names returned by the iterator be actual resources,
    /// e.g. it is acceptable to return names for which is_resource() would be false.
    ///
    /// Allowing non-resource names to be returned is to allow for situations where how a package
    /// and its resources are stored are known a priori and the non-resource names would be useful.
    /// For instance, returning subdirectory names is allowed so that when it is known that the
    /// package and resources are stored on the file system then those subdirectory names can be
    /// used directly.
    def contents(&self) -> PyResult<PyObject> {
        let resources = self.resources(py);
        let mut names = Vec::with_capacity(resources.len());

        for name in resources.keys() {
            names.push(name.to_py_object(py));
        }

        let names_list = names.to_py_object(py);

        Ok(names_list.as_object().clone_ref(py))
    }
});

const DOC: &[u8] = b"Binary representation of Python modules\0";

/// Represents global module state to be passed at interpreter initialization time.
#[derive(Debug)]
pub struct InitModuleState {
    /// Whether to register the filesystem importer on sys.meta_path.
    pub register_filesystem_importer: bool,

    /// Values to set on sys.path.
    pub sys_paths: Vec<String>,

    /// Raw data constituting Python module source code.
    pub py_modules_data: &'static [u8],

    /// Raw data constituting Python resources data.
    pub py_resources_data: &'static [u8],
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
#[derive(Debug)]
struct ModuleState {
    /// Whether to register PathFinder on sys.meta_path.
    register_filesystem_importer: bool,

    /// Values to set on sys.path.
    sys_paths: Vec<String>,

    /// Raw data constituting Python module source code.
    py_modules_data: &'static [u8],

    /// Raw data constituting Python resources data.
    py_resources_data: &'static [u8],

    /// Whether setup() has been called.
    setup_called: bool,
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

    unsafe {
        state.register_filesystem_importer = (*NEXT_MODULE_STATE).register_filesystem_importer;
        // TODO we could move the value if we wanted to avoid the clone().
        state.sys_paths = (*NEXT_MODULE_STATE).sys_paths.clone();
        state.py_modules_data = (*NEXT_MODULE_STATE).py_modules_data;
        state.py_resources_data = (*NEXT_MODULE_STATE).py_resources_data;
    }

    state.setup_called = false;

    m.add(
        py,
        "_setup",
        py_fn!(
            py,
            module_setup(
                m: PyModule,
                bootstrap_module: PyModule,
                marshal_module: PyModule,
                decode_source: PyObject
            )
        ),
    )?;

    Ok(())
}

/// Called after module import/initialization to configure the importing mechanism.
///
/// This does the heavy work of configuring the importing mechanism.
///
/// This function should only be called once as part of
/// _frozen_importlib_external._install_external_importers().
fn module_setup(
    py: Python,
    m: PyModule,
    bootstrap_module: PyModule,
    marshal_module: PyModule,
    decode_source: PyObject,
) -> PyResult<PyObject> {
    let state = get_module_state(py, &m)?;

    if state.setup_called {
        return Err(PyErr::new::<RuntimeError, _>(
            py,
            "PyOxidizer _setup() already called",
        ));
    }

    state.setup_called = true;

    let imp_module = bootstrap_module.get(py, "_imp")?;
    let imp_module = imp_module.cast_into::<PyModule>(py)?;
    let sys_module = bootstrap_module.get(py, "sys")?;
    let sys_module = sys_module.cast_as::<PyModule>(py)?;
    let meta_path_object = sys_module.get(py, "meta_path")?;

    // We should be executing as part of
    // _frozen_importlib_external._install_external_importers().
    // _frozen_importlib._install() should have already been called and set up
    // sys.meta_path with [BuiltinImporter, FrozenImporter]. Those should be the
    // only meta path importers present.

    let meta_path = meta_path_object.cast_as::<PyList>(py)?;

    if meta_path.len(py) != 2 {
        return Err(PyErr::new::<ValueError, _>(
            py,
            "sys.meta_path does not contain 2 values",
        ));
    }

    let builtin_importer = meta_path.get_item(py, 0);
    let frozen_importer = meta_path.get_item(py, 1);

    let mut importer_state = PythonImporterState::default();

    // Populate our importer with entries from the interpreter then in-memory data
    // structures. The last write wins. That's why we load builtins and frozens before
    // data provided by us.
    if let Err(e) = importer_state.load_interpreter_builtin_modules() {
        return Err(PyErr::new::<ValueError, _>(py, e));
    }

    if let Err(e) = importer_state.load_interpreter_frozen_modules() {
        return Err(PyErr::new::<ValueError, _>(py, e));
    }

    if let Err(e) = importer_state.load_modules_data(state.py_modules_data) {
        return Err(PyErr::new::<ValueError, _>(py, e));
    }

    if let Err(e) = importer_state.load_resources_data(state.py_resources_data) {
        return Err(PyErr::new::<ValueError, _>(py, e));
    }

    let marshal_loads = marshal_module.get(py, "loads")?;
    let call_with_frames_removed = bootstrap_module.get(py, "_call_with_frames_removed")?;
    let module_spec_type = bootstrap_module.get(py, "ModuleSpec")?;

    let builtins_module =
        match unsafe { PyObject::from_borrowed_ptr_opt(py, pyffi::PyEval_GetBuiltins()) } {
            Some(o) => o.cast_into::<PyDict>(py),
            None => {
                return Err(PyErr::new::<ValueError, _>(
                    py,
                    "unable to obtain __builtins__",
                ));
            }
        }?;

    let exec_fn = match builtins_module.get_item(py, "exec") {
        Some(v) => v,
        None => {
            return Err(PyErr::new::<ValueError, _>(
                py,
                "could not obtain __builtins__.exec",
            ));
        }
    };

    let resource_readers: RefCell<Box<HashMap<String, PyObject>>> =
        RefCell::new(Box::new(HashMap::new()));

    let unified_importer = PyOxidizerFinder::create_instance(
        py,
        imp_module,
        marshal_loads,
        builtin_importer,
        frozen_importer,
        call_with_frames_removed,
        module_spec_type,
        decode_source,
        exec_fn,
        importer_state,
        resource_readers,
    )?;
    meta_path_object.call_method(py, "clear", NoArgs, None)?;
    meta_path_object.call_method(py, "append", (unified_importer,), None)?;

    // At this point the importing mechanism is fully initialized to use our
    // unified importer, which handles built-in, frozen, and in-memory imports.

    // Because we're probably running during Py_Initialize() and stdlib modules
    // may not be in-memory, we need to register and configure additional importers
    // here, before continuing with Py_Initialize(), otherwise we may not find
    // the standard library!

    if state.register_filesystem_importer {
        // This is what importlib._bootstrap_external usually does:
        // supported_loaders = _get_supported_file_loaders()
        // sys.path_hooks.extend([FileFinder.path_hook(*supported_loaders)])
        // sys.meta_path.append(PathFinder)
        let frozen_importlib_external = py.import("_frozen_importlib_external")?;

        let loaders =
            frozen_importlib_external.call(py, "_get_supported_file_loaders", NoArgs, None)?;
        let loaders_list = loaders.cast_as::<PyList>(py)?;
        let loaders_vec: Vec<PyObject> = loaders_list.iter(py).collect();
        let loaders_tuple = PyTuple::new(py, loaders_vec.as_slice());

        let file_finder = frozen_importlib_external.get(py, "FileFinder")?;
        let path_hook = file_finder.call_method(py, "path_hook", loaders_tuple, None)?;
        let path_hooks = sys_module.get(py, "path_hooks")?;
        path_hooks.call_method(py, "append", (path_hook,), None)?;

        let path_finder = frozen_importlib_external.get(py, "PathFinder")?;
        let meta_path = sys_module.get(py, "meta_path")?;
        meta_path.call_method(py, "append", (path_finder,), None)?;
    }

    // Ideally we should be calling Py_SetPath() before Py_Initialize() to set sys.path.
    // But we tried to do this and only ran into problems due to string conversions,
    // unwanted side-effects. Updating sys.path directly before it is used by PathFinder
    // (which was just registered above) should have the same effect.

    // Always clear out sys.path.
    let sys_path = sys_module.get(py, "path")?;
    sys_path.call_method(py, "clear", NoArgs, None)?;

    // And repopulate it with entries from the config.
    for path in &state.sys_paths {
        let py_path = PyString::new(py, path.as_str());

        sys_path.call_method(py, "append", (py_path,), None)?;
    }

    Ok(py.None())
}

static mut MODULE_DEF: pyffi::PyModuleDef = pyffi::PyModuleDef {
    m_base: pyffi::PyModuleDef_HEAD_INIT,
    m_name: std::ptr::null(),
    m_doc: std::ptr::null(),
    m_size: std::mem::size_of::<ModuleState>() as isize,
    m_methods: 0 as *mut _,
    m_slots: 0 as *mut _,
    m_traverse: None,
    m_clear: None,
    m_free: None,
};

/// Module initialization function.
///
/// This creates the Python module object.
///
/// We don't use the macros in the cpython crate because they are somewhat
/// opinionated about how things should work. e.g. they call
/// PyEval_InitThreads(), which is undesired. We want total control.
#[allow(non_snake_case)]
pub extern "C" fn PyInit__pyoxidizer_importer() -> *mut pyffi::PyObject {
    let py = unsafe { cpython::Python::assume_gil_acquired() };

    // TRACKING RUST1.32 We can't call as_ptr() in const fn in Rust 1.31.
    unsafe {
        if MODULE_DEF.m_name.is_null() {
            MODULE_DEF.m_name = PYOXIDIZER_IMPORTER_NAME.as_ptr() as *const _;
            MODULE_DEF.m_doc = DOC.as_ptr() as *const _;
        }
    }

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
