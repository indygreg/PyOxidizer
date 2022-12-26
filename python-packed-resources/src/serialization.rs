// Copyright 2022 Gregory Szorc.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*! Declares the foundational data primitives inside packed resources data. */

/// Header value for version 2 of resources payload.
pub const HEADER_V3: &[u8] = b"pyembed\x03";

/// Defines interior padding mechanism between entries in blob sections.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlobInteriorPadding {
    /// No padding.
    ///
    /// Entries are packed next to each other. e.g. "foo" + "bar" = "foobar".
    None = 0x01,

    /// NULL byte padding.
    ///
    /// There exists a NULL byte between entries. e.g. "foo" + "bar" = "foo\0bar\0".
    Null = 0x02,
}

impl From<&BlobInteriorPadding> for u8 {
    fn from(source: &BlobInteriorPadding) -> Self {
        match source {
            BlobInteriorPadding::None => 0x01,
            BlobInteriorPadding::Null => 0x02,
        }
    }
}

/// Describes a blob section field type in the blob index.
#[derive(Debug, PartialEq, Eq, PartialOrd)]
pub enum BlobSectionField {
    EndOfIndex = 0x00,
    StartOfEntry = 0x01,
    EndOfEntry = 0xff,
    ResourceFieldType = 0x03,
    RawPayloadLength = 0x04,
    InteriorPadding = 0x05,
}

impl From<BlobSectionField> for u8 {
    fn from(source: BlobSectionField) -> u8 {
        match source {
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
    EndOfIndex = 0x00,
    StartOfEntry = 0x01,
    EndOfEntry = 0xff,
    // Flavor previously occupied slot 0x02.
    Name = 0x03,
    IsPythonPackage = 0x04,
    IsPythonNamespacePackage = 0x05,
    InMemorySource = 0x06,
    InMemoryBytecode = 0x07,
    InMemoryBytecodeOpt1 = 0x08,
    InMemoryBytecodeOpt2 = 0x09,
    InMemoryExtensionModuleSharedLibrary = 0x0a,
    InMemoryResourcesData = 0x0b,
    InMemoryDistributionResource = 0x0c,
    InMemorySharedLibrary = 0x0d,
    SharedLibraryDependencyNames = 0x0e,
    RelativeFilesystemModuleSource = 0x0f,
    RelativeFilesystemModuleBytecode = 0x10,
    RelativeFilesystemModuleBytecodeOpt1 = 0x11,
    RelativeFilesystemModuleBytecodeOpt2 = 0x12,
    RelativeFilesystemExtensionModuleSharedLibrary = 0x13,
    RelativeFilesystemPackageResources = 0x14,
    RelativeFilesystemDistributionResource = 0x15,
    IsPythonModule = 0x16,
    IsPythonBuiltinExtensionModule = 0x17,
    IsPythonFrozenModule = 0x18,
    IsPythonExtensionModule = 0x19,
    IsSharedLibrary = 0x1a,
    IsUtf8FilenameData = 0x1b,
    FileExecutable = 0x1c,
    FileDataEmbedded = 0x1d,
    FileDataUtf8RelativePath = 0x1e,
}

impl From<ResourceField> for u8 {
    fn from(field: ResourceField) -> Self {
        match field {
            ResourceField::EndOfIndex => 0x00,
            ResourceField::StartOfEntry => 0x01,
            ResourceField::Name => 0x03,
            ResourceField::IsPythonPackage => 0x04,
            ResourceField::IsPythonNamespacePackage => 0x05,
            ResourceField::InMemorySource => 0x06,
            ResourceField::InMemoryBytecode => 0x07,
            ResourceField::InMemoryBytecodeOpt1 => 0x08,
            ResourceField::InMemoryBytecodeOpt2 => 0x09,
            ResourceField::InMemoryExtensionModuleSharedLibrary => 0x0a,
            ResourceField::InMemoryResourcesData => 0x0b,
            ResourceField::InMemoryDistributionResource => 0x0c,
            ResourceField::InMemorySharedLibrary => 0x0d,
            ResourceField::SharedLibraryDependencyNames => 0x0e,
            ResourceField::RelativeFilesystemModuleSource => 0x0f,
            ResourceField::RelativeFilesystemModuleBytecode => 0x10,
            ResourceField::RelativeFilesystemModuleBytecodeOpt1 => 0x11,
            ResourceField::RelativeFilesystemModuleBytecodeOpt2 => 0x12,
            ResourceField::RelativeFilesystemExtensionModuleSharedLibrary => 0x13,
            ResourceField::RelativeFilesystemPackageResources => 0x14,
            ResourceField::RelativeFilesystemDistributionResource => 0x15,
            ResourceField::IsPythonModule => 0x16,
            ResourceField::IsPythonBuiltinExtensionModule => 0x17,
            ResourceField::IsPythonFrozenModule => 0x18,
            ResourceField::IsPythonExtensionModule => 0x19,
            ResourceField::IsSharedLibrary => 0x1a,
            ResourceField::IsUtf8FilenameData => 0x1b,
            ResourceField::FileExecutable => 0x1c,
            ResourceField::FileDataEmbedded => 0x1d,
            ResourceField::FileDataUtf8RelativePath => 0x1e,
            ResourceField::EndOfEntry => 0xff,
        }
    }
}

impl TryFrom<u8> for ResourceField {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x00 => Ok(ResourceField::EndOfIndex),
            0x01 => Ok(ResourceField::StartOfEntry),
            0x03 => Ok(ResourceField::Name),
            0x04 => Ok(ResourceField::IsPythonPackage),
            0x05 => Ok(ResourceField::IsPythonNamespacePackage),
            0x06 => Ok(ResourceField::InMemorySource),
            0x07 => Ok(ResourceField::InMemoryBytecode),
            0x08 => Ok(ResourceField::InMemoryBytecodeOpt1),
            0x09 => Ok(ResourceField::InMemoryBytecodeOpt2),
            0x0a => Ok(ResourceField::InMemoryExtensionModuleSharedLibrary),
            0x0b => Ok(ResourceField::InMemoryResourcesData),
            0x0c => Ok(ResourceField::InMemoryDistributionResource),
            0x0d => Ok(ResourceField::InMemorySharedLibrary),
            0x0e => Ok(ResourceField::SharedLibraryDependencyNames),
            0x0f => Ok(ResourceField::RelativeFilesystemModuleSource),
            0x10 => Ok(ResourceField::RelativeFilesystemModuleBytecode),
            0x11 => Ok(ResourceField::RelativeFilesystemModuleBytecodeOpt1),
            0x12 => Ok(ResourceField::RelativeFilesystemModuleBytecodeOpt2),
            0x13 => Ok(ResourceField::RelativeFilesystemExtensionModuleSharedLibrary),
            0x14 => Ok(ResourceField::RelativeFilesystemPackageResources),
            0x15 => Ok(ResourceField::RelativeFilesystemDistributionResource),
            0x16 => Ok(ResourceField::IsPythonModule),
            0x17 => Ok(ResourceField::IsPythonBuiltinExtensionModule),
            0x18 => Ok(ResourceField::IsPythonFrozenModule),
            0x19 => Ok(ResourceField::IsPythonExtensionModule),
            0x1a => Ok(ResourceField::IsSharedLibrary),
            0x1b => Ok(ResourceField::IsUtf8FilenameData),
            0x1c => Ok(ResourceField::FileExecutable),
            0x1d => Ok(ResourceField::FileDataEmbedded),
            0x1e => Ok(ResourceField::FileDataUtf8RelativePath),
            0xff => Ok(ResourceField::EndOfEntry),
            _ => Err("invalid field type"),
        }
    }
}
