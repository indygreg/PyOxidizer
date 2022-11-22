// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Functionality for a Python importer.

This module defines a Python meta path importer and associated functionality
for importing Python modules from memory.
*/

#[cfg(windows)]
use {
    crate::memory_dll::{free_library_memory, get_proc_address_memory, load_library_memory},
    pyo3::exceptions::PySystemError,
    std::ffi::{c_void, CString},
};
use {
    crate::{
        conversion::pyobject_to_pathbuf,
        get_module_state,
        path_entry_finder::OxidizedPathEntryFinder,
        pkg_resources::register_pkg_resources_with_module,
        python_resources::{
            pyobject_to_resource, ModuleFlavor, OxidizedResource, PythonResourcesState,
        },
        resource_reader::OxidizedResourceReader,
        OXIDIZED_IMPORTER_NAME_STR,
    },
    pyo3::{
        exceptions::{PyImportError, PyValueError},
        ffi as pyffi,
        prelude::*,
        types::{PyBytes, PyDict, PyList, PyString, PyTuple},
        AsPyPointer, FromPyPointer, PyNativeType, PyTraverseError, PyVisit,
    },
    python_packaging::resource::BytecodeOptimizationLevel,
    std::sync::Arc,
};

#[cfg(windows)]
#[allow(non_camel_case_types)]
type py_init_fn = extern "C" fn() -> *mut pyffi::PyObject;

/// Implementation of `Loader.create_module()` for in-memory extension modules.
///
/// The equivalent CPython code for importing extension modules is to call
/// `imp.create_dynamic()`. This will:
///
/// 1. Call `_PyImport_FindExtensionObject()`.
/// 2. Call `_PyImport_LoadDynamicModuleWithSpec()` if #1 didn't return anything.
///
/// While `_PyImport_FindExtensionObject()` accepts a `filename` argument, this
/// argument is only used as a key inside an internal dictionary indexing found
/// extension modules. So we can call that function verbatim.
///
/// `_PyImport_LoadDynamicModuleWithSpec()` is more interesting. It takes a
/// `FILE*` for the extension location, so we can't call it. So we need to
/// reimplement it. Documentation of that is inline.
#[cfg(windows)]
fn extension_module_shared_library_create_module(
    resources_state: &PythonResourcesState<u8>,
    py: Python,
    sys_modules: &PyAny,
    spec: &PyAny,
    name_py: &PyAny,
    name: &str,
    library_data: &[u8],
) -> PyResult<Py<PyAny>> {
    let origin = PyString::new(py, "memory");

    let existing_module =
        unsafe { pyffi::_PyImport_FindExtensionObject(name_py.as_ptr(), origin.as_ptr()) };

    // We found an existing module object. Return it.
    if !existing_module.is_null() {
        return Ok(unsafe { PyObject::from_owned_ptr(py, existing_module) });
    }

    // An error occurred calling _PyImport_FindExtensionObjectEx(). Raise it.
    if !unsafe { pyffi::PyErr_Occurred() }.is_null() {
        return Err(PyErr::fetch(py));
    }

    // New module load request. Proceed to _PyImport_LoadDynamicModuleWithSpec()
    // functionality.

    let module = unsafe { load_library_memory(resources_state, library_data) };

    if module.is_null() {
        return Err(PyImportError::new_err((
            "unable to load extension module library from memory",
            name.to_owned(),
        )));
    }

    // Any error past this point should call `MemoryFreeLibrary()` to unload the
    // library.

    load_dynamic_library(py, sys_modules, spec, name_py, name, module).map_err(|e| {
        unsafe {
            free_library_memory(module);
        }
        e
    })
}

#[cfg(unix)]
fn extension_module_shared_library_create_module(
    _resources_state: &PythonResourcesState<u8>,
    _py: Python,
    _sys_modules: &PyAny,
    _spec: &PyAny,
    _name_py: &PyAny,
    _name: &str,
    _library_data: &[u8],
) -> PyResult<Py<PyAny>> {
    panic!("should only be called on Windows");
}

/// Reimplementation of `_PyImport_LoadDynamicModuleWithSpec()`.
#[cfg(windows)]
fn load_dynamic_library(
    py: Python,
    sys_modules: &PyAny,
    spec: &PyAny,
    name_py: &PyAny,
    name: &str,
    library_module: *const c_void,
) -> PyResult<Py<PyAny>> {
    // The init function is `PyInit_<stem>`.
    let last_name_part = if name.contains('.') {
        name.split('.').last().unwrap()
    } else {
        name
    };

    let name_cstring = CString::new(name).unwrap();
    let init_fn_name = CString::new(format!("PyInit_{}", last_name_part)).unwrap();

    let address = unsafe { get_proc_address_memory(library_module, &init_fn_name) };
    if address.is_null() {
        return Err(PyImportError::new_err((
            format!(
                "dynamic module does not define module export function ({})",
                init_fn_name.to_str().unwrap()
            ),
            name.to_owned(),
        )));
    }

    let init_fn: py_init_fn = unsafe { std::mem::transmute(address) };

    // Package context is needed for single-phase init.
    let py_module = unsafe {
        let old_context = pyffi::_Py_PackageContext;
        pyffi::_Py_PackageContext = name_cstring.as_ptr();
        let py_module = init_fn();
        pyffi::_Py_PackageContext = old_context;
        py_module
    };

    // The initialization function will return a new/owned reference for single-phase initialization
    // and a borrowed reference for multi-phase initialization. Since we don't know which form
    // we're using until later, we need to be very careful about handling py_module here! Note
    // that it may be possible to leak an owned reference in the error handling below. This
    // code mimics what CPython does and the leak, if any, likely occurs there as well.

    if py_module.is_null() && unsafe { pyffi::PyErr_Occurred().is_null() } {
        return Err(PySystemError::new_err(format!(
            "initialization of {} failed without raising an exception",
            name
        )));
    }

    if !unsafe { pyffi::PyErr_Occurred().is_null() } {
        unsafe {
            pyffi::PyErr_Clear();
        }
        return Err(PySystemError::new_err(format!(
            "initialization of {} raised unreported exception",
            name
        )));
    }

    if unsafe { pyffi::Py_TYPE(py_module) }.is_null() {
        return Err(PySystemError::new_err(format!(
            "init function of {} returned uninitialized object",
            name
        )));
    }

    // If initialization returned a `PyModuleDef`, this is multi-phase initialization. Construct a
    // module by calling PyModule_FromDefAndSpec(). py_module is a borrowed reference. And
    // PyModule_FromDefAndSpec() returns a new reference. So we don't need to worry about refcounts
    // of py_module.
    if unsafe { pyffi::PyObject_TypeCheck(py_module, &mut pyffi::PyModuleDef_Type) } != 0 {
        let py_module = unsafe {
            pyffi::PyModule_FromDefAndSpec(py_module as *mut pyffi::PyModuleDef, spec.as_ptr())
        };

        return if py_module.is_null() {
            Err(PyErr::fetch(py))
        } else {
            Ok(unsafe { PyObject::from_owned_ptr(py, py_module) })
        };
    }

    // This is the single-phase initialization mechanism. Construct a module by calling
    // PyModule_GetDef(). py_module is a new reference. So we capture it to make sure we don't
    // leak it.
    let py_module = unsafe { PyObject::from_owned_ptr(py, py_module) };

    let mut module_def = unsafe { pyffi::PyModule_GetDef(py_module.as_ptr()) };
    if module_def.is_null() {
        return Err(PySystemError::new_err(format!(
            "initialization of {} did not return an extension module",
            name
        )));
    }

    unsafe {
        (*module_def).m_base.m_init = Some(init_fn);
    }

    // If we wanted to assign __file__ we would do it here.

    let fixup_result = unsafe {
        pyffi::_PyImport_FixupExtensionObject(
            py_module.as_ptr(),
            name_py.as_ptr(),
            name_py.as_ptr(),
            sys_modules.as_ptr(),
        )
    };

    if fixup_result < 0 {
        Err(PyErr::fetch(py))
    } else {
        Ok(py_module)
    }
}

/// Holds state for the custom MetaPathFinder.
pub struct ImporterState {
    /// `imp` Python module.
    pub(crate) imp_module: Py<PyModule>,
    /// `sys` Python module.
    pub(crate) sys_module: Py<PyModule>,
    /// `_io` Python module.
    pub(crate) io_module: Py<PyModule>,
    /// `marshal.loads` Python callable.
    pub(crate) marshal_loads: Py<PyAny>,
    /// `_frozen_importlib.BuiltinImporter` meta path importer for built-in extension modules.
    pub(crate) builtin_importer: Py<PyAny>,
    /// `_frozen_importlib.FrozenImporter` meta path importer for frozen modules.
    pub(crate) frozen_importer: Py<PyAny>,
    /// `importlib._bootstrap._call_with_frames_removed` function.
    pub(crate) call_with_frames_removed: Py<PyAny>,
    /// `importlib._bootstrap.ModuleSpec` class.
    pub(crate) module_spec_type: Py<PyAny>,
    /// Our `decode_source()` function.
    pub(crate) decode_source: Py<PyAny>,
    /// `builtins.exec` function.
    pub(crate) exec_fn: Py<PyAny>,
    /// Bytecode optimization level currently in effect.
    pub(crate) optimize_level: BytecodeOptimizationLevel,
    /// Value to pass to `multiprocessing.set_start_method()` on import of `multiprocessing`.
    ///
    /// If `None`, `set_start_method()` will not be called automatically.
    pub(crate) multiprocessing_set_start_method: Option<String>,
    /// Whether to automatically register ourself with `pkg_resources` when it is imported.
    pub(crate) pkg_resources_import_auto_register: bool,
    /// Holds state about importable resources.
    ///
    /// This field is a PyCapsule and is a glorified wrapper around
    /// a pointer. That pointer refers to heap backed memory.
    ///
    /// The memory behind the pointer can either by owned by us or owned
    /// externally. If owned externally, the memory is likely backed by
    /// the `MainPythonInterpreter` instance that spawned us.
    ///
    /// Storing a pointer this way avoids Rust lifetime checks and allows
    /// us to side-step the requirement that all lifetimes in Python
    /// objects be 'static. This allows us to use proper lifetimes for
    /// the backing memory instead of forcing all resource data to be backed
    /// by 'static.
    pub(crate) resources_state: Py<PyAny>,
}

impl ImporterState {
    fn new<'a>(
        py: Python,
        importer_module: &PyModule,
        bootstrap_module: &PyModule,
        resources_state: Box<PythonResourcesState<'a, u8>>,
    ) -> Result<Self, PyErr> {
        let decode_source = importer_module.getattr("decode_source")?.into_py(py);

        let io_module = py.import("_io")?.into_py(py);
        let marshal_module = py.import("marshal")?;

        let imp_module = bootstrap_module.getattr("_imp")?;
        let imp_module = imp_module.cast_as::<PyModule>()?.into_py(py);
        let sys_module = bootstrap_module.getattr("sys")?;
        let sys_module = sys_module.cast_as::<PyModule>()?;
        let meta_path_object = sys_module.getattr("meta_path")?;

        // We should be executing as part of
        // _frozen_importlib_external._install_external_importers().
        // _frozen_importlib._install() should have already been called and set up
        // sys.meta_path with [BuiltinImporter, FrozenImporter]. Those should be the
        // only meta path importers present.

        let meta_path = meta_path_object.cast_as::<PyList>()?;
        if meta_path.len() < 2 {
            return Err(PyValueError::new_err(
                "sys.meta_path does not contain 2 values",
            ));
        }

        let builtin_importer = meta_path.get_item(0)?.into_py(py);
        let frozen_importer = meta_path.get_item(1)?.into_py(py);

        let marshal_loads = marshal_module.getattr("loads")?.into_py(py);
        let call_with_frames_removed = bootstrap_module
            .getattr("_call_with_frames_removed")?
            .into_py(py);
        let module_spec_type = bootstrap_module.getattr("ModuleSpec")?.into_py(py);

        let builtins_module =
            unsafe { PyDict::from_borrowed_ptr_or_err(py, pyffi::PyEval_GetBuiltins()) }?;

        let exec_fn = match builtins_module.get_item("exec") {
            Some(v) => v,
            None => {
                return Err(PyValueError::new_err("could not obtain __builtins__.exec"));
            }
        }
        .into_py(py);

        let sys_flags = sys_module.getattr("flags")?;
        let sys_module = sys_module.into_py(py);

        let optimize_value = sys_flags.getattr("optimize")?;
        let optimize_value = optimize_value.extract::<i64>()?;

        let optimize_level = match optimize_value {
            0 => Ok(BytecodeOptimizationLevel::Zero),
            1 => Ok(BytecodeOptimizationLevel::One),
            2 => Ok(BytecodeOptimizationLevel::Two),
            _ => Err(PyValueError::new_err(
                "unexpected value for sys.flags.optimize",
            )),
        }?;

        let capsule = unsafe {
            let ptr = pyffi::PyCapsule_New(
                &*resources_state as *const PythonResourcesState<u8> as *mut _,
                std::ptr::null(),
                None,
            );

            if ptr.is_null() {
                return Err(PyValueError::new_err(
                    "unable to convert PythonResourcesState to capsule",
                ));
            }

            PyObject::from_owned_ptr(py, ptr)
        };

        // We store a pointer to the heap memory and take care of destroying
        // it when we are dropped. So we leak the box.
        Box::leak(resources_state);

        Ok(ImporterState {
            imp_module,
            sys_module,
            io_module,
            marshal_loads,
            builtin_importer,
            frozen_importer,
            call_with_frames_removed,
            module_spec_type,
            decode_source,
            exec_fn,
            optimize_level,
            multiprocessing_set_start_method: None,
            // TODO value should come from config.
            pkg_resources_import_auto_register: true,
            resources_state: capsule,
        })
    }

    /// Perform garbage collection traversal on this instance.
    ///
    /// Do NOT make this pub(crate) because in most cases holders do not need to traverse
    /// into this since they have an `Arc<T>` reference, not a `Py<T>` reference. Only the
    /// canonical holder of this instance should call Python's gc visiting.
    fn gc_traverse(&self, visit: PyVisit) -> Result<(), PyTraverseError> {
        visit.call(&self.imp_module)?;
        visit.call(&self.sys_module)?;
        visit.call(&self.io_module)?;
        visit.call(&self.marshal_loads)?;
        visit.call(&self.builtin_importer)?;
        visit.call(&self.frozen_importer)?;
        visit.call(&self.call_with_frames_removed)?;
        visit.call(&self.module_spec_type)?;
        visit.call(&self.decode_source)?;
        visit.call(&self.exec_fn)?;
        visit.call(&self.resources_state)?;

        Ok(())
    }

    /// Obtain the `PythonResourcesState` associated with this instance.
    #[inline]
    pub fn get_resources_state<'a>(&self) -> &PythonResourcesState<'a, u8> {
        let ptr =
            unsafe { pyffi::PyCapsule_GetPointer(self.resources_state.as_ptr(), std::ptr::null()) };

        if ptr.is_null() {
            panic!("null pointer in resources state capsule");
        }

        unsafe { &*(ptr as *const PythonResourcesState<u8>) }
    }

    /// Obtain a mutable `PythonResourcesState` associated with this instance.
    ///
    /// There is no run-time checking for mutation exclusion. So don't like this
    /// leak outside of a single call site that needs to access it!
    #[allow(clippy::mut_from_ref)]
    pub fn get_resources_state_mut<'a>(&self) -> &mut PythonResourcesState<'a, u8> {
        let ptr =
            unsafe { pyffi::PyCapsule_GetPointer(self.resources_state.as_ptr(), std::ptr::null()) };

        if ptr.is_null() {
            panic!("null pointer in resources state capsule");
        }

        unsafe { &mut *(ptr as *mut PythonResourcesState<u8>) }
    }

    /// Set the value to call `multiprocessing.set_start_method()` with on import of `multiprocessing`.
    #[allow(unused)]
    pub fn set_multiprocessing_set_start_method(&mut self, value: Option<String>) {
        self.multiprocessing_set_start_method = value;
    }
}

impl Drop for ImporterState {
    fn drop(&mut self) {
        let ptr =
            unsafe { pyffi::PyCapsule_GetPointer(self.resources_state.as_ptr(), std::ptr::null()) };

        if !ptr.is_null() {
            unsafe {
                drop(Box::from_raw(ptr as *mut PythonResourcesState<u8>));
            }
        }
    }
}

/// Python type to import modules.
///
/// This type implements the importlib.abc.MetaPathFinder interface for
/// finding/loading modules. It supports loading various flavors of modules,
/// allowing it to be the only registered sys.meta_path importer.
#[pyclass(module = "oxidized_importer")]
pub struct OxidizedFinder {
    pub(crate) state: Arc<ImporterState>,
}

impl OxidizedFinder {
    pub(crate) fn get_state(&self) -> Arc<ImporterState> {
        self.state.clone()
    }

    /// Construct an instance from a module and resources state.
    pub fn new_from_module_and_resources<'a>(
        py: Python,
        m: &PyModule,
        resources_state: Box<PythonResourcesState<'a, u8>>,
        importer_state_callback: Option<impl FnOnce(&mut ImporterState)>,
    ) -> PyResult<OxidizedFinder> {
        let bootstrap_module = py.import("_frozen_importlib")?;

        let mut importer_state = Arc::new(ImporterState::new(
            py,
            m,
            bootstrap_module,
            resources_state,
        )?);

        if let Some(cb) = importer_state_callback {
            let state_ref = Arc::<ImporterState>::get_mut(&mut importer_state)
                .expect("Arc::get_mut() should work");
            cb(state_ref);
        }

        Ok(OxidizedFinder {
            state: importer_state,
        })
    }
}

#[pymethods]
impl OxidizedFinder {
    fn __traverse__(&self, visit: PyVisit) -> Result<(), PyTraverseError> {
        self.state.gc_traverse(visit)
    }

    // Start of importlib.abc.MetaPathFinder interface.

    #[args(target = "None")]
    fn find_spec<'p>(
        slf: &'p PyCell<Self>,
        fullname: String,
        path: &PyAny,
        target: Option<&PyAny>,
    ) -> PyResult<&'p PyAny> {
        let py = slf.py();
        let finder = slf.borrow();

        let module = match finder
            .state
            .get_resources_state()
            .resolve_importable_module(&fullname, finder.state.optimize_level)
        {
            Some(module) => module,
            None => return Ok(py.None().into_ref(py)),
        };

        match module.flavor {
            ModuleFlavor::Extension | ModuleFlavor::SourceBytecode => module.resolve_module_spec(
                py,
                finder.state.module_spec_type.clone_ref(py).into_ref(py),
                slf,
                finder.state.optimize_level,
            ),
            ModuleFlavor::Builtin => {
                // BuiltinImporter.find_spec() always returns None if `path` is defined.
                // And it doesn't use `target`. So don't proxy these values.
                Ok(finder
                    .state
                    .builtin_importer
                    .call_method(py, "find_spec", (fullname,), None)?
                    .into_ref(py))
            }
            ModuleFlavor::Frozen => Ok(finder
                .state
                .frozen_importer
                .call_method(py, "find_spec", (fullname, path, target), None)?
                .into_ref(py)),
        }
    }

    fn find_module<'p>(
        slf: &'p PyCell<Self>,
        fullname: &PyAny,
        path: &PyAny,
    ) -> PyResult<&'p PyAny> {
        let find_spec = slf.getattr("find_spec")?;
        let spec = find_spec.call((fullname, path), None)?;

        if spec.is_none() {
            Ok(slf.py().None().into_ref(slf.py()))
        } else {
            spec.getattr("loader")
        }
    }

    fn invalidate_caches(&self) -> PyResult<()> {
        Ok(())
    }

    // End of importlib.abc.MetaPathFinder interface.

    // Start of importlib.abc.Loader interface.

    fn create_module(slf: &PyCell<Self>, spec: &PyAny) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let finder = slf.borrow();
        let state = &finder.state;

        let name = spec.getattr("name")?;
        let key = name.extract::<String>()?;

        let module = match state
            .get_resources_state()
            .resolve_importable_module(&key, state.optimize_level)
        {
            Some(module) => module,
            None => return Ok(py.None()),
        };

        // Extension modules need special module creation logic.
        if module.flavor == ModuleFlavor::Extension {
            // We need a custom implementation of create_module() for in-memory shared
            // library extensions because if we wait until `exec_module()` to
            // initialize the module object, this can confuse some CPython
            // internals. A side-effect of initializing extension modules is
            // populating `sys.modules` and this made `LazyLoader` unhappy.
            // If we ever implement our own lazy module importer, we could
            // potentially work around this and move all extension module
            // initialization into `exec_module()`.
            if let Some(library_data) = &module.in_memory_extension_module_shared_library() {
                let sys_modules = state.sys_module.getattr(py, "modules")?;

                extension_module_shared_library_create_module(
                    state.get_resources_state(),
                    py,
                    sys_modules.into_ref(py),
                    spec,
                    name,
                    &key,
                    library_data,
                )
            } else {
                // Call `imp.create_dynamic()` for dynamic extension modules.
                let create_dynamic = state.imp_module.getattr(py, "create_dynamic")?;

                state
                    .call_with_frames_removed
                    .call(py, (&create_dynamic, spec), None)
            }
        } else {
            Ok(py.None())
        }
    }

    fn exec_module(slf: &PyCell<Self>, module: &PyAny) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let finder = slf.borrow();
        let state = &finder.state;

        let name = module.getattr("__name__")?;
        let key = name.extract::<String>()?;

        let mut entry = match state
            .get_resources_state()
            .resolve_importable_module(&key, state.optimize_level)
        {
            Some(entry) => entry,
            None => {
                // Raising here might make more sense, as `find_spec()` shouldn't have returned
                // an entry for something that we don't know how to handle.
                return Ok(py.None());
            }
        };

        if let Some(bytecode) = entry.resolve_bytecode(
            py,
            state.optimize_level,
            state.decode_source.as_ref(py),
            state.io_module.as_ref(py),
        )? {
            let code = state.marshal_loads.call(py, (bytecode,), None)?;
            let dict = module.getattr("__dict__")?;

            state
                .call_with_frames_removed
                .call(py, (&state.exec_fn, code, dict), None)
        } else if entry.flavor == ModuleFlavor::Builtin {
            state
                .builtin_importer
                .call_method(py, "exec_module", (module,), None)
        } else if entry.flavor == ModuleFlavor::Frozen {
            state
                .frozen_importer
                .call_method(py, "exec_module", (module,), None)
        } else if entry.flavor == ModuleFlavor::Extension {
            // `ExtensionFileLoader.exec_module()` simply calls `imp.exec_dynamic()`.
            let exec_dynamic = state.imp_module.getattr(py, "exec_dynamic")?;

            state
                .call_with_frames_removed
                .call(py, (&exec_dynamic, module), None)
        } else {
            Ok(py.None())
        }?;

        // Perform import time side-effects for special modules.
        match key.as_str() {
            "multiprocessing" => {
                if let Some(method) = state.multiprocessing_set_start_method.as_ref() {
                    // We pass force=True to ensure the call doesn't fail.
                    let kwargs = PyDict::new(py);
                    kwargs.set_item("force", true)?;
                    module.call_method("set_start_method", (method,), Some(kwargs))?;
                }
            }
            "pkg_resources" => {
                if state.pkg_resources_import_auto_register {
                    register_pkg_resources_with_module(py, module)?;
                }
            }
            _ => {}
        }

        Ok(py.None())
    }

    // End of importlib.abc.Loader interface.

    // Start of importlib.abc.ResourceLoader interface.

    /// An abstract method to return the bytes for the data located at path.
    ///
    /// Loaders that have a file-like storage back-end that allows storing
    /// arbitrary data can implement this abstract method to give direct access
    /// to the data stored. OSError is to be raised if the path cannot be
    /// found. The path is expected to be constructed using a module’s __file__
    /// attribute or an item from a package’s __path__.
    fn get_data<'p>(slf: &'p PyCell<Self>, path: &str) -> PyResult<&'p PyAny> {
        slf.borrow()
            .state
            .get_resources_state()
            .resolve_resource_data_from_path(slf.py(), path)
    }

    // End of importlib.abs.ResourceLoader interface.

    // Start of importlib.abc.InspectLoader interface.

    fn get_code(slf: &PyCell<Self>, fullname: &str) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let finder = slf.borrow();
        let state = &finder.state;

        let key = fullname.to_string();

        let mut module = match state
            .get_resources_state()
            .resolve_importable_module(&key, state.optimize_level)
        {
            Some(module) => module,
            None => return Ok(py.None()),
        };

        if let Some(bytecode) = module.resolve_bytecode(
            py,
            state.optimize_level,
            state.decode_source.as_ref(py),
            state.io_module.as_ref(py),
        )? {
            state.marshal_loads.call(py, (bytecode,), None)
        } else if module.flavor == ModuleFlavor::Frozen {
            state
                .imp_module
                .call_method(py, "get_frozen_object", (fullname,), None)
        } else {
            Ok(py.None())
        }
    }

    fn get_source(slf: &PyCell<Self>, fullname: &str) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let finder = slf.borrow();
        let state = &finder.state;
        let key = fullname.to_string();

        let module = match state
            .get_resources_state()
            .resolve_importable_module(&key, state.optimize_level)
        {
            Some(module) => module,
            None => return Ok(py.None()),
        };

        let source = module.resolve_source(
            py,
            state.decode_source.as_ref(py),
            state.io_module.as_ref(py),
        )?;

        Ok(if let Some(source) = source {
            source.into_py(py)
        } else {
            py.None()
        })
    }

    // Start of importlib.abc.ExecutionLoader interface.

    /// An abstract method that is to return the value of __file__ for the specified module.
    ///
    /// If no path is available, ImportError is raised.
    ///
    /// If source code is available, then the method should return the path to the
    /// source file, regardless of whether a bytecode was used to load the module.
    fn get_filename<'p>(slf: &'p PyCell<Self>, fullname: &str) -> PyResult<&'p PyAny> {
        let finder = slf.borrow();
        let state = &finder.state;
        let key = fullname.to_string();

        let make_error =
            |msg: &str| -> PyErr { PyImportError::new_err((msg.to_owned(), key.clone())) };

        let module = state
            .get_resources_state()
            .resolve_importable_module(&key, state.optimize_level)
            .ok_or_else(|| make_error("unknown module"))?;

        module
            .resolve_origin(slf.py())
            .map_err(|_| make_error("unable to resolve origin"))?
            .ok_or_else(|| make_error("no origin"))
    }

    // End of importlib.abc.ExecutionLoader interface.

    // End of importlib.abc.InspectLoader interface.

    // Support obtaining ResourceReader instances.

    fn get_resource_reader(slf: &PyCell<Self>, fullname: &str) -> PyResult<Py<PyAny>> {
        let finder = slf.borrow();
        let state = &finder.state;
        let key = fullname.to_string();

        let entry = match state
            .get_resources_state()
            .resolve_importable_module(&key, state.optimize_level)
        {
            Some(entry) => entry,
            None => return Ok(slf.py().None()),
        };

        // Resources are only available on packages.
        if entry.is_package {
            Ok(PyCell::new(
                slf.py(),
                OxidizedResourceReader::new(state.clone(), key.to_string()),
            )?
            .into_py(slf.py()))
        } else {
            Ok(slf.py().None())
        }
    }

    // importlib.metadata interface.

    /// def find_distributions(context=DistributionFinder.Context()):
    ///
    /// Return an iterable of all Distribution instances capable of
    /// loading the metadata for packages for the indicated `context`.
    ///
    /// The DistributionFinder.Context object provides .path and .name
    /// properties indicating the path to search and names to match and
    /// may supply other relevant context.
    ///
    /// What this means in practice is that to support finding distribution
    /// package metadata in locations other than the file system, subclass
    /// Distribution and implement the abstract methods. Then from a custom
    /// finder, return instances of this derived Distribution in the
    /// find_distributions() method.
    #[args(context = "None")]
    fn find_distributions<'p>(
        slf: &'p PyCell<Self>,
        context: Option<&PyAny>,
    ) -> PyResult<&'p PyAny> {
        let py = slf.py();
        let finder = slf.borrow();
        let state = &finder.state;

        let (path, name) = if let Some(context) = context {
            // The passed object should have `path` and `name` attributes. But the
            // values could be `None`, so normalize those to Rust's `None`.
            let path = context.getattr("path")?;
            let path = if path.is_none() { None } else { Some(path) };

            let name = context.getattr("name")?;
            let name = if name.is_none() { None } else { Some(name) };

            (path, name)
        } else {
            // No argument = default Context = find everything.
            (None, None)
        };

        crate::package_metadata::find_distributions(py, state.clone(), name, path)?
            .call_method0("__iter__")
    }

    // pkgutil methods.

    /// def iter_modules(prefix="")
    #[args(prefix = "None")]
    fn iter_modules<'p>(slf: &'p PyCell<Self>, prefix: Option<&str>) -> PyResult<&'p PyList> {
        let finder = slf.borrow();
        let state = &finder.state;

        let resources_state = state.get_resources_state();

        let prefix = prefix.map(|prefix| prefix.to_string());

        resources_state.pkgutil_modules_infos(slf.py(), None, prefix, state.optimize_level)
    }

    // Additional methods provided for convenience.

    /// OxidizedFinder.__new__(relative_path_origin=None))
    #[new]
    #[args(relative_path_origin = "None")]
    fn new(py: Python, relative_path_origin: Option<&PyAny>) -> PyResult<Self> {
        // We need to obtain an ImporterState instance. This requires handles on a
        // few items...

        // The module references are easy to obtain.
        let m = py.import(OXIDIZED_IMPORTER_NAME_STR)?;
        let bootstrap_module = py.import("_frozen_importlib")?;

        let mut resources_state =
            Box::new(PythonResourcesState::new_from_env().map_err(PyValueError::new_err)?);

        // Update origin if a value is given.
        if let Some(py_origin) = relative_path_origin {
            resources_state.set_origin(pyobject_to_pathbuf(py, py_origin)?);
        }

        Ok(OxidizedFinder {
            state: Arc::new(ImporterState::new(
                py,
                m,
                bootstrap_module,
                resources_state,
            )?),
        })
    }

    #[getter]
    fn multiprocessing_set_start_method(&self) -> PyResult<Option<String>> {
        if let Some(v) = &self.state.multiprocessing_set_start_method {
            Ok(Some(v.to_string()))
        } else {
            Ok(None)
        }
    }

    #[getter]
    fn origin<'p>(&self, py: Python<'p>) -> &'p PyAny {
        self.state
            .get_resources_state()
            .origin()
            .into_py(py)
            .into_ref(py)
    }

    #[getter]
    fn path_hook_base_str<'p>(&self, py: Python<'p>) -> &'p PyAny {
        self.state
            .get_resources_state()
            .current_exe()
            .into_py(py)
            .into_ref(py)
    }

    #[getter]
    fn pkg_resources_import_auto_register(&self) -> PyResult<bool> {
        Ok(self.state.pkg_resources_import_auto_register)
    }

    fn path_hook(slf: &PyCell<Self>, path: &PyAny) -> PyResult<OxidizedPathEntryFinder> {
        Self::path_hook_inner(slf, path).map_err(|inner| {
            let err = PyImportError::new_err("error running OxidizedFinder.path_hook");

            if let Err(err) = err.value(slf.py()).setattr("__suppress_context__", true) {
                err
            } else if let Err(err) = err
                .value(slf.py())
                .setattr("__cause__", inner.value(slf.py()))
            {
                err
            } else {
                err
            }
        })
    }

    fn index_bytes(&self, py: Python, data: &PyAny) -> PyResult<()> {
        self.state
            .get_resources_state_mut()
            .index_pyobject(py, data)?;

        Ok(())
    }

    fn index_file_memory_mapped(&self, py: Python, path: &PyAny) -> PyResult<()> {
        let path = pyobject_to_pathbuf(py, path)?;

        self.state
            .get_resources_state_mut()
            .index_path_memory_mapped(path)
            .map_err(PyValueError::new_err)?;

        Ok(())
    }

    fn index_interpreter_builtins(&self) -> PyResult<()> {
        self.state
            .get_resources_state_mut()
            .index_interpreter_builtins()
            .map_err(PyValueError::new_err)?;

        Ok(())
    }

    fn index_interpreter_builtin_extension_modules(&self) -> PyResult<()> {
        self.state
            .get_resources_state_mut()
            .index_interpreter_builtin_extension_modules()
            .map_err(PyValueError::new_err)?;

        Ok(())
    }

    fn index_interpreter_frozen_modules(&self) -> PyResult<()> {
        self.state
            .get_resources_state_mut()
            .index_interpreter_frozen_modules()
            .map_err(PyValueError::new_err)?;

        Ok(())
    }

    fn indexed_resources<'p>(&self, py: Python<'p>) -> PyResult<&'p PyList> {
        let resources_state = self.state.get_resources_state();

        resources_state.resources_as_py_list(py)
    }

    fn add_resource(&self, resource: &OxidizedResource) -> PyResult<()> {
        let resources_state = self.state.get_resources_state_mut();

        resources_state
            .add_resource(pyobject_to_resource(resource))
            .map_err(|_| PyValueError::new_err("unable to add resource to finder"))?;

        Ok(())
    }

    fn add_resources(&self, resources: &PyAny) -> PyResult<()> {
        let resources_state = self.state.get_resources_state_mut();

        for resource in resources.iter()? {
            let resource_raw = resource?;
            let resource = resource_raw.cast_as::<PyCell<OxidizedResource>>()?;

            resources_state
                .add_resource(pyobject_to_resource(&resource.borrow()))
                .map_err(|_| PyValueError::new_err("unable to add resource to finder"))?;
        }

        Ok(())
    }

    #[args(ignore_builtin = true, ignore_frozen = true)]
    fn serialize_indexed_resources<'p>(
        &self,
        py: Python<'p>,
        ignore_builtin: bool,
        ignore_frozen: bool,
    ) -> PyResult<&'p PyBytes> {
        let resources_state = self.state.get_resources_state();

        let data = resources_state
            .serialize_resources(ignore_builtin, ignore_frozen)
            .map_err(|e| PyValueError::new_err(format!("error serializing: {}", e)))?;

        Ok(PyBytes::new(py, &data))
    }
}

impl OxidizedFinder {
    fn path_hook_inner(
        slf: &PyCell<Self>,
        path_original: &PyAny,
    ) -> PyResult<OxidizedPathEntryFinder> {
        let py = slf.py();
        let finder = slf.borrow();

        // We respond to the following paths:
        //
        // * self.path_hook_base_str
        // * virtual sub-directories under self.path_hook_base_str
        //
        // There is a mismatch between the ways that Rust and Python store paths.
        // self.current_exe is a Rust PathBuf and came from Rust. We can get the raw
        // OsString and know the raw bytes. But Python applies text encoding to
        // paths. Normalizing between the 2 could be difficult, especially since
        // Python module names can be quite literally any str value.
        //
        // We restrict accepted paths to Python str that are equal to
        // self.current_exe or have it + a directory separator as a strict prefix.
        // This leaves us with a suffix constituting the relative package path, which we
        // can coerce to a Rust String easily, as Python str are Unicode.

        // Only accept str.
        let path = path_original.cast_as::<PyString>()?;

        let path_hook_base = finder.path_hook_base_str(py).cast_as::<PyString>()?;

        let target_package = if path.compare(path_hook_base)? == std::cmp::Ordering::Equal {
            None
        } else {
            // Accept both directory separators as prefix match.
            let unix_prefix = path_hook_base.call_method("__add__", ("/",), None)?;
            let windows_prefix = path_hook_base.call_method("__add__", ("\\",), None)?;

            let prefix = PyTuple::new(py, [unix_prefix, windows_prefix]);

            if !path
                .call_method("startswith", (prefix,), None)?
                .extract::<bool>()?
            {
                return Err(PyValueError::new_err(format!(
                    "{} is not prefixed by {}",
                    path.to_string_lossy(),
                    path_hook_base.to_string_lossy()
                )));
            }

            // Ideally we'd strip the prefix in the domain of Python so we don't have
            // to worry about text encoding. However, since we need to normalize the
            // suffix to a Rust string anyway to facilitate filtering against UTF-8
            // names, we go ahead and convert to Rust/UTF-8 and do the string
            // operations in Rust.
            //
            // It is tempting to use os.fsencode() here, as sys.path entries are,
            // well, paths. But since sys.path entries are meant to map to our
            // path hook, we get to decide what their format is and we decide that
            // any unicode encoding should be in UTF-8, not whatever the current
            // filesystem encoding is set to. Since Rust won't handle surrogateescape
            // that well, we use the "replace" error handling strategy to ensure a
            // Rust string valid byte sequence.
            let path_hook_base_bytes = path_hook_base
                .call_method("encode", ("utf-8", "replace"), None)?
                .extract::<Vec<u8>>()?;
            let path_bytes = path
                .call_method("encode", ("utf-8", "replace"), None)?
                .extract::<Vec<u8>>()?;

            // +1 for directory separator, which should always be 1 byte in UTF-8.
            let path_suffix: &[u8] = &path_bytes[path_hook_base_bytes.len() + 1..];
            let original_package_path = String::from_utf8(path_suffix.to_vec()).map_err(|e| {
                PyValueError::new_err(format!(
                    "error coercing package suffix to Rust string: {}",
                    e
                ))
            })?;

            let package_path = original_package_path.replace('\\', "/");

            // Ban leading and trailing directory separators.
            if package_path.starts_with('/') || package_path.ends_with('/') {
                return Err(PyValueError::new_err(
                    format!("rejecting virtual sub-directory because package part contains leading or trailing directory separator: {}", original_package_path)));
            }

            // Ban consecutive directory separators.
            if package_path.contains("//") {
                return Err(PyValueError::new_err(format!("rejecting virtual sub-directory because it has consecutive directory separators: {}", original_package_path)));
            }

            // Since we have to normalize to Python package form where dots are
            // special, ban dots in special places.
            if package_path
                .split('/')
                .any(|s| s.starts_with('.') || s.ends_with('.') || s.contains(".."))
            {
                return Err(PyValueError::new_err(
                    format!("rejecting virtual sub-directory because package part contains illegal dot characters: {}", original_package_path)

                ));
            }

            if package_path.is_empty() {
                None
            } else {
                Some(package_path.replace('/', "."))
            }
        };

        Ok(OxidizedPathEntryFinder {
            finder: PyCell::new(
                py,
                OxidizedFinder {
                    state: finder.state.clone(),
                },
            )?
            .into(),
            source_path: path.into_py(py),
            target_package,
        })
    }
}

/// Path-like object facilitating Python resource access.
///
/// This implements importlib.abc.Traversable.
#[pyclass(module = "oxidized_importer")]
pub(crate) struct PyOxidizerTraversable {
    state: Arc<ImporterState>,
    path: String,
}

#[pymethods]
impl PyOxidizerTraversable {
    /// Yield Traversable objects in self.
    fn iterdir(&self) -> PyResult<&PyAny> {
        unimplemented!()
    }

    /// Read contents of self as bytes.
    fn read_bytes(&self) -> PyResult<&PyAny> {
        unimplemented!()
    }

    /// Read contents of self as text.
    fn read_text(&self) -> PyResult<&PyAny> {
        unimplemented!()
    }

    /// Return True if self is a dir.
    fn is_dir(&self) -> PyResult<bool> {
        // We are a directory if the current path is a known package.
        // TODO We may need to expand this definition in the future to cover
        // virtual subdirectories in addressable resources. But this will require
        // changes to the resources data format to capture said annotations.
        if let Some(entry) = self
            .state
            .get_resources_state()
            .resolve_importable_module(&self.path, self.state.optimize_level)
        {
            if entry.is_package {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Return True if self is a file.
    fn is_file(&self) -> PyResult<&PyAny> {
        unimplemented!()
    }

    /// Return Traversable child in self.
    #[allow(unused)]
    fn joinpath(&self, child: &PyAny) -> PyResult<&PyAny> {
        unimplemented!()
    }

    /// Return Traversable child in self.
    #[allow(unused)]
    fn __truediv__(&self, child: &PyAny) -> PyResult<&PyAny> {
        unimplemented!()
    }

    /// mode may be 'r' or 'rb' to open as text or binary. Return a handle
    /// suitable for reading (same as pathlib.Path.open).
    ///
    /// When opening as text, accepts encoding parameters such as those
    /// accepted by io.TextIOWrapper.
    #[allow(unused)]
    #[args(py_args = "*", py_kwargs = "**")]
    fn open(&self, py_args: &PyTuple, py_kwargs: Option<&PyDict>) -> PyResult<&PyAny> {
        unimplemented!()
    }
}

/// Replace all meta path importers with an OxidizedFinder instance and return it.
///
/// This is called after PyInit_* to finish the initialization of the
/// module. Its state struct is updated.
///
/// The [OxidizedFinder] is guaranteed to be on `sys.meta_path[0]` after successful
/// completion.
pub fn replace_meta_path_importers<'a, 'p>(
    py: Python<'p>,
    oxidized_importer: &PyModule,
    resources_state: Box<PythonResourcesState<'a, u8>>,
    importer_state_callback: Option<impl FnOnce(&mut ImporterState)>,
) -> PyResult<&'p PyCell<OxidizedFinder>> {
    let mut state = get_module_state(oxidized_importer)?;

    let sys_module = py.import("sys")?;

    // Construct and register our custom meta path importer. Because our meta path
    // importer is able to handle builtin and frozen modules, the existing meta path
    // importers are removed. The assumption here is that we're called very early
    // during startup and the 2 default meta path importers are installed.
    let oxidized_finder = PyCell::new(
        py,
        OxidizedFinder::new_from_module_and_resources(
            py,
            oxidized_importer,
            resources_state,
            importer_state_callback,
        )?,
    )?;

    let meta_path_object = sys_module.getattr("meta_path")?;

    meta_path_object.call_method0("clear")?;
    meta_path_object.call_method("append", (oxidized_finder,), None)?;

    state.initialized = true;

    Ok(oxidized_finder)
}

/// Undoes the actions of `importlib._bootstrap_external` initialization.
///
/// This will remove types that aren't defined by this extension from
/// `sys.meta_path` and `sys.path_hooks`.
pub fn remove_external_importers(sys_module: &PyModule) -> PyResult<()> {
    let meta_path = sys_module.getattr("meta_path")?;
    let meta_path = meta_path.cast_as::<PyList>()?;

    // We need to mutate the lists in place so any updates are reflected
    // in references to the lists.

    let mut oxidized_path_hooks = vec![];
    let mut index = 0;
    while index < meta_path.len() {
        let entry = meta_path.get_item(index as _)?;

        // We want to preserve `_frozen_importlib.BuiltinImporter` and
        // `_frozen_importlib.FrozenImporter`, if present. Ideally we'd
        // do PyType comparisons. However, there doesn't appear to be a way
        // to easily get a handle on their PyType. We'd also prefer to do
        // PyType.name() checks. But both these report `type`. So we key
        // off `__module__` instead.

        // TODO perform type comparison natively once OxidizedFinder is defined via pyo3.
        if entry.get_type().to_string().contains("OxidizedFinder") {
            oxidized_path_hooks.push(entry.getattr("path_hook")?);
            index += 1;
        } else if entry
            .getattr("__module__")?
            .cast_as::<PyString>()?
            .to_string_lossy()
            == "_frozen_importlib"
        {
            index += 1;
        } else {
            meta_path.call_method1("pop", (index,))?;
        }
    }

    let path_hooks = sys_module.getattr("path_hooks")?;
    let path_hooks = path_hooks.cast_as::<PyList>()?;

    let mut index = 0;
    while index < path_hooks.len() {
        let entry = path_hooks.get_item(index as _)?;

        let mut found = false;
        for candidate in oxidized_path_hooks.iter() {
            if candidate.eq(entry)? {
                found = true;
                break;
            }
        }
        if found {
            index += 1;
        } else {
            path_hooks.call_method1("pop", (index,))?;
        }
    }

    Ok(())
}

/// Prepend a path hook to [`sys.path_hooks`] that works with [OxidizedFinder].
///
/// `sys` must be a reference to the [`sys`] module.
///
/// [`sys.path_hooks`]: https://docs.python.org/3/library/sys.html#sys.path_hooks
/// [`sys`]: https://docs.python.org/3/library/sys.html
pub fn install_path_hook(finder: &PyAny, sys: &PyModule) -> PyResult<()> {
    let hook = finder.getattr("path_hook")?;
    let path_hooks = sys.getattr("path_hooks")?;
    path_hooks
        .call_method("insert", (0, hook), None)
        .map(|_| ())
}
