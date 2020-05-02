// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Functionality for collecting Python resources. */

use {
    crate::resource::DataLocation,
    anyhow::{anyhow, Error, Result},
    python_packed_resources::data::{Resource, ResourceFlavor},
    std::borrow::Cow,
    std::collections::{BTreeMap, HashMap},
    std::convert::TryFrom,
    std::path::PathBuf,
};

/// Describes a policy for the location of Python resources.
#[derive(Clone, Debug, PartialEq)]
pub enum PythonResourcesPolicy {
    /// Only allow Python resources to be loaded from memory.
    ///
    /// If a resource cannot be loaded from memory, attempting to add it should result in
    /// error.
    InMemoryOnly,

    /// Only allow Python resources to be loaded from a filesystem path relative to the binary.
    ///
    /// The `String` represents the path prefix to install resources into.
    FilesystemRelativeOnly(String),

    /// Prefer loading resources from memory and fall back to filesystem relative loading.
    ///
    /// This is a hybrid between `InMemoryOnly` and `FilesystemRelativeOnly`. If
    /// in-memory loading works, it is used. Otherwise loading from a filesystem path
    /// relative to the produced binary is used.
    PreferInMemoryFallbackFilesystemRelative(String),
}

impl TryFrom<&str> for PythonResourcesPolicy {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        if value == "in-memory-only" {
            Ok(PythonResourcesPolicy::InMemoryOnly)
        } else if value.starts_with("filesystem-relative-only:") {
            let prefix = &value["filesystem-relative-only:".len()..];

            Ok(PythonResourcesPolicy::FilesystemRelativeOnly(
                prefix.to_string(),
            ))
        } else if value.starts_with("prefer-in-memory-fallback-filesystem-relative:") {
            let prefix = &value["prefer-in-memory-fallback-filesystem-relative:".len()..];

            Ok(PythonResourcesPolicy::PreferInMemoryFallbackFilesystemRelative(prefix.to_string()))
        } else {
            Err(anyhow!(
                "invalid value for Python Resources Policy: {}",
                value
            ))
        }
    }
}

/// Represents a Python resource entry before it is packaged.
///
/// Instances hold the same fields as `Resource` except fields holding
/// content are backed by a `DataLocation` instead of `Vec<u8>`, since
/// we want data resolution to be lazy.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PrePackagedResource {
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
    pub in_memory_distribution_resources: Option<BTreeMap<String, DataLocation>>,
    pub in_memory_shared_library: Option<DataLocation>,
    pub shared_library_dependency_names: Option<Vec<String>>,
    pub relative_path_module_source: Option<PathBuf>,
    // (prefix, source code)
    pub relative_path_module_bytecode: Option<(String, DataLocation)>,
    pub relative_path_module_bytecode_opt1: Option<(String, DataLocation)>,
    pub relative_path_module_bytecode_opt2: Option<(String, DataLocation)>,
    pub relative_path_extension_module_shared_library: Option<PathBuf>,
    pub relative_path_package_resources: Option<BTreeMap<String, PathBuf>>,
    pub relative_path_distribution_resources: Option<BTreeMap<String, PathBuf>>,
}

impl<'a> TryFrom<&PrePackagedResource> for Resource<'a, u8> {
    type Error = Error;

    fn try_from(value: &PrePackagedResource) -> Result<Self, Self::Error> {
        Ok(Self {
            flavor: if value.in_memory_extension_module_shared_library.is_some()
                || value
                    .relative_path_extension_module_shared_library
                    .is_some()
            {
                ResourceFlavor::Extension
            } else if value.in_memory_shared_library.is_some() {
                ResourceFlavor::SharedLibrary
            } else {
                ResourceFlavor::Module
            },
            name: Cow::Owned(value.name.clone()),
            is_package: value.is_package,
            is_namespace_package: value.is_namespace_package,
            in_memory_source: if let Some(location) = &value.in_memory_source {
                Some(Cow::Owned(location.resolve()?))
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
                Some(Cow::Owned(location.resolve()?))
            } else {
                None
            },
            in_memory_package_resources: if let Some(resources) = &value.in_memory_resources {
                let mut res = HashMap::new();
                for (key, location) in resources {
                    res.insert(Cow::Owned(key.clone()), Cow::Owned(location.resolve()?));
                }
                Some(res)
            } else {
                None
            },
            in_memory_distribution_resources: if let Some(resources) =
                &value.in_memory_distribution_resources
            {
                let mut res = HashMap::new();
                for (key, location) in resources {
                    res.insert(Cow::Owned(key.clone()), Cow::Owned(location.resolve()?));
                }
                Some(res)
            } else {
                None
            },
            in_memory_shared_library: if let Some(location) = &value.in_memory_shared_library {
                Some(Cow::Owned(location.resolve()?))
            } else {
                None
            },
            shared_library_dependency_names: if let Some(names) =
                &value.shared_library_dependency_names
            {
                Some(names.iter().map(|x| Cow::Owned(x.clone())).collect())
            } else {
                None
            },
            relative_path_module_source: if let Some(path) = &value.relative_path_module_source {
                Some(Cow::Owned(path.clone()))
            } else {
                None
            },
            // Data is stored as source that must be compiled. These fields will be populated as
            // part of packaging, as necessary.
            relative_path_module_bytecode: None,
            relative_path_module_bytecode_opt1: None,
            relative_path_module_bytecode_opt2: None,
            relative_path_extension_module_shared_library: if let Some(path) =
                &value.relative_path_extension_module_shared_library
            {
                Some(Cow::Owned(path.clone()))
            } else {
                None
            },
            relative_path_package_resources: if let Some(resources) =
                &value.relative_path_package_resources
            {
                let mut res = HashMap::new();
                for (key, path) in resources {
                    res.insert(Cow::Owned(key.clone()), Cow::Owned(path.clone()));
                }
                Some(res)
            } else {
                None
            },
            relative_path_distribution_resources: if let Some(resources) =
                &value.relative_path_distribution_resources
            {
                let mut res = HashMap::new();
                for (key, path) in resources {
                    res.insert(Cow::Owned(key.clone()), Cow::Owned(path.clone()));
                }
                Some(res)
            } else {
                None
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_policy_from_str() -> Result<()> {
        assert_eq!(
            PythonResourcesPolicy::try_from("in-memory-only")?,
            PythonResourcesPolicy::InMemoryOnly
        );
        assert_eq!(
            PythonResourcesPolicy::try_from("filesystem-relative-only:lib")?,
            PythonResourcesPolicy::FilesystemRelativeOnly("lib".to_string())
        );
        assert_eq!(
            PythonResourcesPolicy::try_from("prefer-in-memory-fallback-filesystem-relative:lib")?,
            PythonResourcesPolicy::PreferInMemoryFallbackFilesystemRelative("lib".to_string())
        );
        assert_eq!(
            PythonResourcesPolicy::try_from("foo")
                .unwrap_err()
                .to_string(),
            "invalid value for Python Resources Policy: foo"
        );

        Ok(())
    }
}
