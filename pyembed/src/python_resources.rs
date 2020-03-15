// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Management of Python resources.
*/

use {
    python3_sys as pyffi,
    python_packed_resources::data::{Resource, ResourceFlavor},
    std::borrow::Cow,
    std::collections::{HashMap, HashSet},
    std::ffi::CStr,
};

/// Whether the module is imported by the importer provided by this crate.
///
/// This excludes builtin and frozen modules, which are merely registered.
pub(crate) fn uses_pyembed_importer<X>(entry: &Resource<X>) -> bool
where
    [X]: ToOwned<Owned = Vec<X>>,
{
    entry.in_memory_bytecode.is_some()
        || entry.in_memory_bytecode_opt1.is_some()
        || entry.in_memory_bytecode_opt2.is_some()
        || entry.in_memory_extension_module_shared_library.is_some()
        || entry.relative_path_module_source.is_some()
        || entry.relative_path_module_bytecode.is_some()
        || entry.relative_path_module_bytecode_opt1.is_some()
        || entry.relative_path_module_bytecode_opt2.is_some()
}

/// Defines Python resources available for import.
#[derive(Debug)]
pub(crate) struct PythonResourcesState<'a, X>
where
    [X]: ToOwned<Owned = Vec<X>>,
{
    /// Names of Python packages.
    pub packages: HashSet<&'static str>,

    pub resources: HashMap<Cow<'a, str>, Resource<'a, X>>,
}

impl<'a> Default for PythonResourcesState<'a, u8> {
    fn default() -> Self {
        Self {
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

        for resource in resources {
            let resource = resource?;

            self.resources.insert(resource.name.clone(), resource);
        }

        Ok(())
    }
}
