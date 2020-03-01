// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Embedded Python resources in a binary.
*/

use {
    super::data::{BlobInteriorPadding, BlobSectionField, Resource, ResourceField, HEADER_V1},
    anyhow::{anyhow, Context, Result},
    byteorder::{LittleEndian, WriteBytesExt},
    std::collections::BTreeMap,
    std::convert::TryFrom,
    std::io::Write,
};

#[derive(Debug)]
struct BlobSection {
    resource_field: ResourceField,
    raw_payload_length: usize,
    interior_padding: Option<BlobInteriorPadding>,
}

impl BlobSection {
    /// Compute length of index entry for version 1 payload format.
    pub fn index_v1_length(&self) -> usize {
        // Start of index entry.
        let mut index = 1;

        // Resource type field + its value.
        index += 2;

        // Raw payload length field + its value.
        index += 9;

        if self.interior_padding.is_some() {
            // Field + value.
            index += 2;
        }

        // End of index entry.
        index += 1;

        index
    }

    pub fn write_index_v1<W: Write>(&self, dest: &mut W) -> Result<()> {
        dest.write_u8(BlobSectionField::StartOfEntry.into())
            .context("writing start of index entry")?;

        dest.write_u8(BlobSectionField::ResourceFieldType.into())
            .context("writing resource field type field")?;
        dest.write_u8(self.resource_field.into())
            .context("writing resource field type value")?;

        dest.write_u8(BlobSectionField::RawPayloadLength.into())
            .context("writing raw payload length field")?;
        dest.write_u64::<LittleEndian>(self.raw_payload_length as u64)
            .context("writing raw payload length")?;

        if let Some(padding) = &self.interior_padding {
            dest.write_u8(BlobSectionField::InteriorPadding.into())
                .context("writing interior padding field")?;
            dest.write_u8(padding.into())
                .context("writing interior padding value")?;
        }

        dest.write_u8(BlobSectionField::EndOfEntry.into())
            .context("writing end of index entry")?;

        Ok(())
    }
}

impl<'a, X: Clone + 'a> Resource<'a, X>
where
    [X]: ToOwned<Owned = Vec<X>>,
{
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
    ///
    /// Interior padding is not part of the returned length.
    pub fn field_blob_length(&self, field: ResourceField) -> usize {
        match field {
            ResourceField::EndOfIndex => 0,
            ResourceField::StartOfEntry => 0,
            ResourceField::EndOfEntry => 0,
            ResourceField::ModuleName => self.name.as_bytes().len(),
            ResourceField::IsPackage => 0,
            ResourceField::IsNamespacePackage => 0,
            ResourceField::InMemorySource => {
                if let Some(source) = &self.in_memory_source {
                    source.len()
                } else {
                    0
                }
            }
            ResourceField::InMemoryBytecode => {
                if let Some(bytecode) = &self.in_memory_bytecode {
                    bytecode.len()
                } else {
                    0
                }
            }
            ResourceField::InMemoryBytecodeOpt1 => {
                if let Some(bytecode) = &self.in_memory_bytecode_opt1 {
                    bytecode.len()
                } else {
                    0
                }
            }
            ResourceField::InMemoryBytecodeOpt2 => {
                if let Some(bytecode) = &self.in_memory_bytecode_opt2 {
                    bytecode.len()
                } else {
                    0
                }
            }
            ResourceField::InMemoryExtensionModuleSharedLibrary => {
                if let Some(library) = &self.in_memory_extension_module_shared_library {
                    library.len()
                } else {
                    0
                }
            }
            ResourceField::InMemoryResourcesData => {
                if let Some(resources) = &self.in_memory_resources {
                    resources
                        .iter()
                        .map(|(key, value)| key.as_bytes().len() + value.len())
                        .sum()
                } else {
                    0
                }
            }
            ResourceField::InMemoryPackageDistribution => {
                if let Some(metadata) = &self.in_memory_package_distribution {
                    metadata
                        .iter()
                        .map(|(key, value)| key.as_bytes().len() + value.len())
                        .sum()
                } else {
                    0
                }
            }
            ResourceField::InMemorySharedLibrary => {
                if let Some(library) = &self.in_memory_shared_library {
                    library.len()
                } else {
                    0
                }
            }
            ResourceField::SharedLibraryDependencyNames => {
                if let Some(names) = &self.shared_library_dependency_names {
                    names.iter().map(|s| s.as_bytes().len()).sum()
                } else {
                    0
                }
            }
        }
    }

    /// Compute the size of interior padding for a specific field.
    pub fn field_blob_interior_padding_length(
        &self,
        field: ResourceField,
        padding: BlobInteriorPadding,
    ) -> usize {
        let elements_count = match field {
            ResourceField::EndOfIndex => 0,
            ResourceField::StartOfEntry => 0,
            ResourceField::EndOfEntry => 0,
            ResourceField::ModuleName => 1,
            ResourceField::IsPackage => 0,
            ResourceField::IsNamespacePackage => 0,
            ResourceField::InMemorySource => {
                if self.in_memory_source.is_some() {
                    1
                } else {
                    0
                }
            }
            ResourceField::InMemoryBytecode => {
                if self.in_memory_bytecode.is_some() {
                    1
                } else {
                    0
                }
            }
            ResourceField::InMemoryBytecodeOpt1 => {
                if self.in_memory_bytecode_opt1.is_some() {
                    1
                } else {
                    0
                }
            }
            ResourceField::InMemoryBytecodeOpt2 => {
                if self.in_memory_bytecode_opt2.is_some() {
                    1
                } else {
                    0
                }
            }
            ResourceField::InMemoryExtensionModuleSharedLibrary => {
                if self.in_memory_extension_module_shared_library.is_some() {
                    1
                } else {
                    0
                }
            }
            ResourceField::InMemoryResourcesData => {
                if let Some(resources) = &self.in_memory_resources {
                    resources.len() * 2
                } else {
                    0
                }
            }
            ResourceField::InMemoryPackageDistribution => {
                if let Some(metadata) = &self.in_memory_package_distribution {
                    metadata.len() * 2
                } else {
                    0
                }
            }
            ResourceField::InMemorySharedLibrary => {
                if self.in_memory_shared_library.is_some() {
                    1
                } else {
                    0
                }
            }
            ResourceField::SharedLibraryDependencyNames => {
                if let Some(names) = &self.shared_library_dependency_names {
                    names.len()
                } else {
                    0
                }
            }
        };

        let overhead = match padding {
            BlobInteriorPadding::None => 0,
            BlobInteriorPadding::Null => 1,
        };

        elements_count * overhead
    }

    /// Write the version 1 index entry for a module instance.
    pub fn write_index_v1<W: Write>(&self, dest: &mut W) -> Result<()> {
        let name_len =
            u16::try_from(self.name.as_bytes().len()).context("converting name to u16")?;

        dest.write_u8(ResourceField::StartOfEntry.into())
            .context("writing start of index entry")?;

        dest.write_u8(ResourceField::ModuleName.into())
            .context("writing module name field")?;

        dest.write_u16::<LittleEndian>(name_len)
            .context("writing module name length")?;

        if self.is_package {
            dest.write_u8(ResourceField::IsPackage.into())
                .context("writing is_package field")?;
        }

        if self.is_namespace_package {
            dest.write_u8(ResourceField::IsNamespacePackage.into())
                .context("writing is_namespace field")?;
        }

        if let Some(source) = &self.in_memory_source {
            let l =
                u32::try_from(source.len()).context("converting in-memory source length to u32")?;
            dest.write_u8(ResourceField::InMemorySource.into())
                .context("writing in-memory source length field")?;
            dest.write_u32::<LittleEndian>(l)
                .context("writing in-memory source length")?;
        }

        if let Some(bytecode) = &self.in_memory_bytecode {
            let l = u32::try_from(bytecode.len())
                .context("converting in-memory bytecode length to u32")?;
            dest.write_u8(ResourceField::InMemoryBytecode.into())
                .context("writing in-memory bytecode length field")?;
            dest.write_u32::<LittleEndian>(l)
                .context("writing in-memory bytecode length")?;
        }

        if let Some(bytecode) = &self.in_memory_bytecode_opt1 {
            let l = u32::try_from(bytecode.len())
                .context("converting in-memory bytecode opt 1 length to u32")?;
            dest.write_u8(ResourceField::InMemoryBytecodeOpt1.into())
                .context("writing in-memory bytecode opt 1 length field")?;
            dest.write_u32::<LittleEndian>(l)
                .context("writing in-memory bytecode opt 1 length")?;
        }

        if let Some(bytecode) = &self.in_memory_bytecode_opt2 {
            let l = u32::try_from(bytecode.len())
                .context("converting in-memory bytecode opt 2 length to u32")?;
            dest.write_u8(ResourceField::InMemoryBytecodeOpt2.into())
                .context("writing in-memory bytecode opt 2 field")?;
            dest.write_u32::<LittleEndian>(l)
                .context("writing in-memory bytecode opt 2 length")?;
        }

        if let Some(library) = &self.in_memory_extension_module_shared_library {
            let l = u32::try_from(library.len())
                .context("converting in-memory library length to u32")?;
            dest.write_u8(ResourceField::InMemoryExtensionModuleSharedLibrary.into())
                .context("writing in-memory extension module shared library field")?;
            dest.write_u32::<LittleEndian>(l)
                .context("writing in-memory extension module shared library length")?;
        }

        if let Some(resources) = &self.in_memory_resources {
            let l = u32::try_from(resources.len())
                .context("converting in-memory resources data length to u32")?;
            dest.write_u8(ResourceField::InMemoryResourcesData.into())
                .context("writing in-memory resources field")?;
            dest.write_u32::<LittleEndian>(l)
                .context("writing in-memory resources data length")?;

            for (name, value) in resources.iter() {
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
            dest.write_u8(ResourceField::InMemoryPackageDistribution.into())
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
            dest.write_u8(ResourceField::InMemorySharedLibrary.into())
                .context("writing in-memory shared library field")?;
            dest.write_u64::<LittleEndian>(l)
                .context("writing in-memory shared library length")?;
        }

        if let Some(names) = &self.shared_library_dependency_names {
            let l = u16::try_from(names.len())
                .context("converting shared library dependency names to u16")?;
            dest.write_u8(ResourceField::SharedLibraryDependencyNames.into())
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

        dest.write_u8(ResourceField::EndOfEntry.into())
            .or_else(|_| Err(anyhow!("error writing end of index entry")))?;

        Ok(())
    }
}

/// Write an embedded resources blob, version 1.
///
/// See the `pyembed` crate for the format of this data structure.
#[allow(clippy::cognitive_complexity)]
pub fn write_embedded_resources_v1<'a, W: Write>(
    modules: &[Resource<u8>],
    dest: &mut W,
    interior_padding: Option<BlobInteriorPadding>,
) -> Result<()> {
    let mut blob_sections = BTreeMap::new();

    let mut blob_section_count = 0;
    // 1 for end of index field.
    let mut blob_index_length = 1;

    // 1 for end of index field.
    let mut module_index_length = 1;

    let process_field = |blob_sections: &mut BTreeMap<ResourceField, BlobSection>,
                         resource: &Resource<u8>,
                         field: ResourceField| {
        let padding = match &interior_padding {
            Some(padding) => *padding,
            None => BlobInteriorPadding::None,
        };

        let l = resource.field_blob_length(field)
            + resource.field_blob_interior_padding_length(field, padding);
        if l > 0 {
            blob_sections
                .entry(field)
                .or_insert_with(|| BlobSection {
                    resource_field: field,
                    raw_payload_length: 0,
                    interior_padding,
                })
                .raw_payload_length += l;
        }
    };

    let add_interior_padding = |dest: &mut W| -> Result<()> {
        if interior_padding == Some(BlobInteriorPadding::Null) {
            dest.write_all(b"\0")?;
        }

        Ok(())
    };

    for module in modules {
        module_index_length += module.index_v1_length();

        process_field(&mut blob_sections, module, ResourceField::ModuleName);
        process_field(&mut blob_sections, module, ResourceField::InMemorySource);
        process_field(&mut blob_sections, module, ResourceField::InMemoryBytecode);
        process_field(
            &mut blob_sections,
            module,
            ResourceField::InMemoryBytecodeOpt1,
        );
        process_field(
            &mut blob_sections,
            module,
            ResourceField::InMemoryBytecodeOpt2,
        );
        process_field(
            &mut blob_sections,
            module,
            ResourceField::InMemoryExtensionModuleSharedLibrary,
        );
        process_field(
            &mut blob_sections,
            module,
            ResourceField::InMemoryResourcesData,
        );
        process_field(
            &mut blob_sections,
            module,
            ResourceField::InMemoryPackageDistribution,
        );
        process_field(
            &mut blob_sections,
            module,
            ResourceField::InMemorySharedLibrary,
        );
        process_field(
            &mut blob_sections,
            module,
            ResourceField::SharedLibraryDependencyNames,
        );
    }

    for section in blob_sections.values() {
        blob_section_count += 1;
        blob_index_length += section.index_v1_length();
    }

    dest.write_all(HEADER_V1)?;

    dest.write_u8(blob_section_count)?;
    dest.write_u32::<LittleEndian>(blob_index_length as u32)?;
    dest.write_u32::<LittleEndian>(modules.len() as u32)?;
    dest.write_u32::<LittleEndian>(module_index_length as u32)?;

    // Write the blob index.
    for section in blob_sections.values() {
        section.write_index_v1(dest)?;
    }
    dest.write_u8(ResourceField::EndOfIndex.into())?;

    // Write the resources index.
    for module in modules {
        module.write_index_v1(dest)?;
    }
    dest.write_u8(ResourceField::EndOfIndex.into())?;

    // Write blob data, one field at a time.
    for module in modules {
        dest.write_all(module.name.as_bytes())?;
        add_interior_padding(dest)?;
    }

    for module in modules {
        if let Some(data) = &module.in_memory_source {
            dest.write_all(data)?;
            add_interior_padding(dest)?;
        }
    }

    for module in modules {
        if let Some(data) = &module.in_memory_bytecode {
            dest.write_all(data)?;
            add_interior_padding(dest)?;
        }
    }

    for module in modules {
        if let Some(data) = &module.in_memory_bytecode_opt1 {
            dest.write_all(data)?;
            add_interior_padding(dest)?;
        }
    }

    for module in modules {
        if let Some(data) = &module.in_memory_bytecode_opt2 {
            dest.write_all(data)?;
            add_interior_padding(dest)?;
        }
    }

    for module in modules {
        if let Some(data) = &module.in_memory_extension_module_shared_library {
            dest.write_all(data)?;
            add_interior_padding(dest)?;
        }
    }

    for module in modules {
        if let Some(resources) = &module.in_memory_resources {
            for (key, value) in resources.iter() {
                dest.write_all(key.as_bytes())?;
                add_interior_padding(dest)?;
                dest.write_all(value)?;
                add_interior_padding(dest)?;
            }
        }
    }

    for module in modules {
        if let Some(resources) = &module.in_memory_package_distribution {
            for (key, value) in resources {
                dest.write_all(key.as_bytes())?;
                add_interior_padding(dest)?;
                dest.write_all(value)?;
                add_interior_padding(dest)?;
            }
        }
    }

    for module in modules {
        if let Some(data) = &module.in_memory_shared_library {
            dest.write_all(data)?;
            add_interior_padding(dest)?;
        }
    }

    for module in modules {
        if let Some(names) = &module.shared_library_dependency_names {
            for name in names {
                dest.write_all(name.as_bytes())?;
                add_interior_padding(dest)?;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use {super::*, std::borrow::Cow};

    #[test]
    fn test_write_empty() -> Result<()> {
        let mut data = Vec::new();
        write_embedded_resources_v1(&[], &mut data, None)?;

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
        let module = Resource {
            name: Cow::Owned("foo".to_string()),
            ..Resource::default()
        };

        write_embedded_resources_v1(&[module], &mut data, None)?;

        let mut expected: Vec<u8> = b"pyembed\x01".to_vec();
        // Number of blob sections.
        expected.write_u8(1)?;
        // Length of blob index. Start of entry, field type, field value, length field, length, end of entry, end of index.
        expected.write_u32::<LittleEndian>(1 + 1 + 1 + 1 + 8 + 1 + 1)?;
        // Number of modules.
        expected.write_u32::<LittleEndian>(1)?;
        // Length of index. Start of entry, module name length field, module name length, end of
        // entry, end of index.
        expected.write_u32::<LittleEndian>(1 + 1 + 2 + 1 + 1)?;
        // Blobs index.
        expected.write_u8(BlobSectionField::StartOfEntry.into())?;
        expected.write_u8(BlobSectionField::ResourceFieldType.into())?;
        expected.write_u8(ResourceField::ModuleName.into())?;
        expected.write_u8(BlobSectionField::RawPayloadLength.into())?;
        expected.write_u64::<LittleEndian>(b"foo".len() as u64)?;
        expected.write_u8(BlobSectionField::EndOfEntry.into())?;
        expected.write_u8(BlobSectionField::EndOfIndex.into())?;
        // Module index.
        expected.write_u8(ResourceField::StartOfEntry.into())?;
        expected.write_u8(ResourceField::ModuleName.into())?;
        expected.write_u16::<LittleEndian>(b"foo".len() as u16)?;
        expected.write_u8(ResourceField::EndOfEntry.into())?;
        expected.write_u8(ResourceField::EndOfIndex.into())?;
        expected.write_all(b"foo")?;

        assert_eq!(data, expected);

        Ok(())
    }
}
