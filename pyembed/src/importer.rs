// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Functionality for a Python importer.

This module defines a Python meta path importer and associated functionality
for importing Python modules from memory.
*/

use {
    super::interpreter::PYOXIDIZER_IMPORTER_NAME,
    super::python_resources::{OptimizeLevel, PythonResourcesState},
    cpython::exc::{FileNotFoundError, ImportError, RuntimeError, ValueError},
    cpython::{
        py_class, py_fn, NoArgs, ObjectProtocol, PyClone, PyDict, PyErr, PyList, PyModule,
        PyObject, PyResult, PyString, PyTuple, Python, PythonObject,
    },
    python3_sys as pyffi,
    python_packed_resources::data::ResourceFlavor,
    std::path::PathBuf,
    std::sync::Arc,
};
#[cfg(windows)]
use {
    super::memory_dll::{free_library_memory, get_proc_address_memory, load_library_memory},
    cpython::exc::SystemError,
    std::ffi::{c_void, CString},
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
        pyffi::_PyImport_FindExtensionObjectEx(
            name_py.as_ptr(),
            origin.as_object().as_ptr(),
            sys_modules.as_ptr(),
        )
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

    load_dynamic_library(py, sys_modules, spec, name_py, name, module).or_else(|e| {
        unsafe {
            free_library_memory(module);
        }
        Err(e)
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

    if py_module.is_null() {
        if unsafe { pyffi::PyErr_Occurred().is_null() } {
            return Err(PyErr::new::<SystemError, _>(
                py,
                format!(
                    "initialization of {} failed without raising an exception",
                    name
                ),
            ));
        }
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
    /// `importlib._bootstrap_external.decode_source` function.
    decode_source: PyObject,
    /// `builtins.exec` function.
    exec_fn: PyObject,
    /// Bytecode optimization level currently in effect.
    optimize_level: OptimizeLevel,
    /// Holds state about importable resources.
    pub resources_state: PythonResourcesState<'static, u8>,
}

impl ImporterState {
    fn new(
        py: Python,
        bootstrap_module: &PyModule,
        marshal_module: &PyModule,
        decode_source: PyObject,
        resources_data: &'static [u8],
        current_exe: PathBuf,
        origin: PathBuf,
    ) -> Result<Self, PyErr> {
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
        if meta_path.len(py) != 2 {
            return Err(PyErr::new::<ValueError, _>(
                py,
                "sys.meta_path does not contain 2 values",
            ));
        }

        let builtin_importer = meta_path.get_item(py, 0);
        let frozen_importer = meta_path.get_item(py, 1);

        let mut resources_state = PythonResourcesState {
            current_exe,
            origin,
            ..PythonResourcesState::default()
        };

        if let Err(e) = resources_state.load(resources_data) {
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

        Ok(ImporterState {
            imp_module,
            sys_module,
            marshal_loads,
            builtin_importer,
            frozen_importer,
            call_with_frames_removed,
            module_spec_type,
            decode_source,
            exec_fn,
            optimize_level,
            resources_state,
        })
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
py_class!(class PyOxidizerFinder |py| {
    data state: Arc<Box<ImporterState>>;

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
});

// importlib.abc.MetaPathFinder interface.
impl PyOxidizerFinder {
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
            .resources_state
            .resolve_importable_module(&key, state.optimize_level)
        {
            Some(module) => module,
            None => return Ok(py.None()),
        };

        match module.flavor {
            ResourceFlavor::Extension | ResourceFlavor::Module => module.resolve_module_spec(
                py,
                &state.module_spec_type,
                self.as_object(),
                state.optimize_level,
            ),
            ResourceFlavor::BuiltinExtensionModule => {
                // BuiltinImporter.find_spec() always returns None if `path` is defined.
                // And it doesn't use `target`. So don't proxy these values.
                state
                    .builtin_importer
                    .call_method(py, "find_spec", (fullname,), None)
            }
            ResourceFlavor::FrozenModule => {
                state
                    .frozen_importer
                    .call_method(py, "find_spec", (fullname, path, target), None)
            }
            _ => Ok(py.None()),
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
impl PyOxidizerFinder {
    fn create_module_impl(&self, py: Python, spec: &PyObject) -> PyResult<PyObject> {
        let state = self.state(py);
        let name = spec.getattr(py, "name")?;
        let key = name.extract::<String>(py)?;

        let entry = match state.resources_state.resources.get(&*key) {
            Some(entry) => entry,
            None => return Ok(py.None()),
        };

        match entry.flavor {
            // Extension modules need special module creation logic.
            ResourceFlavor::Extension => {
                // We need a custom implementation of create_module() for in-memory shared
                // library extensions because if we wait until `exec_module()` to
                // initialize the module object, this can confuse some CPython
                // internals. A side-effect of initializing extension modules is
                // populating `sys.modules` and this made `LazyLoader` unhappy.
                // If we ever implement our own lazy module importer, we could
                // potentially work around this and move all extension module
                // initialization into `exec_module()`.
                if let Some(library_data) = &entry.in_memory_extension_module_shared_library {
                    let sys_modules = state.sys_module.as_object().getattr(py, "modules")?;

                    extension_module_shared_library_create_module(
                        &state.resources_state,
                        py,
                        sys_modules,
                        spec,
                        name,
                        &key,
                        library_data,
                    )
                } else {
                    // Call `imp.create_dynamic()` for dynamic extension modules.
                    let create_dynamic =
                        state.imp_module.as_object().getattr(py, "create_dynamic")?;

                    state
                        .call_with_frames_removed
                        .call(py, (&create_dynamic, spec), None)
                }
            }
            _ => Ok(py.None()),
        }
    }

    fn exec_module_impl(&self, py: Python, module: &PyObject) -> PyResult<PyObject> {
        let state = self.state(py);
        let name = module.getattr(py, "__name__")?;
        let key = name.extract::<String>(py)?;

        let mut entry = match state
            .resources_state
            .resolve_importable_module(&key, state.optimize_level)
        {
            Some(entry) => entry,
            None => {
                // Raising here might make more sense, as `find_spec()` shouldn't have returned
                // an entry for something that we don't know how to handle.
                return Ok(py.None());
            }
        };

        if let Some(bytecode) = entry.resolve_bytecode(py, state.optimize_level)? {
            let code = state.marshal_loads.call(py, (bytecode,), None)?;
            let dict = module.getattr(py, "__dict__")?;

            state
                .call_with_frames_removed
                .call(py, (&state.exec_fn, code, dict), None)
        } else if entry.flavor == &ResourceFlavor::BuiltinExtensionModule {
            state
                .builtin_importer
                .call_method(py, "exec_module", (module,), None)
        } else if entry.flavor == &ResourceFlavor::FrozenModule {
            state
                .frozen_importer
                .call_method(py, "exec_module", (module,), None)
        } else if entry.flavor == &ResourceFlavor::Extension {
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
impl PyOxidizerFinder {
    /// An abstract method to return the bytes for the data located at path.
    ///
    /// Loaders that have a file-like storage back-end that allows storing
    /// arbitrary data can implement this abstract method to give direct access
    /// to the data stored. OSError is to be raised if the path cannot be
    /// found. The path is expected to be constructed using a module’s __file__
    /// attribute or an item from a package’s __path__.
    fn get_data_impl(&self, py: Python, path: &PyString) -> PyResult<PyObject> {
        self.state(py)
            .resources_state
            .resolve_resource_data_from_path(py, path)
    }
}

// importlib.abc.InspectLoader interface.
impl PyOxidizerFinder {
    fn get_code_impl(&self, py: Python, fullname: &PyString) -> PyResult<PyObject> {
        let state = self.state(py);
        let key = fullname.to_string(py)?;

        let mut module = match state
            .resources_state
            .resolve_importable_module(&key, state.optimize_level)
        {
            Some(module) => module,
            None => return Ok(py.None()),
        };

        if let Some(bytecode) = module.resolve_bytecode(py, state.optimize_level)? {
            state.marshal_loads.call(py, (bytecode,), None)
        } else if module.flavor == &ResourceFlavor::FrozenModule {
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
            .resources_state
            .resolve_importable_module(&key, state.optimize_level)
        {
            Some(module) => module,
            None => return Ok(py.None()),
        };

        if let Some(source) = module.resolve_source(py)? {
            state.decode_source.call(py, (source,), None)
        } else {
            Ok(py.None())
        }
    }
}

// importlib.abc.ExecutionLoader interface.
impl PyOxidizerFinder {
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
            .resources_state
            .resolve_importable_module(&key, state.optimize_level)
            .ok_or_else(|| make_error("unknown module"))?;

        module
            .resolve_origin(py)
            .or_else(|_| Err(make_error("unable to resolve origin")))?
            .ok_or_else(|| make_error("no origin"))
    }
}

// Resource loading interface.
impl PyOxidizerFinder {
    fn get_resource_reader_impl(&self, py: Python, fullname: &PyString) -> PyResult<PyObject> {
        let state = self.state(py);
        let key = fullname.to_string(py)?;

        let entry = match state
            .resources_state
            .resolve_importable_module(&key, state.optimize_level)
        {
            Some(entry) => entry,
            None => return Ok(py.None()),
        };

        // Resources are only available on packages.
        if entry.is_package {
            let reader =
                PyOxidizerResourceReader::create_instance(py, state.clone(), key.to_string())?
                    .into_object();
            Ok(reader)
        } else {
            Ok(py.None())
        }
    }
}

// importlib.metadata support.
impl PyOxidizerFinder {
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
            // The passed object should have `path` and `name` attributes.
            let path = context.getattr(py, "path")?;
            let name = context.getattr(py, "name")?;
            (Some(path), Some(name))
        } else {
            // No argument = default Context = find everything.
            (None, None)
        };

        super::package_metadata::find_distributions(py, state.clone(), path, name)
    }
}

// Implements in-memory reading of resource data.
//
// Implements importlib.abc.ResourceReader.
py_class!(class PyOxidizerResourceReader |py| {
    data state: Arc<Box<ImporterState>>;
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

impl PyOxidizerResourceReader {
    /// Returns an opened, file-like object for binary reading of the resource.
    ///
    /// If the resource cannot be found, FileNotFoundError is raised.
    fn open_resource_impl(&self, py: Python, resource: &PyString) -> PyResult<PyObject> {
        let state = self.state(py);
        let package = self.package(py);

        if let Some(file) = state.resources_state.get_package_resource_file(
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
            .resources_state
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

        state.resources_state.package_resource_names(py, &package)
    }
}

// Path-like object facilitating Python resource access.
//
// This implements importlib.abc.Traversable.
py_class!(class PyOxidizerTraversable |py| {
    data state: Arc<Box<ImporterState>>;
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
            .resources_state
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

const DOC: &[u8] = b"Binary representation of Python modules\0";

/// Represents global module state to be passed at interpreter initialization time.
#[derive(Debug)]
pub struct InitModuleState {
    /// Path to currently running executable.
    pub current_exe: PathBuf,

    /// Directory where relative paths are relative to.
    pub origin: PathBuf,

    /// Whether to register the filesystem importer on sys.meta_path.
    pub register_filesystem_importer: bool,

    /// Values to set on sys.path.
    pub sys_paths: Vec<String>,

    /// Raw data describing embedded resources.
    pub packed_resources: &'static [u8],
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
    /// Currently running executable.
    current_exe: PathBuf,

    /// Directory where relative paths are relative to.
    origin: PathBuf,

    /// Whether to register PathFinder on sys.meta_path.
    register_filesystem_importer: bool,

    /// Values to set on sys.path.
    sys_paths: Vec<String>,

    /// Raw data constituting embedded resources.
    packed_resources: &'static [u8],

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
        // TODO we could move the value if we wanted to avoid the clone().
        state.current_exe = (*NEXT_MODULE_STATE).current_exe.clone();
        state.origin = (*NEXT_MODULE_STATE).origin.clone();
        state.register_filesystem_importer = (*NEXT_MODULE_STATE).register_filesystem_importer;
        // TODO we could move the value if we wanted to avoid the clone().
        state.sys_paths = (*NEXT_MODULE_STATE).sys_paths.clone();
        state.packed_resources = (*NEXT_MODULE_STATE).packed_resources;
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

    let sys_module = bootstrap_module.get(py, "sys")?;
    let sys_module = sys_module.cast_into::<PyModule>(py)?;
    let sys_module_ref = sys_module.clone_ref(py);

    // Construct and register our custom meta path importer. Because our meta path
    // importer is able to handle builtin and frozen modules, the existing meta path
    // importers are removed. The assumption here is that we're called very early
    // during startup and the 2 default meta path importers are installed.
    let unified_importer = PyOxidizerFinder::create_instance(
        py,
        Arc::new(Box::new(ImporterState::new(
            py,
            &bootstrap_module,
            &marshal_module,
            decode_source,
            &state.packed_resources,
            state.current_exe.clone(),
            state.origin.clone(),
        )?)),
    )?;

    let meta_path_object = sys_module.get(py, "meta_path")?;

    meta_path_object.call_method(py, "clear", NoArgs, None)?;
    meta_path_object.call_method(py, "append", (unified_importer,), None)?;

    // At this point the importing mechanism is fully initialized to use our
    // unified importer, which handles built-in, frozen, and embedded resources.

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
        let path_hooks = sys_module_ref.get(py, "path_hooks")?;
        path_hooks.call_method(py, "append", (path_hook,), None)?;

        let path_finder = frozen_importlib_external.get(py, "PathFinder")?;
        let meta_path = sys_module_ref.get(py, "meta_path")?;
        meta_path.call_method(py, "append", (path_finder,), None)?;
    }

    // Ideally we should be calling Py_SetPath() before Py_Initialize() to set sys.path.
    // But we tried to do this and only ran into problems due to string conversions,
    // unwanted side-effects. Updating sys.path directly before it is used by PathFinder
    // (which was just registered above) should have the same effect.

    // Always clear out sys.path.
    let sys_path = sys_module_ref.get(py, "path")?;
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
