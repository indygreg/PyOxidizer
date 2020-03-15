// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Management of Python resources.
*/

use {
    cpython::exc::ImportError,
    cpython::{PyBytes, PyErr, PyObject, PyResult, Python},
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
fn is_importable<X>(entry: &Resource<X>, optimize_level: OptimizeLevel) -> bool
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
        } else if let Some(relative_path) = match optimize_level {
            OptimizeLevel::Zero => &self.resource.relative_path_module_bytecode,
            OptimizeLevel::One => &self.resource.relative_path_module_bytecode_opt1,
            OptimizeLevel::Two => &self.resource.relative_path_module_bytecode_opt2,
        } {
            let path = self.origin.join(relative_path);

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
}

/// Defines Python resources available for import.
#[derive(Debug)]
pub(crate) struct PythonResourcesState<'a, X>
where
    [X]: ToOwned<Owned = Vec<X>>,
{
    pub origin: PathBuf,

    /// Names of Python packages.
    pub packages: HashSet<&'static str>,

    /// Named resources available for loading.
    pub resources: HashMap<Cow<'a, str>, Resource<'a, X>>,
}

impl<'a> Default for PythonResourcesState<'a, u8> {
    fn default() -> Self {
        Self {
            origin: PathBuf::new(),
            packages: HashSet::new(),
            resources: HashMap::new(),
        }
    }
}

impl<'a> PythonResourcesState<'a, u8> {
    /// Load state from the environment and by parsing data structures.
    pub fn load(&mut self, resources_data: &'static [u8]) -> Result<(), &'static str> {
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
                if is_importable(resource, optimize_level) {
                    Some(ImportablePythonModule {
                        resource,
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
                origin: &self.origin,
                bytecode: None,
                flavor: &resource.flavor,
                is_package: resource.is_package,
            }),
            ResourceFlavor::BuiltinExtensionModule => Some(ImportablePythonModule {
                resource,
                origin: &self.origin,
                bytecode: None,
                flavor: &resource.flavor,
                is_package: resource.is_package,
            }),
            ResourceFlavor::FrozenModule => Some(ImportablePythonModule {
                resource,
                origin: &self.origin,
                bytecode: None,
                flavor: &resource.flavor,
                is_package: resource.is_package,
            }),
            _ => None,
        }
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
                    // This is probably unsafe.
                    Cow::from(name_str),
                    Resource {
                        flavor: ResourceFlavor::BuiltinExtensionModule,
                        name: Cow::from(name_str),
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
                    // This is probably unsafe.
                    Cow::from(name_str),
                    Resource {
                        flavor: ResourceFlavor::FrozenModule,
                        name: Cow::from(name_str),
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
