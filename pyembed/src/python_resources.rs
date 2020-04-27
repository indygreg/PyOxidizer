// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Management of Python resources.
*/

use {
    super::pystr::path_to_pyobject,
    cpython::exc::{ImportError, OSError},
    cpython::{
        NoArgs, ObjectProtocol, PyBytes, PyClone, PyDict, PyErr, PyList, PyObject, PyResult,
        PyString, Python, PythonObject, ToPyObject,
    },
    python3_sys as pyffi,
    python_packed_resources::data::{Resource, ResourceFlavor},
    std::borrow::Cow,
    std::collections::{HashMap, HashSet},
    std::ffi::CStr,
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

    match optimize_level {
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

    /// Cached bytecode (when read from an external source such as the filesystem).
    bytecode: Option<Vec<u8>>,

    /// The resource/module flavor.
    pub flavor: &'a ResourceFlavor,
    /// Whether this module is a package.
    pub is_package: bool,
}

impl<'a> ImportablePythonModule<'a, u8> {
    /// Attempt to resolve a Python `bytes` for the source code behind this module.
    ///
    /// Will return a PyErr if an error occurs resolving source. If there is no source,
    /// returns `Ok(None)`. Otherwise an `Ok(PyBytes)` is returned.
    ///
    /// We could potentially return a `memoryview` to avoid the extra allocation required
    /// by `PyBytes_FromStringAndSize()`. However, callers of this method typically
    /// call `importlib._bootstrap_external.decode_source()` with the returned value
    /// and this function can't handle `memoryview`. So until callers can support
    /// 0-copy, let's not worry about it.
    pub fn resolve_source(&self, py: Python) -> PyResult<Option<PyBytes>> {
        Ok(if let Some(data) = &self.resource.in_memory_source {
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
        })
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
    ) -> PyResult<Option<PyObject>> {
        if let Some(cached) = &self.bytecode {
            let ptr = unsafe {
                pyffi::PyMemoryView_FromMemory(
                    cached.as_ptr() as _,
                    cached.len() as _,
                    pyffi::PyBUF_READ,
                )
            };

            Ok(unsafe { PyObject::from_owned_ptr_opt(py, ptr) })
        } else if let Some(data) = match optimize_level {
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
            let bytecode = std::fs::read(&path).or_else(|e| {
                Err(PyErr::new::<ImportError, _>(
                    py,
                    (
                        format!("error reading bytecode from {}: {}", path.display(), e),
                        self.resource.name.clone(),
                    ),
                ))
            })?;

            // We could avoid a double allocation if we wanted...
            self.bytecode = Some(Vec::from(&bytecode[16..]));
            let bytecode = self.bytecode.as_ref().unwrap();

            let ptr = unsafe {
                pyffi::PyMemoryView_FromMemory(
                    bytecode.as_ptr() as _,
                    bytecode.len() as _,
                    pyffi::PyBUF_READ,
                )
            };

            Ok(unsafe { PyObject::from_owned_ptr_opt(py, ptr) })
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

    /// Names of Python packages.
    pub packages: HashSet<&'a str>,

    /// Named resources available for loading.
    pub resources: HashMap<Cow<'a, str>, Resource<'a, X>>,
}

impl<'a> Default for PythonResourcesState<'a, u8> {
    fn default() -> Self {
        Self {
            current_exe: PathBuf::new(),
            origin: PathBuf::new(),
            packages: HashSet::new(),
            resources: HashMap::new(),
        }
    }
}

impl<'a> PythonResourcesState<'a, u8> {
    /// Load state from the environment and by parsing data structures.
    pub fn load(&mut self, resources_data: &'a [u8]) -> Result<(), &'static str> {
        // Loading of builtin and frozen knows to mutate existing entries rather
        // than replace. So do these last.
        self.load_resources(resources_data)?;
        self.load_interpreter_builtin_modules()?;
        self.load_interpreter_frozen_modules()?;

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
                        bytecode: None,
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
                bytecode: None,
                flavor: &resource.flavor,
                is_package: resource.is_package,
            }),
            ResourceFlavor::BuiltinExtensionModule => Some(ImportablePythonModule {
                resource,
                current_exe: &self.current_exe,
                origin: &self.origin,
                bytecode: None,
                flavor: &resource.flavor,
                is_package: resource.is_package,
            }),
            ResourceFlavor::FrozenModule => Some(ImportablePythonModule {
                resource,
                current_exe: &self.current_exe,
                origin: &self.origin,
                bytecode: None,
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
    pub fn package_resource_names(&self, py: Python, package: &str) -> PyResult<PyObject> {
        let entry = match self.resources.get(package) {
            Some(entry) => entry,
            None => return Ok(PyList::new(py, &[]).into_object()),
        };

        if let Some(resources) = &entry.in_memory_package_resources {
            let names = resources
                .keys()
                .map(|name| name.to_py_object(py))
                .collect::<Vec<PyString>>();

            return Ok(names.to_py_object(py).as_object().clone_ref(py));
        }

        if let Some(resources) = &entry.relative_path_package_resources {
            let names = resources
                .keys()
                .map(|name| name.to_py_object(py))
                .collect::<Vec<PyString>>();

            return Ok(names.to_py_object(py).as_object().clone_ref(py));
        }

        Ok(PyList::new(py, &[]).into_object())
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
}
