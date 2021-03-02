// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Functionality for a Python importer.

This module defines a Python meta path importer and associated functionality
for importing Python modules from memory.
*/

#[cfg(not(library_mode = "extension"))]
use cpython::NoArgs;
#[cfg(windows)]
use {
    crate::memory_dll::{free_library_memory, get_proc_address_memory, load_library_memory},
    cpython::exc::SystemError,
    std::ffi::{c_void, CString},
};
use {
    crate::{
        conversion::pyobject_to_pathbuf,
        python_resources::{
            pyobject_to_resource, resource_to_pyobject, ModuleFlavor, OptimizeLevel,
            OxidizedResource, PythonResourcesState,
        },
        resource_scanning::find_resources_in_path,
    },
    cpython::{
        exc::{FileNotFoundError, ImportError, ValueError},
        {
            py_class, py_fn, ObjectProtocol, PyBytes, PyCapsule, PyClone, PyDict, PyErr, PyList,
            PyModule, PyObject, PyResult, PyString, PyTuple, Python, PythonObject, ToPyObject,
        },
    },
    python3_sys as pyffi,
    std::sync::Arc,
};

pub const OXIDIZED_IMPORTER_NAME_STR: &str = "oxidized_importer";
pub const OXIDIZED_IMPORTER_NAME: &[u8] = b"oxidized_importer\0";

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
py_class!(class OxidizedFinder |py| {
    data state: Arc<ImporterState>;

    // Start of importlib.abc.MetaPathFinder interface.

    def find_spec(&self, fullname: &PyString, path: &PyObject, target: Option<PyObject> = None) -> PyResult<PyObject> {
        self.find_spec_impl(py, fullname, path, target)
    }

    def find_module(&self, fullname: &PyObject, path: &PyObject) -> PyResult<PyObject> {
        self.find_module_impl(py, fullname, path)
    }

    def invalidate_caches(&self) -> PyResult<PyObject> {
        self.invalidate_caches_impl(py)
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

    def path_hook(&self, path: PyObject) -> PyResult<_PathEntryFinder> {
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

    fn invalidate_caches_impl(&self, py: Python) -> PyResult<PyObject> {
        Ok(py.None())
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
        }
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

        resources_state.pkgutil_modules_infos(py, prefix, state.optimize_level, |resource| {
            _PathEntryFinder::is_visible("", &resource.name)
        })
    }

    // Canonicalize the path to the current executable or raise an OSError.
    //
    // Even `std::env::current_exe()` can fail to `fs::canonicalize()` if, e.g.,
    // the file is deleted after the program starts.
    fn exe_realpath(&self, py: Python) -> PyResult<std::path::PathBuf> {
        let current_exe = &self.state(py).get_resources_state().current_exe;
        current_exe.canonicalize().map_err(|err| {
            let exe = current_exe.display();
            // Use a Python expression instead of PyErr::new to ensure we get the
            // right OSError subclass.
            let strerror = format!("cannot open current executable: '{}'", exe);
            let raw_os_error = err
                .raw_os_error()
                .map_or_else(|| "None".to_string(), |err_code| err_code.to_string());
            let code = if cfg!(windows) {
                format!(
                    "OSError(None, r\"{}\", r\"{}\", {})",
                    strerror, exe, raw_os_error
                )
            } else {
                format!("OSError({}, r\"{}\", r\"{}\")", raw_os_error, strerror, exe)
            };
            py.eval(&code, None, None).map_or_else(
                |err_creating_exc| err_creating_exc,
                |exc| PyErr::from_instance(py, exc),
            )
        })
    }

    fn path_hook_impl(&self, py: Python, path: PyObject) -> PyResult<_PathEntryFinder> {
        // `paths` will become the return value's `path` attribute, which exists
        // only to be passed to `self.find_spec`'s
        // `path: Optional[Iterable[str]]` argument.
        let paths = {
            // `import os` is safe: sys.path_hooks is read only after
            // importlib._bootstrap_internal has been setup, either in that module
            // module or after initialization by PyImport_GetImporter checking
            // for the existence of config->run_filename has been setup.
            let os = py.import("os")?;
            PyList::new(py, &[os.call(py, "fspath", (path.clone_ref(py),), None)?])
        };

        // Compute _PathEntryFinder::package
        let pkg = {
            let current_exe = self.exe_realpath(py)?;
            let not_exe_err = || {
                let msg = format!(
                    "path {} does not begin in \'{}\' (if you're sure it does, check Python's file-system encoding)", &path, current_exe.display());
                let mut err = PyErr::new::<ImportError, _>(py, msg);
                let exc = err.instance(py);
                if let Err(setattr_err) = exc.setattr(py, "path", path.clone_ref(py)) {
                    setattr_err
                } else {
                    err
                }
            };
            let pbuf = pyobject_to_pathbuf(py, path.clone_ref(py))?;
            let mut p: &std::path::Path = pbuf.as_ref();
            // path could point "inside" current_exe. If we can't canonicalize,
            // strip the last component and check again. (zipimporter does
            // somethings similar)
            let mut abs_p = p.canonicalize();
            // Count the number of components at the end of `path` will be in pkg
            let mut parts: usize = 0;
            while abs_p.is_err() {
                p = match p.parent() {
                    Some(parent) => {
                        parts += 1;
                        parent
                    }
                    None => {
                        return Err(not_exe_err());
                    }
                };
                abs_p = p.canonicalize();
            }
            let abs_p = abs_p.unwrap();
            if abs_p != current_exe {
                return Err(not_exe_err());
            }
            // Extract the part of `path` after current_exe
            // FIXME: This takes two allocations, but it shouldn't even use one.
            let tail = pbuf
                .iter()
                .rev()
                .take(parts)
                .collect::<std::path::PathBuf>()
                .iter()
                .rev()
                .collect::<std::path::PathBuf>();
            _PathEntryFinder::parse_path_to_pkg(py, &tail)?
        };
        _PathEntryFinder::create_instance(py, self.as_object().clone_ref(py), paths, pkg)
    }
}

// A (mostly compliant) `importlib.abc.PathEntryFinder` that delegates paths
// within the current executable to the `OxidizedFinder` whose `path_hook`
// method created it. This is a private implementation detail.
py_class!(class _PathEntryFinder |py| {
    // A `importlib.abc.MetaPathFinder`, presumably but not necessarily an
    // `OxidizedFinder`. However, `_PathEntryFinder::iter_modules` will raise
    // a `TypeError` when called if `finder` is not an `OxidizedFinder`.
    data finder: PyObject;
    // A list containing a single either str or bytes, normalized from the `path`
    // argument to `OxidizedFinder::path_hook`. Used only for passing to
    // `finder.find_spec`'s `path` argument.
    data path: PyList;
    // If the entry of `path` is `os.path.join(sys.executable, "pkg", "mod")`,
    // then `package` is `"pkg.mod"`.
    data package: String;

    def find_spec(&self, fullname: &str, target: Option<PyModule> = None) -> PyResult<Option<PyObject>> {
        self.find_spec_impl(py, fullname, target)
    }

    def invalidate_caches(&self) -> PyResult<PyObject> {
        self.finder(py).call_method(py, "invalidate_caches", NoArgs, None)
    }

    def iter_modules(&self, prefix: &str = "") -> PyResult<PyList> {
        self.iter_modules_impl(py, prefix)
    }

    // Private getter. Just for testing.
    @property def _package(&self) -> PyResult<String> {
        Ok(self.package(py).clone())
    }
});

impl _PathEntryFinder {
    // `name` is considered _visible_ in `package` if `name` is in the first
    // level of the package. The computation is purely textual. See this
    // module's `test_path_entry_finder::is_visible` test.
    fn is_visible(package: &str, name: &str) -> bool {
        if package.starts_with('.') || package.ends_with('.') || package.contains("..") {
            return false;
        }
        // This implementation can be simplified when str::rsplit_once stabilize
        // https://doc.rust-lang.org/std/primitive.str.html#method.rsplit_once
        let mut split = name.rsplitn(2, '.');
        match split.next() {
            Some(module) if !module.is_empty() && !module.contains('.') => {}
            _ => {
                return false;
            }
        }
        match split.next() {
            Some(pkg) if !package.is_empty() && pkg == package => true,
            None if package.is_empty() => true,
            _ => false,
        }
    }

    fn find_spec_impl(
        &self,
        py: Python,
        fullname: &str,
        target: Option<PyModule>,
    ) -> PyResult<Option<PyObject>> {
        if !Self::is_visible(self.package(py), fullname) {
            return Ok(Some(py.None()));
        }
        self.finder(py)
            .call_method(py, "find_spec", (fullname, self.path(py), target), None)
            .map(|spec| if spec == py.None() { None } else { Some(spec) })
    }

    fn iter_modules_impl(&self, py: Python, prefix: &str) -> PyResult<PyList> {
        let state = self.finder(py).cast_as::<OxidizedFinder>(py)?.state(py);
        let modules = state.get_resources_state().pkgutil_modules_infos(
            py,
            Some(prefix.to_string()),
            state.optimize_level,
            |resource| Self::is_visible(self.package(py), &resource.name),
        );
        // unwrap() is safe because pkgutil_modules_infos returns a PyList cast
        // into a PyObject.
        Ok(modules?.cast_into(py).unwrap())
    }

    // Transform b"pkg/mod" into "pkg.mod" on behalf of `OxidizedFinder::path_hook`.
    // Raise an `ImportError` if decoding `path` fails.
    fn parse_path_to_pkg(py: Python, path: &std::path::Path) -> PyResult<String> {
        // Use Python's filesystem encoding to ensure correctly
        // round-tripping the str Python package name from the
        // potentially non-Unicode path.
        // https://pubs.opengroup.org/onlinepubs/9699919799/basedefs/V1_chap03.html#tag_03_170
        // https://docs.microsoft.com/en-us/windows/win32/fileio/naming-a-file
        let pkg = crate::conversion::path_to_pyobject(py, path)?
            .cast_as::<PyString>(py)?
            .to_string(py)
            .map_err(|mut unicode_err| {
                // Need to raise an ImportError if can't decode. For
                // clarity, we also attach the UnicodeDecodeError.
                // https://docs.python.org/3/reference/import.html#path-entry-finders
                let mut imp_err = PyErr::new::<ImportError, _>(py, "cannot decode path");
                let imp_exc = imp_err.instance(py);
                if let Err(err) = imp_exc.setattr(py, "__suppress_context__", true) {
                    err
                } else if let Err(err) = imp_exc.setattr(py, "__cause__", unicode_err.instance(py))
                {
                    err
                } else {
                    imp_err
                }
            })?
            .replace("/", ".");
        #[cfg(windows)]
        let pkg = pkg.replace("\\", ".");
        Ok(pkg)
    }
}

impl OxidizedFinder {
    /// Construct an instance from a module and resources state.
    #[cfg(not(library_mode = "extension"))]
    fn new_from_module_and_resources<'a>(
        py: Python,
        m: &PyModule,
        resources_state: Box<PythonResourcesState<'a, u8>>,
    ) -> PyResult<OxidizedFinder> {
        let bootstrap_module = py.import("_frozen_importlib")?;

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

// Implements in-memory reading of resource data.
//
// Implements importlib.abc.ResourceReader.
py_class!(class OxidizedResourceReader |py| {
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
py_class!(class PyOxidizerTraversable |py| {
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
        self.is_dir_impl(py)
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

    fn is_dir_impl(&self, py: Python) -> PyResult<PyObject> {
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
                return Ok(py.True().into_object());
            }
        }

        Ok(py.False().into_object())
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

const DOC: &[u8] = b"A highly-performant importer implemented in Rust\0";

/// State associated with each importer module instance.
///
/// We write per-module state to per-module instances of this struct so
/// we don't rely on global variables and so multiple importer modules can
/// exist without issue.
#[derive(Debug)]
struct ModuleState {
    /// Whether the module has been initialized.
    initialized: bool,
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

/// Decodes source bytes into a str.
///
/// This is effectively a reimplementation of
/// importlib._bootstrap_external.decode_source().
fn decode_source(py: Python, io_module: &PyModule, source_bytes: PyObject) -> PyResult<PyObject> {
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
pub extern "C" fn PyInit_oxidized_importer() -> *mut pyffi::PyObject {
    let py = unsafe { cpython::Python::assume_gil_acquired() };

    // TRACKING RUST1.32 We can't call as_ptr() in const fn in Rust 1.31.
    unsafe {
        if MODULE_DEF.m_name.is_null() {
            MODULE_DEF.m_name = OXIDIZED_IMPORTER_NAME.as_ptr() as *const _;
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
    let sys_module = py.import("sys")?;
    let version_info = sys_module.get(py, "version_info")?;

    let major_version = version_info.getattr(py, "major")?.extract::<i32>(py)?;
    let minor_version = version_info.getattr(py, "minor")?.extract::<i32>(py)?;

    if major_version < 3 || minor_version < 8 {
        return Err(PyErr::new::<ImportError, _>(
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
            decode_source(io_module: &PyModule, source_bytes: PyObject)
        ),
    )?;
    m.add(
        py,
        "find_resources_in_path",
        py_fn!(py, find_resources_in_path(path: PyObject)),
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

    crate::package_metadata::module_init(py, m)?;

    Ok(())
}

/// Replace all meta path importers with an OxidizedFinder instance and return it.
///
/// This is called after PyInit_* to finish the initialization of the
/// module. Its state struct is updated.
#[cfg(not(library_mode = "extension"))]
pub(crate) fn replace_meta_path_importers<'a>(
    py: Python,
    m: &PyModule,
    resources_state: Box<PythonResourcesState<'a, u8>>,
) -> PyResult<PyObject> {
    let mut state = get_module_state(py, m)?;

    let sys_module = py.import("sys")?;

    // Construct and register our custom meta path importer. Because our meta path
    // importer is able to handle builtin and frozen modules, the existing meta path
    // importers are removed. The assumption here is that we're called very early
    // during startup and the 2 default meta path importers are installed.
    let unified_importer = OxidizedFinder::new_from_module_and_resources(py, m, resources_state)?;

    let meta_path_object = sys_module.get(py, "meta_path")?;

    meta_path_object.call_method(py, "clear", NoArgs, None)?;
    meta_path_object.call_method(py, "append", (unified_importer.clone_ref(py),), None)?;

    state.initialized = true;

    Ok(unified_importer.into_object())
}

/// Append [`OxidizedFinder::path_hook`] to [`sys.path_hooks`].
///
/// `sys` must be a reference to the [`sys`] module.
///
/// [`sys.path_hooks`]: https://docs.python.org/3/library/sys.html#sys.path_hooks
/// [`sys`]: https://docs.python.org/3/library/sys.html
#[cfg(not(library_mode = "extension"))]
pub(crate) fn initialize_path_hooks(py: Python, finder: &PyObject, sys: &PyModule) -> PyResult<()> {
    let hook = finder.getattr(py, "path_hook")?;
    sys.get(py, "path_hooks")?
        .call_method(py, "append", (hook,), None)
        .map(|_| ())
}

#[cfg(test)]
mod test_path_entry_finder {
    use super::{_PathEntryFinder, oxidized_finder_new};

    #[test]
    fn is_visible() {
        let is_visible = _PathEntryFinder::is_visible;
        // The empty package name allows top-level imports.
        assert!(is_visible("", "importlib"));

        // Any module on the first nesting level of a package are visible.
        // panic!("am i alive?")
        assert!(is_visible("importlib", "importlib.resources"));
        assert!(is_visible("importlib", "importlib.machinery"));
        assert!(is_visible("importlib", "importlib._private"));

        // Modules a level lower or higher than the first level inside a package are invisible.
        assert!(!is_visible("importlib", "importlib.machinery.internal"));
        assert!(!is_visible("", "importlib.resources"));
        assert!(!is_visible("importlib", "importlib"));

        // Modules in different packages than `package` are invisible.
        assert!(!is_visible("idlelib", "importlib.resources"));
        assert!(!is_visible("imp", "importlib.resources"));

        // Malformed package names always cause `is_visible` to return [`false`].
        assert!(!is_visible(".a.b", ".a.b.c"));
        assert!(!is_visible("a..b", "a..b.c"));
        assert!(!is_visible("a.b.", "a.b.c"));
        assert!(!is_visible(".", "a"));

        // Unicode is fully supported.
        assert!(is_visible("חבילות", "חבילות.מודול"));
    }

    fn get_py(fsencoding: Option<&str>) -> crate::MainPythonInterpreter {
        let mut config = crate::OxidizedPythonInterpreterConfig::default();
        config.interpreter_config.parse_argv = Some(false);
        config.set_missing_path_configuration = false;
        config.oxidized_importer = true;
        if fsencoding.is_some() {
            config.interpreter_config.filesystem_encoding = fsencoding.map(|e| e.to_string());
        }
        crate::MainPythonInterpreter::new(config).unwrap()
    }

    /// _PathEntryFinder::parse_path_to_pkg re-encodes the package part of the
    /// path with Python's filesystem encoding, rather than forcing UTF-8.
    ///
    /// Decoding "\u3030" (WAVY DASH) into UTF-16-LE and encoding the result
    /// with UTF-8 returns the two-character sequence "00". So we set the
    /// filesystem encoding to UTF-16-LE, pass in os.fsencode("\u3030"), and
    /// ensure we get "\u3030" back out.
    #[test]
    fn parse_path_to_pkg_filesystem_encoding() {
        // Verify assumptions about the test itself.
        const WAVY_DASH: &str = "〰";
        const WAVY_DASH_CODE: u16 = 0x3030;
        assert_eq!(WAVY_DASH, "\u{3030}");
        assert_eq!(
            WAVY_DASH.encode_utf16().collect::<Vec<u16>>(),
            [WAVY_DASH_CODE]
        );
        assert_eq!(WAVY_DASH_CODE.to_be_bytes(), [48, 48]); // endianness
        assert_eq!(WAVY_DASH_CODE.to_le_bytes(), [48, 48]); // doesn't matter
        assert_eq!(std::str::from_utf8(&[48, 48]).unwrap(), "00");

        let wavy_dash = {
            // Obtain a Python interpreter with filesystem encoding set to UTF-16-LE
            let mut interp = get_py(Some("UTF-16-LE"));
            let py = interp.acquire_gil();
            assert_eq!(
                py.import("sys")
                    .unwrap()
                    .call(py, "getfilesystemencoding", cpython::NoArgs, None)
                    .unwrap()
                    .to_string(),
                "utf-16-le"
            );
            _PathEntryFinder::parse_path_to_pkg(py, std::path::Path::new("00")).unwrap()
        };

        // The actual test.
        assert_eq!(wavy_dash, WAVY_DASH);
    }

    /// OxidizedFinder::path_hook raises an OSError if current_exe cannot be
    /// canonicalized. The OSError returns the right exception subclass.
    #[test]
    fn bad_current_exe() {
        let exe: std::path::PathBuf = "/../a/../foo.txt".into();
        assert_eq!(
            exe.canonicalize().unwrap_err().kind(),
            std::io::ErrorKind::NotFound
        );
        let exc = {
            let mut interp = get_py(None);
            let py = interp.acquire_gil();
            let finder = oxidized_finder_new(py, None).unwrap();
            finder.state(py).get_resources_state_mut().current_exe = exe;
            let exc = finder
                .exe_realpath(py)
                .expect_err("canonicalize unexpectedly succeeded")
                .instance(py);
            format!("{:?}", exc)
        };
        assert_eq!(
            exc,
            "FileNotFoundError(2, \"cannot open current executable: '/../a/../foo.txt'\")"
        );
    }
}
