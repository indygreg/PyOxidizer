// Copyright 2022 Gregory Szorc.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*! Parsing of packed resources data blobs. */

use {
    crate::{
        resource::Resource,
        serialization::{BlobInteriorPadding, BlobSectionField, ResourceField, HEADER_V3},
    },
    byteorder::{LittleEndian, ReadBytesExt},
    std::{borrow::Cow, collections::HashMap, io::Cursor, path::Path},
};

#[cfg(unix)]
use std::{ffi::OsStr, os::unix::ffi::OsStrExt};
#[cfg(windows)]
use {std::ffi::OsString, std::os::windows::ffi::OsStringExt, std::path::PathBuf};

/// Represents a blob section in the blob index.
#[derive(Debug)]
struct BlobSection {
    resource_field: u8,
    raw_payload_length: usize,
    interior_padding: Option<BlobInteriorPadding>,
}

/// Holds state used to read an individual blob section.
#[derive(Clone, Copy, Debug)]
struct BlobSectionReadState {
    offset: usize,
    interior_padding: BlobInteriorPadding,
}

/// An iterator over an actively parsed packed resources data structure.
///
/// The iterator emits [Resource] instances. The index data for a given resource is
/// not read or validated until the iterator attempts to deserialize it.
pub struct ResourceParserIterator<'a> {
    done: bool,
    data: &'a [u8],
    reader: Cursor<&'a [u8]>,
    blob_sections: [Option<BlobSectionReadState>; 256],
    claimed_resources_count: usize,
    read_resources_count: usize,
}

impl<'a> ResourceParserIterator<'a> {
    /// The expected number of resources we will emit.
    pub fn expected_resources_count(&self) -> usize {
        self.claimed_resources_count
    }

    /// Resolve a slice to an individual blob's data.
    ///
    /// This accepts a reference to the original blobs payload, an array of
    /// current blob section offsets, the resource field being accessed, and the
    /// length of the blob and returns a slice to that blob.
    fn resolve_blob_data(&mut self, resource_field: ResourceField, length: usize) -> &'a [u8] {
        let mut state = self.blob_sections[resource_field as usize]
            .as_mut()
            .expect("blob state not found");

        let blob = &self.data[state.offset..state.offset + length];

        let increment = match &state.interior_padding {
            BlobInteriorPadding::None => length,
            BlobInteriorPadding::Null => length + 1,
        };

        state.offset += increment;

        blob
    }

    #[cfg(unix)]
    fn resolve_path(&mut self, resource_field: ResourceField, length: usize) -> Cow<'a, Path> {
        let path_str = OsStr::from_bytes(self.resolve_blob_data(resource_field, length));
        Cow::Borrowed(Path::new(path_str))
    }

    #[cfg(windows)]
    fn resolve_path(&mut self, resource_field: ResourceField, length: usize) -> Cow<'a, Path> {
        let raw = self.resolve_blob_data(resource_field, length);
        let raw = unsafe { std::slice::from_raw_parts(raw.as_ptr() as *const u16, raw.len() / 2) };

        // There isn't an API that lets us get a OsStr from &[u16]. So we need to use
        // owned types.
        let path_string = OsString::from_wide(raw);

        Cow::Owned(PathBuf::from(path_string))
    }

    fn parse_next(&mut self) -> Result<Option<Resource<'a, u8>>, &'static str> {
        let mut current_resource = Resource::default();
        let mut current_resource_name = None;

        loop {
            let field_type = self
                .reader
                .read_u8()
                .map_err(|_| "failed reading field type")?;

            let field_type = ResourceField::try_from(field_type)?;

            match field_type {
                ResourceField::EndOfIndex => {
                    self.done = true;

                    if self.read_resources_count != self.claimed_resources_count {
                        return Err("mismatch between advertised index count and actual");
                    }

                    return Ok(None);
                }
                ResourceField::StartOfEntry => {
                    self.read_resources_count += 1;
                    current_resource = Resource::default();
                    current_resource_name = None;
                }
                ResourceField::EndOfEntry => {
                    let res = if current_resource_name.is_some() {
                        Ok(Some(current_resource))
                    } else {
                        Err("resource name field is required")
                    };

                    return res;
                }
                ResourceField::Name => {
                    let l = self
                        .reader
                        .read_u16::<LittleEndian>()
                        .map_err(|_| "failed reading resource name length")?
                        as usize;

                    let name = unsafe {
                        std::str::from_utf8_unchecked(self.resolve_blob_data(field_type, l))
                    };

                    current_resource_name = Some(name);
                    current_resource.name = Cow::Borrowed(name);
                }
                ResourceField::IsPythonPackage => {
                    current_resource.is_python_package = true;
                }
                ResourceField::IsPythonNamespacePackage => {
                    current_resource.is_python_namespace_package = true;
                }
                ResourceField::InMemorySource => {
                    let l = self
                        .reader
                        .read_u32::<LittleEndian>()
                        .map_err(|_| "failed reading source length")?
                        as usize;

                    current_resource.in_memory_source =
                        Some(Cow::Borrowed(self.resolve_blob_data(field_type, l)));
                }
                ResourceField::InMemoryBytecode => {
                    let l = self
                        .reader
                        .read_u32::<LittleEndian>()
                        .map_err(|_| "failed reading bytecode length")?
                        as usize;

                    current_resource.in_memory_bytecode =
                        Some(Cow::Borrowed(self.resolve_blob_data(field_type, l)));
                }
                ResourceField::InMemoryBytecodeOpt1 => {
                    let l = self
                        .reader
                        .read_u32::<LittleEndian>()
                        .map_err(|_| "failed reading bytecode length")?
                        as usize;

                    current_resource.in_memory_bytecode_opt1 =
                        Some(Cow::Borrowed(self.resolve_blob_data(field_type, l)));
                }
                ResourceField::InMemoryBytecodeOpt2 => {
                    let l = self
                        .reader
                        .read_u32::<LittleEndian>()
                        .map_err(|_| "failed reading bytecode length")?
                        as usize;

                    current_resource.in_memory_bytecode_opt2 =
                        Some(Cow::Borrowed(self.resolve_blob_data(field_type, l)));
                }
                ResourceField::InMemoryExtensionModuleSharedLibrary => {
                    let l = self
                        .reader
                        .read_u32::<LittleEndian>()
                        .map_err(|_| "failed reading extension module length")?
                        as usize;

                    current_resource.in_memory_extension_module_shared_library =
                        Some(Cow::Borrowed(self.resolve_blob_data(field_type, l)));
                }

                ResourceField::InMemoryResourcesData => {
                    let resource_count = self
                        .reader
                        .read_u32::<LittleEndian>()
                        .map_err(|_| "failed reading resources length")?
                        as usize;

                    let mut resources = HashMap::with_capacity(resource_count);

                    for _ in 0..resource_count {
                        let resource_name_length = self
                            .reader
                            .read_u16::<LittleEndian>()
                            .map_err(|_| "failed reading resource name")?
                            as usize;

                        let resource_name = unsafe {
                            std::str::from_utf8_unchecked(
                                self.resolve_blob_data(field_type, resource_name_length),
                            )
                        };

                        let resource_length = self
                            .reader
                            .read_u64::<LittleEndian>()
                            .map_err(|_| "failed reading resource length")?
                            as usize;

                        let resource_data = self.resolve_blob_data(field_type, resource_length);

                        resources
                            .insert(Cow::Borrowed(resource_name), Cow::Borrowed(resource_data));
                    }

                    current_resource.in_memory_package_resources = Some(resources);
                }

                ResourceField::InMemoryDistributionResource => {
                    let resource_count = self
                        .reader
                        .read_u32::<LittleEndian>()
                        .map_err(|_| "failed reading package distribution length")?
                        as usize;

                    let mut resources = HashMap::with_capacity(resource_count);

                    for _ in 0..resource_count {
                        let name_length = self
                            .reader
                            .read_u16::<LittleEndian>()
                            .map_err(|_| "failed reading distribution metadata name")?
                            as usize;

                        let name = unsafe {
                            std::str::from_utf8_unchecked(
                                self.resolve_blob_data(field_type, name_length),
                            )
                        };

                        let resource_length =
                            self.reader.read_u64::<LittleEndian>().map_err(|_| {
                                "failed reading package distribution resource length"
                            })? as usize;

                        let resource_data = self.resolve_blob_data(field_type, resource_length);

                        resources.insert(Cow::Borrowed(name), Cow::Borrowed(resource_data));
                    }

                    current_resource.in_memory_distribution_resources = Some(resources);
                }

                ResourceField::InMemorySharedLibrary => {
                    let l = self
                        .reader
                        .read_u64::<LittleEndian>()
                        .map_err(|_| "failed reading in-memory shared library length")?
                        as usize;

                    current_resource.in_memory_shared_library =
                        Some(Cow::Borrowed(self.resolve_blob_data(field_type, l)));
                }

                ResourceField::SharedLibraryDependencyNames => {
                    let names_count = self
                        .reader
                        .read_u16::<LittleEndian>()
                        .map_err(|_| "failed reading shared library dependency names length")?
                        as usize;

                    let mut names = Vec::new();

                    for _ in 0..names_count {
                        let name_length =
                            self.reader.read_u16::<LittleEndian>().map_err(|_| {
                                "failed reading shared library dependency name length"
                            })? as usize;

                        let name = unsafe {
                            std::str::from_utf8_unchecked(
                                self.resolve_blob_data(field_type, name_length),
                            )
                        };

                        names.push(Cow::Borrowed(name));
                    }

                    current_resource.shared_library_dependency_names = Some(names);
                }

                ResourceField::RelativeFilesystemModuleSource => {
                    let path_length = self
                        .reader
                        .read_u32::<LittleEndian>()
                        .map_err(|_| "failed reading Python module relative path length")?
                        as usize;

                    let path = self.resolve_path(field_type, path_length);

                    current_resource.relative_path_module_source = Some(path);
                }

                ResourceField::RelativeFilesystemModuleBytecode => {
                    let path_length =
                        self.reader.read_u32::<LittleEndian>().map_err(|_| {
                            "failed reading Python module bytecode relative path length"
                        })? as usize;

                    let path = self.resolve_path(field_type, path_length);

                    current_resource.relative_path_module_bytecode = Some(path);
                }

                ResourceField::RelativeFilesystemModuleBytecodeOpt1 => {
                    let path_length = self.reader.read_u32::<LittleEndian>().map_err(|_| {
                        "failed reading Python module bytecode opt 1 relative path length"
                    })? as usize;

                    let path = self.resolve_path(field_type, path_length);

                    current_resource.relative_path_module_bytecode_opt1 = Some(path);
                }

                ResourceField::RelativeFilesystemModuleBytecodeOpt2 => {
                    let path_length = self.reader.read_u32::<LittleEndian>().map_err(|_| {
                        "failed reading Python module bytecode opt 2 relative path length"
                    })? as usize;

                    let path = self.resolve_path(field_type, path_length);

                    current_resource.relative_path_module_bytecode_opt2 = Some(path);
                }

                ResourceField::RelativeFilesystemExtensionModuleSharedLibrary => {
                    let path_length = self.reader.read_u32::<LittleEndian>().map_err(|_| {
                        "failed reading Python extension module shared library relative path length"
                    })? as usize;

                    let path = self.resolve_path(field_type, path_length);

                    current_resource.relative_path_extension_module_shared_library = Some(path);
                }

                ResourceField::RelativeFilesystemPackageResources => {
                    let resource_count =
                        self.reader.read_u32::<LittleEndian>().map_err(|_| {
                            "failed reading package resources relative path item count"
                        })? as usize;

                    let mut resources = HashMap::with_capacity(resource_count);

                    for _ in 0..resource_count {
                        let resource_name_length = self
                            .reader
                            .read_u16::<LittleEndian>()
                            .map_err(|_| "failed reading resource name")?
                            as usize;

                        let resource_name = unsafe {
                            std::str::from_utf8_unchecked(
                                self.resolve_blob_data(field_type, resource_name_length),
                            )
                        };

                        let path_length = self
                            .reader
                            .read_u32::<LittleEndian>()
                            .map_err(|_| "failed reading resource path length")?
                            as usize;

                        let path = self.resolve_path(field_type, path_length);

                        resources.insert(Cow::Borrowed(resource_name), path);
                    }

                    current_resource.relative_path_package_resources = Some(resources);
                }

                ResourceField::RelativeFilesystemDistributionResource => {
                    let resource_count = self.reader.read_u32::<LittleEndian>().map_err(|_| {
                        "failed reading package distribution relative path item count"
                    })? as usize;

                    let mut resources = HashMap::with_capacity(resource_count);

                    for _ in 0..resource_count {
                        let name_length = self
                            .reader
                            .read_u16::<LittleEndian>()
                            .map_err(|_| "failed reading package distribution metadata name")?
                            as usize;

                        let name = unsafe {
                            std::str::from_utf8_unchecked(
                                self.resolve_blob_data(field_type, name_length),
                            )
                        };

                        let path_length = self
                            .reader
                            .read_u32::<LittleEndian>()
                            .map_err(|_| "failed reading package distribution path length")?
                            as usize;

                        let path = self.resolve_path(field_type, path_length);

                        resources.insert(Cow::Borrowed(name), path);
                    }

                    current_resource.relative_path_distribution_resources = Some(resources);
                }

                ResourceField::IsPythonModule => {
                    current_resource.is_python_module = true;
                }

                ResourceField::IsPythonBuiltinExtensionModule => {
                    current_resource.is_python_builtin_extension_module = true;
                }

                ResourceField::IsPythonFrozenModule => {
                    current_resource.is_python_frozen_module = true;
                }

                ResourceField::IsPythonExtensionModule => {
                    current_resource.is_python_extension_module = true;
                }

                ResourceField::IsSharedLibrary => {
                    current_resource.is_shared_library = true;
                }

                ResourceField::IsUtf8FilenameData => {
                    current_resource.is_utf8_filename_data = true;
                }

                ResourceField::FileExecutable => {
                    current_resource.file_executable = true;
                }

                ResourceField::FileDataEmbedded => {
                    let l = self
                        .reader
                        .read_u64::<LittleEndian>()
                        .map_err(|_| "failed reading embedded file data length")?
                        as usize;

                    current_resource.file_data_embedded =
                        Some(Cow::Borrowed(self.resolve_blob_data(field_type, l)));
                }

                ResourceField::FileDataUtf8RelativePath => {
                    let l = self
                        .reader
                        .read_u32::<LittleEndian>()
                        .map_err(|_| "failed reading file data relative path length")?
                        as usize;

                    current_resource.file_data_utf8_relative_path = Some(Cow::Borrowed(unsafe {
                        std::str::from_utf8_unchecked(self.resolve_blob_data(field_type, l))
                    }));
                }
            }
        }
    }
}

impl<'a> Iterator for ResourceParserIterator<'a> {
    type Item = Result<Resource<'a, u8>, &'static str>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        match self.parse_next() {
            Ok(res) => res.map(Ok),
            Err(e) => Some(Err(e)),
        }
    }
}

/// Parse a packed resources data structure.
///
/// The data structure is parsed lazily via an iterator that emits reconstructed
/// [Resource] instances.
///
/// Performance note: we once attempted to switch to anyhow for error handling and
/// this decreased performance by ~15%. Given the performance sensitivity of this
/// code, we need to keep error handling primitive.
pub fn load_resources<'a>(data: &'a [u8]) -> Result<ResourceParserIterator<'a>, &'static str> {
    if data.len() < HEADER_V3.len() {
        return Err("error reading 8 byte header");
    }

    let header = &data[0..8];

    if header == HEADER_V3 {
        load_resources_v3(&data[8..])
    } else {
        Err("unrecognized file format")
    }
}

fn load_resources_v3<'a>(data: &'a [u8]) -> Result<ResourceParserIterator<'a>, &'static str> {
    let mut reader = Cursor::new(data);

    let blob_section_count = reader
        .read_u8()
        .map_err(|_| "failed reading blob section count")?;
    let blob_index_length = reader
        .read_u32::<LittleEndian>()
        .map_err(|_| "failed reading blob index length")? as usize;
    let resources_count = reader
        .read_u32::<LittleEndian>()
        .map_err(|_| "failed reading resources count")? as usize;
    let resources_index_length = reader
        .read_u32::<LittleEndian>()
        .map_err(|_| "failed reading resources index length")?
        as usize;

    let mut current_blob_field = None;
    let mut current_blob_raw_payload_length = None;
    let mut current_blob_interior_padding = None;
    let mut blob_entry_count = 0;
    let mut blob_sections = Vec::with_capacity(blob_section_count as usize);

    if blob_section_count != 0 || blob_index_length != 0 {
        loop {
            let field_type = reader
                .read_u8()
                .map_err(|_| "failed reading blob section field type")?;

            let field_type = BlobSectionField::try_from(field_type)?;

            match field_type {
                BlobSectionField::EndOfIndex => break,
                BlobSectionField::StartOfEntry => {
                    blob_entry_count += 1;
                    current_blob_field = None;
                    current_blob_raw_payload_length = None;
                    current_blob_interior_padding = None;
                }
                BlobSectionField::EndOfEntry => {
                    if current_blob_field.is_none() {
                        return Err("blob resource field is required");
                    }
                    if current_blob_raw_payload_length.is_none() {
                        return Err("blob raw payload length is required");
                    }

                    blob_sections.push(BlobSection {
                        resource_field: current_blob_field.unwrap(),
                        raw_payload_length: current_blob_raw_payload_length.unwrap(),
                        interior_padding: current_blob_interior_padding,
                    });

                    current_blob_field = None;
                    current_blob_raw_payload_length = None;
                    current_blob_interior_padding = None;
                }
                BlobSectionField::ResourceFieldType => {
                    let field = reader
                        .read_u8()
                        .map_err(|_| "failed reading blob resource field value")?;
                    current_blob_field = Some(field);
                }
                BlobSectionField::RawPayloadLength => {
                    let l = reader
                        .read_u64::<LittleEndian>()
                        .map_err(|_| "failed reading raw payload length")?;
                    current_blob_raw_payload_length = Some(l as usize);
                }
                BlobSectionField::InteriorPadding => {
                    let padding = reader
                        .read_u8()
                        .map_err(|_| "failed reading interior padding field value")?;

                    current_blob_interior_padding = Some(match padding {
                        0x01 => BlobInteriorPadding::None,
                        0x02 => BlobInteriorPadding::Null,
                        _ => return Err("invalid value for interior padding field"),
                    });
                }
            }
        }
    }

    if blob_entry_count != blob_section_count {
        return Err("mismatch between blob sections count");
    }

    // Array indexing resource field to current payload offset within that section.
    let mut blob_offsets: [Option<BlobSectionReadState>; 256] = [None; 256];

    // Global payload offset where blobs data starts.
    let blob_start_offset: usize =
            // Global header.
            1 + 4 + 4 + 4
            + blob_index_length
            + resources_index_length
        ;
    // Current offset from start of blobs data.
    let mut current_blob_offset = 0;

    for section in &blob_sections {
        let section_start_offset = blob_start_offset + current_blob_offset;
        blob_offsets[section.resource_field as usize] = Some(BlobSectionReadState {
            offset: section_start_offset,
            interior_padding: match section.interior_padding {
                Some(padding) => padding,
                None => BlobInteriorPadding::None,
            },
        });
        current_blob_offset += section.raw_payload_length;
    }

    Ok(ResourceParserIterator {
        done: resources_index_length == 0 || resources_count == 0,
        data,
        reader,
        blob_sections: blob_offsets,
        claimed_resources_count: resources_count,
        read_resources_count: 0,
    })
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::{
            resource::Resource, serialization::BlobInteriorPadding,
            writer::write_packed_resources_v3,
        },
    };

    #[test]
    fn test_too_short_header() {
        let data = b"foo";

        let res = load_resources(data);
        assert_eq!(res.err(), Some("error reading 8 byte header"));
    }

    #[test]
    fn test_unrecognized_header() {
        let data = b"pyembed\x00";
        let res = load_resources(data);
        assert_eq!(res.err(), Some("unrecognized file format"));

        let data = b"pyembed\x04";
        let res = load_resources(data);
        assert_eq!(res.err(), Some("unrecognized file format"));
    }

    #[test]
    fn test_no_indices() {
        let data = b"pyembed\x03\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00";
        load_resources(data).unwrap();
    }

    #[test]
    fn test_no_blob_index() {
        let data = b"pyembed\x03\x00\x00\x00\x00\x00\x00\x00\x00\x00\x01\x00\x00\x00\x00";
        load_resources(data).unwrap();
    }

    #[test]
    fn test_no_resource_index() {
        let data = b"pyembed\x03\x00\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00";
        load_resources(data).unwrap();
    }

    #[test]
    fn test_empty_indices() {
        let data = b"pyembed\x03\x00\x01\x00\x00\x00\x00\x00\x00\x00\x01\x00\x00\x00\x00\x00";
        load_resources(data).unwrap();
    }

    #[test]
    fn test_index_count_mismatch() {
        let data = b"pyembed\x03\x00\x00\x00\x00\x00\x01\x00\x00\x00\x01\x00\x00\x00\x00";
        let mut res = load_resources(data).unwrap();
        assert_eq!(
            res.next(),
            Some(Err("mismatch between advertised index count and actual"))
        );
        assert_eq!(res.next(), None);
    }

    #[test]
    fn test_missing_resource_name() {
        let data =
            b"pyembed\x03\x00\x01\x00\x00\x00\x01\x00\x00\x00\x03\x00\x00\x00\x00\x01\xff\x00";
        let mut res = load_resources(data).unwrap();
        assert_eq!(res.next(), Some(Err("resource name field is required")));
        assert_eq!(res.next(), None);
    }

    #[test]
    fn test_just_resource_name() {
        let resource = Resource {
            name: Cow::from("foo"),
            ..Resource::default()
        };

        let mut data = Vec::new();
        write_packed_resources_v3(&[resource], &mut data, None).unwrap();

        let resources = load_resources(&data)
            .unwrap()
            .collect::<Result<Vec<Resource<u8>>, &'static str>>()
            .unwrap();

        assert_eq!(resources.len(), 1);

        let entry = &resources[0];
        assert_eq!(
            entry,
            &Resource {
                name: Cow::from("foo"),
                ..Resource::default()
            }
        );
    }

    #[test]
    fn test_multiple_resources_just_names() {
        let resource1 = Resource {
            name: Cow::from("foo"),
            ..Resource::default()
        };

        let resource2 = Resource {
            name: Cow::from("module2"),
            ..Resource::default()
        };

        let mut data = Vec::new();
        write_packed_resources_v3(&[resource1, resource2], &mut data, None).unwrap();
        let resources = load_resources(&data)
            .unwrap()
            .collect::<Result<Vec<Resource<u8>>, &'static str>>()
            .unwrap();

        assert_eq!(resources.len(), 2);

        let entry = &resources[0];
        assert_eq!(
            entry,
            &Resource {
                name: Cow::Borrowed("foo"),
                ..Resource::default()
            }
        );

        let entry = &resources[1];
        assert_eq!(
            entry,
            &Resource {
                name: Cow::Borrowed("module2"),
                ..Resource::default()
            }
        );
    }

    // Same as above just with null interior padding.
    #[test]
    fn test_multiple_resources_just_names_null_padding() {
        let resource1 = Resource {
            name: Cow::from("foo"),
            ..Resource::default()
        };

        let resource2 = Resource {
            name: Cow::from("module2"),
            ..Resource::default()
        };

        let mut data = Vec::new();
        write_packed_resources_v3(
            &[resource1, resource2],
            &mut data,
            Some(BlobInteriorPadding::Null),
        )
        .unwrap();
        let resources = load_resources(&data)
            .unwrap()
            .collect::<Result<Vec<Resource<u8>>, &'static str>>()
            .unwrap();

        assert_eq!(resources.len(), 2);

        let entry = &resources[0];
        assert_eq!(
            entry,
            &Resource {
                name: Cow::Borrowed("foo"),
                ..Resource::default()
            }
        );

        let entry = &resources[1];
        assert_eq!(
            entry,
            &Resource {
                name: Cow::Borrowed("module2"),
                ..Resource::default()
            }
        );
    }

    #[test]
    fn test_in_memory_source() {
        let resource = Resource {
            name: Cow::from("foo"),
            in_memory_source: Some(Cow::from(b"source".to_vec())),
            ..Resource::default()
        };

        let mut data = Vec::new();
        write_packed_resources_v3(&[resource], &mut data, None).unwrap();
        let resources = load_resources(&data)
            .unwrap()
            .collect::<Result<Vec<Resource<u8>>, &'static str>>()
            .unwrap();

        assert_eq!(resources.len(), 1);

        let entry = &resources[0];

        assert_eq!(entry.in_memory_source.as_ref().unwrap().as_ref(), b"source");

        assert_eq!(
            entry,
            &Resource {
                name: Cow::Borrowed("foo"),
                in_memory_source: Some(Cow::Borrowed(&data[data.len() - 6..data.len()])),
                ..Resource::default()
            }
        );
    }

    #[test]
    fn test_in_memory_bytecode() {
        let resource = Resource {
            name: Cow::from("foo"),
            in_memory_bytecode: Some(Cow::from(b"bytecode".to_vec())),
            ..Resource::default()
        };

        let mut data = Vec::new();
        write_packed_resources_v3(&[resource], &mut data, None).unwrap();
        let resources = load_resources(&data)
            .unwrap()
            .collect::<Result<Vec<Resource<u8>>, &'static str>>()
            .unwrap();

        assert_eq!(resources.len(), 1);

        let entry = &resources[0];

        assert_eq!(
            entry.in_memory_bytecode.as_ref().unwrap().as_ref(),
            b"bytecode"
        );

        assert_eq!(
            entry,
            &Resource {
                name: Cow::Borrowed("foo"),
                in_memory_bytecode: Some(Cow::Borrowed(&data[data.len() - 8..data.len()])),
                ..Resource::default()
            }
        );
    }

    #[test]
    fn test_in_memory_bytecode_opt1() {
        let resource = Resource {
            name: Cow::from("foo"),
            in_memory_bytecode_opt1: Some(Cow::from(b"bytecode".to_vec())),
            ..Resource::default()
        };

        let mut data = Vec::new();
        write_packed_resources_v3(&[resource], &mut data, None).unwrap();
        let resources = load_resources(&data)
            .unwrap()
            .collect::<Result<Vec<Resource<u8>>, &'static str>>()
            .unwrap();

        assert_eq!(resources.len(), 1);

        let entry = &resources[0];

        assert_eq!(
            entry.in_memory_bytecode_opt1.as_ref().unwrap().as_ref(),
            b"bytecode"
        );

        assert_eq!(
            entry,
            &Resource {
                name: Cow::Borrowed("foo"),
                in_memory_bytecode_opt1: Some(Cow::Borrowed(&data[data.len() - 8..data.len()])),
                ..Resource::default()
            }
        );
    }

    #[test]
    fn test_in_memory_bytecode_opt2() {
        let resource = Resource {
            name: Cow::from("foo"),
            in_memory_bytecode_opt2: Some(Cow::from(b"bytecode".to_vec())),
            ..Resource::default()
        };

        let mut data = Vec::new();
        write_packed_resources_v3(&[resource], &mut data, None).unwrap();
        let resources = load_resources(&data)
            .unwrap()
            .collect::<Result<Vec<Resource<u8>>, &'static str>>()
            .unwrap();

        assert_eq!(resources.len(), 1);

        let entry = &resources[0];

        assert_eq!(
            entry.in_memory_bytecode_opt2.as_ref().unwrap().as_ref(),
            b"bytecode"
        );

        assert_eq!(
            entry,
            &Resource {
                name: Cow::Borrowed("foo"),
                in_memory_bytecode_opt2: Some(Cow::Borrowed(&data[data.len() - 8..data.len()])),
                ..Resource::default()
            }
        );
    }

    #[test]
    fn test_in_memory_extension_module_shared_library() {
        let resource = Resource {
            name: Cow::from("foo"),
            in_memory_extension_module_shared_library: Some(Cow::from(b"em".to_vec())),
            ..Resource::default()
        };

        let mut data = Vec::new();
        write_packed_resources_v3(&[resource], &mut data, None).unwrap();
        let resources = load_resources(&data)
            .unwrap()
            .collect::<Result<Vec<Resource<u8>>, &'static str>>()
            .unwrap();

        assert_eq!(resources.len(), 1);

        let entry = &resources[0];

        assert_eq!(
            entry
                .in_memory_extension_module_shared_library
                .as_ref()
                .unwrap()
                .as_ref(),
            b"em"
        );

        assert_eq!(
            entry,
            &Resource {
                name: Cow::Borrowed("foo"),
                in_memory_extension_module_shared_library: Some(Cow::Borrowed(
                    &data[data.len() - 2..data.len()]
                )),
                ..Resource::default()
            }
        );
    }

    #[test]
    fn test_in_memory_package_resources() {
        let mut resources = HashMap::new();
        resources.insert(Cow::from("foo"), Cow::from(b"foovalue".to_vec()));
        resources.insert(Cow::from("another"), Cow::from(b"value2".to_vec()));

        let resource = Resource {
            name: Cow::from("foo"),
            in_memory_package_resources: Some(resources),
            ..Resource::default()
        };

        let mut data = Vec::new();
        write_packed_resources_v3(&[resource], &mut data, None).unwrap();
        let resources = load_resources(&data)
            .unwrap()
            .collect::<Result<Vec<Resource<u8>>, &'static str>>()
            .unwrap();

        assert_eq!(resources.len(), 1);

        let entry = &resources[0];

        let resources = entry.in_memory_package_resources.as_ref().unwrap();
        assert_eq!(resources.len(), 2);
        assert_eq!(resources.get("foo").unwrap().as_ref(), b"foovalue");
        assert_eq!(resources.get("another").unwrap().as_ref(), b"value2");
    }

    #[test]
    fn test_in_memory_package_distribution() {
        let mut resources = HashMap::new();
        resources.insert(Cow::from("foo"), Cow::from(b"foovalue".to_vec()));
        resources.insert(Cow::from("another"), Cow::from(b"value2".to_vec()));

        let resource = Resource {
            name: Cow::from("foo"),
            in_memory_distribution_resources: Some(resources),
            ..Resource::default()
        };

        let mut data = Vec::new();
        write_packed_resources_v3(&[resource], &mut data, None).unwrap();
        let resources = load_resources(&data)
            .unwrap()
            .collect::<Result<Vec<Resource<u8>>, &'static str>>()
            .unwrap();

        assert_eq!(resources.len(), 1);

        let entry = &resources[0];

        let resources = entry.in_memory_distribution_resources.as_ref().unwrap();
        assert_eq!(resources.len(), 2);
        assert_eq!(resources.get("foo").unwrap().as_ref(), b"foovalue");
        assert_eq!(resources.get("another").unwrap().as_ref(), b"value2");
    }

    #[test]
    fn test_in_memory_shared_library() {
        let resource = Resource {
            name: Cow::from("foo"),
            in_memory_shared_library: Some(Cow::from(b"library".to_vec())),
            ..Resource::default()
        };

        let mut data = Vec::new();
        write_packed_resources_v3(&[resource], &mut data, None).unwrap();
        let resources = load_resources(&data)
            .unwrap()
            .collect::<Result<Vec<Resource<u8>>, &'static str>>()
            .unwrap();

        assert_eq!(resources.len(), 1);

        let entry = &resources[0];

        assert_eq!(
            entry.in_memory_shared_library.as_ref().unwrap().as_ref(),
            b"library"
        );

        assert_eq!(
            entry,
            &Resource {
                name: Cow::from("foo"),
                in_memory_shared_library: Some(Cow::Borrowed(&data[data.len() - 7..data.len()])),
                ..Resource::default()
            }
        );
    }

    #[test]
    fn test_shared_library_dependency_names() {
        let names = vec![Cow::from("depends"), Cow::from("libfoo")];

        let resource = Resource {
            name: Cow::from("foo"),
            shared_library_dependency_names: Some(names),
            ..Resource::default()
        };

        let mut data = Vec::new();
        write_packed_resources_v3(&[resource], &mut data, None).unwrap();
        let resources = load_resources(&data)
            .unwrap()
            .collect::<Result<Vec<Resource<u8>>, &'static str>>()
            .unwrap();

        assert_eq!(resources.len(), 1);

        let entry = &resources[0];

        assert_eq!(
            entry.shared_library_dependency_names,
            Some(vec![Cow::Borrowed("depends"), Cow::Borrowed("libfoo")])
        );
    }

    #[test]
    fn test_relative_path_module_source() {
        let resource = Resource {
            name: Cow::from("foo"),
            relative_path_module_source: Some(Cow::from(Path::new("foo.py"))),
            ..Resource::default()
        };

        let mut data = Vec::new();
        write_packed_resources_v3(&[resource], &mut data, None).unwrap();
        let resources = load_resources(&data)
            .unwrap()
            .collect::<Result<Vec<Resource<u8>>, &'static str>>()
            .unwrap();

        assert_eq!(resources.len(), 1);

        let entry = &resources[0];

        assert_eq!(
            entry,
            &Resource {
                name: Cow::Borrowed("foo"),
                relative_path_module_source: Some(Cow::Borrowed(Path::new("foo.py"))),
                ..Resource::default()
            }
        );
    }

    #[test]
    fn test_relative_path_module_bytecode() {
        let resource = Resource {
            name: Cow::from("foo"),
            relative_path_module_bytecode: Some(Cow::from(Path::new("foo.pyc"))),
            ..Resource::default()
        };

        let mut data = Vec::new();
        write_packed_resources_v3(&[resource], &mut data, None).unwrap();
        let resources = load_resources(&data)
            .unwrap()
            .collect::<Result<Vec<Resource<u8>>, &'static str>>()
            .unwrap();

        assert_eq!(resources.len(), 1);

        let entry = &resources[0];

        assert_eq!(
            entry,
            &Resource {
                name: Cow::Borrowed("foo"),
                relative_path_module_bytecode: Some(Cow::Borrowed(Path::new("foo.pyc"))),
                ..Resource::default()
            }
        );
    }

    #[test]
    fn test_relative_path_module_bytecode_opt1() {
        let resource = Resource {
            name: Cow::from("foo"),
            relative_path_module_bytecode_opt1: Some(Cow::from(Path::new("foo.O1.pyc"))),
            ..Resource::default()
        };

        let mut data = Vec::new();
        write_packed_resources_v3(&[resource], &mut data, None).unwrap();
        let resources = load_resources(&data)
            .unwrap()
            .collect::<Result<Vec<Resource<u8>>, &'static str>>()
            .unwrap();

        assert_eq!(resources.len(), 1);

        let entry = &resources[0];

        assert_eq!(
            entry,
            &Resource {
                name: Cow::Borrowed("foo"),
                relative_path_module_bytecode_opt1: Some(Cow::Borrowed(Path::new("foo.O1.pyc"))),
                ..Resource::default()
            }
        );
    }

    #[test]
    fn test_relative_path_module_bytecode_opt2() {
        let resource = Resource {
            name: Cow::from("foo"),
            relative_path_module_bytecode_opt2: Some(Cow::from(Path::new("foo.O2.pyc"))),
            ..Resource::default()
        };

        let mut data = Vec::new();
        write_packed_resources_v3(&[resource], &mut data, None).unwrap();
        let resources = load_resources(&data)
            .unwrap()
            .collect::<Result<Vec<Resource<u8>>, &'static str>>()
            .unwrap();

        assert_eq!(resources.len(), 1);

        let entry = &resources[0];

        assert_eq!(
            entry,
            &Resource {
                name: Cow::Borrowed("foo"),
                relative_path_module_bytecode_opt2: Some(Cow::Borrowed(Path::new("foo.O2.pyc"))),
                ..Resource::default()
            }
        );
    }

    #[test]
    fn test_relative_path_extension_module_shared_library() {
        let resource = Resource {
            name: Cow::from("foo"),
            relative_path_extension_module_shared_library: Some(Cow::from(Path::new("foo.so"))),
            ..Resource::default()
        };

        let mut data = Vec::new();
        write_packed_resources_v3(&[resource], &mut data, None).unwrap();
        let resources = load_resources(&data)
            .unwrap()
            .collect::<Result<Vec<Resource<u8>>, &'static str>>()
            .unwrap();

        assert_eq!(resources.len(), 1);

        let entry = &resources[0];

        assert_eq!(
            entry,
            &Resource {
                name: Cow::Borrowed("foo"),
                relative_path_extension_module_shared_library: Some(Cow::Borrowed(Path::new(
                    "foo.so"
                ))),
                ..Resource::default()
            }
        );
    }

    #[test]
    fn test_relative_path_package_resources() {
        let mut resources = HashMap::new();
        resources.insert(Cow::from("foo"), Cow::from(Path::new("foo")));
        resources.insert(Cow::from("another"), Cow::from(Path::new("another")));

        let resource = Resource {
            name: Cow::from("foo"),
            relative_path_package_resources: Some(resources),
            ..Resource::default()
        };

        let mut data = Vec::new();
        write_packed_resources_v3(&[resource], &mut data, None).unwrap();
        let resources = load_resources(&data)
            .unwrap()
            .collect::<Result<Vec<Resource<u8>>, &'static str>>()
            .unwrap();

        assert_eq!(resources.len(), 1);

        let entry = &resources[0];

        let resources = entry.relative_path_package_resources.as_ref().unwrap();
        assert_eq!(resources.len(), 2);
        assert_eq!(resources.get("foo"), Some(&Cow::Borrowed(Path::new("foo"))));
        assert_eq!(
            resources.get("another"),
            Some(&Cow::Borrowed(Path::new("another")))
        );
    }

    #[test]
    fn test_relative_path_package_distribution() {
        let mut resources = HashMap::new();
        resources.insert(Cow::from("foo"), Cow::from(Path::new("package/foo")));
        resources.insert(
            Cow::from("another"),
            Cow::from(Path::new("package/another")),
        );

        let resource = Resource {
            name: Cow::from("foo"),
            relative_path_distribution_resources: Some(resources),
            ..Resource::default()
        };

        let mut data = Vec::new();
        write_packed_resources_v3(&[resource], &mut data, None).unwrap();
        let resources = load_resources(&data)
            .unwrap()
            .collect::<Result<Vec<Resource<u8>>, &'static str>>()
            .unwrap();

        assert_eq!(resources.len(), 1);

        let entry = &resources[0];

        let resources = entry.relative_path_distribution_resources.as_ref().unwrap();
        assert_eq!(resources.len(), 2);
        assert_eq!(
            resources.get("foo"),
            Some(&Cow::Borrowed(Path::new("package/foo")))
        );
        assert_eq!(
            resources.get("another"),
            Some(&Cow::Borrowed(Path::new("package/another")))
        );
    }

    #[allow(clippy::cognitive_complexity)]
    #[test]
    fn test_all_fields() {
        let mut in_memory_resources = HashMap::new();
        in_memory_resources.insert(
            Cow::from("foo".to_string()),
            Cow::from(b"foovalue".to_vec()),
        );
        in_memory_resources.insert(Cow::from("resource2"), Cow::from(b"value2".to_vec()));

        let mut in_memory_distribution = HashMap::new();
        in_memory_distribution.insert(Cow::from("dist"), Cow::from(b"distvalue".to_vec()));
        in_memory_distribution.insert(Cow::from("dist2"), Cow::from(b"dist2value".to_vec()));

        let mut relative_path_resources = HashMap::new();
        relative_path_resources.insert(
            Cow::from("resource.txt"),
            Cow::from(Path::new("resource.txt")),
        );
        relative_path_resources.insert(Cow::from("foo.txt"), Cow::from(Path::new("foo.txt")));

        let mut relative_path_distribution = HashMap::new();
        relative_path_distribution.insert(
            Cow::from("foo.txt"),
            Cow::from(Path::new("package/foo.txt")),
        );
        relative_path_distribution.insert(
            Cow::from("resource.txt"),
            Cow::from(Path::new("package/resource.txt")),
        );

        let resource = Resource {
            name: Cow::from("module"),
            is_python_package: true,
            is_python_namespace_package: true,
            in_memory_source: Some(Cow::from(b"source".to_vec())),
            in_memory_bytecode: Some(Cow::from(b"bytecode".to_vec())),
            in_memory_bytecode_opt1: Some(Cow::from(b"bytecodeopt1".to_vec())),
            in_memory_bytecode_opt2: Some(Cow::from(b"bytecodeopt2".to_vec())),
            in_memory_extension_module_shared_library: Some(Cow::from(b"library".to_vec())),
            in_memory_package_resources: Some(in_memory_resources),
            in_memory_distribution_resources: Some(in_memory_distribution),
            in_memory_shared_library: Some(Cow::from(b"library".to_vec())),
            shared_library_dependency_names: Some(vec![Cow::from("libfoo"), Cow::from("depends")]),
            relative_path_module_source: Some(Cow::from(Path::new("source_path"))),
            relative_path_module_bytecode: Some(Cow::from(Path::new("bytecode_path"))),
            relative_path_module_bytecode_opt1: Some(Cow::from(Path::new("bytecode_opt1_path"))),
            relative_path_module_bytecode_opt2: Some(Cow::from(Path::new("bytecode_opt2_path"))),
            relative_path_extension_module_shared_library: Some(Cow::from(Path::new("em_path"))),
            relative_path_package_resources: Some(relative_path_resources),
            relative_path_distribution_resources: Some(relative_path_distribution),
            is_python_module: true,
            is_python_builtin_extension_module: true,
            is_python_frozen_module: true,
            is_python_extension_module: true,
            is_shared_library: true,
            is_utf8_filename_data: true,
            file_executable: true,
            file_data_embedded: Some(Cow::from(b"file_data_embedded".to_vec())),
            file_data_utf8_relative_path: Some(Cow::from("file_data_utf8_relative_path")),
        };

        let mut data = Vec::new();
        write_packed_resources_v3(&[resource], &mut data, None).unwrap();
        let resources = load_resources(&data)
            .unwrap()
            .collect::<Result<Vec<Resource<u8>>, &'static str>>()
            .unwrap();

        assert_eq!(resources.len(), 1);

        let entry = &resources[0];

        assert!(entry.is_python_package);
        assert!(entry.is_python_namespace_package);
        assert_eq!(entry.in_memory_source.as_ref().unwrap().as_ref(), b"source");
        assert_eq!(
            entry.in_memory_bytecode.as_ref().unwrap().as_ref(),
            b"bytecode"
        );
        assert_eq!(
            entry.in_memory_bytecode_opt1.as_ref().unwrap().as_ref(),
            b"bytecodeopt1"
        );
        assert_eq!(
            entry.in_memory_bytecode_opt2.as_ref().unwrap().as_ref(),
            b"bytecodeopt2"
        );
        assert_eq!(
            entry
                .in_memory_extension_module_shared_library
                .as_ref()
                .unwrap()
                .as_ref(),
            b"library"
        );

        let resources = entry.in_memory_package_resources.as_ref().unwrap();
        assert_eq!(resources.len(), 2);
        assert_eq!(resources.get("foo").unwrap().as_ref(), b"foovalue");
        assert_eq!(resources.get("resource2").unwrap().as_ref(), b"value2");

        let resources = entry.in_memory_distribution_resources.as_ref().unwrap();
        assert_eq!(resources.len(), 2);
        assert_eq!(resources.get("dist").unwrap().as_ref(), b"distvalue");
        assert_eq!(resources.get("dist2").unwrap().as_ref(), b"dist2value");

        assert_eq!(
            entry.in_memory_shared_library.as_ref().unwrap().as_ref(),
            b"library"
        );
        assert_eq!(
            entry.shared_library_dependency_names.as_ref().unwrap(),
            &vec!["libfoo", "depends"]
        );

        assert_eq!(
            entry.relative_path_module_source,
            Some(Cow::Borrowed(Path::new("source_path")))
        );

        assert_eq!(
            entry.relative_path_module_bytecode,
            Some(Cow::Borrowed(Path::new("bytecode_path")))
        );
        assert_eq!(
            entry.relative_path_module_bytecode_opt1,
            Some(Cow::Borrowed(Path::new("bytecode_opt1_path")))
        );
        assert_eq!(
            entry.relative_path_module_bytecode_opt2,
            Some(Cow::Borrowed(Path::new("bytecode_opt2_path")))
        );
        assert_eq!(
            entry.relative_path_extension_module_shared_library,
            Some(Cow::Borrowed(Path::new("em_path")))
        );

        let resources = entry.relative_path_package_resources.as_ref().unwrap();
        assert_eq!(resources.len(), 2);
        assert_eq!(
            resources.get("resource.txt"),
            Some(&Cow::Borrowed(Path::new("resource.txt")))
        );
        assert_eq!(
            resources.get("foo.txt"),
            Some(&Cow::Borrowed(Path::new("foo.txt")))
        );

        let distribution = entry.relative_path_distribution_resources.as_ref().unwrap();
        assert_eq!(distribution.len(), 2);
        assert_eq!(
            distribution.get("foo.txt"),
            Some(&Cow::Borrowed(Path::new("package/foo.txt")))
        );
        assert_eq!(
            distribution.get("resource.txt"),
            Some(&Cow::Borrowed(Path::new("package/resource.txt")))
        );
        assert!(entry.is_python_module);
        assert!(entry.is_python_builtin_extension_module);
        assert!(entry.is_python_frozen_module);
        assert!(entry.is_python_extension_module);
        assert!(entry.is_shared_library);
        assert!(entry.is_utf8_filename_data);
        assert!(entry.file_executable);
        assert_eq!(
            entry.file_data_embedded.as_ref().unwrap().as_ref(),
            b"file_data_embedded"
        );
        assert_eq!(
            entry.file_data_utf8_relative_path.as_ref().unwrap(),
            "file_data_utf8_relative_path"
        );
    }

    #[test]
    fn test_fields_mix() {
        let resources: Vec<Resource<u8>> = vec![
            Resource {
                name: Cow::from("foo"),
                is_python_module: true,
                in_memory_source: Some(Cow::from(b"import io".to_vec())),
                ..Resource::default()
            },
            Resource {
                name: Cow::from("bar"),
                is_python_module: true,
                in_memory_bytecode: Some(Cow::from(b"fake bytecode".to_vec())),
                ..Resource::default()
            },
        ];

        let mut data = Vec::new();
        write_packed_resources_v3(&resources, &mut data, None).unwrap();
        let loaded = load_resources(&data)
            .unwrap()
            .collect::<Result<Vec<Resource<u8>>, &'static str>>()
            .unwrap();

        assert_eq!(resources, loaded);
    }
}
