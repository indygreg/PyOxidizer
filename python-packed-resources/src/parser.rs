// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Management of Python resources.
*/

use {
    super::data::{EmbeddedBlobSectionField, EmbeddedResourceField, HEADER_V1},
    byteorder::{LittleEndian, ReadBytesExt},
    std::collections::{HashMap, HashSet},
    std::convert::TryFrom,
    std::io::{Cursor, Read},
    std::sync::Arc,
};

#[derive(Clone, Copy, Debug)]
enum BlobInteriorPadding {
    None,
    Null,
}

/// Represents a blob section in the blob index.
#[derive(Debug)]
struct BlobSection {
    resource_field: u8,
    raw_payload_length: usize,
    interior_padding: Option<BlobInteriorPadding>,
}

/// Holds state used to read an individual blob section.
#[derive(Clone, Copy)]
struct BlobSectionReadState {
    offset: usize,
    interior_padding: BlobInteriorPadding,
}

/// Represents a Python module and all its metadata.
///
/// This holds the result of parsing an embedded resources data structure as well
/// as extra state to support importing frozen and builtin modules.
#[derive(Debug, PartialEq)]
pub struct EmbeddedResource<'a> {
    /// The resource name.
    pub name: &'a str,

    /// Whether the resource is a Python package.
    pub is_package: bool,

    /// Whether the resource is a Python namespace package.
    pub is_namespace_package: bool,

    /// Whether the resource is a builtin extension module in the Python interpreter.
    pub is_builtin: bool,

    /// Whether the resource is frozen into the Python interpreter.
    pub is_frozen: bool,

    /// In-memory source code for Python module.
    pub in_memory_source: Option<&'a [u8]>,

    /// In-memory bytecode for Python module.
    pub in_memory_bytecode: Option<&'a [u8]>,

    /// In-memory bytecode optimization level 1 for Python module.
    pub in_memory_bytecode_opt1: Option<&'a [u8]>,

    /// In-memory bytecode optimization level 2 for Python module.
    pub in_memory_bytecode_opt2: Option<&'a [u8]>,

    /// In-memory content of shared library providing Python module.
    pub in_memory_shared_library_extension_module: Option<&'a [u8]>,

    /// Resource "files" in this Python package.
    pub in_memory_resources: Option<Arc<Box<HashMap<&'a str, &'a [u8]>>>>,

    /// Python package distribution files.
    pub in_memory_package_distribution: Option<HashMap<&'a str, &'a [u8]>>,

    /// In-memory content of shared library to be loaded from memory.
    pub in_memory_shared_library: Option<&'a [u8]>,

    /// Names of shared libraries this entry depends on.
    pub shared_library_dependency_names: Option<Vec<&'a str>>,
}

impl<'a> Default for EmbeddedResource<'a> {
    fn default() -> Self {
        Self {
            name: "",
            is_package: false,
            is_namespace_package: false,
            is_builtin: false,
            is_frozen: false,
            in_memory_source: None,
            in_memory_bytecode: None,
            in_memory_bytecode_opt1: None,
            in_memory_bytecode_opt2: None,
            in_memory_shared_library_extension_module: None,
            in_memory_resources: None,
            in_memory_package_distribution: None,
            in_memory_shared_library: None,
            shared_library_dependency_names: None,
        }
    }
}

pub fn load_resources<'a>(
    data: &'a [u8],
    resources: &mut HashMap<&'a str, EmbeddedResource<'a>>,
) -> Result<(), &'static str> {
    let mut reader = Cursor::new(data);

    let mut header = [0; 8];
    reader
        .read_exact(&mut header)
        .or_else(|_| Err("error reading 8 byte header"))?;

    if header == HEADER_V1 {
        load_resources_v1(data, &mut reader, resources)
    } else {
        Err("unrecognized file format")
    }
}

fn load_resources_v1<'a>(
    data: &'a [u8],
    reader: &mut Cursor<&[u8]>,
    resources: &mut HashMap<&'a str, EmbeddedResource<'a>>,
) -> Result<(), &'static str> {
    let blob_section_count = reader
        .read_u8()
        .or_else(|_| Err("failed reading blob section count"))?;
    let blob_index_length = reader
        .read_u32::<LittleEndian>()
        .or_else(|_| Err("failed reading blob index length"))? as usize;
    let resources_count = reader
        .read_u32::<LittleEndian>()
        .or_else(|_| Err("failed reading resources count"))? as usize;
    let resources_index_length = reader
        .read_u32::<LittleEndian>()
        .or_else(|_| Err("failed reading resources index length"))?
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
                .or_else(|_| Err("failed reading blob section field type"))?;

            let field_type = EmbeddedBlobSectionField::try_from(field_type)?;

            match field_type {
                EmbeddedBlobSectionField::EndOfIndex => break,
                EmbeddedBlobSectionField::StartOfEntry => {
                    blob_entry_count += 1;
                    current_blob_field = None;
                    current_blob_raw_payload_length = None;
                    current_blob_interior_padding = None;
                }
                EmbeddedBlobSectionField::EndOfEntry => {
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
                EmbeddedBlobSectionField::ResourceFieldType => {
                    let field = reader
                        .read_u8()
                        .or_else(|_| Err("failed reading blob resource field value"))?;
                    current_blob_field = Some(field);
                }
                EmbeddedBlobSectionField::RawPayloadLength => {
                    let l = reader
                        .read_u64::<LittleEndian>()
                        .or_else(|_| Err("failed reading raw payload length"))?;
                    current_blob_raw_payload_length = Some(l as usize);
                }
                EmbeddedBlobSectionField::InteriorPadding => {
                    let padding = reader
                        .read_u8()
                        .or_else(|_| Err("failed reading interior padding field value"))?;

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
            // Magic.
            HEADER_V1.len()
            // Global header.
            + 1 + 4 + 4 + 4
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

    let mut current_resource = EmbeddedResource::default();
    let mut current_resource_name = None;
    let mut index_entry_count = 0;

    if resources_index_length == 0 || resources_count == 0 {
        return Ok(());
    }

    loop {
        let field_type = reader
            .read_u8()
            .or_else(|_| Err("failed reading field type"))?;

        let field_type = EmbeddedResourceField::try_from(field_type)?;

        match field_type {
            EmbeddedResourceField::EndOfIndex => break,
            EmbeddedResourceField::StartOfEntry => {
                index_entry_count += 1;
                current_resource = EmbeddedResource::default();
                current_resource_name = None;
            }

            EmbeddedResourceField::EndOfEntry => {
                if let Some(name) = current_resource_name {
                    resources.insert(name, current_resource);
                } else {
                    return Err("resource name field is required");
                }

                current_resource = EmbeddedResource::default();
                current_resource_name = None;
            }
            EmbeddedResourceField::ModuleName => {
                let l = reader
                    .read_u16::<LittleEndian>()
                    .or_else(|_| Err("failed reading resource name length"))?
                    as usize;

                let name = unsafe {
                    std::str::from_utf8_unchecked(resolve_blob_data(
                        data,
                        &mut blob_offsets,
                        field_type,
                        l,
                    ))
                };

                current_resource_name = Some(name);
                current_resource.name = name;
            }
            EmbeddedResourceField::IsPackage => {
                current_resource.is_package = true;
            }
            EmbeddedResourceField::IsNamespacePackage => {
                current_resource.is_namespace_package = true;
            }
            EmbeddedResourceField::InMemorySource => {
                let l = reader
                    .read_u32::<LittleEndian>()
                    .or_else(|_| Err("failed reading source length"))?
                    as usize;

                current_resource.in_memory_source =
                    Some(resolve_blob_data(data, &mut blob_offsets, field_type, l));
            }
            EmbeddedResourceField::InMemoryBytecode => {
                let l = reader
                    .read_u32::<LittleEndian>()
                    .or_else(|_| Err("failed reading bytecode length"))?
                    as usize;

                current_resource.in_memory_bytecode =
                    Some(resolve_blob_data(data, &mut blob_offsets, field_type, l));
            }
            EmbeddedResourceField::InMemoryBytecodeOpt1 => {
                let l = reader
                    .read_u32::<LittleEndian>()
                    .or_else(|_| Err("failed reading bytecode length"))?
                    as usize;

                current_resource.in_memory_bytecode_opt1 =
                    Some(resolve_blob_data(data, &mut blob_offsets, field_type, l));
            }
            EmbeddedResourceField::InMemoryBytecodeOpt2 => {
                let l = reader
                    .read_u32::<LittleEndian>()
                    .or_else(|_| Err("failed reading bytecode length"))?
                    as usize;

                current_resource.in_memory_bytecode_opt2 =
                    Some(resolve_blob_data(data, &mut blob_offsets, field_type, l));
            }
            EmbeddedResourceField::InMemoryExtensionModuleSharedLibrary => {
                let l = reader
                    .read_u32::<LittleEndian>()
                    .or_else(|_| Err("failed reading extension module length"))?
                    as usize;

                current_resource.in_memory_shared_library_extension_module =
                    Some(resolve_blob_data(data, &mut blob_offsets, field_type, l));
            }

            EmbeddedResourceField::InMemoryResourcesData => {
                let resource_count = reader
                    .read_u32::<LittleEndian>()
                    .or_else(|_| Err("failed reading resources length"))?
                    as usize;

                let mut resources = Box::new(HashMap::with_capacity(resource_count));

                for _ in 0..resource_count {
                    let resource_name_length = reader
                        .read_u16::<LittleEndian>()
                        .or_else(|_| Err("failed reading resource name"))?
                        as usize;

                    let resource_name = unsafe {
                        std::str::from_utf8_unchecked(resolve_blob_data(
                            data,
                            &mut blob_offsets,
                            field_type,
                            resource_name_length,
                        ))
                    };

                    let resource_length = reader
                        .read_u64::<LittleEndian>()
                        .or_else(|_| Err("failed reading resource length"))?
                        as usize;

                    let resource_data =
                        resolve_blob_data(data, &mut blob_offsets, field_type, resource_length);

                    resources.insert(resource_name, resource_data);
                }

                current_resource.in_memory_resources = Some(Arc::new(resources));
            }

            EmbeddedResourceField::InMemoryPackageDistribution => {
                let resource_count = reader
                    .read_u32::<LittleEndian>()
                    .or_else(|_| Err("failed reading package distribution length"))?
                    as usize;

                let mut resources = HashMap::with_capacity(resource_count);

                for _ in 0..resource_count {
                    let name_length = reader
                        .read_u16::<LittleEndian>()
                        .or_else(|_| Err("failed reading distribution metadata name"))?
                        as usize;

                    let name = unsafe {
                        std::str::from_utf8_unchecked(resolve_blob_data(
                            data,
                            &mut blob_offsets,
                            field_type,
                            name_length,
                        ))
                    };

                    let resource_length = reader
                        .read_u64::<LittleEndian>()
                        .or_else(|_| Err("failed reading package distribution resource length"))?
                        as usize;

                    let resource_data =
                        resolve_blob_data(data, &mut blob_offsets, field_type, resource_length);

                    resources.insert(name, resource_data);
                }

                current_resource.in_memory_package_distribution = Some(resources);
            }

            EmbeddedResourceField::InMemorySharedLibrary => {
                let l = reader
                    .read_u64::<LittleEndian>()
                    .or_else(|_| Err("failed reading in-memory shared library length"))?
                    as usize;

                current_resource.in_memory_shared_library =
                    Some(&resolve_blob_data(data, &mut blob_offsets, field_type, l));
            }

            EmbeddedResourceField::SharedLibraryDependencyNames => {
                let names_count = reader
                    .read_u16::<LittleEndian>()
                    .or_else(|_| Err("failed reading shared library dependency names length"))?
                    as usize;

                let mut names = Vec::new();

                for _ in 0..names_count {
                    let name_length = reader
                        .read_u16::<LittleEndian>()
                        .or_else(|_| Err("failed reading shared library dependency name length"))?
                        as usize;

                    let name = unsafe {
                        std::str::from_utf8_unchecked(resolve_blob_data(
                            data,
                            &mut blob_offsets,
                            field_type,
                            name_length,
                        ))
                    };

                    names.push(name);
                }

                current_resource.shared_library_dependency_names = Some(names);
            }
        }
    }

    if index_entry_count != resources_count {
        return Err("mismatch between advertised index count and actual");
    }

    Ok(())
}

/// Resolve a slice to an individual blob's data.
///
/// This accepts a reference to the original blobs payload, an array of
/// current blob section offsets, the resource field being accessed, and the
/// length of the blob and returns a slice to that blob.
fn resolve_blob_data<'a>(
    data: &'a [u8],
    blob_sections: &mut [Option<BlobSectionReadState>],
    resource_field: EmbeddedResourceField,
    length: usize,
) -> &'a [u8] {
    let mut state = blob_sections[resource_field as usize].as_mut().unwrap();

    let blob = &data[state.offset..state.offset + length];

    let increment = match &state.interior_padding {
        BlobInteriorPadding::None => length,
        BlobInteriorPadding::Null => length + 1,
    };

    state.offset += increment;

    blob
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::data::EmbeddedBlobInteriorPadding,
        crate::writer::{write_embedded_resources_v1, EmbeddedResource as OwnedEmbeddedResource},
        std::collections::BTreeMap,
    };

    #[test]
    fn test_too_short_header() {
        let data = b"foo";

        let mut resources = HashMap::new();
        let res = load_resources(data, &mut resources);
        assert_eq!(res.err(), Some("error reading 8 byte header"));
    }

    #[test]
    fn test_unrecognized_header() {
        let data = b"pyembed\x00";
        let mut resources = HashMap::new();
        let res = load_resources(data, &mut resources);
        assert_eq!(res.err(), Some("unrecognized file format"));

        let data = b"pyembed\x02";
        let mut resources = HashMap::new();
        let res = load_resources(data, &mut resources);
        assert_eq!(res.err(), Some("unrecognized file format"));
    }

    #[test]
    fn test_no_indices() {
        let data = b"pyembed\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00";
        let mut resources = HashMap::new();
        load_resources(data, &mut resources).unwrap();
    }

    #[test]
    fn test_no_blob_index() {
        let data = b"pyembed\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x01\x00\x00\x00\x00";
        let mut resources = HashMap::new();
        load_resources(data, &mut resources).unwrap();
    }

    #[test]
    fn test_no_resource_index() {
        let data = b"pyembed\x01\x00\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00";
        let mut resources = HashMap::new();
        load_resources(data, &mut resources).unwrap();
    }

    #[test]
    fn test_empty_indices() {
        let data = b"pyembed\x01\x00\x01\x00\x00\x00\x00\x00\x00\x00\x01\x00\x00\x00\x00\x00";
        let mut resources = HashMap::new();
        load_resources(data, &mut resources).unwrap();
    }

    #[test]
    fn test_index_count_mismatch() {
        let data = b"pyembed\x01\x00\x00\x00\x00\x00\x01\x00\x00\x00\x01\x00\x00\x00\x00";
        let mut resources = HashMap::new();
        let res = load_resources(data, &mut resources);
        assert_eq!(
            res.err(),
            Some("mismatch between advertised index count and actual")
        );
    }

    #[test]
    fn test_missing_resource_name() {
        let data =
            b"pyembed\x01\x00\x01\x00\x00\x00\x01\x00\x00\x00\x03\x00\x00\x00\x00\x01\x02\x00";
        let mut resources = HashMap::new();
        let res = load_resources(data, &mut resources);
        assert_eq!(res.err(), Some("resource name field is required"));
    }

    #[test]
    fn test_just_resource_name() {
        let resource = OwnedEmbeddedResource {
            name: "foo".to_string(),
            ..OwnedEmbeddedResource::default()
        };

        let mut data = Vec::new();
        write_embedded_resources_v1(&[resource], &mut data, None).unwrap();

        let mut resources = HashMap::new();
        load_resources(&data, &mut resources).unwrap();

        assert_eq!(resources.len(), 1);

        let entry = resources.get("foo").unwrap();
        assert_eq!(
            entry,
            &EmbeddedResource {
                name: "foo",
                ..EmbeddedResource::default()
            }
        );
    }

    #[test]
    fn test_multiple_resources_just_names() {
        let resource1 = OwnedEmbeddedResource {
            name: "foo".to_string(),
            ..OwnedEmbeddedResource::default()
        };

        let resource2 = OwnedEmbeddedResource {
            name: "module2".to_string(),
            ..OwnedEmbeddedResource::default()
        };

        let mut data = Vec::new();
        write_embedded_resources_v1(&[resource1, resource2], &mut data, None).unwrap();

        let mut resources = HashMap::new();
        load_resources(&data, &mut resources).unwrap();

        assert_eq!(resources.len(), 2);

        let entry = resources.get("foo").unwrap();
        assert_eq!(
            entry,
            &EmbeddedResource {
                name: "foo",
                ..EmbeddedResource::default()
            }
        );

        let entry = resources.get("module2").unwrap();
        assert_eq!(
            entry,
            &EmbeddedResource {
                name: "module2",
                ..EmbeddedResource::default()
            }
        );
    }

    // Same as above just with null interior padding.
    #[test]
    fn test_multiple_resources_just_names_null_padding() {
        let resource1 = OwnedEmbeddedResource {
            name: "foo".to_string(),
            ..OwnedEmbeddedResource::default()
        };

        let resource2 = OwnedEmbeddedResource {
            name: "module2".to_string(),
            ..OwnedEmbeddedResource::default()
        };

        let mut data = Vec::new();
        write_embedded_resources_v1(
            &[resource1, resource2],
            &mut data,
            Some(EmbeddedBlobInteriorPadding::Null),
        )
        .unwrap();

        let mut resources = HashMap::new();
        load_resources(&data, &mut resources).unwrap();

        assert_eq!(resources.len(), 2);

        let entry = resources.get("foo").unwrap();
        assert_eq!(
            entry,
            &EmbeddedResource {
                name: "foo",
                ..EmbeddedResource::default()
            }
        );

        let entry = resources.get("module2").unwrap();
        assert_eq!(
            entry,
            &EmbeddedResource {
                name: "module2",
                ..EmbeddedResource::default()
            }
        );
    }

    #[test]
    fn test_in_memory_source() {
        let resource = OwnedEmbeddedResource {
            name: "foo".to_string(),
            in_memory_source: Some(b"source".to_vec()),
            ..OwnedEmbeddedResource::default()
        };

        let mut data = Vec::new();
        write_embedded_resources_v1(&[resource], &mut data, None).unwrap();
        let mut resources = HashMap::new();
        load_resources(&data, &mut resources).unwrap();

        assert_eq!(resources.len(), 1);

        let entry = resources.get("foo").unwrap();

        assert_eq!(entry.in_memory_source.unwrap(), b"source");

        assert_eq!(
            entry,
            &EmbeddedResource {
                name: "foo",
                in_memory_source: Some(&data[data.len() - 6..data.len()]),
                ..EmbeddedResource::default()
            }
        );
    }

    #[test]
    fn test_in_memory_bytecode() {
        let resource = OwnedEmbeddedResource {
            name: "foo".to_string(),
            in_memory_bytecode: Some(b"bytecode".to_vec()),
            ..OwnedEmbeddedResource::default()
        };

        let mut data = Vec::new();
        write_embedded_resources_v1(&[resource], &mut data, None).unwrap();
        let mut resources = HashMap::new();
        load_resources(&data, &mut resources).unwrap();

        assert_eq!(resources.len(), 1);

        let entry = resources.get("foo").unwrap();

        assert_eq!(entry.in_memory_bytecode.unwrap(), b"bytecode");

        assert_eq!(
            entry,
            &EmbeddedResource {
                name: "foo",
                in_memory_bytecode: Some(&data[data.len() - 8..data.len()]),
                ..EmbeddedResource::default()
            }
        );
    }

    #[test]
    fn test_in_memory_bytecode_opt1() {
        let resource = OwnedEmbeddedResource {
            name: "foo".to_string(),
            in_memory_bytecode_opt1: Some(b"bytecode".to_vec()),
            ..OwnedEmbeddedResource::default()
        };

        let mut data = Vec::new();
        write_embedded_resources_v1(&[resource], &mut data, None).unwrap();
        let mut resources = HashMap::new();
        load_resources(&data, &mut resources).unwrap();

        assert_eq!(resources.len(), 1);

        let entry = resources.get("foo").unwrap();

        assert_eq!(entry.in_memory_bytecode_opt1.unwrap(), b"bytecode");

        assert_eq!(
            entry,
            &EmbeddedResource {
                name: "foo",
                in_memory_bytecode_opt1: Some(&data[data.len() - 8..data.len()]),
                ..EmbeddedResource::default()
            }
        );
    }

    #[test]
    fn test_in_memory_bytecode_opt2() {
        let resource = OwnedEmbeddedResource {
            name: "foo".to_string(),
            in_memory_bytecode_opt2: Some(b"bytecode".to_vec()),
            ..OwnedEmbeddedResource::default()
        };

        let mut data = Vec::new();
        write_embedded_resources_v1(&[resource], &mut data, None).unwrap();
        let mut resources = HashMap::new();
        load_resources(&data, &mut resources).unwrap();

        assert_eq!(resources.len(), 1);

        let entry = resources.get("foo").unwrap();

        assert_eq!(entry.in_memory_bytecode_opt2.unwrap(), b"bytecode");

        assert_eq!(
            entry,
            &EmbeddedResource {
                name: "foo",
                in_memory_bytecode_opt2: Some(&data[data.len() - 8..data.len()]),
                ..EmbeddedResource::default()
            }
        );
    }

    #[test]
    fn test_in_memory_extension_module_shared_library() {
        let resource = OwnedEmbeddedResource {
            name: "foo".to_string(),
            in_memory_extension_module_shared_library: Some(b"em".to_vec()),
            ..OwnedEmbeddedResource::default()
        };

        let mut data = Vec::new();
        write_embedded_resources_v1(&[resource], &mut data, None).unwrap();
        let mut resources = HashMap::new();
        load_resources(&data, &mut resources).unwrap();

        assert_eq!(resources.len(), 1);

        let entry = resources.get("foo").unwrap();

        assert_eq!(
            entry.in_memory_shared_library_extension_module.unwrap(),
            b"em"
        );

        assert_eq!(
            entry,
            &EmbeddedResource {
                name: "foo",
                in_memory_shared_library_extension_module: Some(&data[data.len() - 2..data.len()]),
                ..EmbeddedResource::default()
            }
        );
    }

    #[test]
    fn test_in_memory_resources_data() {
        let mut resources = BTreeMap::new();
        resources.insert("foo".to_string(), b"foovalue".to_vec());
        resources.insert("another".to_string(), b"value2".to_vec());

        let resource = OwnedEmbeddedResource {
            name: "foo".to_string(),
            in_memory_resources: Some(resources),
            ..OwnedEmbeddedResource::default()
        };

        let mut data = Vec::new();
        write_embedded_resources_v1(&[resource], &mut data, None).unwrap();
        let mut resources = HashMap::new();
        load_resources(&data, &mut resources).unwrap();

        assert_eq!(resources.len(), 1);

        let entry = resources.get("foo").unwrap();

        let resources = entry.in_memory_resources.as_ref().unwrap();
        assert_eq!(resources.len(), 2);
        assert_eq!(resources.get("foo").unwrap(), b"foovalue");
        assert_eq!(resources.get("another").unwrap(), b"value2");
    }

    #[test]
    fn test_in_memory_package_distribution() {
        let mut resources = BTreeMap::new();
        resources.insert("foo".to_string(), b"foovalue".to_vec());
        resources.insert("another".to_string(), b"value2".to_vec());

        let resource = OwnedEmbeddedResource {
            name: "foo".to_string(),
            in_memory_package_distribution: Some(resources),
            ..OwnedEmbeddedResource::default()
        };

        let mut data = Vec::new();
        write_embedded_resources_v1(&[resource], &mut data, None).unwrap();
        let mut resources = HashMap::new();
        load_resources(&data, &mut resources).unwrap();

        assert_eq!(resources.len(), 1);

        let entry = resources.get("foo").unwrap();

        let resources = entry.in_memory_package_distribution.as_ref().unwrap();
        assert_eq!(resources.len(), 2);
        assert_eq!(resources.get("foo").unwrap(), b"foovalue");
        assert_eq!(resources.get("another").unwrap(), b"value2");
    }

    #[test]
    fn test_in_memory_shared_library() {
        let resource = OwnedEmbeddedResource {
            name: "foo".to_string(),
            in_memory_shared_library: Some(b"library".to_vec()),
            ..OwnedEmbeddedResource::default()
        };

        let mut data = Vec::new();
        write_embedded_resources_v1(&[resource], &mut data, None).unwrap();
        let mut resources = HashMap::new();
        load_resources(&data, &mut resources).unwrap();

        assert_eq!(resources.len(), 1);

        let entry = resources.get("foo").unwrap();

        assert_eq!(entry.in_memory_shared_library.unwrap(), b"library");

        assert_eq!(
            entry,
            &EmbeddedResource {
                name: "foo",
                in_memory_shared_library: Some(&data[data.len() - 7..data.len()]),
                ..EmbeddedResource::default()
            }
        );
    }

    #[test]
    fn test_shared_library_dependency_names() {
        let names = vec!["depends".to_string(), "libfoo".to_string()];

        let resource = OwnedEmbeddedResource {
            name: "foo".to_string(),
            shared_library_dependency_names: Some(names),
            ..OwnedEmbeddedResource::default()
        };

        let mut data = Vec::new();
        write_embedded_resources_v1(&[resource], &mut data, None).unwrap();
        let mut resources = HashMap::new();
        load_resources(&data, &mut resources).unwrap();

        assert_eq!(resources.len(), 1);

        let entry = resources.get("foo").unwrap();

        assert_eq!(
            entry.shared_library_dependency_names,
            Some(vec!["depends", "libfoo"])
        );
    }

    #[test]
    fn test_all_fields() {
        let mut resources = BTreeMap::new();
        resources.insert("foo".to_string(), b"foovalue".to_vec());
        resources.insert("resource2".to_string(), b"value2".to_vec());

        let mut distribution = BTreeMap::new();
        distribution.insert("dist".to_string(), b"distvalue".to_vec());
        distribution.insert("dist2".to_string(), b"dist2value".to_vec());

        let resource = OwnedEmbeddedResource {
            name: "module".to_string(),
            is_package: true,
            is_namespace_package: true,
            in_memory_source: Some(b"source".to_vec()),
            in_memory_bytecode: Some(b"bytecode".to_vec()),
            in_memory_bytecode_opt1: Some(b"bytecodeopt1".to_vec()),
            in_memory_bytecode_opt2: Some(b"bytecodeopt2".to_vec()),
            in_memory_extension_module_shared_library: Some(b"library".to_vec()),
            in_memory_resources: Some(resources),
            in_memory_package_distribution: Some(distribution),
            in_memory_shared_library: Some(b"library".to_vec()),
            shared_library_dependency_names: Some(vec![
                "libfoo".to_string(),
                "depends".to_string(),
            ]),
        };

        let mut data = Vec::new();
        write_embedded_resources_v1(&[resource], &mut data, None).unwrap();
        let mut resources = HashMap::new();
        load_resources(&data, &mut resources).unwrap();

        assert_eq!(resources.len(), 1);

        let entry = resources.get("module").unwrap();

        assert!(entry.is_package);
        assert!(entry.is_namespace_package);
        assert_eq!(entry.in_memory_source.unwrap(), b"source");
        assert_eq!(entry.in_memory_bytecode.unwrap(), b"bytecode");
        assert_eq!(entry.in_memory_bytecode_opt1.unwrap(), b"bytecodeopt1");
        assert_eq!(entry.in_memory_bytecode_opt2.unwrap(), b"bytecodeopt2");
        assert_eq!(
            entry.in_memory_shared_library_extension_module.unwrap(),
            b"library"
        );

        let resources = entry.in_memory_resources.as_ref().unwrap();
        assert_eq!(resources.len(), 2);
        assert_eq!(resources.get("foo").unwrap(), b"foovalue");
        assert_eq!(resources.get("resource2").unwrap(), b"value2");

        let resources = entry.in_memory_package_distribution.as_ref().unwrap();
        assert_eq!(resources.len(), 2);
        assert_eq!(resources.get("dist").unwrap(), b"distvalue");
        assert_eq!(resources.get("dist2").unwrap(), b"dist2value");

        assert_eq!(entry.in_memory_shared_library.unwrap(), b"library");
        assert_eq!(
            entry.shared_library_dependency_names.as_ref().unwrap(),
            &vec!["libfoo", "depends"]
        );
    }
}
