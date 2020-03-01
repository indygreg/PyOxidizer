// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/// Header value for version 1 of resources payload.
pub const EMBEDDED_RESOURCES_HEADER_V1: &[u8] = b"pyembed\x01";

#[derive(Clone, Debug, PartialEq)]
pub enum EmbeddedBlobInteriorPadding {
    None,
    Null,
}

impl Into<u8> for &EmbeddedBlobInteriorPadding {
    fn into(self) -> u8 {
        match self {
            EmbeddedBlobInteriorPadding::None => 0x01,
            EmbeddedBlobInteriorPadding::Null => 0x02,
        }
    }
}

/// Describes a blob section field type in the embedded resources payload.
#[derive(Debug, PartialEq, PartialOrd)]
pub enum EmbeddedBlobSectionField {
    EndOfIndex,
    StartOfEntry,
    EndOfEntry,
    ResourceFieldType,
    RawPayloadLength,
    InteriorPadding,
}

impl Into<u8> for EmbeddedBlobSectionField {
    fn into(self) -> u8 {
        match self {
            EmbeddedBlobSectionField::EndOfIndex => 0x00,
            EmbeddedBlobSectionField::StartOfEntry => 0x01,
            EmbeddedBlobSectionField::ResourceFieldType => 0x02,
            EmbeddedBlobSectionField::RawPayloadLength => 0x03,
            EmbeddedBlobSectionField::InteriorPadding => 0x04,
            EmbeddedBlobSectionField::EndOfEntry => 0xff,
        }
    }
}

/// Describes a data field type in the embedded resources payload.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Eq, Ord)]
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
