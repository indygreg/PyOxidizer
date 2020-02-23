// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Embedded Python resources in a binary.
*/

use {
    super::bytecode::{BytecodeCompiler, CompileMode},
    super::filtering::{filter_btreemap, resolve_resource_names_from_files},
    super::resource::{
        packages_from_module_name, packages_from_module_names, BytecodeModule,
        BytecodeOptimizationLevel, DataLocation, ExtensionModuleData, ResourceData, SourceModule,
    },
    super::standalone_distribution::ExtensionModule,
    anyhow::{anyhow, Context, Result},
    byteorder::{LittleEndian, WriteBytesExt},
    lazy_static::lazy_static,
    slog::warn,
    std::collections::{BTreeMap, BTreeSet},
    std::convert::{TryFrom, TryInto},
    std::io::Write,
    std::path::Path,
};

lazy_static! {
    /// Python extension modules that should never be included.
    ///
    /// Ideally this data structure doesn't exist. But there are some problems
    /// with various extensions on various targets.
    pub static ref OS_IGNORE_EXTENSIONS: Vec<&'static str> = {
        let mut v = Vec::new();

        if cfg!(target_os = "linux") {
            // Linking issues.
            v.push("_crypt");

            // Linking issues.
            v.push("nis");
        }

        else if cfg!(target_os = "macos") {
            // curses and readline have linking issues.
            v.push("_curses");
            v.push("_curses_panel");
            v.push("readline");
        }

        v
    };
}

/// Header value for version 1 of resources payload.
const EMBEDDED_RESOURCES_HEADER_V1: &[u8] = b"pyembed\x01";

/// Describes a data field type in the embedded resources payload.
#[derive(Debug)]
pub enum EmbeddedPythonModuleField {
    EndOfIndex,
    StartOfEntry,
    EndOfEntry,
    ModuleName,
    IsPackage,
    IsNamespacePackage,
    InMemorySource,
    InMemoryBytecode,
    InMemoryBytecodeOpt1,
    InMemoryBytecodeOpt2,
    InMemoryExtensionModuleSharedLibrary,
    InMemoryResourcesData,
    InMemoryPackageDistribution,
}

impl Into<u8> for EmbeddedPythonModuleField {
    fn into(self) -> u8 {
        match self {
            EmbeddedPythonModuleField::EndOfIndex => 0,
            EmbeddedPythonModuleField::StartOfEntry => 1,
            EmbeddedPythonModuleField::EndOfEntry => 2,
            EmbeddedPythonModuleField::ModuleName => 3,
            EmbeddedPythonModuleField::IsPackage => 4,
            EmbeddedPythonModuleField::IsNamespacePackage => 5,
            EmbeddedPythonModuleField::InMemorySource => 6,
            EmbeddedPythonModuleField::InMemoryBytecode => 7,
            EmbeddedPythonModuleField::InMemoryBytecodeOpt1 => 8,
            EmbeddedPythonModuleField::InMemoryBytecodeOpt2 => 9,
            EmbeddedPythonModuleField::InMemoryExtensionModuleSharedLibrary => 10,
            EmbeddedPythonModuleField::InMemoryResourcesData => 11,
            EmbeddedPythonModuleField::InMemoryPackageDistribution => 12,
        }
    }
}

/// Represents a Python module and all its metadata.
///
/// All memory used by fields is held within each instance.
///
/// This type holds data required for serializing a Python module to the
/// embedded resources data structure. See the `pyembed` crate for more.
#[derive(Clone, Debug)]
pub struct EmbeddedResourcePythonModule {
    /// The module name.
    pub name: String,

    /// Whether the module is a package.
    pub is_package: bool,

    /// Whether the module is a namespace package.
    pub is_namespace_package: bool,

    /// Source code to use to import module from memory.
    pub in_memory_source: Option<Vec<u8>>,

    /// Bytecode to use to import module from memory.
    pub in_memory_bytecode: Option<Vec<u8>>,

    /// Bytecode at optimized level 1 to use to import from memory.
    pub in_memory_bytecode_opt1: Option<Vec<u8>>,

    /// Bytecode at optimized level 2 to use to import from memory.
    pub in_memory_bytecode_opt2: Option<Vec<u8>>,

    /// Native machine code constituting a shared library for an extension module
    /// which can be imported from memory. (Not supported on all platforms.)
    pub in_memory_extension_module_shared_library: Option<Vec<u8>>,

    /// Mapping of virtual filename to data for resources to expose to Python's
    /// `importlib.resources` API via in-memory data access.
    pub in_memory_resources: Option<BTreeMap<String, Vec<u8>>>,

    /// Mapping of virtual filename to data for package distribution metadata
    /// to expose to Python's `importlib.metadata` API via in-memory data access.
    pub in_memory_package_distribution: Option<BTreeMap<String, Vec<u8>>>,
}

impl Default for EmbeddedResourcePythonModule {
    fn default() -> Self {
        Self {
            name: "".to_string(),
            is_package: false,
            is_namespace_package: false,
            in_memory_source: None,
            in_memory_bytecode: None,
            in_memory_bytecode_opt1: None,
            in_memory_bytecode_opt2: None,
            in_memory_extension_module_shared_library: None,
            in_memory_resources: None,
            in_memory_package_distribution: None,
        }
    }
}

impl EmbeddedResourcePythonModule {
    /// Whether the module is meaningful.
    ///
    /// The module is meaningful if it has data attached or is a package.
    pub fn is_meaningful(&self) -> bool {
        self.is_package
            || self.is_namespace_package
            || self.in_memory_source.is_some()
            || self.in_memory_bytecode.is_some()
            || self.in_memory_bytecode_opt1.is_some()
            || self.in_memory_bytecode_opt2.is_some()
            || self.in_memory_extension_module_shared_library.is_some()
            || self.in_memory_resources.is_some()
            || self.in_memory_package_distribution.is_some()
    }

    /// Compute length of index entry for version 1 payload format.
    pub fn index_v1_length(&self) -> usize {
        // Start of index entry.
        let mut index = 1;

        // Module name field + module length.
        index += 3;

        if self.is_package {
            index += 1;
        }

        if self.is_namespace_package {
            index += 1;
        }

        if self.in_memory_source.is_some() {
            index += 5;
        }

        if self.in_memory_bytecode.is_some() {
            index += 5;
        }

        if self.in_memory_bytecode_opt1.is_some() {
            index += 5;
        }

        if self.in_memory_bytecode_opt2.is_some() {
            index += 5;
        }

        if self.in_memory_extension_module_shared_library.is_some() {
            index += 5;
        }

        if let Some(resources) = &self.in_memory_resources {
            index += 5;

            // u16 + u64 for resource name and data.
            index += 10 * resources.len();
        }

        if let Some(metadata) = &self.in_memory_package_distribution {
            index += 5;
            // Same as resources.
            index += 10 * metadata.len();
        }

        // End of index entry.
        index += 1;

        index
    }

    /// Compute the length of a field.
    pub fn field_blob_length(&self, field: EmbeddedPythonModuleField) -> usize {
        match field {
            EmbeddedPythonModuleField::EndOfIndex => 0,
            EmbeddedPythonModuleField::StartOfEntry => 0,
            EmbeddedPythonModuleField::EndOfEntry => 0,
            EmbeddedPythonModuleField::ModuleName => self.name.as_bytes().len(),
            EmbeddedPythonModuleField::IsPackage => 0,
            EmbeddedPythonModuleField::IsNamespacePackage => 0,
            EmbeddedPythonModuleField::InMemorySource => {
                if let Some(source) = &self.in_memory_source {
                    source.len()
                } else {
                    0
                }
            }
            EmbeddedPythonModuleField::InMemoryBytecode => {
                if let Some(bytecode) = &self.in_memory_bytecode {
                    bytecode.len()
                } else {
                    0
                }
            }
            EmbeddedPythonModuleField::InMemoryBytecodeOpt1 => {
                if let Some(bytecode) = &self.in_memory_bytecode_opt1 {
                    bytecode.len()
                } else {
                    0
                }
            }
            EmbeddedPythonModuleField::InMemoryBytecodeOpt2 => {
                if let Some(bytecode) = &self.in_memory_bytecode_opt2 {
                    bytecode.len()
                } else {
                    0
                }
            }
            EmbeddedPythonModuleField::InMemoryExtensionModuleSharedLibrary => {
                if let Some(library) = &self.in_memory_extension_module_shared_library {
                    library.len()
                } else {
                    0
                }
            }
            EmbeddedPythonModuleField::InMemoryResourcesData => {
                if let Some(resources) = &self.in_memory_resources {
                    resources
                        .iter()
                        .map(|(key, value)| key.as_bytes().len() + value.len())
                        .sum()
                } else {
                    0
                }
            }
            EmbeddedPythonModuleField::InMemoryPackageDistribution => {
                if let Some(metadata) = &self.in_memory_package_distribution {
                    metadata
                        .iter()
                        .map(|(key, value)| key.as_bytes().len() + value.len())
                        .sum()
                } else {
                    0
                }
            }
        }
    }

    /// Write the version 1 index entry for a module instance.
    pub fn write_index_v1<W: Write>(&self, dest: &mut W) -> Result<()> {
        let name_len =
            u16::try_from(self.name.as_bytes().len()).context("converting name to u16")?;

        dest.write_u8(EmbeddedPythonModuleField::StartOfEntry.into())
            .context("writing start of index entry")?;

        dest.write_u8(EmbeddedPythonModuleField::ModuleName.into())
            .context("writing module name field")?;

        dest.write_u16::<LittleEndian>(name_len)
            .context("writing module name length")?;

        if self.is_package {
            dest.write_u8(EmbeddedPythonModuleField::IsPackage.into())
                .context("writing is_package field")?;
        }

        if self.is_namespace_package {
            dest.write_u8(EmbeddedPythonModuleField::IsNamespacePackage.into())
                .context("writing is_namespace field")?;
        }

        if let Some(source) = &self.in_memory_source {
            let l =
                u32::try_from(source.len()).context("converting in-memory source length to u32")?;
            dest.write_u8(EmbeddedPythonModuleField::InMemorySource.into())
                .context("writing in-memory source length field")?;
            dest.write_u32::<LittleEndian>(l)
                .context("writing in-memory source length")?;
        }

        if let Some(bytecode) = &self.in_memory_bytecode {
            let l = u32::try_from(bytecode.len())
                .context("converting in-memory bytecode length to u32")?;
            dest.write_u8(EmbeddedPythonModuleField::InMemoryBytecode.into())
                .context("writing in-memory bytecode length field")?;
            dest.write_u32::<LittleEndian>(l)
                .context("writing in-memory bytecode length")?;
        }

        if let Some(bytecode) = &self.in_memory_bytecode_opt1 {
            let l = u32::try_from(bytecode.len())
                .context("converting in-memory bytecode opt 1 length to u32")?;
            dest.write_u8(EmbeddedPythonModuleField::InMemoryBytecodeOpt1.into())
                .context("writing in-memory bytecode opt 1 length field")?;
            dest.write_u32::<LittleEndian>(l)
                .context("writing in-memory bytecode opt 1 length")?;
        }

        if let Some(bytecode) = &self.in_memory_bytecode_opt2 {
            let l = u32::try_from(bytecode.len())
                .context("converting in-memory bytecode opt 2 length to u32")?;
            dest.write_u8(EmbeddedPythonModuleField::InMemoryBytecodeOpt2.into())
                .context("writing in-memory bytecode opt 2 field")?;
            dest.write_u32::<LittleEndian>(l)
                .context("writing in-memory bytecode opt 2 length")?;
        }

        if let Some(library) = &self.in_memory_extension_module_shared_library {
            let l = u32::try_from(library.len())
                .context("converting in-memory library length to u32")?;
            dest.write_u8(EmbeddedPythonModuleField::InMemoryExtensionModuleSharedLibrary.into())
                .context("writing in-memory extension module shared library field")?;
            dest.write_u32::<LittleEndian>(l)
                .context("writing in-memory shared library length")?;
        }

        if let Some(resources) = &self.in_memory_resources {
            let l = u32::try_from(resources.len())
                .context("converting in-memory resources data length to u32")?;
            dest.write_u8(EmbeddedPythonModuleField::InMemoryResourcesData.into())
                .context("writing in-memory resources field")?;
            dest.write_u32::<LittleEndian>(l)
                .context("writing in-memory resources data length")?;

            for (name, value) in resources {
                let name_length = u16::try_from(name.as_bytes().len())
                    .context("converting resource name length to u16")?;
                dest.write_u16::<LittleEndian>(name_length)
                    .context("writing resource name length")?;
                dest.write_u64::<LittleEndian>(value.len() as u64)
                    .context("writing resource data length")?;
            }
        }

        if let Some(metadata) = &self.in_memory_package_distribution {
            let l = u32::try_from(metadata.len())
                .context("converting in-memory distribution metadata length to u32")?;
            dest.write_u8(EmbeddedPythonModuleField::InMemoryPackageDistribution.into())
                .context("writing in-memory package distribution field")?;
            dest.write_u32::<LittleEndian>(l)
                .context("writing in-memory package distribution length")?;

            for (name, value) in metadata {
                let name_length = u16::try_from(name.as_bytes().len())
                    .context("converting distribution name length to u16")?;
                dest.write_u16::<LittleEndian>(name_length)
                    .context("writing distribution name length")?;
                dest.write_u64::<LittleEndian>(value.len() as u64)
                    .context("writing distribution data length")?;
            }
        }

        dest.write_u8(EmbeddedPythonModuleField::EndOfEntry.into())
            .or_else(|_| Err(anyhow!("error writing end of index entry")))?;

        Ok(())
    }
}

/// Write an embedded resources blob, version 1.
///
/// See the `pyembed` crate for the format of this data structure.
#[allow(clippy::cognitive_complexity)]
pub fn write_embedded_resources_v1<W: Write>(
    modules: &[EmbeddedResourcePythonModule],
    dest: &mut W,
) -> Result<()> {
    let mut blob_section_count = 0;
    // 1 for end of index field.
    let mut blob_index_length = 1;

    // 1 for end of index field.
    let mut module_index_length = 1;

    // TODO surely there's a better way to use the enum here to avoid the copy pasta.
    let mut module_name_length = 0;
    let mut in_memory_source_length = 0;
    let mut in_memory_bytecode_length = 0;
    let mut in_memory_bytecode_opt1_length = 0;
    let mut in_memory_bytecode_opt2_length = 0;
    let mut in_memory_extension_module_shared_library_length = 0;
    let mut in_memory_resources_data_length = 0;
    let mut in_memory_package_distribution_length = 0;

    for module in modules {
        module_index_length += module.index_v1_length();

        module_name_length += module.field_blob_length(EmbeddedPythonModuleField::ModuleName);
        in_memory_source_length +=
            module.field_blob_length(EmbeddedPythonModuleField::InMemorySource);
        in_memory_bytecode_length +=
            module.field_blob_length(EmbeddedPythonModuleField::InMemoryBytecode);
        in_memory_bytecode_opt1_length +=
            module.field_blob_length(EmbeddedPythonModuleField::InMemoryBytecodeOpt1);
        in_memory_bytecode_opt2_length +=
            module.field_blob_length(EmbeddedPythonModuleField::InMemoryBytecodeOpt2);
        in_memory_extension_module_shared_library_length += module
            .field_blob_length(EmbeddedPythonModuleField::InMemoryExtensionModuleSharedLibrary);
        in_memory_resources_data_length +=
            module.field_blob_length(EmbeddedPythonModuleField::InMemoryResourcesData);
        in_memory_package_distribution_length +=
            module.field_blob_length(EmbeddedPythonModuleField::InMemoryPackageDistribution);
    }

    const BLOB_INDEX_ENTRY_SIZE: usize = 9;

    if module_name_length > 0 {
        blob_index_length += BLOB_INDEX_ENTRY_SIZE;
        blob_section_count += 1;
    }
    if in_memory_source_length > 0 {
        blob_index_length += BLOB_INDEX_ENTRY_SIZE;
        blob_section_count += 1;
    }
    if in_memory_bytecode_length > 0 {
        blob_index_length += BLOB_INDEX_ENTRY_SIZE;
        blob_section_count += 1;
    }
    if in_memory_bytecode_opt1_length > 0 {
        blob_index_length += BLOB_INDEX_ENTRY_SIZE;
        blob_section_count += 1;
    }
    if in_memory_bytecode_opt2_length > 0 {
        blob_index_length += BLOB_INDEX_ENTRY_SIZE;
        blob_section_count += 1;
    }
    if in_memory_extension_module_shared_library_length > 0 {
        blob_index_length += BLOB_INDEX_ENTRY_SIZE;
        blob_section_count += 1;
    }
    if in_memory_resources_data_length > 0 {
        blob_index_length += BLOB_INDEX_ENTRY_SIZE;
        blob_section_count += 1;
    }
    if in_memory_package_distribution_length > 0 {
        blob_index_length += BLOB_INDEX_ENTRY_SIZE;
        blob_section_count += 1;
    }

    dest.write_all(EMBEDDED_RESOURCES_HEADER_V1)?;

    dest.write_u8(blob_section_count)?;
    dest.write_u32::<LittleEndian>(blob_index_length as u32)?;
    dest.write_u32::<LittleEndian>(modules.len() as u32)?;
    dest.write_u32::<LittleEndian>(module_index_length as u32)?;

    if module_name_length > 0 {
        dest.write_u8(EmbeddedPythonModuleField::ModuleName.into())?;
        dest.write_u64::<LittleEndian>(module_name_length.try_into().unwrap())?;
    }

    if in_memory_source_length > 0 {
        dest.write_u8(EmbeddedPythonModuleField::InMemorySource.into())?;
        dest.write_u64::<LittleEndian>(in_memory_source_length.try_into().unwrap())?;
    }

    if in_memory_bytecode_length > 0 {
        dest.write_u8(EmbeddedPythonModuleField::InMemoryBytecode.into())?;
        dest.write_u64::<LittleEndian>(in_memory_bytecode_length.try_into().unwrap())?;
    }

    if in_memory_bytecode_opt1_length > 0 {
        dest.write_u8(EmbeddedPythonModuleField::InMemoryBytecodeOpt1.into())?;
        dest.write_u64::<LittleEndian>(in_memory_bytecode_opt1_length.try_into().unwrap())?;
    }

    if in_memory_bytecode_opt2_length > 0 {
        dest.write_u8(EmbeddedPythonModuleField::InMemoryBytecodeOpt2.into())?;
        dest.write_u64::<LittleEndian>(in_memory_bytecode_opt2_length.try_into().unwrap())?;
    }

    if in_memory_extension_module_shared_library_length > 0 {
        dest.write_u8(EmbeddedPythonModuleField::InMemoryExtensionModuleSharedLibrary.into())?;
        dest.write_u64::<LittleEndian>(
            in_memory_extension_module_shared_library_length
                .try_into()
                .unwrap(),
        )?;
    }

    if in_memory_resources_data_length > 0 {
        dest.write_u8(EmbeddedPythonModuleField::InMemoryResourcesData.into())?;
        dest.write_u64::<LittleEndian>(in_memory_resources_data_length.try_into().unwrap())?;
    }

    if in_memory_package_distribution_length > 0 {
        dest.write_u8(EmbeddedPythonModuleField::InMemoryPackageDistribution.into())?;
        dest.write_u64::<LittleEndian>(in_memory_package_distribution_length.try_into().unwrap())?;
    }

    dest.write_u8(EmbeddedPythonModuleField::EndOfIndex.into())?;

    // Write the index entries.
    for module in modules {
        module.write_index_v1(dest)?;
    }

    dest.write_u8(EmbeddedPythonModuleField::EndOfIndex.into())?;

    // Write blob data, one field at a time.
    for module in modules {
        dest.write_all(module.name.as_bytes())?;
    }

    for module in modules {
        if let Some(data) = &module.in_memory_source {
            dest.write_all(data)?;
        }
    }

    for module in modules {
        if let Some(data) = &module.in_memory_bytecode {
            dest.write_all(data)?;
        }
    }

    for module in modules {
        if let Some(data) = &module.in_memory_bytecode_opt1 {
            dest.write_all(data)?;
        }
    }

    for module in modules {
        if let Some(data) = &module.in_memory_bytecode_opt2 {
            dest.write_all(data)?;
        }
    }

    for module in modules {
        if let Some(data) = &module.in_memory_extension_module_shared_library {
            dest.write_all(data)?;
        }
    }

    for module in modules {
        if let Some(resources) = &module.in_memory_resources {
            for (key, value) in resources {
                dest.write_all(key.as_bytes())?;
                dest.write_all(value)?;
            }
        }
    }

    for module in modules {
        if let Some(resources) = &module.in_memory_package_distribution {
            for (key, value) in resources {
                dest.write_all(key.as_bytes())?;
                dest.write_all(value)?;
            }
        }
    }

    Ok(())
}

/// Represents Python resources to embed in a binary.
///
/// This collection holds resources before packaging. This type is
/// transformed to `EmbeddedPythonResources` as part of packaging.
#[derive(Debug, Default, Clone)]
pub struct EmbeddedPythonResourcesPrePackaged {
    source_modules: BTreeMap<String, SourceModule>,
    bytecode_modules: BTreeMap<String, BytecodeModule>,
    pub resources: BTreeMap<String, BTreeMap<String, Vec<u8>>>,
    // TODO combine into single extension module type.
    pub extension_modules: BTreeMap<String, ExtensionModule>,
    pub extension_module_datas: BTreeMap<String, ExtensionModuleData>,
}

impl EmbeddedPythonResourcesPrePackaged {
    /// Obtain `SourceModule` in this instance.
    pub fn get_source_modules(&self) -> BTreeMap<String, SourceModule> {
        self.source_modules.clone()
    }

    pub fn get_bytecode_modules(&self) -> BTreeMap<String, BytecodeModule> {
        self.bytecode_modules.clone()
    }

    /// Add a source module to the collection of embedded source modules.
    pub fn add_source_module(&mut self, module: &SourceModule) {
        self.source_modules
            .insert(module.name.clone(), module.clone());

        // Automatically insert empty modules for missing parent packages.
        for package in packages_from_module_name(&module.name) {
            if !self.source_modules.contains_key(&package) {
                self.source_modules.insert(
                    package.clone(),
                    SourceModule {
                        name: package,
                        source: DataLocation::Memory(vec![]),
                        is_package: true,
                    },
                );
            }
        }
    }

    /// Add a bytecode module to the collection of embedded bytecode modules.
    pub fn add_bytecode_module(&mut self, module: &BytecodeModule) {
        self.bytecode_modules
            .insert(module.name.clone(), module.clone());

        // Automatically insert empty modules for missing parent packages.
        for package in packages_from_module_name(&module.name) {
            if !self.bytecode_modules.contains_key(&package) {
                self.bytecode_modules.insert(
                    package.clone(),
                    BytecodeModule {
                        name: package,
                        source: DataLocation::Memory(vec![]),
                        optimize_level: module.optimize_level,
                        is_package: true,
                    },
                );
            }
        }
    }

    /// Add resource data.
    ///
    /// Resource data belongs to a Python package and has a name and bytes data.
    pub fn add_resource(&mut self, resource: &ResourceData) {
        if !self.resources.contains_key(&resource.package) {
            self.resources
                .insert(resource.package.clone(), BTreeMap::new());
        }

        let inner = self.resources.get_mut(&resource.package).unwrap();
        inner.insert(resource.name.clone(), resource.data.resolve().unwrap());
    }

    /// Add an extension module.
    pub fn add_extension_module(&mut self, module: &ExtensionModule) {
        self.extension_modules
            .insert(module.module.clone(), module.clone());

        // Add empty bytecode for missing parent packages.
        // TODO should we choose source if we only have a specific module flavor?
        for package in packages_from_module_name(&module.module) {
            if !self.bytecode_modules.contains_key(&package) {
                self.bytecode_modules.insert(
                    package.clone(),
                    BytecodeModule {
                        name: package,
                        source: DataLocation::Memory(vec![]),
                        optimize_level: BytecodeOptimizationLevel::Zero,
                        is_package: true,
                    },
                );
            }
        }
    }

    /// Add an extension module.
    pub fn add_extension_module_data(&mut self, module: &ExtensionModuleData) {
        self.extension_module_datas
            .insert(module.name.clone(), module.clone());

        // Add empty bytecode for missing parent packages.
        // TODO should we choose source if we only have a specific module flavor?
        for package in packages_from_module_name(&module.name) {
            if !self.bytecode_modules.contains_key(&package) {
                self.bytecode_modules.insert(
                    package.clone(),
                    BytecodeModule {
                        name: package,
                        source: DataLocation::Memory(vec![]),
                        optimize_level: BytecodeOptimizationLevel::Zero,
                        is_package: true,
                    },
                );
            }
        }
    }

    /// Filter the entities in this instance against names in files.
    pub fn filter_from_files(
        &mut self,
        logger: &slog::Logger,
        files: &[&Path],
        glob_patterns: &[&str],
    ) -> Result<()> {
        let resource_names = resolve_resource_names_from_files(files, glob_patterns)?;

        warn!(logger, "filtering embedded extension modules");
        filter_btreemap(logger, &mut self.extension_modules, &resource_names);
        warn!(logger, "filtering embedded module sources");
        filter_btreemap(logger, &mut self.source_modules, &resource_names);
        warn!(logger, "filtering embedded module bytecode");
        filter_btreemap(logger, &mut self.bytecode_modules, &resource_names);
        warn!(logger, "filtering embedded resources");
        filter_btreemap(logger, &mut self.resources, &resource_names);

        Ok(())
    }

    /// Searches for embedded module sources for references to __file__.
    ///
    /// __file__ usage can be problematic for in-memory modules. This method searches
    /// for its occurrences and returns module names having it present.
    pub fn find_dunder_file(&self) -> Result<BTreeSet<String>> {
        let mut res = BTreeSet::new();

        for (name, module) in &self.source_modules {
            if module.has_dunder_file()? {
                res.insert(name.clone());
            }
        }

        for (name, module) in &self.bytecode_modules {
            if module.has_dunder_file()? {
                res.insert(name.clone());
            }
        }

        Ok(res)
    }

    /// Transform this instance into embedded resources data.
    ///
    /// This method performs actions necessary to produce entities which will allow the
    /// resources to be embedded in a binary.
    pub fn package(
        &self,
        logger: &slog::Logger,
        python_exe: &Path,
    ) -> Result<EmbeddedPythonResources> {
        let mut file_seen = false;
        for module in self.find_dunder_file()? {
            file_seen = true;
            warn!(logger, "warning: {} contains __file__", module);
        }

        if file_seen {
            warn!(logger, "__file__ was encountered in some embedded modules");
            warn!(
                logger,
                "PyOxidizer does not set __file__ and this may create problems at run-time"
            );
            warn!(
                logger,
                "See https://github.com/indygreg/PyOxidizer/issues/69 for more"
            );
        }

        let mut modules = BTreeMap::new();

        for (name, module) in &self.source_modules {
            if !modules.contains_key(name) {
                modules.insert(
                    name.clone(),
                    EmbeddedResourcePythonModule {
                        name: name.clone(),
                        ..EmbeddedResourcePythonModule::default()
                    },
                );
            }

            let mut entry = modules.get_mut(name).unwrap();

            if module.is_package {
                entry.is_package = true;
            }

            entry.in_memory_source = Some(module.source.resolve()?);
        }

        {
            let mut compiler = BytecodeCompiler::new(&python_exe)?;

            for (name, request) in &self.bytecode_modules {
                let bytecode = compiler.compile(
                    &request.source.resolve()?,
                    &request.name,
                    request.optimize_level,
                    CompileMode::Bytecode,
                )?;

                if !modules.contains_key(name) {
                    modules.insert(
                        name.clone(),
                        EmbeddedResourcePythonModule {
                            name: name.clone(),
                            ..EmbeddedResourcePythonModule::default()
                        },
                    );
                }

                let mut entry = modules.get_mut(name).unwrap();
                if request.is_package {
                    entry.is_package = true;
                }

                // TODO assign to proper field depending on bytecode level.
                entry.in_memory_bytecode = Some(bytecode);
            }
        }

        let ignored = OS_IGNORE_EXTENSIONS
            .iter()
            .map(|k| (*k).to_string())
            .collect::<Vec<String>>();

        let mut extension_modules = BTreeMap::new();
        for (name, em) in &self.extension_modules {
            if ignored.contains(name) {
                continue;
            }

            if !modules.contains_key(name) {
                modules.insert(
                    name.clone(),
                    EmbeddedResourcePythonModule {
                        name: name.clone(),
                        ..EmbeddedResourcePythonModule::default()
                    },
                );
            }

            extension_modules.insert(name.clone(), em.clone());
        }

        let mut built_extension_modules = BTreeMap::new();
        for (name, em) in &self.extension_module_datas {
            if ignored.contains(name) {
                continue;
            }

            if !modules.contains_key(name) {
                modules.insert(
                    name.clone(),
                    EmbeddedResourcePythonModule {
                        name: name.clone(),
                        ..EmbeddedResourcePythonModule::default()
                    },
                );
            }

            let mut entry = modules.get_mut(name).unwrap();

            if em.is_package {
                entry.is_package = true;
            }

            built_extension_modules.insert(name.clone(), em.clone());
        }

        for (package, resources) in &self.resources {
            if !modules.contains_key(package) {
                modules.insert(
                    package.clone(),
                    EmbeddedResourcePythonModule {
                        name: package.clone(),
                        ..EmbeddedResourcePythonModule::default()
                    },
                );
            }

            let mut entry = modules.get_mut(package).unwrap();

            // For a module to contain resources, it must be a package.
            entry.is_package = true;

            entry.in_memory_resources = Some(resources.clone());
        }

        let derived_package_names = packages_from_module_names(modules.keys().cloned());

        for package in derived_package_names {
            if !modules.contains_key(&package) {
                modules.insert(
                    package.clone(),
                    EmbeddedResourcePythonModule {
                        name: package.clone(),
                        ..EmbeddedResourcePythonModule::default()
                    },
                );
            }

            let mut entry = modules.get_mut(&package).unwrap();

            if !entry.is_package {
                warn!(
                    logger,
                    "package {} not initially detected as such; possible package detection bug",
                    package
                );
                entry.is_package = true;
            }
        }

        Ok(EmbeddedPythonResources {
            modules,
            extension_modules,
            built_extension_modules,
        })
    }
}

/// Represents Python resources to embed in a binary.
#[derive(Debug, Default, Clone)]
pub struct EmbeddedPythonResources {
    /// Python modules described by an embeddable resource.
    pub modules: BTreeMap<String, EmbeddedResourcePythonModule>,

    // TODO combine the extension module types.
    pub extension_modules: BTreeMap<String, ExtensionModule>,
    pub built_extension_modules: BTreeMap<String, ExtensionModuleData>,
}

impl EmbeddedPythonResources {
    pub fn write_blobs<W: Write>(&self, module_names: &mut W, resources: &mut W) {
        for name in self.modules.keys() {
            module_names
                .write_all(name.as_bytes())
                .expect("failed to write");
            module_names.write_all(b"\n").expect("failed to write");
        }

        write_embedded_resources_v1(
            &self
                .modules
                .values()
                .cloned()
                .collect::<Vec<EmbeddedResourcePythonModule>>(),
            resources,
        )
        .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_source_module() {
        let mut r = EmbeddedPythonResourcesPrePackaged::default();
        r.add_source_module(&SourceModule {
            name: "foo".to_string(),
            source: DataLocation::Memory(vec![42]),
            is_package: false,
        });

        assert!(r.source_modules.contains_key("foo"));
        assert_eq!(
            r.source_modules.get("foo"),
            Some(&SourceModule {
                name: "foo".to_string(),
                source: DataLocation::Memory(vec![42]),
                is_package: false,
            })
        );
    }

    #[test]
    fn test_add_source_module_parents() {
        let mut r = EmbeddedPythonResourcesPrePackaged::default();
        r.add_source_module(&SourceModule {
            name: "root.parent.child".to_string(),
            source: DataLocation::Memory(vec![42]),
            is_package: true,
        });

        assert_eq!(r.source_modules.len(), 3);
        assert_eq!(
            r.source_modules.get("root.parent.child"),
            Some(&SourceModule {
                name: "root.parent.child".to_string(),
                source: DataLocation::Memory(vec![42]),
                is_package: true,
            })
        );
        assert_eq!(
            r.source_modules.get("root.parent"),
            Some(&SourceModule {
                name: "root.parent".to_string(),
                source: DataLocation::Memory(vec![]),
                is_package: true,
            })
        );
        assert_eq!(
            r.source_modules.get("root"),
            Some(&SourceModule {
                name: "root".to_string(),
                source: DataLocation::Memory(vec![]),
                is_package: true,
            })
        );
    }

    #[test]
    fn test_add_bytecode_module() {
        let mut r = EmbeddedPythonResourcesPrePackaged::default();
        r.add_bytecode_module(&BytecodeModule {
            name: "foo".to_string(),
            source: DataLocation::Memory(vec![42]),
            optimize_level: BytecodeOptimizationLevel::Zero,
            is_package: false,
        });

        assert!(r.bytecode_modules.contains_key("foo"));
        assert_eq!(
            r.bytecode_modules.get("foo"),
            Some(&BytecodeModule {
                name: "foo".to_string(),
                source: DataLocation::Memory(vec![42]),
                optimize_level: BytecodeOptimizationLevel::Zero,
                is_package: false,
            })
        );
    }

    #[test]
    fn test_add_bytecode_module_parents() {
        let mut r = EmbeddedPythonResourcesPrePackaged::default();
        r.add_bytecode_module(&BytecodeModule {
            name: "root.parent.child".to_string(),
            source: DataLocation::Memory(vec![42]),
            optimize_level: BytecodeOptimizationLevel::One,
            is_package: true,
        });

        assert_eq!(r.bytecode_modules.len(), 3);
        assert_eq!(
            r.bytecode_modules.get("root.parent.child"),
            Some(&BytecodeModule {
                name: "root.parent.child".to_string(),
                source: DataLocation::Memory(vec![42]),
                optimize_level: BytecodeOptimizationLevel::One,
                is_package: true,
            })
        );
        assert_eq!(
            r.bytecode_modules.get("root.parent"),
            Some(&BytecodeModule {
                name: "root.parent".to_string(),
                source: DataLocation::Memory(vec![]),
                optimize_level: BytecodeOptimizationLevel::One,
                is_package: true,
            })
        );
        assert_eq!(
            r.bytecode_modules.get("root"),
            Some(&BytecodeModule {
                name: "root".to_string(),
                source: DataLocation::Memory(vec![]),
                optimize_level: BytecodeOptimizationLevel::One,
                is_package: true,
            })
        );
    }

    #[test]
    fn test_add_resource() {
        let mut r = EmbeddedPythonResourcesPrePackaged::default();
        r.add_resource(&ResourceData {
            package: "foo".to_string(),
            name: "resource.txt".to_string(),
            data: DataLocation::Memory(vec![42]),
        });

        assert_eq!(r.resources.len(), 1);
        assert!(r.resources.contains_key("foo"));

        let foo = r.resources.get("foo").unwrap();
        assert_eq!(foo.len(), 1);
        assert_eq!(foo.get("resource.txt"), Some(&vec![42]));
    }

    #[test]
    fn test_add_extension_module() {
        let mut r = EmbeddedPythonResourcesPrePackaged::default();
        let em = ExtensionModule {
            module: "foo.bar".to_string(),
            init_fn: None,
            builtin_default: false,
            disableable: false,
            object_paths: vec![],
            static_library: None,
            shared_library: None,
            links: vec![],
            required: false,
            variant: "".to_string(),
            licenses: None,
            license_paths: None,
            license_public_domain: None,
        };

        r.add_extension_module(&em);
        assert_eq!(r.extension_modules.len(), 1);
        assert_eq!(r.extension_modules.get("foo.bar"), Some(&em));

        assert_eq!(r.bytecode_modules.len(), 1);
        assert_eq!(
            r.bytecode_modules.get("foo"),
            Some(&BytecodeModule {
                name: "foo".to_string(),
                source: DataLocation::Memory(vec![]),
                optimize_level: BytecodeOptimizationLevel::Zero,
                is_package: true
            })
        );
    }

    #[test]
    fn test_add_extension_module_data() {
        let mut r = EmbeddedPythonResourcesPrePackaged::default();
        let em = ExtensionModuleData {
            name: "foo.bar".to_string(),
            init_fn: Some("".to_string()),
            extension_file_suffix: "".to_string(),
            extension_data: None,
            object_file_data: vec![],
            is_package: false,
            libraries: vec![],
            library_dirs: vec![],
        };

        r.add_extension_module_data(&em);
        assert_eq!(r.extension_module_datas.len(), 1);
        assert_eq!(r.extension_module_datas.get("foo.bar"), Some(&em));

        assert_eq!(r.bytecode_modules.len(), 1);
        assert_eq!(
            r.bytecode_modules.get("foo"),
            Some(&BytecodeModule {
                name: "foo".to_string(),
                source: DataLocation::Memory(vec![]),
                optimize_level: BytecodeOptimizationLevel::Zero,
                is_package: true,
            })
        );
    }

    #[test]
    fn test_find_dunder_file() -> Result<()> {
        let mut r = EmbeddedPythonResourcesPrePackaged::default();
        assert_eq!(r.find_dunder_file()?.len(), 0);

        r.add_source_module(&SourceModule {
            name: "foo.bar".to_string(),
            source: DataLocation::Memory(vec![]),
            is_package: false,
        });
        assert_eq!(r.find_dunder_file()?.len(), 0);

        r.add_source_module(&SourceModule {
            name: "baz".to_string(),
            source: DataLocation::Memory(Vec::from("import foo; if __file__ == 'ignored'")),
            is_package: false,
        });
        assert_eq!(r.find_dunder_file()?.len(), 1);
        assert!(r.find_dunder_file()?.contains("baz"));

        r.add_bytecode_module(&BytecodeModule {
            name: "bytecode".to_string(),
            source: DataLocation::Memory(Vec::from("import foo; if __file__")),
            optimize_level: BytecodeOptimizationLevel::Zero,
            is_package: false,
        });
        assert_eq!(r.find_dunder_file()?.len(), 2);
        assert!(r.find_dunder_file()?.contains("bytecode"));

        Ok(())
    }

    #[test]
    fn test_write_empty() -> Result<()> {
        let mut data = Vec::new();
        write_embedded_resources_v1(&[], &mut data)?;

        let mut expected: Vec<u8> = b"pyembed\x01".to_vec();
        // Number of blob sections.
        expected.write_u8(0)?;
        // Length of blob index (end of index marker).
        expected.write_u32::<LittleEndian>(1)?;
        // Number of modules.
        expected.write_u32::<LittleEndian>(0)?;
        // Lenght of index (end of index marker).
        expected.write_u32::<LittleEndian>(1)?;
        // End of index for blob and modules.
        expected.write_u8(0)?;
        expected.write_u8(0)?;

        assert_eq!(data, expected);

        Ok(())
    }

    #[test]
    fn test_write_module_name() -> Result<()> {
        let mut data = Vec::new();
        let module = EmbeddedResourcePythonModule {
            name: "foo".to_string(),
            ..EmbeddedResourcePythonModule::default()
        };

        write_embedded_resources_v1(&[module], &mut data)?;

        let mut expected: Vec<u8> = b"pyembed\x01".to_vec();
        // Number of blob sections.
        expected.write_u8(1)?;
        // Length of blob index. Field, length, end of index.
        expected.write_u32::<LittleEndian>(1 + 8 + 1)?;
        // Number of modules.
        expected.write_u32::<LittleEndian>(1)?;
        // Length of index. Start of entry, module name length field, module name length, end of
        // entry, end of index.
        expected.write_u32::<LittleEndian>(1 + 1 + 2 + 1 + 1)?;
        // Blobs index. Module names field, module names length, end of index.
        expected.write_u8(EmbeddedPythonModuleField::ModuleName.into())?;
        expected.write_u64::<LittleEndian>(b"foo".len() as u64)?;
        expected.write_u8(EmbeddedPythonModuleField::EndOfIndex.into())?;
        // Module index.
        expected.write_u8(EmbeddedPythonModuleField::StartOfEntry.into())?;
        expected.write_u8(EmbeddedPythonModuleField::ModuleName.into())?;
        expected.write_u16::<LittleEndian>(b"foo".len() as u16)?;
        expected.write_u8(EmbeddedPythonModuleField::EndOfEntry.into())?;
        expected.write_u8(EmbeddedPythonModuleField::EndOfIndex.into())?;
        expected.write_all(b"foo")?;

        assert_eq!(data, expected);

        Ok(())
    }
}
