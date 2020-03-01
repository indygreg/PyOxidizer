// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Management of Python resources.
*/

use {
    python3_sys as pyffi,
    python_packed_resources::parser::EmbeddedResource,
    std::collections::{HashMap, HashSet},
    std::ffi::CStr,
};

/// Whether the module is imported by the importer provided by this crate.
///
/// This excludes builtin and frozen modules, which are merely registered.
pub fn uses_pyembed_importer(resource: &EmbeddedResource) -> bool {
    resource.in_memory_bytecode.is_some()
        || resource.in_memory_bytecode_opt1.is_some()
        || resource.in_memory_bytecode_opt2.is_some()
        || resource.in_memory_shared_library_extension_module.is_some()
}

/// Defines Python resources available for import.
#[derive(Debug)]
pub(crate) struct PythonImporterState<'a> {
    /// Names of Python packages.
    pub packages: HashSet<&'static str>,

    pub resources: HashMap<&'a str, EmbeddedResource<'a>>,
}

impl<'a> Default for PythonImporterState<'a> {
    fn default() -> Self {
        Self {
            packages: HashSet::new(),
            resources: HashMap::new(),
        }
    }
}

impl<'a> PythonImporterState<'a> {
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
                entry.is_builtin = true;
            } else {
                self.resources.insert(
                    name_str,
                    EmbeddedResource {
                        name: name_str,
                        is_builtin: true,
                        ..EmbeddedResource::default()
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
                entry.is_frozen = true;
            } else {
                self.resources.insert(
                    name_str,
                    EmbeddedResource {
                        name: name_str,
                        is_frozen: true,
                        ..EmbeddedResource::default()
                    },
                );
            }
        }

        Ok(())
    }

    /// Load resources by parsing a blob.
    fn load_resources(&mut self, data: &'a [u8]) -> Result<(), &'static str> {
        python_packed_resources::parser::load_resources(data, &mut self.resources)
    }
}
