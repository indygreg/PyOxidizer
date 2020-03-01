// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::convert::TryFrom;

/// Header value for version 1 of resources payload.
pub const HEADER_V1: &[u8] = b"pyembed\x01";

/// Defines interior padding mechanism between entries in blob sections.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BlobInteriorPadding {
    /// No padding.
    ///
    /// Entries are packed next to each other. e.g. "foo" + "bar" = "foobar".
    None,

    /// NULL byte padding.
    ///
    /// There exists a NULL byte between entries. e.g. "foo" + "bar" = "foo\0bar\0".
    Null,
}

impl Into<u8> for &BlobInteriorPadding {
    fn into(self) -> u8 {
        match self {
            BlobInteriorPadding::None => 0x01,
            BlobInteriorPadding::Null => 0x02,
        }
    }
}

/// Describes a blob section field type in the blob index.
#[derive(Debug, PartialEq, PartialOrd)]
pub enum BlobSectionField {
    EndOfIndex,
    StartOfEntry,
    EndOfEntry,
    ResourceFieldType,
    RawPayloadLength,
    InteriorPadding,
}

impl Into<u8> for BlobSectionField {
    fn into(self) -> u8 {
        match self {
            BlobSectionField::EndOfIndex => 0x00,
            BlobSectionField::StartOfEntry => 0x01,
            BlobSectionField::ResourceFieldType => 0x02,
            BlobSectionField::RawPayloadLength => 0x03,
            BlobSectionField::InteriorPadding => 0x04,
            BlobSectionField::EndOfEntry => 0xff,
        }
    }
}

impl TryFrom<u8> for BlobSectionField {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x00 => Ok(BlobSectionField::EndOfIndex),
            0x01 => Ok(BlobSectionField::StartOfEntry),
            0x02 => Ok(BlobSectionField::ResourceFieldType),
            0x03 => Ok(BlobSectionField::RawPayloadLength),
            0x04 => Ok(BlobSectionField::InteriorPadding),
            0xff => Ok(BlobSectionField::EndOfEntry),
            _ => Err("invalid blob index field type"),
        }
    }
}

/// Describes a resource field type in the resource index.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Eq, Ord)]
pub enum ResourceField {
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

impl Into<u8> for ResourceField {
    fn into(self) -> u8 {
        match self {
            ResourceField::EndOfIndex => 0,
            ResourceField::StartOfEntry => 1,
            ResourceField::EndOfEntry => 2,
            ResourceField::ModuleName => 3,
            ResourceField::IsPackage => 4,
            ResourceField::IsNamespacePackage => 5,
            ResourceField::InMemorySource => 6,
            ResourceField::InMemoryBytecode => 7,
            ResourceField::InMemoryBytecodeOpt1 => 8,
            ResourceField::InMemoryBytecodeOpt2 => 9,
            ResourceField::InMemoryExtensionModuleSharedLibrary => 10,
            ResourceField::InMemoryResourcesData => 11,
            ResourceField::InMemoryPackageDistribution => 12,
            ResourceField::InMemorySharedLibrary => 13,
            ResourceField::SharedLibraryDependencyNames => 14,
        }
    }
}

impl TryFrom<u8> for ResourceField {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x00 => Ok(ResourceField::EndOfIndex),
            0x01 => Ok(ResourceField::StartOfEntry),
            0x02 => Ok(ResourceField::EndOfEntry),
            0x03 => Ok(ResourceField::ModuleName),
            0x04 => Ok(ResourceField::IsPackage),
            0x05 => Ok(ResourceField::IsNamespacePackage),
            0x06 => Ok(ResourceField::InMemorySource),
            0x07 => Ok(ResourceField::InMemoryBytecode),
            0x08 => Ok(ResourceField::InMemoryBytecodeOpt1),
            0x09 => Ok(ResourceField::InMemoryBytecodeOpt2),
            0x0a => Ok(ResourceField::InMemoryExtensionModuleSharedLibrary),
            0x0b => Ok(ResourceField::InMemoryResourcesData),
            0x0c => Ok(ResourceField::InMemoryPackageDistribution),
            0x0d => Ok(ResourceField::InMemorySharedLibrary),
            0x0e => Ok(ResourceField::SharedLibraryDependencyNames),
            _ => Err("invalid field type"),
        }
    }
}
