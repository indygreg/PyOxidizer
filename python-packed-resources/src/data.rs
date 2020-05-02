// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Declares the foundational data primitives inside packed resources data. */

use {
    std::borrow::Cow, std::collections::HashMap, std::convert::TryFrom, std::iter::FromIterator,
    std::path::Path,
};

/// Header value for version 1 of resources payload.
pub const HEADER_V1: &[u8] = b"pyembed\x01";

/// Defines the type of a resource.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ResourceFlavor {
    None = 0x00,
    Module = 0x01,
    BuiltinExtensionModule = 0x02,
    FrozenModule = 0x03,
    Extension = 0x04,
    SharedLibrary = 0x05,
}

impl Default for ResourceFlavor {
    fn default() -> Self {
        ResourceFlavor::None
    }
}

impl Into<u8> for ResourceFlavor {
    fn into(self) -> u8 {
        match self {
            ResourceFlavor::None => 0x00,
            ResourceFlavor::Module => 0x01,
            ResourceFlavor::BuiltinExtensionModule => 0x02,
            ResourceFlavor::FrozenModule => 0x03,
            ResourceFlavor::Extension => 0x04,
            ResourceFlavor::SharedLibrary => 0x05,
        }
    }
}

impl TryFrom<u8> for ResourceFlavor {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x00 => Ok(ResourceFlavor::None),
            0x01 => Ok(ResourceFlavor::Module),
            0x02 => Ok(ResourceFlavor::BuiltinExtensionModule),
            0x03 => Ok(ResourceFlavor::FrozenModule),
            0x04 => Ok(ResourceFlavor::Extension),
            0x05 => Ok(ResourceFlavor::SharedLibrary),
            _ => Err("unrecognized resource flavor"),
        }
    }
}

/// Defines interior padding mechanism between entries in blob sections.
#[derive(Clone, Copy, Debug, PartialEq)]
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
    EndOfIndex = 0x00,
    StartOfEntry = 0x01,
    EndOfEntry = 0xff,
    ResourceFieldType = 0x03,
    RawPayloadLength = 0x04,
    InteriorPadding = 0x05,
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
    EndOfIndex = 0x00,
    StartOfEntry = 0x01,
    EndOfEntry = 0xff,
    Flavor = 0x02,
    ModuleName = 0x03,
    IsPackage = 0x04,
    IsNamespacePackage = 0x05,
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
}

impl Into<u8> for ResourceField {
    fn into(self) -> u8 {
        match self {
            ResourceField::EndOfIndex => 0x00,
            ResourceField::StartOfEntry => 0x01,
            ResourceField::Flavor => 0x02,
            ResourceField::ModuleName => 0x03,
            ResourceField::IsPackage => 0x04,
            ResourceField::IsNamespacePackage => 0x05,
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
            0x02 => Ok(ResourceField::Flavor),
            0x03 => Ok(ResourceField::ModuleName),
            0x04 => Ok(ResourceField::IsPackage),
            0x05 => Ok(ResourceField::IsNamespacePackage),
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
    /// The flavor of the resource.
    pub flavor: ResourceFlavor,

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
    pub in_memory_package_resources: Option<HashMap<Cow<'a, str>, Cow<'a, [X]>>>,

    /// Mapping of virtual filename to data for package distribution metadata
    /// to expose to Python's `importlib.metadata` API via in-memory data access.
    pub in_memory_distribution_resources: Option<HashMap<Cow<'a, str>, Cow<'a, [X]>>>,

    /// Native machine code constituting a shared library which can be imported from memory.
    ///
    /// In-memory loading of shared libraries is not supported on all platforms.
    pub in_memory_shared_library: Option<Cow<'a, [X]>>,

    /// Sequence of names of shared libraries this resource depends on.
    pub shared_library_dependency_names: Option<Vec<Cow<'a, str>>>,

    /// Relative path to file containing Python module source code.
    pub relative_path_module_source: Option<Cow<'a, Path>>,

    /// Relative path to file containing Python module bytecode.
    pub relative_path_module_bytecode: Option<Cow<'a, Path>>,

    /// Relative path to file containing Python module bytecode at optimization level 1.
    pub relative_path_module_bytecode_opt1: Option<Cow<'a, Path>>,

    /// Relative path to file containing Python module bytecode at optimization level 2.
    pub relative_path_module_bytecode_opt2: Option<Cow<'a, Path>>,

    /// Relative path to file containing Python extension module loadable as a shared library.
    pub relative_path_extension_module_shared_library: Option<Cow<'a, Path>>,

    /// Mapping of Python package resource names to relative filesystem paths for those resources.
    pub relative_path_package_resources: Option<HashMap<Cow<'a, str>, Cow<'a, Path>>>,

    /// Mapping of Python package distribution files to relative filesystem paths for those resources.
    pub relative_path_distribution_resources: Option<HashMap<Cow<'a, str>, Cow<'a, Path>>>,
}

impl<'a, X> Default for Resource<'a, X>
where
    [X]: ToOwned<Owned = Vec<X>>,
{
    fn default() -> Self {
        Resource {
            flavor: ResourceFlavor::None,
            name: Cow::Borrowed(""),
            is_package: false,
            is_namespace_package: false,
            in_memory_source: None,
            in_memory_bytecode: None,
            in_memory_bytecode_opt1: None,
            in_memory_bytecode_opt2: None,
            in_memory_extension_module_shared_library: None,
            in_memory_package_resources: None,
            in_memory_distribution_resources: None,
            in_memory_shared_library: None,
            shared_library_dependency_names: None,
            relative_path_module_source: None,
            relative_path_module_bytecode: None,
            relative_path_module_bytecode_opt1: None,
            relative_path_module_bytecode_opt2: None,
            relative_path_extension_module_shared_library: None,
            relative_path_package_resources: None,
            relative_path_distribution_resources: None,
        }
    }
}

impl<'a, X> AsRef<Resource<'a, X>> for Resource<'a, X>
where
    [X]: ToOwned<Owned = Vec<X>>,
{
    fn as_ref(&self) -> &Resource<'a, X> {
        self
    }
}

impl<'a, X> Resource<'a, X>
where
    [X]: ToOwned<Owned = Vec<X>>,
{
    pub fn to_owned(&self) -> Resource<'static, X> {
        Resource {
            flavor: self.flavor,
            name: Cow::Owned(self.name.clone().into_owned()),
            is_package: self.is_package,
            is_namespace_package: self.is_namespace_package,
            in_memory_source: self
                .in_memory_source
                .as_ref()
                .map(|value| Cow::Owned(value.clone().into_owned())),
            in_memory_bytecode: self
                .in_memory_bytecode
                .as_ref()
                .map(|value| Cow::Owned(value.clone().into_owned())),
            in_memory_bytecode_opt1: self
                .in_memory_bytecode_opt1
                .as_ref()
                .map(|value| Cow::Owned(value.clone().into_owned())),
            in_memory_bytecode_opt2: self
                .in_memory_bytecode_opt2
                .as_ref()
                .map(|value| Cow::Owned(value.clone().into_owned())),
            in_memory_extension_module_shared_library: self
                .in_memory_extension_module_shared_library
                .as_ref()
                .map(|value| Cow::Owned(value.clone().into_owned())),
            in_memory_package_resources: self.in_memory_package_resources.as_ref().map(|value| {
                HashMap::from_iter(value.iter().map(|(k, v)| {
                    (
                        Cow::Owned(k.clone().into_owned()),
                        Cow::Owned(v.clone().into_owned()),
                    )
                }))
            }),
            in_memory_distribution_resources: self.in_memory_distribution_resources.as_ref().map(
                |value| {
                    HashMap::from_iter(value.iter().map(|(k, v)| {
                        (
                            Cow::Owned(k.clone().into_owned()),
                            Cow::Owned(v.clone().into_owned()),
                        )
                    }))
                },
            ),
            in_memory_shared_library: self
                .in_memory_source
                .as_ref()
                .map(|value| Cow::Owned(value.clone().into_owned())),
            shared_library_dependency_names: self.shared_library_dependency_names.as_ref().map(
                |value| Vec::from_iter(value.iter().map(|x| Cow::Owned(x.clone().into_owned()))),
            ),
            relative_path_module_source: self
                .relative_path_module_source
                .as_ref()
                .map(|value| Cow::Owned(value.clone().into_owned())),
            relative_path_module_bytecode: self
                .relative_path_module_bytecode
                .as_ref()
                .map(|value| Cow::Owned(value.clone().into_owned())),
            relative_path_module_bytecode_opt1: self
                .relative_path_module_bytecode_opt1
                .as_ref()
                .map(|value| Cow::Owned(value.clone().into_owned())),
            relative_path_module_bytecode_opt2: self
                .relative_path_module_bytecode_opt2
                .as_ref()
                .map(|value| Cow::Owned(value.clone().into_owned())),
            relative_path_extension_module_shared_library: self
                .relative_path_extension_module_shared_library
                .as_ref()
                .map(|value| Cow::Owned(value.clone().into_owned())),
            relative_path_package_resources: self.relative_path_package_resources.as_ref().map(
                |value| {
                    HashMap::from_iter(value.iter().map(|(k, v)| {
                        (
                            Cow::Owned(k.clone().into_owned()),
                            Cow::Owned(v.clone().into_owned()),
                        )
                    }))
                },
            ),
            relative_path_distribution_resources: self
                .relative_path_distribution_resources
                .as_ref()
                .map(|value| {
                    HashMap::from_iter(value.iter().map(|(k, v)| {
                        (
                            Cow::Owned(k.clone().into_owned()),
                            Cow::Owned(v.clone().into_owned()),
                        )
                    }))
                }),
        }
    }
}
