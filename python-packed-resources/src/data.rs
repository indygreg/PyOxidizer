// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {std::borrow::Cow, std::collections::HashMap, std::convert::TryFrom, std::sync::Arc};

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
            ResourceField::EndOfIndex => 0x00,
            ResourceField::StartOfEntry => 0x01,
            ResourceField::ModuleName => 0x03,
            ResourceField::IsPackage => 0x04,
            ResourceField::IsNamespacePackage => 0x05,
            ResourceField::InMemorySource => 0x06,
            ResourceField::InMemoryBytecode => 0x07,
            ResourceField::InMemoryBytecodeOpt1 => 0x08,
            ResourceField::InMemoryBytecodeOpt2 => 0x09,
            ResourceField::InMemoryExtensionModuleSharedLibrary => 0x0a,
            ResourceField::InMemoryResourcesData => 0x0b,
            ResourceField::InMemoryPackageDistribution => 0x0c,
            ResourceField::InMemorySharedLibrary => 0x0d,
            ResourceField::SharedLibraryDependencyNames => 0x0e,
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
            0xff => Ok(ResourceField::EndOfEntry),
            _ => Err("invalid field type"),
        }
    }
}

/// Represents an embedded resource and all its metadata.
#[derive(Clone, Debug, PartialEq)]
pub struct Resource<'a, X: 'a>
where
    [X]: ToOwned<Owned = Vec<X>>,
{
    /// The resource name.
    pub name: Cow<'a, str>,

    /// Whether the Python module is a package.
    pub is_package: bool,

    /// Whether the Python module is a namespace package.
    pub is_namespace_package: bool,

    /// Python module source code to use to import module from memory.
    pub in_memory_source: Option<Cow<'a, [X]>>,

    /// Python module bytecode to use to import module from memory.
    pub in_memory_bytecode: Option<Cow<'a, [X]>>,

    /// Python module bytecode at optimized level 1 to use to import from memory.
    pub in_memory_bytecode_opt1: Option<Cow<'a, [X]>>,

    /// Python module bytecode at optimized level 2 to use to import from memory.
    pub in_memory_bytecode_opt2: Option<Cow<'a, [X]>>,

    /// Native machine code constituting a shared library for an extension module
    /// which can be imported from memory. (Not supported on all platforms.)
    pub in_memory_extension_module_shared_library: Option<Cow<'a, [X]>>,

    /// Mapping of virtual filename to data for resources to expose to Python's
    /// `importlib.resources` API via in-memory data access.
    pub in_memory_resources: Option<Arc<Box<HashMap<Cow<'a, str>, Cow<'a, [X]>>>>>,

    /// Mapping of virtual filename to data for package distribution metadata
    /// to expose to Python's `importlib.metadata` API via in-memory data access.
    pub in_memory_package_distribution: Option<HashMap<Cow<'a, str>, Cow<'a, [X]>>>,

    /// Native machine code constituting a shared library which can be imported from memory.
    ///
    /// In-memory loading of shared libraries is not supported on all platforms.
    pub in_memory_shared_library: Option<Cow<'a, [X]>>,

    /// Sequence of names of shared libraries this resource depends on.
    pub shared_library_dependency_names: Option<Vec<Cow<'a, str>>>,
}

impl<'a, X> Default for Resource<'a, X>
where
    [X]: ToOwned<Owned = Vec<X>>,
{
    fn default() -> Self {
        Resource {
            name: Cow::Borrowed(""),
            is_package: false,
            is_namespace_package: false,
            in_memory_source: None,
            in_memory_bytecode: None,
            in_memory_bytecode_opt1: None,
            in_memory_bytecode_opt2: None,
            in_memory_extension_module_shared_library: None,
            in_memory_resources: None,
            in_memory_package_distribution: None,
            in_memory_shared_library: None,
            shared_library_dependency_names: None,
        }
    }
}
