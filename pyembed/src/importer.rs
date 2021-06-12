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
    cpython::exc::SystemError,
    std::ffi::{c_void, CString},
};
use {
    crate::{
        conversion::{path_to_pyobject, pyobject_to_pathbuf},
        extension::{get_module_state, OXIDIZED_IMPORTER_NAME_STR},
        pkg_resources::register_pkg_resources_with_module,
        python_resources::{
            name_at_package_hierarchy, pyobject_to_resource, resource_to_pyobject, ModuleFlavor,
            OptimizeLevel, OxidizedResource, PythonResourcesState,
        },
    },
    cpython::{
        exc::{FileNotFoundError, ImportError, ValueError},
        {
            py_class, NoArgs, ObjectProtocol, PyBytes, PyCapsule, PyClone, PyDict, PyErr, PyList,
            PyModule, PyObject, PyResult, PyString, PyTuple, Python, PythonObject, ToPyObject,
        },
    },
    python3_sys as pyffi,
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
    sys_modules: PyObject,
    spec: &PyObject,
    name_py: PyObject,
    name: &str,
    library_data: &[u8],
) -> PyResult<PyObject> {
    let origin = PyString::new(py, "memory");

    let existing_module = unsafe {
        pyffi::_PyImport_FindExtensionObject(name_py.as_ptr(), origin.as_object().as_ptr())
    };

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
        return Err(PyErr::new::<ImportError, _>(
            py,
            ("unable to load extension module library from memory", name),
        ));
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
    _sys_modules: PyObject,
    _spec: &PyObject,
    _name_py: PyObject,
    _name: &str,
    _library_data: &[u8],
) -> PyResult<PyObject> {
    panic!("should only be called on Windows");
}

/// Reimplementation of `_PyImport_LoadDynamicModuleWithSpec()`.
#[cfg(windows)]
fn load_dynamic_library(
    py: Python,
    sys_modules: PyObject,
    spec: &PyObject,
    name_py: PyObject,
    name: &str,
    library_module: *const c_void,
) -> PyResult<PyObject> {
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
        return Err(PyErr::new::<ImportError, _>(
            py,
            (
                format!(
                    "dynamic module does not define module export function ({})",
                    init_fn_name.to_str().unwrap()
                ),
                name,
            ),
        ));
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

    if py_module.is_null() && unsafe { pyffi::PyErr_Occurred().is_null() } {
        return Err(PyErr::new::<SystemError, _>(
            py,
            format!(
                "initialization of {} failed without raising an exception",
                name
            ),
        ));
    }

    // Cast to owned type to help prevent refcount/memory leaks.
    let py_module = unsafe { PyObject::from_owned_ptr(py, py_module) };

    if !unsafe { pyffi::PyErr_Occurred().is_null() } {
        unsafe {
            pyffi::PyErr_Clear();
        }
        return Err(PyErr::new::<SystemError, _>(
            py,
            format!("initialization of {} raised unreported exception", name),
        ));
    }

    if unsafe { pyffi::Py_TYPE(py_module.as_ptr()) }.is_null() {
        return Err(PyErr::new::<SystemError, _>(
            py,
            format!("init function of {} returned uninitialized object", name),
        ));
    }

    // If initialization returned a `PyModuleDef`, construct a module from it.
    if unsafe { pyffi::PyObject_TypeCheck(py_module.as_ptr(), &mut pyffi::PyModuleDef_Type) } != 0 {
        let py_module = unsafe {
            pyffi::PyModule_FromDefAndSpec(
                py_module.as_ptr() as *mut pyffi::PyModuleDef,
                spec.as_ptr(),
            )
        };

        return if py_module.is_null() {
            Err(PyErr::fetch(py))
        } else {
            Ok(unsafe { PyObject::from_owned_ptr(py, py_module) })
        };
    }

    // Else fall back to single-phase init mechanism.

    let mut module_def = unsafe { pyffi::PyModule_GetDef(py_module.as_ptr()) };
    if module_def.is_null() {
        return Err(PyErr::new::<SystemError, _>(
            py,
            format!(
                "initialization of {} did not return an extension module",
                name
            ),
        ));
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
pub(crate) struct ImporterState {
    /// `imp` Python module.
    imp_module: PyModule,
    /// `sys` Python module.
    sys_module: PyModule,
    /// `_io` Python module.
    io_module: PyModule,
    /// `marshal.loads` Python callable.
    marshal_loads: PyObject,
    /// `_frozen_importlib.BuiltinImporter` meta path importer for built-in extension modules.
    builtin_importer: PyObject,
    /// `_frozen_importlib.FrozenImporter` meta path importer for frozen modules.
    frozen_importer: PyObject,
    /// `importlib._bootstrap._call_with_frames_removed` function.
    call_with_frames_removed: PyObject,
    /// `importlib._bootstrap.ModuleSpec` class.
    module_spec_type: PyObject,
    /// Our `decode_source()` function.
    decode_source: PyObject,
    /// `builtins.exec` function.
    exec_fn: PyObject,
    /// Bytecode optimization level currently in effect.
    optimize_level: OptimizeLevel,
    /// Value to pass to `multiprocessing.set_start_method()` on import of `multiprocessing`.
    ///
    /// If `None`, `set_start_method()` will not be called automatically.
    multiprocessing_set_start_method: Option<String>,
    /// Whether to automatically register ourself with `pkg_resources` when it is imported.
    pkg_resources_import_auto_register: bool,
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
    resources_state: PyCapsule,
}

impl ImporterState {
    fn new<'a>(
        py: Python,
        importer_module: &PyModule,
        bootstrap_module: &PyModule,
        resources_state: Box<PythonResourcesState<'a, u8>>,
    ) -> Result<Self, PyErr> {
        let decode_source = importer_module.get(py, "decode_source")?;

        let io_module = py.import("_io")?;
        let marshal_module = py.import("marshal")?;

        let imp_module = bootstrap_module.get(py, "_imp")?;
        let imp_module = imp_module.cast_into::<PyModule>(py)?;
        let sys_module = bootstrap_module.get(py, "sys")?;
        let sys_module = sys_module.cast_into::<PyModule>(py)?;
        let meta_path_object = sys_module.get(py, "meta_path")?;

        // We should be executing as part of
        // _frozen_importlib_external._install_external_importers().
        // _frozen_importlib._install() should have already been called and set up
        // sys.meta_path with [BuiltinImporter, FrozenImporter]. Those should be the
        // only meta path importers present.

        let meta_path = meta_path_object.cast_as::<PyList>(py)?;
        if meta_path.len(py) < 2 {
            return Err(PyErr::new::<ValueError, _>(
                py,
                "sys.meta_path does not contain 2 values",
            ));
        }

        let builtin_importer = meta_path.get_item(py, 0);
        let frozen_importer = meta_path.get_item(py, 1);

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

        let sys_flags = sys_module.get(py, "flags")?;
        let optimize_value = sys_flags.getattr(py, "optimize")?;
        let optimize_value = optimize_value.extract::<i64>(py)?;

        let optimize_level = match optimize_value {
            0 => Ok(OptimizeLevel::Zero),
            1 => Ok(OptimizeLevel::One),
            2 => Ok(OptimizeLevel::Two),
            _ => Err(PyErr::new::<ValueError, _>(
                py,
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
                return Err(PyErr::new::<ValueError, _>(
                    py,
                    "unable to convert PythonResourcesState to capsule",
                ));
            }

            PyObject::from_owned_ptr(py, ptr).unchecked_cast_into()
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

    /// Obtain the `PythonResourcesState` associated with this instance.
    #[inline]
    pub fn get_resources_state<'a>(&self) -> &PythonResourcesState<'a, u8> {
        let ptr = unsafe {
            pyffi::PyCapsule_GetPointer(self.resources_state.as_object().as_ptr(), std::ptr::null())
        };

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
        let ptr = unsafe {
            pyffi::PyCapsule_GetPointer(self.resources_state.as_object().as_ptr(), std::ptr::null())
        };

        if ptr.is_null() {
            panic!("null pointer in resources state capsule");
        }

        unsafe { &mut *(ptr as *mut PythonResourcesState<u8>) }
    }

    /// Set the value to call `multiprocessing.set_start_method()` with on import of `multiprocessing`.
    pub fn set_multiprocessing_set_start_method(&mut self, value: Option<String>) {
        self.multiprocessing_set_start_method = value;
    }
}

impl Drop for ImporterState {
    fn drop(&mut self) {
        let ptr = unsafe {
            pyffi::PyCapsule_GetPointer(self.resources_state.as_object().as_ptr(), std::ptr::null())
        };

        if !ptr.is_null() {
            unsafe {
                Box::from_raw(ptr as *mut PythonResourcesState<u8>);
            }
        }
    }
}

// Python type to import modules.
//
// This type implements the importlib.abc.MetaPathFinder interface for
// finding/loading modules. It supports loading various flavors of modules,
// allowing it to be the only registered sys.meta_path importer.
//
// Because macro expansion confuses IDE type hinting and rustfmt, most
// methods call into non-macro implemented methods named <method>_impl which
// are defined below in separate `impl {}` blocks.
py_class!(pub(crate) class OxidizedFinder |py| {
    data state: Arc<ImporterState>;

    // Start of importlib.abc.MetaPathFinder interface.

    def find_spec(&self, fullname: &PyString, path: &PyObject, target: Option<PyObject> = None) -> PyResult<PyObject> {
        self.find_spec_impl(py, fullname, path, target)
    }

    def find_module(&self, fullname: &PyObject, path: &PyObject) -> PyResult<PyObject> {
        self.find_module_impl(py, fullname, path)
    }

    def invalidate_caches(&self) -> PyResult<PyObject> {
        Ok(self.invalidate_caches_impl(py))
    }

    // End of importlib.abc.MetaPathFinder interface.

    // Start of importlib.abc.Loader interface.

    def create_module(&self, spec: &PyObject) -> PyResult<PyObject> {
        self.create_module_impl(py, spec)
    }

    def exec_module(&self, module: &PyObject) -> PyResult<PyObject> {
        self.exec_module_impl(py, module)
    }

    // End of importlib.abc.Loader interface.

    // Start of importlib.abc.ResourceLoader interface.

    def get_data(&self, path: &PyString) -> PyResult<PyObject> {
        self.get_data_impl(py, path)
    }

    // End of importlib.abs.ResourceLoader interface.

    // Start of importlib.abc.InspectLoader interface.

    def get_code(&self, fullname: &PyString) -> PyResult<PyObject> {
        self.get_code_impl(py, fullname)
    }

    def get_source(&self, fullname: &PyString) -> PyResult<PyObject> {
        self.get_source_impl(py, fullname)
    }

    // Start of importlib.abc.ExecutionLoader interface.

    def get_filename(&self, fullname: &PyString) -> PyResult<PyObject> {
        self.get_filename_impl(py, fullname)
    }

    // End of importlib.abc.ExecutionLoader interface.

    // End of importlib.abc.InspectLoader interface.

    // Support obtaining ResourceReader instances.
    def get_resource_reader(&self, fullname: &PyString) -> PyResult<PyObject> {
        self.get_resource_reader_impl(py, fullname)
    }

    // importlib.metadata interface.
    def find_distributions(&self, context: Option<PyObject> = None) -> PyResult<PyObject> {
        self.find_distributions_impl(py, context)
    }

    // pkgutil methods.
    def iter_modules(&self, prefix: Option<PyString> = None) -> PyResult<PyObject> {
        self.iter_modules_impl(py, prefix)
    }

    // Additional methods provided for convenience.

    def __new__(_cls, relative_path_origin: Option<PyObject> = None) -> PyResult<OxidizedFinder> {
        oxidized_finder_new(py, relative_path_origin)
    }

    @property def multiprocessing_set_start_method(&self) -> PyResult<PyObject> {
        self.multiprocessing_set_start_method_impl(py)
    }

    @property def origin(&self) -> PyResult<PyObject> {
        self.origin_impl(py)
    }

    @property def path_hook_base_str(&self) -> PyResult<PyObject> {
        self.path_hook_base_str_impl(py)
    }

    @property def pkg_resources_import_auto_register(&self) -> PyResult<bool> {
        self.pkg_resources_import_auto_register_impl(py)
    }

    def path_hook(&self, path: PyObject) -> PyResult<OxidizedPathEntryFinder> {
        self.path_hook_impl(py, path)
    }

    def index_bytes(&self, data: PyObject) -> PyResult<PyObject> {
        self.index_bytes_impl(py, data)
    }

    def index_file_memory_mapped(&self, path: PyObject) -> PyResult<PyObject> {
        self.index_file_memory_mapped_impl(py, path)
    }

    def index_interpreter_builtins(&self) -> PyResult<PyObject> {
        self.index_interpreter_builtins_impl(py)
    }

    def index_interpreter_builtin_extension_modules(&self) -> PyResult<PyObject> {
        self.index_interpreter_builtin_extension_modules_impl(py)
    }

    def index_interpreter_frozen_modules(&self) -> PyResult<PyObject> {
        self.index_interpreter_frozen_modules_impl(py)
    }

    def indexed_resources(&self) -> PyResult<PyObject> {
        self.indexed_resources_impl(py)
    }

    def add_resource(&self, resource: OxidizedResource) -> PyResult<PyObject> {
        self.add_resource_impl(py, resource)
    }

    def add_resources(&self, resources: Vec<OxidizedResource>) -> PyResult<PyObject> {
        self.add_resources_impl(py, resources)
    }

    def serialize_indexed_resources(&self, ignore_builtin: bool = true, ignore_frozen: bool = true) -> PyResult<PyObject> {
        self.serialize_indexed_resources_impl(py, ignore_builtin, ignore_frozen)
    }
});

// importlib.abc.MetaPathFinder interface.
impl OxidizedFinder {
    fn find_spec_impl(
        &self,
        py: Python,
        fullname: &PyString,
        path: &PyObject,
        target: Option<PyObject>,
    ) -> PyResult<PyObject> {
        let state = self.state(py);
        let key = fullname.to_string(py)?;

        let module = match state
            .get_resources_state()
            .resolve_importable_module(&key, state.optimize_level)
        {
            Some(module) => module,
            None => return Ok(py.None()),
        };

        match module.flavor {
            ModuleFlavor::Extension | ModuleFlavor::SourceBytecode => module.resolve_module_spec(
                py,
                &state.module_spec_type,
                self.as_object(),
                state.optimize_level,
            ),
            ModuleFlavor::Builtin => {
                // BuiltinImporter.find_spec() always returns None if `path` is defined.
                // And it doesn't use `target`. So don't proxy these values.
                state
                    .builtin_importer
                    .call_method(py, "find_spec", (fullname,), None)
            }
            ModuleFlavor::Frozen => {
                state
                    .frozen_importer
                    .call_method(py, "find_spec", (fullname, path, target), None)
            }
        }
    }

    fn invalidate_caches_impl(&self, py: Python) -> PyObject {
        py.None()
    }

    fn find_module_impl(
        &self,
        py: Python,
        fullname: &PyObject,
        path: &PyObject,
    ) -> PyResult<PyObject> {
        let finder = self.as_object();
        let find_spec = finder.getattr(py, "find_spec")?;
        let spec = find_spec.call(py, (fullname, path), None)?;

        if spec == py.None() {
            Ok(py.None())
        } else {
            spec.getattr(py, "loader")
        }
    }
}

// importlib.abc.MetaPathFinder interface.
impl OxidizedFinder {
    fn create_module_impl(&self, py: Python, spec: &PyObject) -> PyResult<PyObject> {
        let state = self.state(py);
        let name = spec.getattr(py, "name")?;
        let key = name.extract::<String>(py)?;

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
                let sys_modules = state.sys_module.as_object().getattr(py, "modules")?;

                extension_module_shared_library_create_module(
                    state.get_resources_state(),
                    py,
                    sys_modules,
                    spec,
                    name,
                    &key,
                    library_data,
                )
            } else {
                // Call `imp.create_dynamic()` for dynamic extension modules.
                let create_dynamic = state.imp_module.as_object().getattr(py, "create_dynamic")?;

                state
                    .call_with_frames_removed
                    .call(py, (&create_dynamic, spec), None)
            }
        } else {
            Ok(py.None())
        }
    }

    fn exec_module_impl(&self, py: Python, module: &PyObject) -> PyResult<PyObject> {
        let state = self.state(py);
        let name = module.getattr(py, "__name__")?;
        let key = name.extract::<String>(py)?;

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
            &state.decode_source,
            &state.io_module,
        )? {
            let code = state.marshal_loads.call(py, (bytecode,), None)?;
            let dict = module.getattr(py, "__dict__")?;

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
            let exec_dynamic = state.imp_module.as_object().getattr(py, "exec_dynamic")?;

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
                    kwargs.set_item(py, "force", true)?;
                    module.call_method(py, "set_start_method", (method,), Some(&kwargs))?;
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
}

// importlib.abc.ResourceLoader interface.
impl OxidizedFinder {
    /// An abstract method to return the bytes for the data located at path.
    ///
    /// Loaders that have a file-like storage back-end that allows storing
    /// arbitrary data can implement this abstract method to give direct access
    /// to the data stored. OSError is to be raised if the path cannot be
    /// found. The path is expected to be constructed using a module’s __file__
    /// attribute or an item from a package’s __path__.
    fn get_data_impl(&self, py: Python, path: &PyString) -> PyResult<PyObject> {
        self.state(py)
            .get_resources_state()
            .resolve_resource_data_from_path(py, path)
    }
}

// importlib.abc.InspectLoader interface.
impl OxidizedFinder {
    fn get_code_impl(&self, py: Python, fullname: &PyString) -> PyResult<PyObject> {
        let state = self.state(py);
        let key = fullname.to_string(py)?;

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
            &state.decode_source,
            &state.io_module,
        )? {
            state.marshal_loads.call(py, (bytecode,), None)
        } else if module.flavor == ModuleFlavor::Frozen {
            state
                .imp_module
                .call(py, "get_frozen_object", (fullname,), None)
        } else {
            Ok(py.None())
        }
    }

    fn get_source_impl(&self, py: Python, fullname: &PyString) -> PyResult<PyObject> {
        let state = self.state(py);
        let key = fullname.to_string(py)?;

        let module = match state
            .get_resources_state()
            .resolve_importable_module(&key, state.optimize_level)
        {
            Some(module) => module,
            None => return Ok(py.None()),
        };

        let source = module.resolve_source(py, &state.decode_source, &state.io_module)?;

        Ok(if let Some(source) = source {
            source
        } else {
            py.None()
        })
    }
}

// importlib.abc.ExecutionLoader interface.
impl OxidizedFinder {
    /// An abstract method that is to return the value of __file__ for the specified module.
    ///
    /// If no path is available, ImportError is raised.
    ///
    /// If source code is available, then the method should return the path to the
    /// source file, regardless of whether a bytecode was used to load the module.
    fn get_filename_impl(&self, py: Python, fullname: &PyString) -> PyResult<PyObject> {
        let state = self.state(py);
        let key = fullname.to_string(py)?;

        let make_error = |msg: &str| -> PyErr { PyErr::new::<ImportError, _>(py, (msg, &key)) };

        let module = state
            .get_resources_state()
            .resolve_importable_module(&key, state.optimize_level)
            .ok_or_else(|| make_error("unknown module"))?;

        module
            .resolve_origin(py)
            .map_err(|_| make_error("unable to resolve origin"))?
            .ok_or_else(|| make_error("no origin"))
    }
}

// Resource loading interface.
impl OxidizedFinder {
    fn get_resource_reader_impl(&self, py: Python, fullname: &PyString) -> PyResult<PyObject> {
        let state = self.state(py);
        let key = fullname.to_string(py)?;

        let entry = match state
            .get_resources_state()
            .resolve_importable_module(&key, state.optimize_level)
        {
            Some(entry) => entry,
            None => return Ok(py.None()),
        };

        // Resources are only available on packages.
        if entry.is_package {
            let reader =
                OxidizedResourceReader::create_instance(py, state.clone(), key.to_string())?
                    .into_object();
            Ok(reader)
        } else {
            Ok(py.None())
        }
    }
}

// importlib.metadata support.
impl OxidizedFinder {
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
    fn find_distributions_impl(&self, py: Python, context: Option<PyObject>) -> PyResult<PyObject> {
        let state = self.state(py);

        let (path, name) = if let Some(context) = context {
            // The passed object should have `path` and `name` attributes. But the
            // values could be `None`, so normalize those to Rust's `None`.
            let path = context.getattr(py, "path")?;
            let path = if path == py.None() { None } else { Some(path) };

            let name = context.getattr(py, "name")?;
            let name = if name == py.None() { None } else { Some(name) };

            (path, name)
        } else {
            // No argument = default Context = find everything.
            (None, None)
        };

        crate::package_metadata::find_distributions(py, state.clone(), name, path)
    }
}

// pkgutil support.
impl OxidizedFinder {
    /// def iter_modules(prefix="")
    fn iter_modules_impl(&self, py: Python, prefix: Option<PyString>) -> PyResult<PyObject> {
        let state: &ImporterState = self.state(py);
        let resources_state = state.get_resources_state();

        let prefix = if let Some(prefix) = prefix {
            Some(prefix.to_string(py)?.to_string())
        } else {
            None
        };

        resources_state.pkgutil_modules_infos(py, None, prefix, state.optimize_level)
    }
}

// Path hooks support.
impl OxidizedFinder {
    fn path_hook_impl(&self, py: Python, path: PyObject) -> PyResult<OxidizedPathEntryFinder> {
        self.path_hook_inner(py, path).map_err(|mut inner| {
            let mut err =
                PyErr::new::<ImportError, _>(py, "error running OxidizedFinder.path_hook");

            if let Err(err) = err.instance(py).setattr(py, "__suppress_context__", true) {
                err
            } else if let Err(err) = err
                .instance(py)
                .setattr(py, "__cause__", inner.instance(py))
            {
                err
            } else {
                err
            }
        })
    }

    fn path_hook_inner(
        &self,
        py: Python,
        path_original: PyObject,
    ) -> PyResult<OxidizedPathEntryFinder> {
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
        let path = path_original.cast_as::<PyString>(py)?;

        let path_hook_base = self.path_hook_base_str(py)?.cast_into::<PyString>(py)?;

        let target_package = if path.as_object().compare(py, path_hook_base.as_object())?
            == std::cmp::Ordering::Equal
        {
            None
        } else {
            // Accept both directory separators as prefix match.
            let unix_prefix =
                path_hook_base
                    .as_object()
                    .call_method(py, "__add__", ("/",), None)?;
            let windows_prefix =
                path_hook_base
                    .as_object()
                    .call_method(py, "__add__", ("\\",), None)?;

            let prefix = PyTuple::new(py, &[unix_prefix, windows_prefix]);

            if !path
                .as_object()
                .call_method(py, "startswith", (prefix,), None)?
                .extract::<bool>(py)?
            {
                return Err(PyErr::new::<ValueError, _>(
                    py,
                    format!(
                        "{} is not prefixed by {}",
                        path.to_string_lossy(py),
                        path_hook_base.to_string_lossy(py)
                    ),
                ));
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
                .as_object()
                .call_method(py, "encode", ("utf-8", "replace"), None)?
                .extract::<Vec<u8>>(py)?;
            let path_bytes = path
                .as_object()
                .call_method(py, "encode", ("utf-8", "replace"), None)?
                .extract::<Vec<u8>>(py)?;

            // +1 for directory separator, which should always be 1 byte in UTF-8.
            let path_suffix: &[u8] = &path_bytes[path_hook_base_bytes.len() + 1..];
            let original_package_path = String::from_utf8(path_suffix.to_vec()).map_err(|e| {
                PyErr::new::<ValueError, _>(
                    py,
                    format!("error coercing package suffix to Rust string: {}", e),
                )
            })?;

            let package_path = original_package_path.replace('\\', "/");

            // Ban leading and trailing directory separators.
            if package_path.starts_with('/') || package_path.ends_with('/') {
                return Err(PyErr::new::<ValueError, _>(py, format!("rejecting virtual sub-directory because package part contains leading or trailing directory separator: {}", original_package_path)));
            }

            // Ban consecutive directory separators.
            if package_path.contains("//") {
                return Err(PyErr::new::<ValueError, _>(
                    py, format!("rejecting virtual sub-directory because it has consecutive directory separators: {}", original_package_path))
                );
            }

            // Since we have to normalize to Python package form where dots are
            // special, ban dots in special places.
            if package_path
                .split('/')
                .any(|s| s.starts_with('.') || s.ends_with('.') || s.contains(".."))
            {
                return Err(PyErr::new::<ValueError, _>(
                    py, format!("rejecting virtual sub-directory because package part contains illegal dot characters: {}", original_package_path)
                ));
            }

            if package_path.is_empty() {
                None
            } else {
                Some(package_path.replace('/', "."))
            }
        };

        OxidizedPathEntryFinder::create_instance(
            py,
            OxidizedFinder::create_instance(py, self.state(py).clone())?,
            path.clone_ref(py),
            target_package,
        )
    }
}

impl OxidizedFinder {
    /// Construct an instance from a module and resources state.
    #[cfg(not(library_mode = "extension"))]
    fn new_from_module_and_resources<'a>(
        py: Python,
        m: &PyModule,
        resources_state: Box<PythonResourcesState<'a, u8>>,
        importer_state_callback: Option<impl FnOnce(&mut ImporterState)>,
    ) -> PyResult<OxidizedFinder> {
        let bootstrap_module = py.import("_frozen_importlib")?;

        let mut importer_state = Arc::new(ImporterState::new(
            py,
            &m,
            &bootstrap_module,
            resources_state,
        )?);

        if let Some(cb) = importer_state_callback {
            let state_ref = Arc::<ImporterState>::get_mut(&mut importer_state)
                .expect("Arc::get_mut() should work");
            cb(state_ref);
        }

        let importer = OxidizedFinder::create_instance(py, importer_state)?;

        Ok(importer)
    }
}

/// OxidizedFinder.__new__(relative_path_origin=None))
fn oxidized_finder_new(
    py: Python,
    relative_path_origin: Option<PyObject>,
) -> PyResult<OxidizedFinder> {
    // We need to obtain an ImporterState instance. This requires handles on a
    // few items...

    // The module references are easy to obtain.
    let m = py.import(OXIDIZED_IMPORTER_NAME_STR)?;
    let bootstrap_module = py.import("_frozen_importlib")?;

    let mut resources_state = Box::new(
        PythonResourcesState::new_from_env().map_err(|err| PyErr::new::<ValueError, _>(py, err))?,
    );

    // Update origin if a value is given.
    if let Some(py_origin) = relative_path_origin {
        resources_state.origin = pyobject_to_pathbuf(py, py_origin)?;
    }

    let importer = OxidizedFinder::create_instance(
        py,
        Arc::new(ImporterState::new(
            py,
            &m,
            &bootstrap_module,
            resources_state,
        )?),
    )?;

    Ok(importer)
}

impl OxidizedFinder {
    pub(crate) fn get_state(&self, py: Python) -> Arc<ImporterState> {
        self.state(py).clone()
    }

    fn multiprocessing_set_start_method_impl(&self, py: Python) -> PyResult<PyObject> {
        let state = self.state(py);

        if let Some(v) = &state.multiprocessing_set_start_method {
            Ok(PyString::new(py, v.as_str()).into_object())
        } else {
            Ok(py.None())
        }
    }

    fn origin_impl(&self, py: Python) -> PyResult<PyObject> {
        path_to_pyobject(py, &self.state(py).get_resources_state().origin)
    }

    fn path_hook_base_str_impl(&self, py: Python) -> PyResult<PyObject> {
        path_to_pyobject(py, &self.state(py).get_resources_state().current_exe)
    }

    fn pkg_resources_import_auto_register_impl(&self, py: Python) -> PyResult<bool> {
        Ok(self.state(py).pkg_resources_import_auto_register)
    }

    fn index_bytes_impl(&self, py: Python, data: PyObject) -> PyResult<PyObject> {
        let resources_state: &mut PythonResourcesState<u8> =
            self.state(py).get_resources_state_mut();
        resources_state.index_pyobject(py, data)?;

        Ok(py.None())
    }

    fn index_file_memory_mapped_impl(&self, py: Python, path: PyObject) -> PyResult<PyObject> {
        let path = pyobject_to_pathbuf(py, path)?;

        let resources_state: &mut PythonResourcesState<u8> =
            self.state(py).get_resources_state_mut();
        resources_state
            .index_path_memory_mapped(path)
            .map_err(|e| PyErr::new::<ValueError, _>(py, e))?;

        Ok(py.None())
    }

    fn index_interpreter_builtins_impl(&self, py: Python) -> PyResult<PyObject> {
        let resources_state: &mut PythonResourcesState<u8> =
            self.state(py).get_resources_state_mut();
        resources_state
            .index_interpreter_builtins()
            .map_err(|e| PyErr::new::<ValueError, _>(py, e))?;

        Ok(py.None())
    }

    fn index_interpreter_builtin_extension_modules_impl(&self, py: Python) -> PyResult<PyObject> {
        let resources_state: &mut PythonResourcesState<u8> =
            self.state(py).get_resources_state_mut();
        resources_state
            .index_interpreter_builtin_extension_modules()
            .map_err(|e| PyErr::new::<ValueError, _>(py, e))?;

        Ok(py.None())
    }

    fn index_interpreter_frozen_modules_impl(&self, py: Python) -> PyResult<PyObject> {
        let resources_state: &mut PythonResourcesState<u8> =
            self.state(py).get_resources_state_mut();
        resources_state
            .index_interpreter_frozen_modules()
            .map_err(|e| PyErr::new::<ValueError, _>(py, e))?;

        Ok(py.None())
    }

    fn indexed_resources_impl(&self, py: Python) -> PyResult<PyObject> {
        let resources_state: &PythonResourcesState<u8> = self.state(py).get_resources_state();

        let mut resources = resources_state
            .resources
            .values()
            .collect::<Vec<&python_packed_resources::data::Resource<u8>>>();

        resources.sort_by_key(|r| &r.name);

        let objects: Result<Vec<PyObject>, PyErr> = resources
            .iter()
            .map(|r| resource_to_pyobject(py, r))
            .collect();

        Ok(objects?.to_py_object(py).into_object())
    }

    fn add_resource_impl(&self, py: Python, resource: OxidizedResource) -> PyResult<PyObject> {
        let resources_state: &mut PythonResourcesState<u8> =
            self.state(py).get_resources_state_mut();

        resources_state
            .add_resource(pyobject_to_resource(py, resource))
            .map_err(|_| PyErr::new::<ValueError, _>(py, "unable to add resource to finder"))?;

        Ok(py.None())
    }

    fn add_resources_impl(
        &self,
        py: Python,
        resources: Vec<OxidizedResource>,
    ) -> PyResult<PyObject> {
        let resources_state: &mut PythonResourcesState<u8> =
            self.state(py).get_resources_state_mut();

        for resource in resources {
            resources_state
                .add_resource(pyobject_to_resource(py, resource))
                .map_err(|_| PyErr::new::<ValueError, _>(py, "unable to add resource to finder"))?;
        }

        Ok(py.None())
    }

    fn serialize_indexed_resources_impl(
        &self,
        py: Python,
        ignore_builtin: bool,
        ignore_frozen: bool,
    ) -> PyResult<PyObject> {
        let resources_state: &PythonResourcesState<u8> = self.state(py).get_resources_state();

        let data = resources_state
            .serialize_resources(ignore_builtin, ignore_frozen)
            .map_err(|e| PyErr::new::<ValueError, _>(py, format!("error serializing: {}", e)))?;

        Ok(PyBytes::new(py, &data).into_object())
    }
}

// A (mostly compliant) `importlib.abc.PathEntryFinder` that delegates paths
// within the current executable to the `OxidizedFinder` whose `path_hook`
// method created it.
py_class!(pub(crate) class OxidizedPathEntryFinder |py| {
    // A clone of the meta path finder from which we came.
    data finder: OxidizedFinder;

    // The sys.path value this instance was created with.
    data source_path: PyString;

    // Name of package being targeted.
    //
    // None is the top-level. Some(T) is a specific package in the hierarchy.
    data target_package: Option<String>;

    def find_spec(&self, fullname: &str, target: Option<PyModule> = None) -> PyResult<Option<PyObject>> {
        self.find_spec_impl(py, fullname, target)
    }

    def invalidate_caches(&self) -> PyResult<PyObject> {
        self.finder(py).as_object().call_method(py, "invalidate_caches", NoArgs, None)
    }

    def iter_modules(&self, prefix: &str = "") -> PyResult<PyList> {
        self.iter_modules_impl(py, prefix)
    }

    // Private getter. Just for testing.
    @property def _package(&self) -> PyResult<Option<String>> {
        Ok(self.target_package(py).clone())
    }
});

impl OxidizedPathEntryFinder {
    pub(crate) fn get_finder<'a>(&'a self, py: Python<'a>) -> &'a OxidizedFinder {
        self.finder(py)
    }

    pub(crate) fn get_source_path<'a>(&'a self, py: Python<'a>) -> &'a PyString {
        self.source_path(py)
    }

    pub(crate) fn get_target_package<'a>(&'a self, py: Python<'a>) -> &'a Option<String> {
        self.target_package(py)
    }
}

impl OxidizedPathEntryFinder {
    fn find_spec_impl(
        &self,
        py: Python,
        fullname: &str,
        target: Option<PyModule>,
    ) -> PyResult<Option<PyObject>> {
        if !name_at_package_hierarchy(
            fullname,
            self.target_package(py).as_ref().map(|s| s.as_str()),
        ) {
            return Ok(Some(py.None()));
        }

        self.finder(py)
            .as_object()
            .call_method(
                py,
                "find_spec",
                (
                    fullname,
                    PyList::new(py, &[self.source_path(py).as_object().clone_ref(py)]).as_object(),
                    target,
                ),
                None,
            )
            .map(|spec| if spec == py.None() { None } else { Some(spec) })
    }

    fn iter_modules_impl(&self, py: Python, prefix: &str) -> PyResult<PyList> {
        let state = self.finder(py).state(py);
        let modules = state.get_resources_state().pkgutil_modules_infos(
            py,
            self.target_package(py).as_ref().map(|s| s.as_str()),
            Some(prefix.to_string()),
            state.optimize_level,
        );
        // unwrap() is safe because pkgutil_modules_infos returns a PyList cast
        // into a PyObject.
        Ok(modules?.cast_into(py).unwrap())
    }
}

// Implements in-memory reading of resource data.
//
// Implements importlib.abc.ResourceReader.
py_class!(pub(crate) class OxidizedResourceReader |py| {
    data state: Arc<ImporterState>;
    data package: String;

    def open_resource(&self, resource: &PyString) -> PyResult<PyObject> {
        self.open_resource_impl(py, resource)
    }

    def resource_path(&self, resource: &PyString) -> PyResult<PyObject> {
        self.resource_path_impl(py, resource)
    }

    def is_resource(&self, name: &PyString) -> PyResult<PyObject> {
        self.is_resource_impl(py, name)
    }

    def contents(&self) -> PyResult<PyObject> {
        self.contents_impl(py)
    }
});

impl OxidizedResourceReader {
    /// Returns an opened, file-like object for binary reading of the resource.
    ///
    /// If the resource cannot be found, FileNotFoundError is raised.
    fn open_resource_impl(&self, py: Python, resource: &PyString) -> PyResult<PyObject> {
        let state = self.state(py);
        let package = self.package(py);

        if let Some(file) = state.get_resources_state().get_package_resource_file(
            py,
            &package,
            &resource.to_string(py)?,
        )? {
            Ok(file)
        } else {
            Err(PyErr::new::<FileNotFoundError, _>(py, "resource not found"))
        }
    }

    /// Returns the file system path to the resource.
    ///
    /// If the resource does not concretely exist on the file system, raise
    /// FileNotFoundError.
    fn resource_path_impl(&self, py: Python, _resource: &PyString) -> PyResult<PyObject> {
        Err(PyErr::new::<FileNotFoundError, _>(
            py,
            "in-memory resources do not have filesystem paths",
        ))
    }

    /// Returns True if the named name is considered a resource. FileNotFoundError
    /// is raised if name does not exist.
    fn is_resource_impl(&self, py: Python, name: &PyString) -> PyResult<PyObject> {
        let state = self.state(py);
        let package = self.package(py);

        if state
            .get_resources_state()
            .is_package_resource(&package, &name.to_string(py)?)
        {
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
    fn contents_impl(&self, py: Python) -> PyResult<PyObject> {
        let state = self.state(py);
        let package = self.package(py);

        state
            .get_resources_state()
            .package_resource_names(py, &package)
    }
}

// Path-like object facilitating Python resource access.
//
// This implements importlib.abc.Traversable.
py_class!(pub(crate) class PyOxidizerTraversable |py| {
    data state: Arc<ImporterState>;
    data path: String;

    // Yield Traversable objects in self.
    def iterdir(&self) -> PyResult<PyObject> {
        self.iterdir_impl(py)
    }

    // Read contents of self as bytes.
    def read_bytes(&self) -> PyResult<PyObject> {
        self.read_bytes_impl(py)
    }

    // Read contents of self as text.
    def read_text(&self) -> PyResult<PyObject> {
        self.read_text_impl(py)
    }

    // Return True if self is a dir.
    def is_dir(&self) -> PyResult<PyObject> {
        Ok(self.is_dir_impl(py))
    }

    // Return True if self is a file.
    def is_file(&self) -> PyResult<PyObject> {
        self.is_file_impl(py)
    }

    // Return Traversable child in self.
    def joinpath(&self, child: &PyObject) -> PyResult<PyObject> {
        self.joinpath_impl(py, child)
    }

    /// Return Traversable child in self.
    def __truediv__(&self, child: &PyObject) -> PyResult<PyObject> {
        self.joinpath_impl(py, child)
    }

    // mode may be 'r' or 'rb' to open as text or binary. Return a handle
    // suitable for reading (same as pathlib.Path.open).
    //
    // When opening as text, accepts encoding parameters such as those
    // accepted by io.TextIOWrapper.
    def open(&self, *args, **kwargs) -> PyResult<PyObject> {
        self.open_impl(py, args, kwargs)
    }
});

impl PyOxidizerTraversable {
    fn iterdir_impl(&self, _py: Python) -> PyResult<PyObject> {
        unimplemented!();
    }

    fn read_bytes_impl(&self, _py: Python) -> PyResult<PyObject> {
        unimplemented!();
    }

    fn read_text_impl(&self, _py: Python) -> PyResult<PyObject> {
        unimplemented!();
    }

    fn is_dir_impl(&self, py: Python) -> PyObject {
        let state = self.state(py);
        let path = self.path(py);

        // We are a directory if the current path is a known package.
        // TODO We may need to expand this definition in the future to cover
        // virtual subdirectories in addressable resources. But this will require
        // changes to the resources data format to capture said annotations.
        if let Some(entry) = state
            .get_resources_state()
            .resolve_importable_module(&path, state.optimize_level)
        {
            if entry.is_package {
                return py.True().into_object();
            }
        }

        py.False().into_object()
    }

    fn is_file_impl(&self, _py: Python) -> PyResult<PyObject> {
        unimplemented!();
    }

    fn joinpath_impl(&self, _py: Python, _child: &PyObject) -> PyResult<PyObject> {
        unimplemented!();
    }

    fn open_impl(
        &self,
        _py: Python,
        _args: &PyTuple,
        _kwargs: Option<&PyDict>,
    ) -> PyResult<PyObject> {
        unimplemented!();
    }
}

/// Replace all meta path importers with an OxidizedFinder instance and return it.
///
/// This is called after PyInit_* to finish the initialization of the
/// module. Its state struct is updated.
#[cfg(not(library_mode = "extension"))]
pub(crate) fn replace_meta_path_importers<'a>(
    py: Python,
    oxidized_importer: &PyModule,
    resources_state: Box<PythonResourcesState<'a, u8>>,
    importer_state_callback: Option<impl FnOnce(&mut ImporterState)>,
) -> PyResult<PyObject> {
    let mut state = get_module_state(py, oxidized_importer)?;

    let sys_module = py.import("sys")?;

    // Construct and register our custom meta path importer. Because our meta path
    // importer is able to handle builtin and frozen modules, the existing meta path
    // importers are removed. The assumption here is that we're called very early
    // during startup and the 2 default meta path importers are installed.
    let oxidized_finder = OxidizedFinder::new_from_module_and_resources(
        py,
        oxidized_importer,
        resources_state,
        importer_state_callback,
    )?;

    let meta_path_object = sys_module.get(py, "meta_path")?;

    meta_path_object.call_method(py, "clear", NoArgs, None)?;
    meta_path_object.call_method(py, "append", (oxidized_finder.clone_ref(py),), None)?;

    state.initialized = true;

    Ok(oxidized_finder.into_object())
}

/// Undoes the actions of `importlib._bootstrap_external` initialization.
///
/// This will remove types that aren't defined by this extension from
/// `sys.meta_path` and `sys.path_hooks`.
#[cfg(not(library_mode = "extension"))]
pub(crate) fn remove_external_importers(py: Python, sys_module: &PyModule) -> PyResult<()> {
    let meta_path = sys_module.get(py, "meta_path")?;
    let meta_path = meta_path.cast_into::<PyList>(py)?;

    // We need to mutate the lists in place so any updates are reflected
    // in references to the lists.

    let mut oxidized_path_hooks = vec![];
    let mut index = 0;
    while index < meta_path.len(py) {
        let entry = meta_path.get_item(py, index);

        // We want to preserve `_frozen_importlib.BuiltinImporter` and
        // `_frozen_importlib.FrozenImporter`, if present. Ideally we'd
        // do PyType comparisons. However, there doesn't appear to be a way
        // to easily get a handle on their PyType. We'd also prefer to do
        // PyType.name() checks. But both these report `type`. So we key
        // of `__module__` instead.
        if entry.get_type(py) == py.get_type::<OxidizedFinder>() {
            oxidized_path_hooks.push(entry.getattr(py, "path_hook")?);
            index += 1;
        } else if entry
            .getattr(py, "__module__")?
            .cast_as::<PyString>(py)?
            .to_string_lossy(py)
            == "_frozen_importlib"
        {
            index += 1;
        } else {
            meta_path
                .as_object()
                .call_method(py, "pop", (index,), None)?;
        }
    }

    let path_hooks = sys_module.get(py, "path_hooks")?;
    let path_hooks = path_hooks.cast_into::<PyList>(py)?;

    let mut index = 0;
    while index < path_hooks.len(py) {
        let entry = path_hooks.get_item(py, index);

        if oxidized_path_hooks
            .iter()
            .any(|candidate| candidate == &entry)
        {
            index += 1;
        } else {
            path_hooks
                .as_object()
                .call_method(py, "pop", (index,), None)?;
        }
    }

    Ok(())
}

/// Append [`OxidizedFinder::path_hook`] to [`sys.path_hooks`].
///
/// `sys` must be a reference to the [`sys`] module.
///
/// [`sys.path_hooks`]: https://docs.python.org/3/library/sys.html#sys.path_hooks
/// [`sys`]: https://docs.python.org/3/library/sys.html
#[cfg(not(library_mode = "extension"))]
pub(crate) fn install_path_hook(py: Python, finder: &PyObject, sys: &PyModule) -> PyResult<()> {
    let hook = finder.getattr(py, "path_hook")?;
    sys.get(py, "path_hooks")?
        .call_method(py, "insert", (0, hook), None)
        .map(|_| ())
}
