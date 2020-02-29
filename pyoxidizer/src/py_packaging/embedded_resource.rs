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
        has_dunder_file, packages_from_module_name, packages_from_module_names, BytecodeModule,
        BytecodeOptimizationLevel, DataLocation, ExtensionModuleData, ResourceData, SourceModule,
    },
    super::standalone_distribution::ExtensionModule,
    anyhow::{anyhow, Context, Error, Result},
    byteorder::{LittleEndian, WriteBytesExt},
    lazy_static::lazy_static,
    slog::warn,
    std::collections::{BTreeMap, BTreeSet},
    std::convert::{TryFrom, TryInto},
    std::io::Write,
    std::iter::FromIterator,
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
pub enum EmbeddedResourceField {
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
    InMemorySharedLibrary,
    SharedLibraryDependencyNames,
}

impl Into<u8> for EmbeddedResourceField {
    fn into(self) -> u8 {
        match self {
            EmbeddedResourceField::EndOfIndex => 0,
            EmbeddedResourceField::StartOfEntry => 1,
            EmbeddedResourceField::EndOfEntry => 2,
            EmbeddedResourceField::ModuleName => 3,
            EmbeddedResourceField::IsPackage => 4,
            EmbeddedResourceField::IsNamespacePackage => 5,
            EmbeddedResourceField::InMemorySource => 6,
            EmbeddedResourceField::InMemoryBytecode => 7,
            EmbeddedResourceField::InMemoryBytecodeOpt1 => 8,
            EmbeddedResourceField::InMemoryBytecodeOpt2 => 9,
            EmbeddedResourceField::InMemoryExtensionModuleSharedLibrary => 10,
            EmbeddedResourceField::InMemoryResourcesData => 11,
            EmbeddedResourceField::InMemoryPackageDistribution => 12,
            EmbeddedResourceField::InMemorySharedLibrary => 13,
            EmbeddedResourceField::SharedLibraryDependencyNames => 14,
        }
    }
}

/// Represents an embedded resource and all its metadata.
///
/// All memory used by fields is held within each instance.
///
/// This type holds data required for serializing a resource to the
/// embedded resources data structure. See the `pyembed` crate for more.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EmbeddedResource {
    /// The resource name.
    pub name: String,

    /// Whether the Python module is a package.
    pub is_package: bool,

    /// Whether the Python module is a namespace package.
    pub is_namespace_package: bool,

    /// Python module source code to use to import module from memory.
    pub in_memory_source: Option<Vec<u8>>,

    /// Python module bytecode to use to import module from memory.
    pub in_memory_bytecode: Option<Vec<u8>>,

    /// Python module bytecode at optimized level 1 to use to import from memory.
    pub in_memory_bytecode_opt1: Option<Vec<u8>>,

    /// Python module bytecode at optimized level 2 to use to import from memory.
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

    /// Native machine code constituting a shared library which can be imported from memory.
    ///
    /// In-memory loading of shared libraries is not supported on all platforms.
    pub in_memory_shared_library: Option<Vec<u8>>,

    /// Sequence of names of shared libraries this resource depends on.
    pub shared_library_dependency_names: Option<Vec<String>>,
}

impl EmbeddedResource {
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
            || self.in_memory_shared_library.is_some()
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

        if self.in_memory_shared_library.is_some() {
            index += 9;
        }

        if let Some(names) = &self.shared_library_dependency_names {
            index += 3 + 2 * names.len();
        }

        // End of index entry.
        index += 1;

        index
    }

    /// Compute the length of a field.
    pub fn field_blob_length(&self, field: EmbeddedResourceField) -> usize {
        match field {
            EmbeddedResourceField::EndOfIndex => 0,
            EmbeddedResourceField::StartOfEntry => 0,
            EmbeddedResourceField::EndOfEntry => 0,
            EmbeddedResourceField::ModuleName => self.name.as_bytes().len(),
            EmbeddedResourceField::IsPackage => 0,
            EmbeddedResourceField::IsNamespacePackage => 0,
            EmbeddedResourceField::InMemorySource => {
                if let Some(source) = &self.in_memory_source {
                    source.len()
                } else {
                    0
                }
            }
            EmbeddedResourceField::InMemoryBytecode => {
                if let Some(bytecode) = &self.in_memory_bytecode {
                    bytecode.len()
                } else {
                    0
                }
            }
            EmbeddedResourceField::InMemoryBytecodeOpt1 => {
                if let Some(bytecode) = &self.in_memory_bytecode_opt1 {
                    bytecode.len()
                } else {
                    0
                }
            }
            EmbeddedResourceField::InMemoryBytecodeOpt2 => {
                if let Some(bytecode) = &self.in_memory_bytecode_opt2 {
                    bytecode.len()
                } else {
                    0
                }
            }
            EmbeddedResourceField::InMemoryExtensionModuleSharedLibrary => {
                if let Some(library) = &self.in_memory_extension_module_shared_library {
                    library.len()
                } else {
                    0
                }
            }
            EmbeddedResourceField::InMemoryResourcesData => {
                if let Some(resources) = &self.in_memory_resources {
                    resources
                        .iter()
                        .map(|(key, value)| key.as_bytes().len() + value.len())
                        .sum()
                } else {
                    0
                }
            }
            EmbeddedResourceField::InMemoryPackageDistribution => {
                if let Some(metadata) = &self.in_memory_package_distribution {
                    metadata
                        .iter()
                        .map(|(key, value)| key.as_bytes().len() + value.len())
                        .sum()
                } else {
                    0
                }
            }
            EmbeddedResourceField::InMemorySharedLibrary => {
                if let Some(library) = &self.in_memory_shared_library {
                    library.len()
                } else {
                    0
                }
            }
            EmbeddedResourceField::SharedLibraryDependencyNames => {
                if let Some(names) = &self.shared_library_dependency_names {
                    names.iter().map(|s| s.as_bytes().len()).sum()
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

        dest.write_u8(EmbeddedResourceField::StartOfEntry.into())
            .context("writing start of index entry")?;

        dest.write_u8(EmbeddedResourceField::ModuleName.into())
            .context("writing module name field")?;

        dest.write_u16::<LittleEndian>(name_len)
            .context("writing module name length")?;

        if self.is_package {
            dest.write_u8(EmbeddedResourceField::IsPackage.into())
                .context("writing is_package field")?;
        }

        if self.is_namespace_package {
            dest.write_u8(EmbeddedResourceField::IsNamespacePackage.into())
                .context("writing is_namespace field")?;
        }

        if let Some(source) = &self.in_memory_source {
            let l =
                u32::try_from(source.len()).context("converting in-memory source length to u32")?;
            dest.write_u8(EmbeddedResourceField::InMemorySource.into())
                .context("writing in-memory source length field")?;
            dest.write_u32::<LittleEndian>(l)
                .context("writing in-memory source length")?;
        }

        if let Some(bytecode) = &self.in_memory_bytecode {
            let l = u32::try_from(bytecode.len())
                .context("converting in-memory bytecode length to u32")?;
            dest.write_u8(EmbeddedResourceField::InMemoryBytecode.into())
                .context("writing in-memory bytecode length field")?;
            dest.write_u32::<LittleEndian>(l)
                .context("writing in-memory bytecode length")?;
        }

        if let Some(bytecode) = &self.in_memory_bytecode_opt1 {
            let l = u32::try_from(bytecode.len())
                .context("converting in-memory bytecode opt 1 length to u32")?;
            dest.write_u8(EmbeddedResourceField::InMemoryBytecodeOpt1.into())
                .context("writing in-memory bytecode opt 1 length field")?;
            dest.write_u32::<LittleEndian>(l)
                .context("writing in-memory bytecode opt 1 length")?;
        }

        if let Some(bytecode) = &self.in_memory_bytecode_opt2 {
            let l = u32::try_from(bytecode.len())
                .context("converting in-memory bytecode opt 2 length to u32")?;
            dest.write_u8(EmbeddedResourceField::InMemoryBytecodeOpt2.into())
                .context("writing in-memory bytecode opt 2 field")?;
            dest.write_u32::<LittleEndian>(l)
                .context("writing in-memory bytecode opt 2 length")?;
        }

        if let Some(library) = &self.in_memory_extension_module_shared_library {
            let l = u32::try_from(library.len())
                .context("converting in-memory library length to u32")?;
            dest.write_u8(EmbeddedResourceField::InMemoryExtensionModuleSharedLibrary.into())
                .context("writing in-memory extension module shared library field")?;
            dest.write_u32::<LittleEndian>(l)
                .context("writing in-memory extension module shared library length")?;
        }

        if let Some(resources) = &self.in_memory_resources {
            let l = u32::try_from(resources.len())
                .context("converting in-memory resources data length to u32")?;
            dest.write_u8(EmbeddedResourceField::InMemoryResourcesData.into())
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
            dest.write_u8(EmbeddedResourceField::InMemoryPackageDistribution.into())
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

        if let Some(library) = &self.in_memory_shared_library {
            let l = u64::try_from(library.len())
                .context("converting in-memory shared library length to u64")?;
            dest.write_u8(EmbeddedResourceField::InMemorySharedLibrary.into())
                .context("writing in-memory shared library field")?;
            dest.write_u64::<LittleEndian>(l)
                .context("writing in-memory shared library length")?;
        }

        if let Some(names) = &self.shared_library_dependency_names {
            let l = u16::try_from(names.len())
                .context("converting shared library dependency names to u16")?;
            dest.write_u8(EmbeddedResourceField::SharedLibraryDependencyNames.into())
                .context("writing shared library dependency names field")?;
            dest.write_u16::<LittleEndian>(l)
                .context("writing shared library dependency names length")?;

            for name in names {
                let name_length = u16::try_from(name.as_bytes().len())
                    .context("converting shared library dependency name length to u16")?;
                dest.write_u16::<LittleEndian>(name_length)
                    .context("writing shared library dependency name length")?;
            }
        }

        dest.write_u8(EmbeddedResourceField::EndOfEntry.into())
            .or_else(|_| Err(anyhow!("error writing end of index entry")))?;

        Ok(())
    }
}

/// Write an embedded resources blob, version 1.
///
/// See the `pyembed` crate for the format of this data structure.
#[allow(clippy::cognitive_complexity)]
pub fn write_embedded_resources_v1<W: Write>(
    modules: &[EmbeddedResource],
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
    let mut in_memory_shared_library_length = 0;
    let mut shared_library_dependency_names_length = 0;

    for module in modules {
        module_index_length += module.index_v1_length();

        module_name_length += module.field_blob_length(EmbeddedResourceField::ModuleName);
        in_memory_source_length += module.field_blob_length(EmbeddedResourceField::InMemorySource);
        in_memory_bytecode_length +=
            module.field_blob_length(EmbeddedResourceField::InMemoryBytecode);
        in_memory_bytecode_opt1_length +=
            module.field_blob_length(EmbeddedResourceField::InMemoryBytecodeOpt1);
        in_memory_bytecode_opt2_length +=
            module.field_blob_length(EmbeddedResourceField::InMemoryBytecodeOpt2);
        in_memory_extension_module_shared_library_length +=
            module.field_blob_length(EmbeddedResourceField::InMemoryExtensionModuleSharedLibrary);
        in_memory_resources_data_length +=
            module.field_blob_length(EmbeddedResourceField::InMemoryResourcesData);
        in_memory_package_distribution_length +=
            module.field_blob_length(EmbeddedResourceField::InMemoryPackageDistribution);
        in_memory_shared_library_length +=
            module.field_blob_length(EmbeddedResourceField::InMemorySharedLibrary);
        shared_library_dependency_names_length +=
            module.field_blob_length(EmbeddedResourceField::SharedLibraryDependencyNames);
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
    if in_memory_shared_library_length > 0 {
        blob_index_length += BLOB_INDEX_ENTRY_SIZE;
        blob_section_count += 1;
    }
    if shared_library_dependency_names_length > 0 {
        blob_index_length += BLOB_INDEX_ENTRY_SIZE;
        blob_section_count += 1;
    }

    dest.write_all(EMBEDDED_RESOURCES_HEADER_V1)?;

    dest.write_u8(blob_section_count)?;
    dest.write_u32::<LittleEndian>(blob_index_length as u32)?;
    dest.write_u32::<LittleEndian>(modules.len() as u32)?;
    dest.write_u32::<LittleEndian>(module_index_length as u32)?;

    if module_name_length > 0 {
        dest.write_u8(EmbeddedResourceField::ModuleName.into())?;
        dest.write_u64::<LittleEndian>(module_name_length.try_into().unwrap())?;
    }

    if in_memory_source_length > 0 {
        dest.write_u8(EmbeddedResourceField::InMemorySource.into())?;
        dest.write_u64::<LittleEndian>(in_memory_source_length.try_into().unwrap())?;
    }

    if in_memory_bytecode_length > 0 {
        dest.write_u8(EmbeddedResourceField::InMemoryBytecode.into())?;
        dest.write_u64::<LittleEndian>(in_memory_bytecode_length.try_into().unwrap())?;
    }

    if in_memory_bytecode_opt1_length > 0 {
        dest.write_u8(EmbeddedResourceField::InMemoryBytecodeOpt1.into())?;
        dest.write_u64::<LittleEndian>(in_memory_bytecode_opt1_length.try_into().unwrap())?;
    }

    if in_memory_bytecode_opt2_length > 0 {
        dest.write_u8(EmbeddedResourceField::InMemoryBytecodeOpt2.into())?;
        dest.write_u64::<LittleEndian>(in_memory_bytecode_opt2_length.try_into().unwrap())?;
    }

    if in_memory_extension_module_shared_library_length > 0 {
        dest.write_u8(EmbeddedResourceField::InMemoryExtensionModuleSharedLibrary.into())?;
        dest.write_u64::<LittleEndian>(
            in_memory_extension_module_shared_library_length
                .try_into()
                .unwrap(),
        )?;
    }

    if in_memory_resources_data_length > 0 {
        dest.write_u8(EmbeddedResourceField::InMemoryResourcesData.into())?;
        dest.write_u64::<LittleEndian>(in_memory_resources_data_length.try_into().unwrap())?;
    }

    if in_memory_package_distribution_length > 0 {
        dest.write_u8(EmbeddedResourceField::InMemoryPackageDistribution.into())?;
        dest.write_u64::<LittleEndian>(in_memory_package_distribution_length.try_into().unwrap())?;
    }

    if in_memory_shared_library_length > 0 {
        dest.write_u8(EmbeddedResourceField::InMemorySharedLibrary.into())?;
        dest.write_u64::<LittleEndian>(in_memory_shared_library_length.try_into().unwrap())?;
    }

    if shared_library_dependency_names_length > 0 {
        dest.write_u8(EmbeddedResourceField::SharedLibraryDependencyNames.into())?;
        dest.write_u64::<LittleEndian>(shared_library_dependency_names_length.try_into().unwrap())?;
    }

    dest.write_u8(EmbeddedResourceField::EndOfIndex.into())?;

    // Write the index entries.
    for module in modules {
        module.write_index_v1(dest)?;
    }

    dest.write_u8(EmbeddedResourceField::EndOfIndex.into())?;

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

    for module in modules {
        if let Some(data) = &module.in_memory_shared_library {
            dest.write_all(data)?;
        }
    }

    for module in modules {
        if let Some(names) = &module.shared_library_dependency_names {
            for name in names {
                dest.write_all(name.as_bytes())?;
            }
        }
    }

    Ok(())
}

/// Represents an embedded Python module resource entry before it is packaged.
///
/// Instances hold the same fields as `EmbeddedResourcePythonModule` except
/// content backing fields is a `DataLocation` instead of `Vec<u8>`, since
/// it may not be available yet.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EmbeddedResourcePythonModulePrePackaged {
    pub name: String,
    pub is_package: bool,
    pub is_namespace_package: bool,
    pub in_memory_source: Option<DataLocation>,
    // This is actually source code to be compiled to bytecode.
    pub in_memory_bytecode: Option<DataLocation>,
    pub in_memory_bytecode_opt1: Option<DataLocation>,
    pub in_memory_bytecode_opt2: Option<DataLocation>,
    pub in_memory_extension_module_shared_library: Option<DataLocation>,
    pub in_memory_resources: Option<BTreeMap<String, DataLocation>>,
    pub in_memory_package_distribution: Option<BTreeMap<String, DataLocation>>,
    pub in_memory_shared_library: Option<DataLocation>,
    pub shared_library_dependency_names: Option<Vec<String>>,
}

impl TryFrom<&EmbeddedResourcePythonModulePrePackaged> for EmbeddedResource {
    type Error = Error;

    fn try_from(value: &EmbeddedResourcePythonModulePrePackaged) -> Result<Self, Self::Error> {
        Ok(Self {
            name: value.name.clone(),
            is_package: value.is_package,
            is_namespace_package: value.is_namespace_package,
            in_memory_source: if let Some(location) = &value.in_memory_source {
                Some(location.resolve()?)
            } else {
                None
            },
            // Stored data is source, not bytecode. So don't populate bytecode with
            // wrong data type.
            in_memory_bytecode: None,
            in_memory_bytecode_opt1: None,
            in_memory_bytecode_opt2: None,
            in_memory_extension_module_shared_library: if let Some(location) =
                &value.in_memory_extension_module_shared_library
            {
                Some(location.resolve()?)
            } else {
                None
            },
            in_memory_resources: if let Some(resources) = &value.in_memory_resources {
                let mut res = BTreeMap::new();
                for (key, location) in resources {
                    res.insert(key.clone(), location.resolve()?);
                }
                Some(res)
            } else {
                None
            },
            in_memory_package_distribution: if let Some(resources) =
                &value.in_memory_package_distribution
            {
                let mut res = BTreeMap::new();
                for (key, location) in resources {
                    res.insert(key.clone(), location.resolve()?);
                }
                Some(res)
            } else {
                None
            },
            in_memory_shared_library: if let Some(location) = &value.in_memory_shared_library {
                Some(location.resolve()?)
            } else {
                None
            },
            shared_library_dependency_names: if let Some(names) =
                &value.shared_library_dependency_names
            {
                Some(names.clone())
            } else {
                None
            },
        })
    }
}

/// Represents Python resources to embed in a binary.
///
/// This collection holds resources before packaging. This type is
/// transformed to `EmbeddedPythonResources` as part of packaging.
#[derive(Debug, Default, Clone)]
pub struct EmbeddedPythonResourcesPrePackaged {
    modules: BTreeMap<String, EmbeddedResourcePythonModulePrePackaged>,

    // TODO combine into single extension module type.
    extension_modules: BTreeMap<String, ExtensionModule>,
    extension_module_datas: BTreeMap<String, ExtensionModuleData>,
}

impl EmbeddedPythonResourcesPrePackaged {
    /// Obtain `SourceModule` in this instance.
    pub fn get_source_modules(&self) -> BTreeMap<String, SourceModule> {
        BTreeMap::from_iter(self.modules.iter().filter_map(|(name, module)| {
            if let Some(location) = &module.in_memory_source {
                Some((
                    name.clone(),
                    SourceModule {
                        name: name.clone(),
                        is_package: module.is_package,
                        source: location.clone(),
                    },
                ))
            } else {
                None
            }
        }))
    }

    /// Obtain `BytecodeModule` in this instance.
    pub fn get_bytecode_modules(&self) -> BTreeMap<String, BytecodeModule> {
        BTreeMap::from_iter(self.modules.iter().filter_map(|(name, module)| {
            if let Some(location) = &module.in_memory_bytecode {
                Some((
                    name.clone(),
                    BytecodeModule {
                        name: name.clone(),
                        is_package: module.is_package,
                        source: location.clone(),
                        optimize_level: BytecodeOptimizationLevel::Zero,
                    },
                ))
            } else if let Some(location) = &module.in_memory_bytecode_opt1 {
                Some((
                    name.clone(),
                    BytecodeModule {
                        name: name.clone(),
                        is_package: module.is_package,
                        source: location.clone(),
                        optimize_level: BytecodeOptimizationLevel::One,
                    },
                ))
            } else if let Some(location) = &module.in_memory_bytecode_opt2 {
                Some((
                    name.clone(),
                    BytecodeModule {
                        name: name.clone(),
                        is_package: module.is_package,
                        source: location.clone(),
                        optimize_level: BytecodeOptimizationLevel::Two,
                    },
                ))
            } else {
                None
            }
        }))
    }

    /// Obtain resource files in this instance.
    pub fn get_resources(&self) -> BTreeMap<String, BTreeMap<String, Vec<u8>>> {
        BTreeMap::from_iter(self.modules.iter().filter_map(|(name, module)| {
            if let Some(resources) = &module.in_memory_resources {
                Some((
                    name.clone(),
                    BTreeMap::from_iter(resources.iter().map(|(key, value)| {
                        (
                            key.clone(),
                            // TODO should return a DataLocation or Result.
                            value.resolve().expect("resolved resource location"),
                        )
                    })),
                ))
            } else {
                None
            }
        }))
    }

    /// Obtain `ExtensionModule` in this instance.
    pub fn get_extension_modules(&self) -> BTreeMap<String, ExtensionModule> {
        self.extension_modules.clone()
    }

    /// Obtain `ExtensionModuleData` in this instance.
    pub fn get_extension_module_datas(&self) -> BTreeMap<String, ExtensionModuleData> {
        self.extension_module_datas.clone()
    }

    /// Add a source module to the collection of embedded source modules.
    pub fn add_source_module(&mut self, module: &SourceModule) {
        if !self.modules.contains_key(&module.name) {
            self.modules.insert(
                module.name.clone(),
                EmbeddedResourcePythonModulePrePackaged {
                    name: module.name.clone(),
                    ..EmbeddedResourcePythonModulePrePackaged::default()
                },
            );
        }

        let mut entry = self.modules.get_mut(&module.name).unwrap();

        entry.is_package = module.is_package;
        entry.in_memory_source = Some(module.source.clone());

        // Automatically insert empty modules for missing parent packages.
        for package in packages_from_module_name(&module.name) {
            if !self.modules.contains_key(&package) {
                self.modules.insert(
                    package.clone(),
                    EmbeddedResourcePythonModulePrePackaged {
                        name: package.clone(),
                        is_package: true,
                        in_memory_source: Some(DataLocation::Memory(vec![])),
                        ..EmbeddedResourcePythonModulePrePackaged::default()
                    },
                );
            }
        }
    }

    /// Add a bytecode module to the collection of embedded bytecode modules.
    pub fn add_bytecode_module(&mut self, module: &BytecodeModule) {
        if !self.modules.contains_key(&module.name) {
            self.modules.insert(
                module.name.clone(),
                EmbeddedResourcePythonModulePrePackaged {
                    name: module.name.clone(),
                    ..EmbeddedResourcePythonModulePrePackaged::default()
                },
            );
        }

        let mut entry = self.modules.get_mut(&module.name).unwrap();
        entry.is_package = module.is_package;

        match module.optimize_level {
            BytecodeOptimizationLevel::Zero => {
                entry.in_memory_bytecode = Some(module.source.clone());
            }
            BytecodeOptimizationLevel::One => {
                entry.in_memory_bytecode_opt1 = Some(module.source.clone());
            }
            BytecodeOptimizationLevel::Two => {
                entry.in_memory_bytecode_opt2 = Some(module.source.clone());
            }
        }

        // Automatically insert empty modules for missing parent packages.
        for package in packages_from_module_name(&module.name) {
            if !self.modules.contains_key(&package) {
                let mut entry = EmbeddedResourcePythonModulePrePackaged {
                    name: package.clone(),
                    is_package: true,
                    ..EmbeddedResourcePythonModulePrePackaged::default()
                };

                match module.optimize_level {
                    BytecodeOptimizationLevel::Zero => {
                        entry.in_memory_bytecode = Some(DataLocation::Memory(vec![]));
                    }
                    BytecodeOptimizationLevel::One => {
                        entry.in_memory_bytecode_opt1 = Some(DataLocation::Memory(vec![]));
                    }
                    BytecodeOptimizationLevel::Two => {
                        entry.in_memory_bytecode_opt2 = Some(DataLocation::Memory(vec![]));
                    }
                }

                self.modules.insert(package.clone(), entry);
            }
        }
    }

    /// Add resource data.
    ///
    /// Resource data belongs to a Python package and has a name and bytes data.
    pub fn add_resource(&mut self, resource: &ResourceData) {
        if !self.modules.contains_key(&resource.package) {
            self.modules.insert(
                resource.package.clone(),
                EmbeddedResourcePythonModulePrePackaged {
                    name: resource.package.clone(),
                    ..EmbeddedResourcePythonModulePrePackaged::default()
                },
            );
        }

        let mut entry = self.modules.get_mut(&resource.package).unwrap();
        entry.is_package = true;

        if entry.in_memory_resources.is_none() {
            entry.in_memory_resources = Some(BTreeMap::new());
        }

        entry
            .in_memory_resources
            .as_mut()
            .unwrap()
            .insert(resource.name.clone(), resource.data.clone());
    }

    /// Add an extension module.
    pub fn add_extension_module(&mut self, module: &ExtensionModule) {
        self.extension_modules
            .insert(module.module.clone(), module.clone());

        // Add empty bytecode for missing parent packages.
        for package in packages_from_module_name(&module.module) {
            if !self.modules.contains_key(&package) {
                self.modules.insert(
                    package.clone(),
                    EmbeddedResourcePythonModulePrePackaged {
                        name: package.clone(),
                        is_package: true,
                        // TODO should we populate opt1, opt2?
                        in_memory_bytecode: Some(DataLocation::Memory(vec![])),
                        ..EmbeddedResourcePythonModulePrePackaged::default()
                    },
                );
            }

            let mut entry = self.modules.get_mut(&package).unwrap();
            entry.is_package = true;
        }
    }

    /// Add an extension module.
    pub fn add_extension_module_data(&mut self, module: &ExtensionModuleData) {
        self.extension_module_datas
            .insert(module.name.clone(), module.clone());

        // Add empty bytecode for missing parent packages.
        for package in packages_from_module_name(&module.name) {
            if !self.modules.contains_key(&package) {
                self.modules.insert(
                    package.clone(),
                    EmbeddedResourcePythonModulePrePackaged {
                        name: package.clone(),
                        is_package: true,
                        // TODO should we populate opt1, opt2?
                        in_memory_bytecode: Some(DataLocation::Memory(vec![])),
                        ..EmbeddedResourcePythonModulePrePackaged::default()
                    },
                );
            }

            let mut entry = self.modules.get_mut(&package).unwrap();
            entry.is_package = true;
        }
    }

    /// Add an extension module shared library that should be imported from memory.
    pub fn add_in_memory_extension_module_shared_library(
        &mut self,
        module: &str,
        is_package: bool,
        data: &[u8],
    ) {
        if !self.modules.contains_key(module) {
            self.modules.insert(
                module.to_string(),
                EmbeddedResourcePythonModulePrePackaged {
                    name: module.to_string(),
                    ..EmbeddedResourcePythonModulePrePackaged::default()
                },
            );
        }

        let mut entry = self.modules.get_mut(module).unwrap();
        if is_package {
            entry.is_package = true;
        }
        entry.in_memory_extension_module_shared_library = Some(DataLocation::Memory(data.to_vec()));

        // Add empty bytecode for missing parent packages.
        for package in packages_from_module_name(module) {
            if !self.modules.contains_key(&package) {
                self.modules.insert(
                    package.clone(),
                    EmbeddedResourcePythonModulePrePackaged {
                        name: package.clone(),
                        // TODO should we populate opt1, opt2?
                        in_memory_bytecode: Some(DataLocation::Memory(vec![])),
                        ..EmbeddedResourcePythonModulePrePackaged::default()
                    },
                );
            }

            let mut entry = self.modules.get_mut(&package).unwrap();
            entry.is_package = true;
        }

        // TODO add shared library dependencies to be packaged as well.
        // TODO add shared library dependency names.
    }

    /// Filter the entities in this instance against names in files.
    pub fn filter_from_files(
        &mut self,
        logger: &slog::Logger,
        files: &[&Path],
        glob_patterns: &[&str],
    ) -> Result<()> {
        let resource_names = resolve_resource_names_from_files(files, glob_patterns)?;

        warn!(logger, "filtering module entries");
        filter_btreemap(logger, &mut self.modules, &resource_names);
        warn!(logger, "filtering embedded extension modules");
        filter_btreemap(logger, &mut self.extension_modules, &resource_names);

        Ok(())
    }

    /// Searches for embedded module sources for references to __file__.
    ///
    /// __file__ usage can be problematic for in-memory modules. This method searches
    /// for its occurrences and returns module names having it present.
    pub fn find_dunder_file(&self) -> Result<BTreeSet<String>> {
        let mut res = BTreeSet::new();

        for (name, module) in &self.modules {
            if let Some(location) = &module.in_memory_source {
                if has_dunder_file(&location.resolve()?)? {
                    res.insert(name.clone());
                }
            }

            if let Some(location) = &module.in_memory_bytecode {
                if has_dunder_file(&location.resolve()?)? {
                    res.insert(name.clone());
                }
            }

            if let Some(location) = &module.in_memory_bytecode_opt1 {
                if has_dunder_file(&location.resolve()?)? {
                    res.insert(name.clone());
                }
            }

            if let Some(location) = &module.in_memory_bytecode_opt2 {
                if has_dunder_file(&location.resolve()?)? {
                    res.insert(name.clone());
                }
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

        let mut compiler = BytecodeCompiler::new(&python_exe)?;
        {
            for (name, module) in &self.modules {
                let mut entry = EmbeddedResource::try_from(module)?;

                if let Some(location) = &module.in_memory_bytecode {
                    entry.in_memory_bytecode = Some(compiler.compile(
                        &location.resolve()?,
                        &name,
                        BytecodeOptimizationLevel::Zero,
                        CompileMode::Bytecode,
                    )?);
                }

                if let Some(location) = &module.in_memory_bytecode_opt1 {
                    entry.in_memory_bytecode_opt1 = Some(compiler.compile(
                        &location.resolve()?,
                        &name,
                        BytecodeOptimizationLevel::One,
                        CompileMode::Bytecode,
                    )?);
                }

                if let Some(location) = &module.in_memory_bytecode_opt2 {
                    entry.in_memory_bytecode_opt2 = Some(compiler.compile(
                        &location.resolve()?,
                        &name,
                        BytecodeOptimizationLevel::Two,
                        CompileMode::Bytecode,
                    )?);
                }

                modules.insert(name.clone(), entry);
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
                    EmbeddedResource {
                        name: name.clone(),
                        ..EmbeddedResource::default()
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
                    EmbeddedResource {
                        name: name.clone(),
                        ..EmbeddedResource::default()
                    },
                );
            }

            let mut entry = modules.get_mut(name).unwrap();

            if em.is_package {
                entry.is_package = true;
            }

            built_extension_modules.insert(name.clone(), em.clone());
        }

        let derived_package_names = packages_from_module_names(modules.keys().cloned());

        for package in derived_package_names {
            if !modules.contains_key(&package) {
                modules.insert(
                    package.clone(),
                    EmbeddedResource {
                        name: package.clone(),
                        ..EmbeddedResource::default()
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
    pub modules: BTreeMap<String, EmbeddedResource>,

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
                .collect::<Vec<EmbeddedResource>>(),
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

        assert!(r.modules.contains_key("foo"));
        assert_eq!(
            r.modules.get("foo"),
            Some(&EmbeddedResourcePythonModulePrePackaged {
                name: "foo".to_string(),
                is_package: false,
                in_memory_source: Some(DataLocation::Memory(vec![42])),
                ..EmbeddedResourcePythonModulePrePackaged::default()
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

        assert_eq!(r.modules.len(), 3);
        assert_eq!(
            r.modules.get("root.parent.child"),
            Some(&EmbeddedResourcePythonModulePrePackaged {
                name: "root.parent.child".to_string(),
                is_package: true,
                in_memory_source: Some(DataLocation::Memory(vec![42])),
                ..EmbeddedResourcePythonModulePrePackaged::default()
            })
        );

        assert_eq!(
            r.modules.get("root.parent"),
            Some(&EmbeddedResourcePythonModulePrePackaged {
                name: "root.parent".to_string(),
                is_package: true,
                in_memory_source: Some(DataLocation::Memory(vec![])),
                ..EmbeddedResourcePythonModulePrePackaged::default()
            })
        );

        assert_eq!(
            r.modules.get("root"),
            Some(&EmbeddedResourcePythonModulePrePackaged {
                name: "root".to_string(),
                is_package: true,
                in_memory_source: Some(DataLocation::Memory(vec![])),
                ..EmbeddedResourcePythonModulePrePackaged::default()
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

        assert!(r.modules.contains_key("foo"));
        assert_eq!(
            r.modules.get("foo"),
            Some(&EmbeddedResourcePythonModulePrePackaged {
                name: "foo".to_string(),
                in_memory_bytecode: Some(DataLocation::Memory(vec![42])),
                is_package: false,
                ..EmbeddedResourcePythonModulePrePackaged::default()
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

        assert_eq!(r.modules.len(), 3);
        assert_eq!(
            r.modules.get("root.parent.child"),
            Some(&EmbeddedResourcePythonModulePrePackaged {
                name: "root.parent.child".to_string(),
                in_memory_bytecode_opt1: Some(DataLocation::Memory(vec![42])),
                is_package: true,
                ..EmbeddedResourcePythonModulePrePackaged::default()
            })
        );
        assert_eq!(
            r.modules.get("root.parent"),
            Some(&EmbeddedResourcePythonModulePrePackaged {
                name: "root.parent".to_string(),
                in_memory_bytecode_opt1: Some(DataLocation::Memory(vec![])),
                is_package: true,
                ..EmbeddedResourcePythonModulePrePackaged::default()
            })
        );
        assert_eq!(
            r.modules.get("root"),
            Some(&EmbeddedResourcePythonModulePrePackaged {
                name: "root".to_string(),
                in_memory_bytecode_opt1: Some(DataLocation::Memory(vec![])),
                is_package: true,
                ..EmbeddedResourcePythonModulePrePackaged::default()
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

        assert_eq!(r.modules.len(), 1);
        assert_eq!(
            r.modules.get("foo"),
            Some(&EmbeddedResourcePythonModulePrePackaged {
                name: "foo".to_string(),
                is_package: true,
                in_memory_resources: Some(BTreeMap::from_iter(
                    [("resource.txt".to_string(), DataLocation::Memory(vec![42]))]
                        .iter()
                        .cloned()
                )),
                ..EmbeddedResourcePythonModulePrePackaged::default()
            })
        );
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

        assert_eq!(r.modules.len(), 1);
        assert_eq!(
            r.modules.get("foo"),
            Some(&EmbeddedResourcePythonModulePrePackaged {
                name: "foo".to_string(),
                in_memory_bytecode: Some(DataLocation::Memory(vec![])),
                is_package: true,
                ..EmbeddedResourcePythonModulePrePackaged::default()
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

        assert_eq!(r.modules.len(), 1);
        assert_eq!(
            r.modules.get("foo"),
            Some(&EmbeddedResourcePythonModulePrePackaged {
                name: "foo".to_string(),
                in_memory_bytecode: Some(DataLocation::Memory(vec![])),
                is_package: true,
                ..EmbeddedResourcePythonModulePrePackaged::default()
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
        let module = EmbeddedResource {
            name: "foo".to_string(),
            ..EmbeddedResource::default()
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
        expected.write_u8(EmbeddedResourceField::ModuleName.into())?;
        expected.write_u64::<LittleEndian>(b"foo".len() as u64)?;
        expected.write_u8(EmbeddedResourceField::EndOfIndex.into())?;
        // Module index.
        expected.write_u8(EmbeddedResourceField::StartOfEntry.into())?;
        expected.write_u8(EmbeddedResourceField::ModuleName.into())?;
        expected.write_u16::<LittleEndian>(b"foo".len() as u16)?;
        expected.write_u8(EmbeddedResourceField::EndOfEntry.into())?;
        expected.write_u8(EmbeddedResourceField::EndOfIndex.into())?;
        expected.write_all(b"foo")?;

        assert_eq!(data, expected);

        Ok(())
    }
}
