// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::convert::TryFrom;

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

impl TryFrom<u8> for EmbeddedBlobSectionField {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x00 => Ok(EmbeddedBlobSectionField::EndOfIndex),
            0x01 => Ok(EmbeddedBlobSectionField::StartOfEntry),
            0x02 => Ok(EmbeddedBlobSectionField::ResourceFieldType),
            0x03 => Ok(EmbeddedBlobSectionField::RawPayloadLength),
            0x04 => Ok(EmbeddedBlobSectionField::InteriorPadding),
            0xff => Ok(EmbeddedBlobSectionField::EndOfEntry),
            _ => Err("invalid blob index field type"),
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

impl TryFrom<u8> for EmbeddedResourceField {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x00 => Ok(EmbeddedResourceField::EndOfIndex),
            0x01 => Ok(EmbeddedResourceField::StartOfEntry),
            0x02 => Ok(EmbeddedResourceField::EndOfEntry),
            0x03 => Ok(EmbeddedResourceField::ModuleName),
            0x04 => Ok(EmbeddedResourceField::IsPackage),
            0x05 => Ok(EmbeddedResourceField::IsNamespacePackage),
            0x06 => Ok(EmbeddedResourceField::InMemorySource),
            0x07 => Ok(EmbeddedResourceField::InMemoryBytecode),
            0x08 => Ok(EmbeddedResourceField::InMemoryBytecodeOpt1),
            0x09 => Ok(EmbeddedResourceField::InMemoryBytecodeOpt2),
            0x0a => Ok(EmbeddedResourceField::InMemoryExtensionModuleSharedLibrary),
            0x0b => Ok(EmbeddedResourceField::InMemoryResourcesData),
            0x0c => Ok(EmbeddedResourceField::InMemoryPackageDistribution),
            0x0d => Ok(EmbeddedResourceField::InMemorySharedLibrary),
            0x0e => Ok(EmbeddedResourceField::SharedLibraryDependencyNames),
            _ => Err("invalid field type"),
        }
    }
}
