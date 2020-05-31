// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Management of Python resources.
*/

use {
    super::conversion::{
        path_to_pathlib_path, path_to_pyobject, pyobject_optional_resources_map_to_owned_bytes,
        pyobject_optional_resources_map_to_pathbuf, pyobject_to_owned_bytes_optional,
        pyobject_to_pathbuf_optional,
    },
    anyhow::Result,
    cpython::exc::{ImportError, OSError, TypeError, ValueError},
    cpython::{
        py_class, py_class_call_slot_impl_with_ref, py_class_prop_getter, py_class_prop_setter,
        NoArgs, ObjectProtocol, PyBytes, PyDict, PyErr, PyList, PyModule, PyObject, PyResult,
        PyString, PyTuple, Python, PythonObject, ToPyObject,
    },
    python3_sys as pyffi,
    python_packed_resources::data::{Resource, ResourceFlavor},
    std::borrow::Cow,
    std::cell::RefCell,
    std::collections::HashMap,
    std::ffi::CStr,
    std::iter::FromIterator,
    std::path::{Path, PathBuf},
};

/// Python bytecode optimization level.
#[derive(Clone, Copy, Debug)]
pub(crate) enum OptimizeLevel {
    Zero,
    One,
    Two,
}

/// Determines whether an entry represents an importable Python module.
///
/// Should only be called on module flavors.
fn is_module_importable<X>(entry: &Resource<X>, optimize_level: OptimizeLevel) -> bool
where
    [X]: ToOwned<Owned = Vec<X>>,
{
    assert_eq!(entry.flavor, ResourceFlavor::Module);

    entry.in_memory_source.is_some()
        || entry.relative_path_module_source.is_some()
        || match optimize_level {
            OptimizeLevel::Zero => {
                entry.in_memory_bytecode.is_some() || entry.relative_path_module_bytecode.is_some()
            }
            OptimizeLevel::One => {
                entry.in_memory_bytecode_opt1.is_some() || entry.in_memory_bytecode_opt1.is_some()
            }
            OptimizeLevel::Two => {
                entry.in_memory_bytecode_opt2.is_some() || entry.in_memory_bytecode_opt2.is_some()
            }
        }
}

/// Holds state for an importable Python module.
///
/// This essentially is an abstraction over raw `Resource` entries that
/// allows the importer code to be simpler.
pub(crate) struct ImportablePythonModule<'a, X: 'a>
where
    [X]: ToOwned<Owned = Vec<X>>,
{
    /// The raw resource backing this importable module.
    resource: &'a Resource<'a, X>,

    /// Path to current executable.
    current_exe: &'a Path,

    /// Path from which relative paths should be interpreted.
    origin: &'a Path,

    /// The resource/module flavor.
    pub flavor: &'a ResourceFlavor,
    /// Whether this module is a package.
    pub is_package: bool,
}

impl<'a> ImportablePythonModule<'a, u8> {
    /// Attempt to resolve a Python `bytes` for the source code behind this module.
    ///
    /// Will return a PyErr if an error occurs resolving source. If there is no source,
    /// returns `Ok(None)`. Otherwise an `Ok(PyString)` cast into a `PyObject` is
    /// returned.
    pub fn resolve_source(
        &self,
        py: Python,
        decode_source: &PyObject,
        io_module: &PyModule,
    ) -> PyResult<Option<PyObject>> {
        let bytes = if let Some(data) = &self.resource.in_memory_source {
            Some(PyBytes::new(py, data))
        } else if let Some(relative_path) = &self.resource.relative_path_module_source {
            let path = self.origin.join(relative_path);

            let source = std::fs::read(&path).or_else(|e| {
                Err(PyErr::new::<ImportError, _>(
                    py,
                    (
                        format!("error reading module source from {}: {}", path.display(), e),
                        self.resource.name.clone(),
                    ),
                ))
            })?;

            Some(PyBytes::new(py, &source))
        } else {
            None
        };

        if let Some(bytes) = bytes {
            Ok(Some(decode_source.call(py, (io_module, bytes), None)?))
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
        optimize_level: OptimizeLevel,
        decode_source: &PyObject,
        io_module: &PyModule,
    ) -> PyResult<Option<PyObject>> {
        if let Some(data) = match optimize_level {
            OptimizeLevel::Zero => &self.resource.in_memory_bytecode,
            OptimizeLevel::One => &self.resource.in_memory_bytecode_opt1,
            OptimizeLevel::Two => &self.resource.in_memory_bytecode_opt2,
        } {
            let ptr = unsafe {
                pyffi::PyMemoryView_FromMemory(
                    data.as_ptr() as _,
                    data.len() as _,
                    pyffi::PyBUF_READ,
                )
            };

            Ok(unsafe { PyObject::from_owned_ptr_opt(py, ptr) })
        } else if let Some(path) = self.bytecode_path(optimize_level) {
            // TODO we could potentially avoid the double allocation for bytecode
            // by reading directly into a buffer transferred to Python.
            let bytecode = std::fs::read(&path).or_else(|e| {
                Err(PyErr::new::<ImportError, _>(
                    py,
                    (
                        format!("error reading bytecode from {}: {}", path.display(), e),
                        self.resource.name.clone(),
                    ),
                ))
            })?;

            if bytecode.len() < 16 {
                return Err(PyErr::new::<ImportError, _>(
                    py,
                    "bytecode file does not contain enough data",
                ));
            }

            // First 16 bytes of .pyc files are a header.
            Ok(Some(PyBytes::new(py, &bytecode[16..]).into_object()))
        } else if let Some(source) = self.resolve_source(py, decode_source, io_module)? {
            let builtins = py.import("builtins")?;
            let marshal = py.import("marshal")?;

            let code = builtins.call(py, "compile", (source, &self.resource.name, "exec"), None)?;
            let bytecode = marshal.call(py, "dumps", (code,), None)?;

            Ok(Some(bytecode))
        } else {
            Ok(None)
        }
    }

    /// Resolve the `importlib.machinery.ModuleSpec` for this module.
    pub fn resolve_module_spec(
        &self,
        py: Python,
        module_spec_type: &PyObject,
        loader: &PyObject,
        optimize_level: OptimizeLevel,
    ) -> PyResult<PyObject> {
        let name = PyString::new(py, &self.resource.name);

        let kwargs = PyDict::new(py);
        kwargs.set_item(py, "is_package", self.is_package)?;

        // If we pass `origin=` and set `spec.has_location = True`, `__file__`
        // will be set on the module. This is appropriate for modules backed by
        // the filesystem.

        let origin = self.resolve_origin(py)?;
        if let Some(origin) = &origin {
            kwargs.set_item(py, "origin", origin)?;
        }

        let spec = module_spec_type.call(py, (name, loader), Some(&kwargs))?;

        if origin.is_some() {
            spec.setattr(py, "has_location", py.True())?;
        }

        // If we set `spec.cached`, it gets turned into `__cached__`.
        if let Some(cached) = self.resolve_cached(py, optimize_level)? {
            spec.setattr(py, "cached", cached)?;
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
            // open for our code later seeing this path and doing something
            // special with it. For example, the documentation for the deprecated
            // `importlib.abc.ResourceLoader.get_data()` says consumers could use
            // `__path__` to construct the `path` to pass into that function
            // (probably via `os.path.join()`). If we set `__path__` and our
            // `get_data()` is called, we could recognize the special value and
            // route to our importer accordingly. If we don't set `__path__` to
            // any value, we can't do this.
            //
            // As a point of reference, the zip importer in the Python standard
            // library sets `__path__` to the path to the zip file with the package
            // names `os.path.join()`d to the end. e.g.
            // `/path/to/myapp.zip/mypackage/subpackage`.
            let mut locations = if let Some(origin_path) = self.origin_path() {
                if let Some(parent_path) = origin_path.parent() {
                    vec![path_to_pyobject(py, parent_path)?]
                } else {
                    vec![]
                }
            } else {
                vec![]
            };

            if locations.is_empty() {
                let mut path = self.current_exe.to_path_buf();
                path.extend(self.resource.name.split('.'));

                locations.push(path_to_pyobject(py, &path)?);
            }

            spec.setattr(py, "submodule_search_locations", locations)?;
        }

        Ok(spec)
    }

    /// Resolve the value of a `ModuleSpec` origin.
    ///
    /// The value gets turned into `__file__`
    pub fn resolve_origin(&self, py: Python) -> PyResult<Option<PyObject>> {
        Ok(if let Some(path) = self.origin_path() {
            Some(path_to_pyobject(py, &path)?)
        } else {
            None
        })
    }

    /// Resolve the value of a `ModuleSpec` `cached` attribute.
    ///
    /// The value gets turned into `__cached__`.
    fn resolve_cached(
        &self,
        py: Python,
        optimize_level: OptimizeLevel,
    ) -> PyResult<Option<PyObject>> {
        let path = match self.flavor {
            ResourceFlavor::Module => self.bytecode_path(optimize_level),
            _ => None,
        };

        Ok(if let Some(path) = path {
            Some(path_to_pyobject(py, &path)?)
        } else {
            None
        })
    }

    /// Obtain the filesystem path to this resource to be used for `ModuleSpec.origin`.
    fn origin_path(&self) -> Option<PathBuf> {
        match self.flavor {
            ResourceFlavor::Module => {
                if let Some(path) = &self.resource.relative_path_module_source {
                    Some(self.origin.join(path))
                } else {
                    None
                }
            }
            ResourceFlavor::Extension => {
                if let Some(path) = &self.resource.relative_path_extension_module_shared_library {
                    Some(self.origin.join(path))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Obtain the filesystem path to bytecode for this module.
    fn bytecode_path(&self, optimize_level: OptimizeLevel) -> Option<PathBuf> {
        let bytecode_path = match optimize_level {
            OptimizeLevel::Zero => &self.resource.relative_path_module_bytecode,
            OptimizeLevel::One => &self.resource.relative_path_module_bytecode_opt1,
            OptimizeLevel::Two => &self.resource.relative_path_module_bytecode_opt2,
        };

        if let Some(bytecode_path) = bytecode_path {
            Some(self.origin.join(bytecode_path))
        } else {
            None
        }
    }
}

/// Defines Python resources available for import.
#[derive(Debug)]
pub(crate) struct PythonResourcesState<'a, X>
where
    [X]: ToOwned<Owned = Vec<X>>,
{
    /// Path to currently running executable.
    pub current_exe: PathBuf,

    /// Directory from which relative paths should be evaluated.
    ///
    /// Probably the directory of `current_exe`.
    pub origin: PathBuf,

    /// Named resources available for loading.
    pub resources: HashMap<Cow<'a, str>, Resource<'a, X>>,
}

impl<'a> Default for PythonResourcesState<'a, u8> {
    fn default() -> Self {
        Self {
            current_exe: PathBuf::new(),
            origin: PathBuf::new(),
            resources: HashMap::new(),
        }
    }
}

impl<'a> PythonResourcesState<'a, u8> {
    /// Construct an instance from environment state.
    pub fn new_from_env() -> Result<Self, &'static str> {
        let exe =
            std::env::current_exe().or_else(|_| Err("unable to obtain current executable"))?;
        let origin = exe
            .parent()
            .ok_or_else(|| "unable to get executable parent")?
            .to_path_buf();

        Ok(Self {
            current_exe: exe,
            origin,
            resources: Default::default(),
        })
    }

    /// Load state from the environment and by parsing data structures.
    pub fn load(&mut self, resources_data: Option<&'a [u8]>) -> Result<(), &'static str> {
        // Loading of builtin and frozen knows to mutate existing entries rather
        // than replace. So do these last.
        if let Some(data) = resources_data {
            self.load_resources(data)?;
        }
        self.load_interpreter_builtin_modules()?;
        self.load_interpreter_frozen_modules()?;

        Ok(())
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
        optimize_level: OptimizeLevel,
    ) -> Option<ImportablePythonModule<u8>> {
        let resource = match self.resources.get(name) {
            Some(entry) => entry,
            None => return None,
        };

        match resource.flavor {
            ResourceFlavor::Module => {
                if is_module_importable(resource, optimize_level) {
                    Some(ImportablePythonModule {
                        resource,
                        current_exe: &self.current_exe,
                        origin: &self.origin,
                        flavor: &resource.flavor,
                        is_package: resource.is_package,
                    })
                } else {
                    None
                }
            }
            ResourceFlavor::Extension => Some(ImportablePythonModule {
                resource,
                current_exe: &self.current_exe,
                origin: &self.origin,
                flavor: &resource.flavor,
                is_package: resource.is_package,
            }),
            ResourceFlavor::BuiltinExtensionModule => Some(ImportablePythonModule {
                resource,
                current_exe: &self.current_exe,
                origin: &self.origin,
                flavor: &resource.flavor,
                is_package: resource.is_package,
            }),
            ResourceFlavor::FrozenModule => Some(ImportablePythonModule {
                resource,
                current_exe: &self.current_exe,
                origin: &self.origin,
                flavor: &resource.flavor,
                is_package: resource.is_package,
            }),
            _ => None,
        }
    }

    /// Obtain a single named resource in a package.
    ///
    /// Err occurs if loading the resource data fails. `Ok(None)` is returned
    /// if the resource does not exist. Otherwise the returned `PyObject`
    /// is a file-like object to read the resource data.
    pub fn get_package_resource_file(
        &self,
        py: Python,
        package: &str,
        resource_name: &str,
    ) -> PyResult<Option<PyObject>> {
        let entry = match self.resources.get(package) {
            Some(entry) => entry,
            None => return Ok(None),
        };

        if let Some(resources) = &entry.in_memory_package_resources {
            if let Some(data) = resources.get(resource_name) {
                let io_module = py.import("io")?;
                let bytes_io = io_module.get(py, "BytesIO")?;

                let data = PyBytes::new(py, data);
                return Ok(Some(bytes_io.call(py, (data,), None)?));
            }
        }

        if let Some(resources) = &entry.relative_path_package_resources {
            if let Some(path) = resources.get(resource_name) {
                let path = self.origin.join(path);
                let io_module = py.import("io")?;

                return Ok(Some(io_module.call(
                    py,
                    "FileIO",
                    (path_to_pyobject(py, &path)?, "r"),
                    None,
                )?));
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
    pub fn package_resource_names(&self, py: Python, package: &str) -> PyResult<PyObject> {
        let entry = match self.resources.get(package) {
            Some(entry) => entry,
            None => return Ok(PyList::new(py, &[]).into_object()),
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
            .map(|x| x.to_py_object(py).into_object())
            .collect::<Vec<PyObject>>();

        Ok(PyList::new(py, &names).into_object())
    }

    /// Attempt to resolve a PyBytes for resource data given a relative path.
    ///
    /// Raises OSerror on failure.
    ///
    /// This method is meant to be an implementation of `ResourceLoader.get_data()` and
    /// should only be used for that purpose.
    pub fn resolve_resource_data_from_path(
        &self,
        py: Python,
        path: &PyString,
    ) -> PyResult<PyObject> {
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
        let native_path = PathBuf::from(path.to_string_lossy(py).to_string());

        let (relative_path, check_in_memory, check_relative_path) =
            if let Ok(relative_path) = native_path.strip_prefix(&self.current_exe) {
                (relative_path, true, false)
            } else if let Ok(relative_path) = native_path.strip_prefix(&self.origin) {
                (relative_path, false, true)
            } else {
                return Err(PyErr::new::<OSError, _>(
                    py,
                    (libc::ENOENT, "resource not known", path.clone()),
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
            return Err(PyErr::new::<OSError, _>(
                py,
                (
                    libc::ENOENT,
                    "illegal resource name: missing package component",
                    path.clone(),
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
                            return Ok(PyBytes::new(py, data).into_object());
                        }
                    }
                }

                if check_relative_path {
                    if let Some(resources) = &entry.relative_path_package_resources {
                        if let Some(resource_relative_path) = resources.get(resource_name_ref) {
                            let resource_path = self.origin.join(resource_relative_path);

                            let io_module = py.import("io")?;

                            let fh = io_module.call(
                                py,
                                "FileIO",
                                (path_to_pyobject(py, &resource_path)?, "r"),
                                None,
                            )?;

                            return fh.call_method(py, "read", NoArgs, None);
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

        Err(PyErr::new::<OSError, _>(
            py,
            (libc::ENOENT, "resource not known", path.clone()),
        ))
    }

    /// Obtain a PyList of pkgutil.ModuleInfo for known resources.
    ///
    /// This is intended to be used as the implementation for Finder.iter_modules().
    pub fn pkgutil_modules_infos(
        &self,
        py: Python,
        prefix: Option<String>,
        optimize_level: OptimizeLevel,
    ) -> PyResult<PyObject> {
        let infos: PyResult<Vec<PyObject>> = self
            .resources
            .values()
            .filter(|r| match r.flavor {
                ResourceFlavor::Module => is_module_importable(r, optimize_level),
                ResourceFlavor::Extension => true,
                _ => false,
            })
            .map(|r| {
                let name = if let Some(prefix) = &prefix {
                    format!("{}{}", prefix, r.name)
                } else {
                    r.name.to_string()
                };

                let name = name.to_py_object(py).into_object();
                let is_package = r.is_package.to_py_object(py).into_object();

                Ok(PyTuple::new(py, &[name, is_package]).into_object())
            })
            .collect();

        let infos = infos?;

        let res = PyList::new(py, &infos);

        Ok(res.into_object())
    }

    /// Load `builtin` modules from the Python interpreter.
    fn load_interpreter_builtin_modules(&mut self) -> Result<(), &'static str> {
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

            // Module can be defined by embedded resources data. If exists, just
            // update the big.
            if let Some(mut entry) = self.resources.get_mut(name_str) {
                entry.flavor = ResourceFlavor::BuiltinExtensionModule;
            } else {
                self.resources.insert(
                    Cow::Owned(name_str.to_string()),
                    Resource {
                        flavor: ResourceFlavor::BuiltinExtensionModule,
                        name: Cow::Owned(name_str.to_string()),
                        ..Resource::default()
                    },
                );
            }
        }

        Ok(())
    }

    /// Load `frozen` modules from the Python interpreter.
    fn load_interpreter_frozen_modules(&mut self) -> Result<(), &'static str> {
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

            // Module can be defined by embedded resources data. If exists, just
            // update the big.
            if let Some(mut entry) = self.resources.get_mut(name_str) {
                entry.flavor = ResourceFlavor::FrozenModule;
            } else {
                self.resources.insert(
                    Cow::Owned(name_str.to_string()),
                    Resource {
                        flavor: ResourceFlavor::FrozenModule,
                        name: Cow::Owned(name_str.to_string()),
                        ..Resource::default()
                    },
                );
            }
        }

        Ok(())
    }

    /// Load resources by parsing a blob.
    fn load_resources(&mut self, data: &'a [u8]) -> Result<(), &'static str> {
        let resources = python_packed_resources::parser::load_resources(data)?;

        // Reserve space for expected number of incoming items so we can avoid extra
        // allocations.
        self.resources.reserve(resources.expected_resources_count());

        for resource in resources {
            let resource = resource?;

            self.resources.insert(resource.name.clone(), resource);
        }

        Ok(())
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
            .filter(|resource| match resource.flavor {
                ResourceFlavor::BuiltinExtensionModule => !ignore_builtin,
                ResourceFlavor::FrozenModule => !ignore_frozen,
                _ => true,
            })
            .collect::<Vec<&Resource<u8>>>();

        // Sort so behavior is deterministic.
        resources.sort_by_key(|v| &v.name);

        let mut buffer = Vec::new();

        python_packed_resources::writer::write_packed_resources_v1(&resources, &mut buffer, None)?;

        Ok(buffer)
    }
}

py_class!(pub class OxidizedResource |py| {
    data resource: RefCell<Resource<'static, u8>>;

    def __new__(_cls) -> PyResult<OxidizedResource> {
        let resource = OxidizedResource::create_instance(py, RefCell::new(Resource::<u8>::default()))?;

        Ok(resource)
    }

    def __repr__(&self) -> PyResult<String> {
        Ok(format!("<OxidizedResource name=\"{}\">", self.resource(py).borrow().name.to_string()))
    }

    @property def flavor(&self) -> PyResult<&'static str> {
        Ok(match self.resource(py).borrow().flavor {
            ResourceFlavor::None => "none",
            ResourceFlavor::Module => "module",
            ResourceFlavor::BuiltinExtensionModule => "builtin",
            ResourceFlavor::FrozenModule => "frozen",
            ResourceFlavor::Extension => "extension",
            ResourceFlavor::SharedLibrary => "shared_library",
        })
    }

    @flavor.setter def set_flavor(&self, value: Option<&str>) -> PyResult<()> {
        if let Some(value) = value {
            self.resource(py).borrow_mut().flavor = match value {
                "none" => ResourceFlavor::None,
                "module" => ResourceFlavor::Module,
                "builtin" => ResourceFlavor::BuiltinExtensionModule,
                "frozen" => ResourceFlavor::FrozenModule,
                "extension" => ResourceFlavor::Extension,
                "shared_library" => ResourceFlavor::SharedLibrary,
                _ => return Err(PyErr::new::<ValueError, _>(py, "unknown resource flavor"))
            };

            Ok(())
        } else {
            Err(PyErr::new::<TypeError, _>(py, "cannot delete flavor"))
        }
    }

    @property def name(&self) -> PyResult<String> {
        Ok(self.resource(py).borrow().name.to_string())
    }

    @name.setter def set_name(&self, value: Option<&str>) -> PyResult<()> {
        if let Some(value) = value {
            self.resource(py).borrow_mut().name = Cow::Owned(value.to_owned());

            Ok(())
        } else {
            Err(PyErr::new::<TypeError, _>(py, "cannot delete name"))
        }
    }

    @property def is_package(&self) -> PyResult<bool> {
        Ok(self.resource(py).borrow().is_package)
    }

    @is_package.setter def set_is_package(&self, value: Option<bool>) -> PyResult<()> {
        if let Some(value) = value {
            self.resource(py).borrow_mut().is_package = value;
            Ok(())
        } else {
            Err(PyErr::new::<TypeError, _>(py, "cannot delete is_package"))
        }
    }

    @property def is_namespace_package(&self) -> PyResult<bool> {
        Ok(self.resource(py).borrow().is_namespace_package)
    }

    @is_namespace_package.setter def set_is_namespace_package(&self, value: Option<bool>) -> PyResult<()> {
        if let Some(value) = value {
            self.resource(py).borrow_mut().is_namespace_package = value;
            Ok(())
        } else {
            Err(PyErr::new::<TypeError, _>(py, "cannot delete is_namespace_package"))
        }
    }

    @property def in_memory_source(&self) -> PyResult<Option<PyBytes>> {
        Ok(self.resource(py).borrow().in_memory_source.as_ref().map(|x| PyBytes::new(py, x)))
    }

    @in_memory_source.setter def set_in_memory_source(&self, value: Option<PyObject>) -> PyResult<()> {
        if let Some(value) = value {
            self.resource(py).borrow_mut().in_memory_source =
                pyobject_to_owned_bytes_optional(py, &value)?
                    .map(|x| Cow::Owned(x));
            Ok(())
        } else {
            Err(PyErr::new::<TypeError, _>(py, "cannot delete in_memory_source"))
        }
    }

    @property def in_memory_bytecode(&self) -> PyResult<Option<PyBytes>> {
        Ok(self.resource(py).borrow().in_memory_bytecode.as_ref().map(|x| PyBytes::new(py, x)))
    }

    @in_memory_bytecode.setter def set_in_memory_bytecode(&self, value: Option<PyObject>) -> PyResult<()> {
        if let Some(value) = value {
            self.resource(py).borrow_mut().in_memory_bytecode =
                pyobject_to_owned_bytes_optional(py, &value)?
                    .map(|x| Cow::Owned(x));
            Ok(())
        } else {
            Err(PyErr::new::<TypeError, _>(py, "cannot delete in_memory_bytecode"))
        }
    }

    @property def in_memory_bytecode_opt1(&self) -> PyResult<Option<PyBytes>> {
        Ok(self.resource(py).borrow().in_memory_bytecode_opt1.as_ref().map(|x| PyBytes::new(py, x)))
    }

    @in_memory_bytecode_opt1.setter def set_in_memory_bytecode_opt1(&self, value: Option<PyObject>) -> PyResult<()> {
        if let Some(value) = value {
            self.resource(py).borrow_mut().in_memory_bytecode_opt1 =
                pyobject_to_owned_bytes_optional(py, &value)?
                    .map(|x| Cow::Owned(x));
            Ok(())
        } else {
            Err(PyErr::new::<TypeError, _>(py, "cannot delete in_memory_bytecode_opt1"))
        }
    }

    @property def in_memory_bytecode_opt2(&self) -> PyResult<Option<PyBytes>> {
        Ok(self.resource(py).borrow().in_memory_bytecode_opt2.as_ref().map(|x| PyBytes::new(py, x)))
    }

    @in_memory_bytecode_opt2.setter def set_in_memory_bytecode_opt2(&self, value: Option<PyObject>) -> PyResult<()> {
        if let Some(value) = value {
            self.resource(py).borrow_mut().in_memory_bytecode_opt2 =
                pyobject_to_owned_bytes_optional(py, &value)?
                    .map(|x| Cow::Owned(x));
            Ok(())
        } else {
            Err(PyErr::new::<TypeError, _>(py, "cannot delete in_memory_bytecode_opt2"))
        }
    }

    @property def in_memory_extension_module_shared_library(&self) -> PyResult<Option<PyBytes>> {
        Ok(self.resource(py).borrow().in_memory_extension_module_shared_library.as_ref().map(|x| PyBytes::new(py, x)))
    }

    @in_memory_extension_module_shared_library.setter def set_in_memory_extension_module_shared_library(&self, value: Option<PyObject>) -> PyResult<()> {
        if let Some(value) = value {
            self.resource(py).borrow_mut().in_memory_extension_module_shared_library =
                pyobject_to_owned_bytes_optional(py, &value)?
                    .map(|x| Cow::Owned(x));
            Ok(())
        } else {
            Err(PyErr::new::<TypeError, _>(py, "cannot delete in_memory_extension_module_shared_library"))
        }
    }

    @property def in_memory_package_resources(&self) -> PyResult<Option<HashMap<String, PyBytes>>> {
        Ok(self.resource(py).borrow().in_memory_package_resources.as_ref().map(|x| {
            HashMap::from_iter(x.iter().map(|(k, v)| (k.to_string(), PyBytes::new(py, v))))
        }))
    }

    @in_memory_package_resources.setter def set_in_memory_package_resources(&self, value: Option<PyObject>) -> PyResult<()> {
        if let Some(value) = value {
            self.resource(py).borrow_mut().in_memory_package_resources =
                pyobject_optional_resources_map_to_owned_bytes(py, &value)?
                    .map(|x| HashMap::from_iter(
                        x.iter().map(|(k, v)| (Cow::Owned(k.to_owned()), Cow::Owned(v.to_owned())))
                     ));

            Ok(())
        } else {
            Err(PyErr::new::<TypeError, _>(py, "cannot delete in_memory_package_resources"))
        }
    }

    @property def in_memory_distribution_resources(&self) -> PyResult<Option<HashMap<String, PyBytes>>> {
        Ok(self.resource(py).borrow().in_memory_distribution_resources.as_ref().map(|x| {
            HashMap::from_iter(x.iter().map(|(k, v)| (k.to_string(), PyBytes::new(py, v))))
        }))
    }

    @in_memory_distribution_resources.setter def set_in_memory_distribution_resources(&self, value: Option<PyObject>) -> PyResult<()> {
        if let Some(value) = value {
            self.resource(py).borrow_mut().in_memory_distribution_resources =
                pyobject_optional_resources_map_to_owned_bytes(py, &value)?
                    .map(|x| HashMap::from_iter(
                        x.iter().map(|(k, v)| (Cow::Owned(k.to_owned()), Cow::Owned(v.to_owned())))
                     ));

            Ok(())
        } else {
            Err(PyErr::new::<TypeError, _>(py, "cannot delete in_memory_distribution_resources"))
        }
    }

    @property def in_memory_shared_library(&self) -> PyResult<Option<PyBytes>> {
        Ok(self.resource(py).borrow().in_memory_shared_library.as_ref().map(|x| PyBytes::new(py, x)))
    }

    @in_memory_shared_library.setter def set_in_memory_shared_library(&self, value: Option<PyObject>) -> PyResult<()> {
        if let Some(value) = value {
            self.resource(py).borrow_mut().in_memory_shared_library =
                pyobject_to_owned_bytes_optional(py, &value)?
                    .map(|x| Cow::Owned(x));
            Ok(())
        } else {
            Err(PyErr::new::<TypeError, _>(py, "cannot delete in_memory_shared_library"))
        }
    }

    @property def shared_library_dependency_names(&self) -> PyResult<Option<Vec<String>>> {
        Ok(self.resource(py).borrow().shared_library_dependency_names.as_ref().map(|x| {
            Vec::from_iter(x.iter().map(|v| v.to_string()))
        }))
    }

    @shared_library_dependency_names.setter def set_shared_library_dependency_names(&self, value: Option<Option<Vec<String>>>) -> PyResult<()> {
        if let Some(value) = value {
            self.resource(py).borrow_mut().shared_library_dependency_names =
                value.map(|x| Vec::from_iter(x.iter().map(|v| Cow::Owned(v.to_owned()))));

            Ok(())
        } else {
            Err(PyErr::new::<TypeError, _>(py, "cannot delete shared_library_dependency_names"))
        }
    }

    @property def relative_path_module_source(&self) -> PyResult<PyObject> {
        Ok(self.resource(py).borrow().relative_path_module_source.as_ref().map_or_else(
            || Ok(py.None()),
            |x| path_to_pathlib_path(py, x)
        )?)
    }

    @relative_path_module_source.setter def set_relative_path_module_source(&self, value: Option<PyObject>) -> PyResult<()> {
        if let Some(value) = value {
            self.resource(py).borrow_mut().relative_path_module_source =
                pyobject_to_pathbuf_optional(py, value)?
                    .map(|x| Cow::Owned(x));

            Ok(())
        } else {
            Err(PyErr::new::<TypeError, _>(py, "cannot delete relative_path_module_source"))
        }
    }

    @property def relative_path_module_bytecode(&self) -> PyResult<PyObject> {
        Ok(self.resource(py).borrow().relative_path_module_bytecode.as_ref().map_or_else(
            || Ok(py.None()),
            |x| path_to_pathlib_path(py, x)
        )?)
    }

    @relative_path_module_bytecode.setter def set_relative_path_module_bytecode(&self, value: Option<PyObject>) -> PyResult<()> {
        if let Some(value) = value {
            self.resource(py).borrow_mut().relative_path_module_bytecode =
                pyobject_to_pathbuf_optional(py, value)?
                    .map(|x| Cow::Owned(x));

            Ok(())
        } else {
            Err(PyErr::new::<TypeError, _>(py, "cannot delete relative_path_module_bytecode"))
        }
    }

    @property def relative_path_module_bytecode_opt1(&self) -> PyResult<PyObject> {
        Ok(self.resource(py).borrow().relative_path_module_bytecode_opt1.as_ref().map_or_else(
            || Ok(py.None()),
            |x| path_to_pathlib_path(py, x)
        )?)
    }

    @relative_path_module_bytecode_opt1.setter def set_relative_path_module_bytecode_opt1(&self, value: Option<PyObject>) -> PyResult<()> {
        if let Some(value) = value {
            self.resource(py).borrow_mut().relative_path_module_bytecode_opt1 =
                pyobject_to_pathbuf_optional(py, value)?
                    .map(|x| Cow::Owned(x));

            Ok(())
        } else {
            Err(PyErr::new::<TypeError, _>(py, "cannot delete relative_path_module_bytecode_opt1"))
        }
    }

    @property def relative_path_module_bytecode_opt2(&self) -> PyResult<PyObject> {
        Ok(self.resource(py).borrow().relative_path_module_bytecode_opt2.as_ref().map_or_else(
            || Ok(py.None()),
            |x| path_to_pathlib_path(py, x)
        )?)
    }

    @relative_path_module_bytecode_opt2.setter def set_relative_path_module_bytecode_opt2(&self, value: Option<PyObject>) -> PyResult<()> {
        if let Some(value) = value {
            self.resource(py).borrow_mut().relative_path_module_bytecode_opt2 =
                pyobject_to_pathbuf_optional(py, value)?
                    .map(|x| Cow::Owned(x));

            Ok(())
        } else {
            Err(PyErr::new::<TypeError, _>(py, "cannot delete relative_path_module_bytecode_opt2"))
        }
    }

    @property def relative_path_extension_module_shared_library(&self) -> PyResult<PyObject> {
        Ok(self.resource(py).borrow().relative_path_extension_module_shared_library.as_ref().map_or_else(
            || Ok(py.None()),
            |x| path_to_pathlib_path(py, x)
        )?)
    }

    @relative_path_extension_module_shared_library.setter def set_relative_path_extension_module_shared_library(&self, value: Option<PyObject>) -> PyResult<()> {
        if let Some(value) = value {
            self.resource(py).borrow_mut().relative_path_extension_module_shared_library =
                pyobject_to_pathbuf_optional(py, value)?
                    .map(|x| Cow::Owned(x));

            Ok(())
        } else {
            Err(PyErr::new::<TypeError, _>(py, "cannot delete relative_path_extension_module_shared_library"))
        }
    }

    @property def relative_path_package_resources(&self) -> PyResult<PyObject> {
        Ok(self.resource(py).borrow().relative_path_package_resources.as_ref().map_or_else(
            || Ok(py.None()),
            |x| -> PyResult<PyObject> {
                let res = PyDict::new(py);

                for (k, v) in x.iter() {
                    res.set_item(py, k, path_to_pathlib_path(py, v)?)?;
                }

                Ok(res.into_object())
            }
        )?)
    }

    @relative_path_package_resources.setter def set_relative_path_package_resources(&self, value: Option<PyObject>) -> PyResult<()> {
        if let Some(value) = value {
            self.resource(py).borrow_mut().relative_path_package_resources =
                pyobject_optional_resources_map_to_pathbuf(py, &value)?
                    .map(|x| HashMap::from_iter(
                        x.iter().map(|(k, v)| (Cow::Owned(k.to_owned()), Cow::Owned(v.to_owned())))
                     ));

            Ok(())
        } else {
            Err(PyErr::new::<TypeError, _>(py, "cannot delete relative_path_package_resources"))
        }
    }

    @property def relative_path_distribution_resources(&self) -> PyResult<PyObject> {
        Ok(self.resource(py).borrow().relative_path_distribution_resources.as_ref().map_or_else(
            || Ok(py.None()),
            |x| -> PyResult<PyObject> {
                let res = PyDict::new(py);

                for (k, v) in x.iter() {
                    res.set_item(py, k, path_to_pathlib_path(py, v)?)?;
                }

                Ok(res.into_object())
            }
        )?)
    }

    @relative_path_distribution_resources.setter def set_relative_path_distribution_resources(&self, value: Option<PyObject>) -> PyResult<()> {
        if let Some(value) = value {
            self.resource(py).borrow_mut().relative_path_distribution_resources =
                pyobject_optional_resources_map_to_pathbuf(py, &value)?
                    .map(|x| HashMap::from_iter(
                        x.iter().map(|(k, v)| (Cow::Owned(k.to_owned()), Cow::Owned(v.to_owned())))
                     ));

            Ok(())
        } else {
            Err(PyErr::new::<TypeError, _>(py, "cannot delete relative_path_distribution_resources"))
        }
    }

});

/// Convert a Resource to an OxidizedResource.
pub fn resource_to_pyobject(py: Python, resource: &Resource<u8>) -> PyResult<PyObject> {
    let resource = OxidizedResource::create_instance(py, RefCell::new(resource.to_owned()))?;

    Ok(resource.into_object())
}

#[inline]
pub fn pyobject_to_resource(py: Python, resource: OxidizedResource) -> Resource<u8> {
    resource.resource(py).borrow().clone()
}
