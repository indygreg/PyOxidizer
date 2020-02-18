// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Management of Python resources.
*/

use {
    byteorder::{LittleEndian, ReadBytesExt},
    python3_sys as pyffi,
    std::collections::{HashMap, HashSet},
    std::ffi::CStr,
    std::io::Cursor,
    std::sync::Arc,
};

/// Named resources in a single Python package.
type PythonPackageResources = HashMap<&'static str, &'static [u8]>;

#[derive(Debug)]
pub(crate) enum PythonModuleLocation {
    Builtin,
    Frozen,
    InMemory {
        source: Option<&'static [u8]>,
        bytecode: Option<&'static [u8]>,
    },
}

type KnownModules = HashMap<&'static str, PythonModuleLocation>;

/// Defines Python resources available for import.
#[derive(Debug)]
pub(crate) struct PythonImporterState {
    /// Names of Python packages.
    pub packages: HashSet<&'static str>,

    /// importlib resources indexed by Python package.
    pub package_resources: HashMap<&'static str, Arc<Box<PythonPackageResources>>>,

    /// Comprehensive mapping of importable modules.
    pub modules: KnownModules,
}

impl Default for PythonImporterState {
    fn default() -> Self {
        Self {
            packages: HashSet::new(),
            package_resources: HashMap::new(),
            modules: KnownModules::new(),
        }
    }
}

impl PythonImporterState {
    /// Load `builtin` modules from the Python interpreter.
    pub fn load_interpreter_builtin_modules(&mut self) -> Result<(), &'static str> {
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

            self.modules.insert(name_str, PythonModuleLocation::Builtin);
        }

        Ok(())
    }

    /// Load `frozen` modules from the Python interpreter.
    pub fn load_interpreter_frozen_modules(&mut self) -> Result<(), &'static str> {
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

            self.modules.insert(name_str, PythonModuleLocation::Frozen);
        }

        Ok(())
    }

    /// Parse binary modules data and update current data structure.
    pub fn load_modules_data(&mut self, data: &'static [u8]) -> Result<(), &'static str> {
        let mut reader = Cursor::new(data);

        let count = reader
            .read_u32::<LittleEndian>()
            .or_else(|_| Err("failed reading count"))? as usize;

        let mut index = Vec::with_capacity(count as usize);
        let mut total_names_length = 0;
        let mut total_sources_length = 0;
        let mut package_count = 0;

        for _ in 0..count {
            let name_length = reader
                .read_u32::<LittleEndian>()
                .or_else(|_| Err("failed reading name length"))?
                as usize;
            let source_length = reader
                .read_u32::<LittleEndian>()
                .or_else(|_| Err("failed reading source length"))?
                as usize;
            let bytecode_length = reader
                .read_u32::<LittleEndian>()
                .or_else(|_| Err("failed reading bytecode length"))?
                as usize;
            let flags = reader
                .read_u32::<LittleEndian>()
                .or_else(|_| Err("failed reading module flags"))?;

            let is_package = flags & 0x01 != 0;

            if is_package {
                package_count += 1;
            }

            index.push((name_length, source_length, bytecode_length, is_package));
            total_names_length += name_length;
            total_sources_length += source_length;
        }

        if package_count > self.packages.capacity() {
            self.packages
                .reserve(package_count - self.packages.capacity());
        }

        if count > self.modules.capacity() {
            self.modules.reserve(count - self.modules.capacity());
        }

        let sources_start_offset = reader.position() as usize + total_names_length;
        let bytecodes_start_offset = sources_start_offset + total_sources_length;

        let mut sources_current_offset: usize = 0;
        let mut bytecodes_current_offset: usize = 0;

        for (name_length, source_length, bytecode_length, is_package) in index {
            let offset = reader.position() as usize;

            let name =
                unsafe { std::str::from_utf8_unchecked(&data[offset..offset + name_length]) };

            let source_offset = sources_start_offset + sources_current_offset;
            let source = if source_length > 0 {
                Some(&data[source_offset..source_offset + source_length])
            } else {
                None
            };

            let bytecode_offset = bytecodes_start_offset + bytecodes_current_offset;
            let bytecode = if bytecode_length > 0 {
                Some(&data[bytecode_offset..bytecode_offset + bytecode_length])
            } else {
                None
            };

            reader.set_position(offset as u64 + name_length as u64);

            sources_current_offset += source_length;
            bytecodes_current_offset += bytecode_length;

            if is_package {
                self.packages.insert(name);
            }

            // Extension modules will have their names present to populate the
            // packages set. So only populate module data if we have data for it.
            if source.is_some() || bytecode.is_some() {
                self.modules
                    .insert(name, PythonModuleLocation::InMemory { source, bytecode });
            }
        }

        Ok(())
    }

    pub fn load_resources_data(&mut self, data: &'static [u8]) -> Result<(), &'static str> {
        let mut reader = Cursor::new(data);

        let package_count = reader
            .read_u32::<LittleEndian>()
            .or_else(|_| Err("failed reading package count"))? as usize;

        let mut index = Vec::with_capacity(package_count);
        let mut total_names_length = 0;

        for _ in 0..package_count {
            let package_name_length = reader
                .read_u32::<LittleEndian>()
                .or_else(|_| Err("failed reading package name length"))?
                as usize;
            let resource_count = reader
                .read_u32::<LittleEndian>()
                .or_else(|_| Err("failed reading resource count"))?
                as usize;

            total_names_length += package_name_length;

            let mut package_index = Vec::with_capacity(resource_count);

            for _ in 0..resource_count {
                let resource_name_length = reader
                    .read_u32::<LittleEndian>()
                    .or_else(|_| Err("failed reading resource name length"))?
                    as usize;
                let resource_data_length = reader
                    .read_u32::<LittleEndian>()
                    .or_else(|_| Err("failed reading resource data length"))?
                    as usize;

                total_names_length += resource_name_length;

                package_index.push((resource_name_length, resource_data_length));
            }

            index.push((package_name_length, package_index));
        }

        let mut name_offset = reader.position() as usize;
        let mut data_offset = name_offset + total_names_length;

        for (package_name_length, package_index) in index {
            let package_name = unsafe {
                std::str::from_utf8_unchecked(&data[name_offset..name_offset + package_name_length])
            };

            name_offset += package_name_length;

            let mut package_data = HashMap::new();

            for (resource_name_length, resource_data_length) in package_index {
                let resource_name = unsafe {
                    std::str::from_utf8_unchecked(
                        &data[name_offset..name_offset + resource_name_length],
                    )
                };

                name_offset += resource_name_length;

                let resource_data = &data[data_offset..data_offset + resource_data_length];

                data_offset += resource_data_length;

                package_data.insert(resource_name, resource_data);
            }

            self.package_resources
                .insert(package_name, Arc::new(Box::new(package_data)));
        }

        Ok(())
    }
}
