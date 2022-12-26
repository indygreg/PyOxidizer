// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Management of Python resources.
*/

use {
    crate::conversion::{
        path_to_pathlib_path, pyobject_optional_resources_map_to_owned_bytes,
        pyobject_optional_resources_map_to_pathbuf, pyobject_to_owned_bytes_optional,
        pyobject_to_pathbuf_optional,
    },
    anyhow::Result,
    pyo3::{
        buffer::PyBuffer,
        exceptions::{PyImportError, PyOSError, PyValueError},
        ffi as pyffi,
        prelude::*,
        types::{PyBytes, PyDict, PyList, PyString, PyTuple},
        PyTypeInfo,
    },
    python_packaging::resource::BytecodeOptimizationLevel,
    python_packed_resources::Resource,
    std::{
        borrow::Cow,
        cell::RefCell,
        collections::{hash_map::Entry, BTreeSet, HashMap},
        ffi::CStr,
        os::raw::c_int,
        path::{Path, PathBuf},
    },
};

const ENOENT: c_int = 2;

/// Determines whether an entry represents an importable Python module.
///
/// Should only be called on module flavors.
fn is_module_importable<X>(entry: &Resource<X>, optimize_level: BytecodeOptimizationLevel) -> bool
where
    [X]: ToOwned<Owned = Vec<X>>,
{
    entry.in_memory_source.is_some()
        || entry.relative_path_module_source.is_some()
        || match optimize_level {
            BytecodeOptimizationLevel::Zero => {
                entry.in_memory_bytecode.is_some() || entry.relative_path_module_bytecode.is_some()
            }
            BytecodeOptimizationLevel::One => {
                entry.in_memory_bytecode_opt1.is_some() || entry.in_memory_bytecode_opt1.is_some()
            }
            BytecodeOptimizationLevel::Two => {
                entry.in_memory_bytecode_opt2.is_some() || entry.in_memory_bytecode_opt2.is_some()
            }
        }
}

/// Whether a resource name matches a package target.
///
/// This function is used for filtering through resources at a specific
/// level in the package hierarchy, as defined by `package_target`.
///
/// `None` means the root level and will only yield top-level elements.
/// `Some(T)` defines a specific package level.
///
/// Note that a resource is emitted by its parent. e.g. the `foo` resource
/// would be emitted by the `None` target and `foo.bar` would be emitted by
/// the `foo` target.
///
/// Targeting resources at specific levels in the package hierarchy is
/// likely encountered in path entry finders and iter_modules().
pub(crate) fn name_at_package_hierarchy(fullname: &str, package_target: Option<&str>) -> bool {
    match package_target {
        None => !fullname.contains('.'),
        Some(package) => match fullname.strip_prefix(&format!("{}.", package)) {
            Some(suffix) => !suffix.contains('.'),
            None => false,
        },
    }
}

/// Whether a resource name is within a given package hierarchy.
///
/// This is like [name_at_package_hierarchy] except non-immediate descendants
/// of the target package match. The `None` target matches everything.
/// `fullname == package_target` will never match, as it should be matched
/// by its parent.
pub(crate) fn name_within_package_hierarchy(fullname: &str, package_target: Option<&str>) -> bool {
    match package_target {
        None => true,
        Some(package) => fullname.starts_with(&format!("{}.", package)),
    }
}

/// Describes the type of an importable Python module.
#[derive(Debug, PartialEq, Eq)]
pub enum ModuleFlavor {
    Builtin,
    Frozen,
    Extension,
    SourceBytecode,
}

/// Holds state for an importable Python module.
///
/// This essentially is an abstraction over raw `Resource` entries that
/// allows the importer code to be simpler.
pub struct ImportablePythonModule<'a, X: 'a>
where
    [X]: ToOwned<Owned = Vec<X>>,
{
    /// The raw resource backing this importable module.
    resource: &'a Resource<'a, X>,

    /// Path to current executable.
    current_exe: &'a Path,

    /// Path from which relative paths should be interpreted.
    origin: &'a Path,

    /// The type of importable module.
    pub flavor: ModuleFlavor,
    /// Whether this module is a package.
    pub is_package: bool,
}

impl<'a> ImportablePythonModule<'a, u8> {
    /// Attempt to resolve a Python `bytes` for the source code behind this module.
    ///
    /// Will return a PyErr if an error occurs resolving source. If there is no source,
    /// returns `Ok(None)`. Otherwise an `Ok(PyString)` cast into a `PyObject` is
    /// returned.
    pub fn resolve_source<'p>(
        &self,
        py: Python<'p>,
        decode_source: &'p PyAny,
        io_module: &PyAny,
    ) -> PyResult<Option<&'p PyAny>> {
        let bytes = if let Some(data) = &self.resource.in_memory_source {
            Some(PyBytes::new(py, data))
        } else if let Some(relative_path) = &self.resource.relative_path_module_source {
            let path = self.origin.join(relative_path);

            let source = std::fs::read(&path).map_err(|e| {
                PyErr::from_type(
                    PyImportError::type_object(py),
                    (
                        format!("error reading module source from {}: {}", path.display(), e),
                        self.resource.name.clone().into_py(py),
                    ),
                )
            })?;

            Some(PyBytes::new(py, &source))
        } else {
            None
        };

        if let Some(bytes) = bytes {
            Ok(Some(decode_source.call((io_module, bytes), None)?))
        } else {
            Ok(None)
        }
    }

    /// Attempt to resolve bytecode for this module.
    ///
    /// Will return a `PyErr` if an error occurs resolving the bytecode. If there is
    /// no bytecode, returns `Ok(None)`. Bytecode may still be available for this
    /// module in this scenario, but it isn't known to the resources data structure
    /// (e.g. the case of frozen modules).
    ///
    /// The returned `PyObject` will be an instance of `memoryview`.
    pub fn resolve_bytecode(
        &mut self,
        py: Python,
        optimize_level: BytecodeOptimizationLevel,
        decode_source: &PyAny,
        io_module: &PyModule,
    ) -> PyResult<Option<Py<PyAny>>> {
        if let Some(data) = match optimize_level {
            BytecodeOptimizationLevel::Zero => &self.resource.in_memory_bytecode,
            BytecodeOptimizationLevel::One => &self.resource.in_memory_bytecode_opt1,
            BytecodeOptimizationLevel::Two => &self.resource.in_memory_bytecode_opt2,
        } {
            let ptr = unsafe {
                pyffi::PyMemoryView_FromMemory(
                    data.as_ptr() as _,
                    data.len() as _,
                    pyffi::PyBUF_READ,
                )
            };

            if ptr.is_null() {
                Ok(None)
            } else {
                Ok(Some(unsafe { PyObject::from_owned_ptr(py, ptr) }))
            }
        } else if let Some(path) = self.bytecode_path(optimize_level) {
            // TODO we could potentially avoid the double allocation for bytecode
            // by reading directly into a buffer transferred to Python.
            let bytecode = std::fs::read(&path).map_err(|e| {
                PyErr::from_type(
                    PyImportError::type_object(py),
                    (
                        format!("error reading bytecode from {}: {}", path.display(), e)
                            .into_py(py),
                        self.resource.name.clone().into_py(py),
                    ),
                )
            })?;

            if bytecode.len() < 16 {
                return Err(PyImportError::new_err(
                    "bytecode file does not contain enough data",
                ));
            }

            // First 16 bytes of .pyc files are a header.
            Ok(Some(PyBytes::new(py, &bytecode[16..]).into_py(py)))
        } else if let Some(source) = self.resolve_source(py, decode_source, io_module)? {
            let builtins = py.import("builtins")?;
            let marshal = py.import("marshal")?;

            let code = builtins
                .getattr("compile")?
                .call((source, self.resource.name.as_ref(), "exec"), None)?;
            let bytecode = marshal.getattr("dumps")?.call((code,), None)?;

            Ok(Some(bytecode.into_py(py)))
        } else {
            Ok(None)
        }
    }

    /// Resolve the `importlib.machinery.ModuleSpec` for this module.
    pub fn resolve_module_spec<'p>(
        &self,
        py: Python,
        module_spec_type: &'p PyAny,
        loader: &PyAny,
        optimize_level: BytecodeOptimizationLevel,
    ) -> PyResult<&'p PyAny> {
        let name = PyString::new(py, &self.resource.name);

        let kwargs = PyDict::new(py);
        kwargs.set_item("is_package", self.is_package)?;

        // If we pass `origin=` and set `spec.has_location = True`, `__file__`
        // will be set on the module. This is appropriate for modules backed by
        // the filesystem.

        let origin = self.resolve_origin(py)?;
        if let Some(origin) = &origin {
            kwargs.set_item("origin", origin)?;
        }

        let spec = module_spec_type.call((name, loader), Some(kwargs))?;

        if origin.is_some() {
            spec.setattr("has_location", true)?;
        }

        // If we set `spec.cached`, it gets turned into `__cached__`.
        if let Some(cached) = self.resolve_cached(py, optimize_level)? {
            spec.setattr("cached", cached)?;
        }

        // `__path__` MUST be set on packages per
        // https://docs.python.org/3/reference/import.html#__path__.
        //
        // `__path__` is an iterable of strings, which can be empty.
        //
        // The role of `__path__` is to influence import machinery when dealing
        // with sub-packages.
        //
        // The default code for turning `ModuleSpec` into modules will copy
        // `spec.submodule_search_locations` into `__path__`.
        if self.is_package {
            // If we are filesystem based, use the parent directory of the module
            // file, if available.
            //
            // Otherwise, we construct a filesystem path from the current executable
            // and package name. e.g. `/path/to/myapp/foo/bar`. This path likely
            // doesn't exist. So why expose it? Couldn't this lead to unexpected
            // behavior by consumers who expect `__path__` to point to a valid
            // directory? Perhaps.
            //
            // By setting `__path__` to a meaningful value, we leave the door
            // open for `pkgutil.iter_modules(foo.bar.__path__)`, which
            // `OxidizedFinder.path_hook` supports with these semantics.
            //
            // As a point of reference, the zip importer in the Python standard
            // library sets `__path__` to the path to the zip file with the package
            // names `os.path.join()`d to the end. e.g.
            // `/path/to/myapp.zip/mypackage/subpackage`.
            let mut locations = if let Some(origin_path) = self.origin_path() {
                if let Some(parent_path) = origin_path.parent() {
                    vec![parent_path.into_py(py).into_ref(py)]
                } else {
                    vec![]
                }
            } else {
                vec![]
            };

            if locations.is_empty() {
                let mut path = self.current_exe.to_path_buf();
                path.extend(self.resource.name.split('.'));

                locations.push(path.into_py(py).into_ref(py));
            }

            spec.setattr("submodule_search_locations", locations)?;
        }

        Ok(spec)
    }

    /// Resolve the value of a `ModuleSpec` origin.
    ///
    /// The value gets turned into `__file__`
    pub fn resolve_origin<'p>(&self, py: Python<'p>) -> PyResult<Option<&'p PyAny>> {
        Ok(if let Some(path) = self.origin_path() {
            Some(path.into_py(py).into_ref(py))
        } else {
            None
        })
    }

    /// Resolve the value of a `ModuleSpec` `cached` attribute.
    ///
    /// The value gets turned into `__cached__`.
    fn resolve_cached<'p>(
        &self,
        py: Python<'p>,
        optimize_level: BytecodeOptimizationLevel,
    ) -> PyResult<Option<&'p PyAny>> {
        let path = match self.flavor {
            ModuleFlavor::SourceBytecode => self.bytecode_path(optimize_level),
            _ => None,
        };

        Ok(if let Some(path) = path {
            Some(path.into_py(py).into_ref(py))
        } else {
            None
        })
    }

    /// Obtain the filesystem path to this resource to be used for `ModuleSpec.origin`.
    fn origin_path(&self) -> Option<PathBuf> {
        match self.flavor {
            ModuleFlavor::SourceBytecode => self
                .resource
                .relative_path_module_source
                .as_ref()
                .map(|path| self.origin.join(path)),
            ModuleFlavor::Extension => self
                .resource
                .relative_path_extension_module_shared_library
                .as_ref()
                .map(|path| self.origin.join(path)),
            _ => None,
        }
    }

    /// Obtain the filesystem path to bytecode for this module.
    fn bytecode_path(&self, optimize_level: BytecodeOptimizationLevel) -> Option<PathBuf> {
        let bytecode_path = match optimize_level {
            BytecodeOptimizationLevel::Zero => &self.resource.relative_path_module_bytecode,
            BytecodeOptimizationLevel::One => &self.resource.relative_path_module_bytecode_opt1,
            BytecodeOptimizationLevel::Two => &self.resource.relative_path_module_bytecode_opt2,
        };

        bytecode_path
            .as_ref()
            .map(|bytecode_path| self.origin.join(bytecode_path))
    }

    pub fn in_memory_extension_module_shared_library(&self) -> &'a Option<Cow<'a, [u8]>> {
        &self.resource.in_memory_extension_module_shared_library
    }
}

/// A source for packed resources data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PackedResourcesSource<'a> {
    /// A reference to raw resources data in memory.
    Memory(&'a [u8]),

    /// Load resources data from a filesystem path using memory mapped I/O.
    #[allow(unused)]
    MemoryMappedPath(PathBuf),
}

impl<'a> From<&'a [u8]> for PackedResourcesSource<'a> {
    fn from(data: &'a [u8]) -> Self {
        Self::Memory(data)
    }
}

/// Defines Python resources available for import.
#[derive(Debug)]
pub struct PythonResourcesState<'a, X>
where
    [X]: ToOwned<Owned = Vec<X>>,
{
    /// Path to currently running executable.
    current_exe: PathBuf,

    /// Directory from which relative paths should be evaluated.
    ///
    /// Probably the directory of `current_exe`.
    origin: PathBuf,

    /// Named resources available for loading.
    resources: HashMap<Cow<'a, str>, Resource<'a, X>>,

    /// List of `PyObject` that back indexed data.
    ///
    /// Holding a reference to these prevents them from being gc'd and for
    /// memory referenced by `self.resources` from being freed.
    backing_py_objects: Vec<Py<PyAny>>,

    /// Holds memory mapped file instances that resources data came from.
    backing_mmaps: Vec<memmap2::Mmap>,
}

impl<'a> Default for PythonResourcesState<'a, u8> {
    fn default() -> Self {
        Self {
            current_exe: PathBuf::new(),
            origin: PathBuf::new(),
            resources: HashMap::new(),
            backing_py_objects: vec![],
            backing_mmaps: vec![],
        }
    }
}

impl<'a> PythonResourcesState<'a, u8> {
    /// Construct an instance from environment state.
    pub fn new_from_env() -> Result<Self, &'static str> {
        let exe = std::env::current_exe().map_err(|_| "unable to obtain current executable")?;
        let origin = exe
            .parent()
            .ok_or("unable to get executable parent")?
            .to_path_buf();

        Ok(Self {
            current_exe: exe,
            origin,
            ..Default::default()
        })
    }

    /// Obtain the path of the current executable.
    pub fn current_exe(&self) -> &Path {
        &self.current_exe
    }

    /// Set the path of the current executable.
    pub fn set_current_exe(&mut self, path: PathBuf) {
        self.current_exe = path;
    }

    /// Obtain the source path that relative paths are relative to.
    pub fn origin(&self) -> &Path {
        &self.origin
    }

    /// Set the source path that relative paths are relative to.
    pub fn set_origin(&mut self, path: PathBuf) {
        self.origin = path;
    }

    /// Load resources by parsing a blob.
    ///
    /// If an existing entry exists, the new entry will be merged into it. Set fields
    /// on the incoming entry will overwrite fields on the existing entry.
    ///
    /// If an entry doesn't exist, the resource will be inserted as-is.
    pub fn index_data(&mut self, data: &'a [u8]) -> Result<(), &'static str> {
        let resources = python_packed_resources::load_resources(data)?;

        // Reserve space for expected number of incoming items so we can avoid extra
        // allocations.
        self.resources.reserve(resources.expected_resources_count());

        for resource in resources {
            let resource = resource?;

            match self.resources.entry(resource.name.clone()) {
                Entry::Occupied(existing) => {
                    existing.into_mut().merge_from(resource)?;
                }
                Entry::Vacant(vacant) => {
                    vacant.insert(resource);
                }
            }
        }

        Ok(())
    }

    /// Load resources data from a filesystem path using memory mapped I/O.
    pub fn index_path_memory_mapped(&mut self, path: impl AsRef<Path>) -> Result<(), String> {
        let path = path.as_ref();
        let f = std::fs::File::open(path).map_err(|e| e.to_string())?;

        let mapped = unsafe { memmap2::Mmap::map(&f) }.map_err(|e| e.to_string())?;

        let data = unsafe { std::slice::from_raw_parts::<u8>(mapped.as_ptr(), mapped.len()) };

        self.index_data(data)?;
        self.backing_mmaps.push(mapped);

        Ok(())
    }

    /// Load resources from packed data stored in a PyObject.
    ///
    /// The `PyObject` must conform to the buffer protocol.
    pub fn index_pyobject(&mut self, py: Python, obj: &PyAny) -> PyResult<()> {
        let buffer = PyBuffer::<u8>::get(obj)?;

        let data = unsafe {
            std::slice::from_raw_parts::<u8>(buffer.buf_ptr() as *const _, buffer.len_bytes())
        };

        self.index_data(data).map_err(PyValueError::new_err)?;
        self.backing_py_objects.push(obj.to_object(py));

        Ok(())
    }

    /// Load `builtin` modules from the Python interpreter.
    pub fn index_interpreter_builtin_extension_modules(&mut self) -> Result<(), &'static str> {
        for i in 0.. {
            let record = unsafe { pyffi::PyImport_Inittab.offset(i) };

            if unsafe { *record }.name.is_null() {
                break;
            }

            let name = unsafe { CStr::from_ptr((*record).name as _) };
            let name_str = match name.to_str() {
                Ok(v) => v,
                Err(_) => {
                    return Err("unable to parse PyImport_Inittab");
                }
            };

            self.resources
                .entry(name_str.into())
                .and_modify(|r| {
                    r.is_python_builtin_extension_module = true;
                })
                .or_insert_with(|| Resource {
                    is_python_builtin_extension_module: true,
                    name: Cow::Owned(name_str.to_string()),
                    ..Resource::default()
                });
        }

        Ok(())
    }

    /// Load `frozen` modules from the Python interpreter.
    pub fn index_interpreter_frozen_modules(&mut self) -> Result<(), &'static str> {
        for i in 0.. {
            let record = unsafe { pyffi::PyImport_FrozenModules.offset(i) };

            if unsafe { *record }.name.is_null() {
                break;
            }

            let name = unsafe { CStr::from_ptr((*record).name as _) };
            let name_str = match name.to_str() {
                Ok(v) => v,
                Err(_) => {
                    return Err("unable to parse PyImport_FrozenModules");
                }
            };

            self.resources
                .entry(name_str.into())
                .and_modify(|r| {
                    r.is_python_frozen_module = true;
                })
                .or_insert_with(|| Resource {
                    is_python_frozen_module: true,
                    name: Cow::Owned(name_str.to_string()),
                    ..Resource::default()
                });
        }

        Ok(())
    }

    /// Load resources that are built-in to the Python interpreter.
    ///
    /// If this instance's resources are being used by the sole Python importer,
    /// this needs to be called to ensure modules required during interpreter
    /// initialization are indexed and loadable by our importer.
    pub fn index_interpreter_builtins(&mut self) -> Result<(), &'static str> {
        self.index_interpreter_builtin_extension_modules()?;
        self.index_interpreter_frozen_modules()?;

        Ok(())
    }

    /// Says whether a named resource exists.
    pub fn has_resource(&self, name: &str) -> bool {
        self.resources.contains_key(name)
    }

    /// Add a resource to the instance.
    ///
    /// Memory in the resource must live for at least as long as the lifetime of
    /// the resources this instance was created with.
    pub fn add_resource<'resource: 'a>(
        &mut self,
        resource: Resource<'resource, u8>,
    ) -> Result<(), &'static str> {
        self.resources.insert(resource.name.clone(), resource);

        Ok(())
    }

    /// Attempt to resolve an importable Python module.
    pub fn resolve_importable_module(
        &self,
        name: &str,
        optimize_level: BytecodeOptimizationLevel,
    ) -> Option<ImportablePythonModule<u8>> {
        // Python's filesystem based importer accepts `foo.__init__` as a valid
        // module name. When these names are encountered, it fails to recognize
        // that `__init__` is special and happily searches for and uses/imports a
        // file with `__init__` in it, resulting in a new module object and
        // `sys.modules` entry (as opposed to silently normalizing to and reusing
        // `foo`. See https://github.com/indygreg/PyOxidizer/issues/317
        // and https://bugs.python.org/issue42564 for more.
        //
        // Our strategy is to strip off trailing `.__init__` from the requested
        // module name, effectively aliasing the resource entry for `foo.__init__`
        // to `foo`. The aliasing of the resource name is pretty uncontroversial.
        // However, the name stored inside the resource is the actual indexed name,
        // not the requested name (which may have `.__init__`). If the caller uses
        // the indexed name instead of the requested name, behavior will diverge from
        // Python, as an extra `foo.__init__` module object will not be created
        // and used.
        //
        // At the time this comment was written, find_spec() used the resource's
        // internal name, not the requested name, thus silently treating
        // `foo.__init__` as `foo`. This behavior is incompatible with CPython's path
        // importer. But we think it makes more sense, as `__init__` is a filename
        // encoding and the importer shouldn't even allow it. We only provide support
        // for recognizing `__init__` because Python code in the wild relies on it.
        let name = name.strip_suffix(".__init__").unwrap_or(name);

        let resource = match self.resources.get(name) {
            Some(entry) => entry,
            None => return None,
        };

        // Since resources can exist as multiple types and it is possible
        // that a single resource will express itself as multiple types
        // (e.g. we have both bytecode and an extension module available),
        // we have to break ties and choose an order of preference. Our
        // default mimics the default order of the meta path importers
        // registered on sys.meta_path:
        //
        // 1. built-in extension modules
        // 2. frozen modules
        // 3. path-based
        //
        // Within the path-based importer, the loader order as defined by
        // sys.path_hooks is:
        // 1. zip files
        // 2. extensions
        // 3. source
        // 4. bytecode
        //
        // "source" here really checks for .pyc files and "bytecode" is
        // "sourceless" modules. So our effective order is:
        //
        // 1. built-in extension modules
        // 2. frozen modules
        // 3. extension modules
        // 4. module (covers both source and bytecode)

        if resource.is_python_builtin_extension_module {
            Some(ImportablePythonModule {
                resource,
                current_exe: &self.current_exe,
                origin: &self.origin,
                flavor: ModuleFlavor::Builtin,
                is_package: resource.is_python_package,
            })
        } else if resource.is_python_frozen_module {
            Some(ImportablePythonModule {
                resource,
                current_exe: &self.current_exe,
                origin: &self.origin,
                flavor: ModuleFlavor::Frozen,
                is_package: resource.is_python_package,
            })
        } else if resource.is_python_extension_module {
            Some(ImportablePythonModule {
                resource,
                current_exe: &self.current_exe,
                origin: &self.origin,
                flavor: ModuleFlavor::Extension,
                is_package: resource.is_python_package,
            })
        } else if resource.is_python_module {
            if is_module_importable(resource, optimize_level) {
                Some(ImportablePythonModule {
                    resource,
                    current_exe: &self.current_exe,
                    origin: &self.origin,
                    flavor: ModuleFlavor::SourceBytecode,
                    is_package: resource.is_python_package,
                })
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Obtain a single named resource in a package.
    ///
    /// Err occurs if loading the resource data fails. `Ok(None)` is returned
    /// if the resource does not exist. Otherwise the returned `PyObject`
    /// is a file-like object to read the resource data.
    pub fn get_package_resource_file<'p>(
        &self,
        py: Python<'p>,
        package: &str,
        resource_name: &str,
    ) -> PyResult<Option<&'p PyAny>> {
        let entry = match self.resources.get(package) {
            Some(entry) => entry,
            None => return Ok(None),
        };

        if let Some(resources) = &entry.in_memory_package_resources {
            if let Some(data) = resources.get(resource_name) {
                let io_module = py.import("io")?;
                let bytes_io = io_module.getattr("BytesIO")?;

                let data = PyBytes::new(py, data);
                return Ok(Some(bytes_io.call((data,), None)?));
            }
        }

        if let Some(resources) = &entry.relative_path_package_resources {
            if let Some(path) = resources.get(resource_name) {
                let path = self.origin.join(path);
                let io_module = py.import("io")?;

                return Ok(Some(
                    io_module
                        .getattr("FileIO")?
                        .call((path.into_py(py), "r"), None)?,
                ));
            }
        }

        Ok(None)
    }

    /// Determines whether a specific package + name pair is a known Python package resource.
    pub fn is_package_resource(&self, package: &str, resource_name: &str) -> bool {
        if let Some(entry) = self.resources.get(package) {
            if let Some(resources) = &entry.in_memory_package_resources {
                if resources.contains_key(resource_name) {
                    return true;
                }
            }

            if let Some(resources) = &entry.relative_path_package_resources {
                if resources.contains_key(resource_name) {
                    return true;
                }
            }
        }

        false
    }

    /// Obtain the resources available in a Python package, as a Python list.
    ///
    /// The names are returned in sorted order.
    pub fn package_resource_names<'p>(&self, py: Python<'p>, package: &str) -> PyResult<&'p PyAny> {
        let entry = match self.resources.get(package) {
            Some(entry) => entry,
            None => return Ok(PyList::empty(py).into()),
        };

        let mut names = if let Some(resources) = &entry.in_memory_package_resources {
            resources.keys().collect()
        } else if let Some(resources) = &entry.relative_path_package_resources {
            resources.keys().collect()
        } else {
            vec![]
        };

        names.sort();

        let names = names
            .iter()
            .map(|x| x.to_object(py))
            .collect::<Vec<Py<PyAny>>>();

        Ok(PyList::new(py, &names).into())
    }

    /// Whether the given resource name is a directory with resources.
    pub fn is_package_resource_directory(&self, package: &str, name: &str) -> bool {
        // Normalize to UNIX style paths.
        let name = name.replace('\\', "/");

        let prefix = if name.ends_with('/') {
            name
        } else {
            format!("{}/", name)
        };

        if let Some(entry) = self.resources.get(package) {
            if let Some(resources) = &entry.in_memory_package_resources {
                if resources.keys().any(|path| path.starts_with(&prefix)) {
                    return true;
                }
            }

            if let Some(resources) = &entry.relative_path_package_resources {
                if resources.keys().any(|path| path.starts_with(&prefix)) {
                    return true;
                }
            }

            false
        } else {
            false
        }
    }

    /// Resolve package resources in a directory.
    pub fn package_resources_list_directory(&self, package: &str, name: &str) -> Vec<String> {
        let name = name.replace('\\', "/");

        let prefix = if name.ends_with('/') {
            Some(name)
        } else if name.is_empty() {
            None
        } else {
            Some(format!("{}/", name))
        };

        let filter_map_resource = |path: &'_ Cow<'_, str>| -> Option<String> {
            match &prefix {
                Some(prefix) => {
                    if let Some(name) = path.strip_prefix(prefix) {
                        if name.contains('/') {
                            None
                        } else {
                            Some(name.to_string())
                        }
                    } else {
                        None
                    }
                }
                None => {
                    // Empty string input matches root directory.
                    if path.contains('/') {
                        None
                    } else {
                        Some(path.to_string())
                    }
                }
            }
        };

        let mut entries = BTreeSet::new();

        if let Some(entry) = self.resources.get(package) {
            if let Some(resources) = &entry.in_memory_package_resources {
                entries.extend(resources.keys().filter_map(filter_map_resource));
            }

            if let Some(resources) = &entry.relative_path_package_resources {
                entries.extend(resources.keys().filter_map(filter_map_resource));
            }
        }

        entries.into_iter().collect::<Vec<_>>()
    }

    /// Attempt to resolve a PyBytes for resource data given a relative path.
    ///
    /// Raises OSerror on failure.
    ///
    /// This method is meant to be an implementation of `ResourceLoader.get_data()` and
    /// should only be used for that purpose.
    pub fn resolve_resource_data_from_path<'p>(
        &self,
        py: Python<'p>,
        path: &str,
    ) -> PyResult<&'p PyAny> {
        // Paths prefixed with the current executable path are recognized as
        // in-memory resources. This emulates behavior of zipimporter, which
        // does something similar.
        //
        // Paths prefixed with the current resources origin are recognized as
        // path-relative resources. We need to service these paths because we
        // hand out a __path__ that points to the package directory and someone
        // could os.path.join() that with a resource name and call into get_data()
        // with that full path.
        //
        // All other paths are ignored.
        //
        // Why do we ignore all other paths? Couldn't we try to read them?
        // This is a very good question!
        //
        // We absolutely could try to load all other paths! However, doing so
        // would introduce inconsistent behavior.
        //
        // Python's filesystem importer relies on directory scanning to find
        // resources: resources are not registered ahead of time. This is all fine.
        // Our resources, however, are registered. The resources data structure
        // has awareness of all resources that should exist. In the case of memory
        // resources, it MUST have awareness of the resource, as there is no other
        // location to fall back to to find them.
        //
        // If we were to service arbitrary paths that happened to be files but
        // weren't resources registered with our data structure, our behavior would
        // be inconsistent. For in-memory resources, we'd require resources be
        // registered. For filesystem resources, we wouldn't. This inconsistency
        // feels wrong.
        //
        // Now, that inconsistency may be desirable by some users. So we may add
        // this functionality some day. But it should likely never be the default
        // because it goes against the spirit of requiring all resources to be
        // known ahead-of-time.
        let path = path.to_owned();
        let native_path = PathBuf::from(&path);

        let (relative_path, check_in_memory, check_relative_path) =
            if let Ok(relative_path) = native_path.strip_prefix(&self.current_exe) {
                (relative_path, true, false)
            } else if let Ok(relative_path) = native_path.strip_prefix(&self.origin) {
                (relative_path, false, true)
            } else {
                return Err(PyErr::from_type(
                    PyOSError::type_object(py),
                    (ENOENT, "resource not known", path),
                ));
            };

        // There is also an additional wrinkle with resolving resources from paths.
        // And that is the boundary between the package name and the resource name.
        // The relative path to the resource logically consists of a package name
        // part and a resource name part and the division between them is unknown.
        // Since resource names can have directory separators, a relative path of
        // `foo/bar/resource.txt` could either be `(foo, bar/resource.txt)` or
        // `(foo.bar, resource.txt)`. Our strategy then is to walk the path
        // components and pop them from the package name to the resource name until
        // we find a match.
        //
        // We stop as soon as we find a known Python package because this is the
        // behavior of ResourceReader. If we ever teach one to cross package
        // boundaries, we should extend this to the other.
        let components = relative_path.components().collect::<Vec<_>>();

        // Our indexed resources require the existence of a package. So there should be
        // at least 2 components for the path to be valid.
        if components.len() < 2 {
            return Err(PyErr::from_type(
                PyOSError::type_object(py),
                (
                    ENOENT,
                    "illegal resource name: missing package component",
                    path,
                ),
            ));
        }

        let mut name_parts = vec![components[components.len() - 1]
            .as_os_str()
            .to_string_lossy()];
        let mut package_parts = components[0..components.len() - 1]
            .iter()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>();

        while !package_parts.is_empty() {
            let package_name = package_parts.join(".");
            let package_name_ref: &str = &package_name;

            // Internally resources are normalized to POSIX separators.
            let resource_name = name_parts.join("/");
            let resource_name_ref: &str = &resource_name;

            if let Some(entry) = self.resources.get(package_name_ref) {
                if check_in_memory {
                    if let Some(resources) = &entry.in_memory_package_resources {
                        if let Some(data) = resources.get(resource_name_ref) {
                            return Ok(PyBytes::new(py, data).into());
                        }
                    }
                }

                if check_relative_path {
                    if let Some(resources) = &entry.relative_path_package_resources {
                        if let Some(resource_relative_path) = resources.get(resource_name_ref) {
                            let resource_path = self.origin.join(resource_relative_path);

                            let io_module = py.import("io")?;

                            let fh = io_module
                                .getattr("FileIO")?
                                .call((resource_path.into_py(py).into_ref(py), "r"), None)?;

                            return fh.call_method0("read");
                        }
                    }
                }

                // We found a package above. Stop the walk, as we don't want to allow crossing
                // package boundaries.
                break;
            }

            name_parts.insert(0, package_parts.pop().unwrap());
        }

        // If we got here, we couldn't find a resource in our data structure.

        Err(PyErr::from_type(
            PyOSError::type_object(py),
            (ENOENT, "resource not known", path),
        ))
    }

    /// Obtain a PyList of pkgutil.ModuleInfo for known resources.
    ///
    /// This is intended to be used as the implementation for Finder.iter_modules().
    ///
    /// `package_filter` defines the target package to return results for. The
    /// empty string denotes top-level packages only.
    pub fn pkgutil_modules_infos<'p>(
        &self,
        py: Python<'p>,
        package_filter: Option<&str>,
        prefix: Option<String>,
        optimize_level: BytecodeOptimizationLevel,
    ) -> PyResult<&'p PyList> {
        let infos: PyResult<Vec<_>> = self
            .resources
            .values()
            .filter(|r| {
                r.is_python_extension_module
                    || (r.is_python_module && is_module_importable(r, optimize_level))
            })
            .filter(|r| name_at_package_hierarchy(&r.name, package_filter))
            .map(|r| {
                // We always take the leaf-most name.
                let name = r.name.rsplit('.').next().unwrap();

                let name = if let Some(prefix) = &prefix {
                    format!("{}{}", prefix, name)
                } else {
                    name.to_string()
                };

                let name = name.to_object(py);
                let is_package = r.is_python_package.to_object(py);

                Ok(PyTuple::new(py, &[name, is_package]))
            })
            .collect();

        let infos = infos?;

        Ok(PyList::new(py, &infos))
    }

    /// Resolve the names of package distributions matching a name filter.
    pub fn package_distribution_names(&self, filter: impl Fn(&str) -> bool) -> Vec<&'_ str> {
        self.resources
            .values()
            .filter(|r| {
                r.is_python_package
                    && (r.in_memory_distribution_resources.is_some()
                        || r.relative_path_distribution_resources.is_some())
            })
            .filter(|r| filter(r.name.as_ref()))
            .map(|r| r.name.as_ref())
            .collect::<Vec<_>>()
    }

    /// Resolve data belonging to a package distribution resource.
    pub fn resolve_package_distribution_resource(
        &self,
        package: &str,
        name: &str,
    ) -> Result<Option<Cow<'_, [u8]>>> {
        if let Some(entry) = self.resources.get(package) {
            if let Some(resources) = &entry.in_memory_distribution_resources {
                if let Some(data) = resources.get(name) {
                    return Ok(Some(Cow::Borrowed(data.as_ref())));
                }
            }

            if let Some(resources) = &entry.relative_path_distribution_resources {
                if let Some(path) = resources.get(name) {
                    let path = &self.origin.join(path);
                    let data = std::fs::read(path)?;

                    return Ok(Some(Cow::Owned(data)));
                }
            }

            Ok(None)
        } else {
            Ok(None)
        }
    }

    /// Whether a package distribution resource name is a directory.
    pub fn package_distribution_resource_name_is_directory(
        &self,
        package: &str,
        name: &str,
    ) -> bool {
        let name = name.replace('\\', "/");

        let prefix = if name.ends_with('/') {
            name
        } else {
            format!("{}/", name)
        };

        if let Some(entry) = &self.resources.get(package) {
            if let Some(resources) = &entry.in_memory_distribution_resources {
                if resources.keys().any(|path| path.starts_with(&prefix)) {
                    return true;
                }
            }

            if let Some(resources) = &entry.relative_path_distribution_resources {
                if resources.keys().any(|path| path.starts_with(&prefix)) {
                    return true;
                }
            }

            false
        } else {
            false
        }
    }

    /// Obtain contents in a package distribution resources "directory."
    pub fn package_distribution_resources_list_directory<'slf>(
        &'slf self,
        package: &str,
        name: &str,
    ) -> Vec<&'slf str> {
        let name = name.replace('\\', "/");

        let prefix = if name.ends_with('/') {
            Some(name)
        } else if name.is_empty() {
            None
        } else {
            Some(format!("{}/", name))
        };

        let filter_map_resource = |path: &'slf Cow<'slf, str>| -> Option<&'slf str> {
            match &prefix {
                Some(prefix) => {
                    path.strip_prefix(prefix).filter(|&name| !name.contains('/'))
                }
                None => {
                    // Empty string input matches root directory.
                    if path.contains('/') {
                        None
                    } else {
                        Some(path)
                    }
                }
            }
        };

        let mut entries = BTreeSet::new();

        if let Some(entry) = self.resources.get(package) {
            if let Some(resources) = &entry.in_memory_distribution_resources {
                entries.extend(resources.keys().filter_map(filter_map_resource));
            }

            if let Some(resources) = &entry.relative_path_distribution_resources {
                entries.extend(resources.keys().filter_map(filter_map_resource));
            }
        }

        entries.into_iter().collect::<Vec<_>>()
    }

    /// Resolve content of a shared library to load from memory.
    pub fn resolve_in_memory_shared_library_data(&self, name: &str) -> Option<&[u8]> {
        if let Some(entry) = &self.resources.get(name) {
            if let Some(library_data) = &entry.in_memory_shared_library {
                Some(library_data.as_ref())
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Convert indexed resources to a [PyList].
    pub fn resources_as_py_list<'p>(&self, py: Python<'p>) -> PyResult<&'p PyList> {
        let mut resources = self.resources.values().collect::<Vec<_>>();
        resources.sort_by_key(|r| &r.name);

        let objects = resources
            .iter()
            .map(|r| resource_to_pyobject(py, r))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(PyList::new(py, objects))
    }

    /// Serialize resources contained in this data structure.
    ///
    /// `ignore_built` and `ignore_frozen` specify whether to ignore built-in
    /// extension modules and frozen modules, respectively.
    pub fn serialize_resources(
        &self,
        ignore_builtin: bool,
        ignore_frozen: bool,
    ) -> Result<Vec<u8>> {
        let mut resources = self
            .resources
            .values()
            .filter(|resource| {
                // This assumes builtins and frozen are mutually exclusive with other types.
                !((resource.is_python_builtin_extension_module && ignore_builtin)
                    || (resource.is_python_frozen_module && ignore_frozen))
            })
            .collect::<Vec<&Resource<u8>>>();

        // Sort so behavior is deterministic.
        resources.sort_by_key(|v| &v.name);

        let mut buffer = Vec::new();

        python_packed_resources::write_packed_resources_v3(&resources, &mut buffer, None)?;

        Ok(buffer)
    }
}

#[pyclass(module = "oxidized_importer")]
pub(crate) struct OxidizedResource {
    resource: RefCell<Resource<'static, u8>>,
}

#[pymethods]
impl OxidizedResource {
    fn __repr__(&self) -> String {
        format!(
            "<OxidizedResource name=\"{}\">",
            self.resource.borrow().name
        )
    }

    #[new]
    fn new() -> PyResult<Self> {
        Ok(Self {
            resource: RefCell::new(Resource::<u8>::default()),
        })
    }

    #[getter]
    fn get_is_module(&self) -> bool {
        self.resource.borrow().is_python_module
    }

    #[setter]
    fn set_is_module(&self, value: bool) -> PyResult<()> {
        self.resource.borrow_mut().is_python_module = value;

        Ok(())
    }

    #[getter]
    fn get_is_builtin_extension_module(&self) -> bool {
        self.resource.borrow().is_python_builtin_extension_module
    }

    #[setter]
    fn set_is_builtin_extension_module(&self, value: bool) -> PyResult<()> {
        self.resource
            .borrow_mut()
            .is_python_builtin_extension_module = value;

        Ok(())
    }

    #[getter]
    fn get_is_frozen_module(&self) -> bool {
        self.resource.borrow().is_python_frozen_module
    }

    #[setter]
    fn set_is_frozen_module(&self, value: bool) -> PyResult<()> {
        self.resource.borrow_mut().is_python_frozen_module = value;

        Ok(())
    }

    #[getter]
    fn get_is_extension_module(&self) -> bool {
        self.resource.borrow().is_python_extension_module
    }

    #[setter]
    fn set_is_extension_module(&self, value: bool) -> PyResult<()> {
        self.resource.borrow_mut().is_python_extension_module = value;

        Ok(())
    }

    #[getter]
    fn get_is_shared_library(&self) -> bool {
        self.resource.borrow().is_shared_library
    }

    #[setter]
    fn set_is_shared_library(&self, value: bool) -> PyResult<()> {
        self.resource.borrow_mut().is_shared_library = value;

        Ok(())
    }

    #[getter]
    fn get_name(&self) -> String {
        self.resource.borrow().name.to_string()
    }

    #[setter]
    fn set_name(&self, value: &str) -> PyResult<()> {
        self.resource.borrow_mut().name = Cow::Owned(value.to_owned());

        Ok(())
    }

    #[getter]
    fn get_is_package(&self) -> bool {
        self.resource.borrow().is_python_package
    }

    #[setter]
    fn set_is_package(&self, value: bool) -> PyResult<()> {
        self.resource.borrow_mut().is_python_package = value;

        Ok(())
    }

    #[getter]
    fn get_is_namespace_package(&self) -> bool {
        self.resource.borrow().is_python_namespace_package
    }

    #[setter]
    fn set_is_namespace_package(&self, value: bool) -> PyResult<()> {
        self.resource.borrow_mut().is_python_namespace_package = value;

        Ok(())
    }

    #[getter]
    fn get_in_memory_source<'p>(&self, py: Python<'p>) -> Option<&'p PyBytes> {
        self.resource
            .borrow()
            .in_memory_source
            .as_ref()
            .map(|x| PyBytes::new(py, x))
    }

    #[setter]
    fn set_in_memory_source(&self, value: &PyAny) -> PyResult<()> {
        self.resource.borrow_mut().in_memory_source =
            pyobject_to_owned_bytes_optional(value)?.map(Cow::Owned);

        Ok(())
    }

    #[getter]
    fn get_in_memory_bytecode<'p>(&self, py: Python<'p>) -> Option<&'p PyBytes> {
        self.resource
            .borrow()
            .in_memory_bytecode
            .as_ref()
            .map(|x| PyBytes::new(py, x))
    }

    #[setter]
    fn set_in_memory_bytecode(&self, value: &PyAny) -> PyResult<()> {
        self.resource.borrow_mut().in_memory_bytecode =
            pyobject_to_owned_bytes_optional(value)?.map(Cow::Owned);

        Ok(())
    }

    #[getter]
    fn get_in_memory_bytecode_opt1<'p>(&self, py: Python<'p>) -> Option<&'p PyBytes> {
        self.resource
            .borrow()
            .in_memory_bytecode_opt1
            .as_ref()
            .map(|x| PyBytes::new(py, x))
    }

    #[setter]
    fn set_in_memory_bytecode_opt1(&self, value: &PyAny) -> PyResult<()> {
        self.resource.borrow_mut().in_memory_bytecode_opt1 =
            pyobject_to_owned_bytes_optional(value)?.map(Cow::Owned);

        Ok(())
    }

    #[getter]
    fn get_in_memory_bytecode_opt2<'p>(&self, py: Python<'p>) -> Option<&'p PyBytes> {
        self.resource
            .borrow()
            .in_memory_bytecode_opt2
            .as_ref()
            .map(|x| PyBytes::new(py, x))
    }

    #[setter]
    fn set_in_memory_bytecode_opt2(&self, value: &PyAny) -> PyResult<()> {
        self.resource.borrow_mut().in_memory_bytecode_opt2 =
            pyobject_to_owned_bytes_optional(value)?.map(Cow::Owned);

        Ok(())
    }

    #[getter]
    fn get_in_memory_extension_module_shared_library<'p>(
        &self,
        py: Python<'p>,
    ) -> Option<&'p PyBytes> {
        self.resource
            .borrow()
            .in_memory_extension_module_shared_library
            .as_ref()
            .map(|x| PyBytes::new(py, x))
    }

    #[setter]
    fn set_in_memory_extension_module_shared_library(&self, value: &PyAny) -> PyResult<()> {
        self.resource
            .borrow_mut()
            .in_memory_extension_module_shared_library =
            pyobject_to_owned_bytes_optional(value)?.map(Cow::Owned);

        Ok(())
    }

    #[getter]
    fn get_in_memory_package_resources<'p>(
        &self,
        py: Python<'p>,
    ) -> Option<HashMap<String, &'p PyBytes>> {
        self.resource
            .borrow()
            .in_memory_package_resources
            .as_ref()
            .map(|x| {
                x.iter()
                    .map(|(k, v)| (k.to_string(), PyBytes::new(py, v)))
                    .collect()
            })
    }

    #[setter]
    fn set_in_memory_package_resources(&self, value: &PyAny) -> PyResult<()> {
        self.resource.borrow_mut().in_memory_package_resources =
            pyobject_optional_resources_map_to_owned_bytes(value)?.map(|x| {
                x.into_iter()
                    .map(|(k, v)| (Cow::Owned(k), Cow::Owned(v)))
                    .collect()
            });

        Ok(())
    }

    #[getter]
    fn get_in_memory_distribution_resources<'p>(
        &self,
        py: Python<'p>,
    ) -> Option<HashMap<String, &'p PyBytes>> {
        self.resource
            .borrow()
            .in_memory_distribution_resources
            .as_ref()
            .map(|x| {
                x.iter()
                    .map(|(k, v)| (k.to_string(), PyBytes::new(py, v)))
                    .collect()
            })
    }

    #[setter]
    fn set_in_memory_distribution_resources(&self, value: &PyAny) -> PyResult<()> {
        self.resource.borrow_mut().in_memory_distribution_resources =
            pyobject_optional_resources_map_to_owned_bytes(value)?.map(|x| {
                x.into_iter()
                    .map(|(k, v)| (Cow::Owned(k), Cow::Owned(v)))
                    .collect()
            });

        Ok(())
    }

    #[getter]
    fn get_in_memory_shared_library<'p>(&self, py: Python<'p>) -> Option<&'p PyBytes> {
        self.resource
            .borrow()
            .in_memory_shared_library
            .as_ref()
            .map(|x| PyBytes::new(py, x))
    }

    #[setter]
    fn set_in_memory_shared_library(&self, value: &PyAny) -> PyResult<()> {
        self.resource.borrow_mut().in_memory_shared_library =
            pyobject_to_owned_bytes_optional(value)?.map(Cow::Owned);

        Ok(())
    }

    #[getter]
    fn get_shared_library_dependency_names(&self) -> Option<Vec<String>> {
        self.resource
            .borrow()
            .shared_library_dependency_names
            .as_ref()
            .map(|x| x.iter().map(|v| v.to_string()).collect())
    }

    #[setter]
    fn set_shared_library_dependency_names(&self, value: Option<Vec<String>>) -> PyResult<()> {
        self.resource.borrow_mut().shared_library_dependency_names =
            value.map(|x| x.into_iter().map(Cow::Owned).collect());

        Ok(())
    }

    #[getter]
    fn get_relative_path_module_source<'p>(&self, py: Python<'p>) -> PyResult<&'p PyAny> {
        self.resource
            .borrow()
            .relative_path_module_source
            .as_ref()
            .map_or_else(
                || Ok(py.None().into_ref(py)),
                |x| path_to_pathlib_path(py, x),
            )
    }

    #[setter]
    fn set_relative_path_module_source(&self, py: Python, value: &PyAny) -> PyResult<()> {
        self.resource.borrow_mut().relative_path_module_source =
            pyobject_to_pathbuf_optional(py, value)?.map(Cow::Owned);

        Ok(())
    }

    #[getter]
    fn get_relative_path_module_bytecode<'p>(&self, py: Python<'p>) -> PyResult<&'p PyAny> {
        self.resource
            .borrow()
            .relative_path_module_bytecode
            .as_ref()
            .map_or_else(
                || Ok(py.None().into_ref(py)),
                |x| path_to_pathlib_path(py, x),
            )
    }

    #[setter]
    fn set_relative_path_module_bytecode(&self, py: Python, value: &PyAny) -> PyResult<()> {
        self.resource.borrow_mut().relative_path_module_bytecode =
            pyobject_to_pathbuf_optional(py, value)?.map(Cow::Owned);

        Ok(())
    }

    #[getter]
    fn get_relative_path_module_bytecode_opt1<'p>(&self, py: Python<'p>) -> PyResult<&'p PyAny> {
        self.resource
            .borrow()
            .relative_path_module_bytecode_opt1
            .as_ref()
            .map_or_else(
                || Ok(py.None().into_ref(py)),
                |x| path_to_pathlib_path(py, x),
            )
    }

    #[setter]
    fn set_relative_path_module_bytecode_opt1(&self, py: Python, value: &PyAny) -> PyResult<()> {
        self.resource
            .borrow_mut()
            .relative_path_module_bytecode_opt1 =
            pyobject_to_pathbuf_optional(py, value)?.map(Cow::Owned);

        Ok(())
    }

    #[getter]
    fn get_relative_path_module_bytecode_opt2<'p>(&self, py: Python<'p>) -> PyResult<&'p PyAny> {
        self.resource
            .borrow()
            .relative_path_module_bytecode_opt2
            .as_ref()
            .map_or_else(
                || Ok(py.None().into_ref(py)),
                |x| path_to_pathlib_path(py, x),
            )
    }

    #[setter]
    fn set_relative_path_module_bytecode_opt2(&self, py: Python, value: &PyAny) -> PyResult<()> {
        self.resource
            .borrow_mut()
            .relative_path_module_bytecode_opt2 =
            pyobject_to_pathbuf_optional(py, value)?.map(Cow::Owned);

        Ok(())
    }

    #[getter]
    fn get_relative_path_extension_module_shared_library<'p>(
        &self,
        py: Python<'p>,
    ) -> PyResult<&'p PyAny> {
        self.resource
            .borrow()
            .relative_path_extension_module_shared_library
            .as_ref()
            .map_or_else(
                || Ok(py.None().into_ref(py)),
                |x| path_to_pathlib_path(py, x),
            )
    }

    #[setter]
    fn set_relative_path_extension_module_shared_library(
        &self,
        py: Python,
        value: &PyAny,
    ) -> PyResult<()> {
        self.resource
            .borrow_mut()
            .relative_path_extension_module_shared_library =
            pyobject_to_pathbuf_optional(py, value)?.map(Cow::Owned);

        Ok(())
    }

    #[getter]
    fn get_relative_path_package_resources<'p>(&self, py: Python<'p>) -> PyResult<&'p PyAny> {
        self.resource
            .borrow()
            .relative_path_package_resources
            .as_ref()
            .map_or_else(
                || Ok(py.None().into_ref(py)),
                |x| -> PyResult<&PyAny> {
                    let res = PyDict::new(py);

                    for (k, v) in x.iter() {
                        res.set_item(k, path_to_pathlib_path(py, v)?)?;
                    }

                    Ok(res)
                },
            )
    }

    #[setter]
    fn set_relative_path_package_resources(&self, py: Python, value: &PyAny) -> PyResult<()> {
        self.resource.borrow_mut().relative_path_package_resources =
            pyobject_optional_resources_map_to_pathbuf(py, value)?.map(|x| {
                x.into_iter()
                    .map(|(k, v)| (Cow::Owned(k), Cow::Owned(v)))
                    .collect()
            });

        Ok(())
    }

    #[getter]
    fn get_relative_path_distribution_resources<'p>(&self, py: Python<'p>) -> PyResult<&'p PyAny> {
        self.resource
            .borrow()
            .relative_path_distribution_resources
            .as_ref()
            .map_or_else(
                || Ok(py.None().into_ref(py)),
                |x| -> PyResult<&PyAny> {
                    let res = PyDict::new(py);

                    for (k, v) in x.iter() {
                        res.set_item(k, path_to_pathlib_path(py, v)?)?;
                    }

                    Ok(res.into())
                },
            )
    }

    #[setter]
    fn set_relative_path_distribution_resources(&self, py: Python, value: &PyAny) -> PyResult<()> {
        self.resource
            .borrow_mut()
            .relative_path_distribution_resources =
            pyobject_optional_resources_map_to_pathbuf(py, value)?.map(|x| {
                x.into_iter()
                    .map(|(k, v)| (Cow::Owned(k), Cow::Owned(v)))
                    .collect()
            });

        Ok(())
    }
}

/// Convert a Resource to an OxidizedResource.
pub(crate) fn resource_to_pyobject<'p>(
    py: Python<'p>,
    resource: &Resource<u8>,
) -> PyResult<&'p PyCell<OxidizedResource>> {
    PyCell::new(
        py,
        OxidizedResource {
            resource: RefCell::new(resource.to_owned()),
        },
    )
}

#[inline]
pub(crate) fn pyobject_to_resource(resource: &OxidizedResource) -> Resource<'static, u8> {
    resource.resource.borrow().clone()
}
