// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Functionality for defining how Python resources should be packaged.
*/

use {
    crate::resource::PythonResource, anyhow::anyhow, std::collections::HashMap,
    std::convert::TryFrom,
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

impl Into<String> for &PythonResourcesPolicy {
    fn into(self) -> String {
        match self {
            PythonResourcesPolicy::FilesystemRelativeOnly(ref prefix) => {
                format!("filesystem-relative-only:{}", prefix)
            }
            PythonResourcesPolicy::InMemoryOnly => "in-memory-only".to_string(),
            PythonResourcesPolicy::PreferInMemoryFallbackFilesystemRelative(ref prefix) => {
                format!("prefer-in-memory-fallback-filesystem-relative:{}", prefix)
            }
        }
    }
}

/// Denotes methods to filter extension modules.
#[derive(Clone, Debug, PartialEq)]
pub enum ExtensionModuleFilter {
    Minimal,
    All,
    NoLibraries,
    NoGPL,
}

impl TryFrom<&str> for ExtensionModuleFilter {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "minimal" => Ok(ExtensionModuleFilter::Minimal),
            "all" => Ok(ExtensionModuleFilter::All),
            "no-libraries" => Ok(ExtensionModuleFilter::NoLibraries),
            "no-gpl" => Ok(ExtensionModuleFilter::NoGPL),
            t => Err(format!("{} is not a valid extension module filter", t)),
        }
    }
}

/// Defines how Python resources should be packaged.
#[derive(Clone, Debug)]
pub struct PythonPackagingPolicy {
    /// Which extension modules should be included.
    pub extension_module_filter: ExtensionModuleFilter,

    /// Preferred variants of extension modules.
    pub preferred_extension_module_variants: Option<HashMap<String, String>>,

    /// Where resources should be packaged by default.
    pub resources_policy: PythonResourcesPolicy,

    /// Whether to include source modules.
    pub include_sources: bool,

    /// Whether to include package resource files.
    pub include_resources: bool,

    /// Whether to include test files.
    pub include_test: bool,
}

impl Default for PythonPackagingPolicy {
    fn default() -> Self {
        PythonPackagingPolicy {
            extension_module_filter: ExtensionModuleFilter::All,
            preferred_extension_module_variants: None,
            resources_policy: PythonResourcesPolicy::InMemoryOnly,
            include_sources: true,
            include_resources: false,
            include_test: false,
        }
    }
}

impl PythonPackagingPolicy {
    /// Determine if a Python resource is applicable to the current policy.
    ///
    /// Given a `PythonResource`, this answers the question of whether that
    /// resource meets the inclusion requirements for the current policy.
    ///
    /// Returns true if the resource should be included, false otherwise.
    pub fn filter_python_resource(&self, resource: &PythonResource) -> bool {
        match resource {
            PythonResource::ModuleSource(module) => {
                if !self.include_test && module.is_test {
                    false
                } else {
                    self.include_sources
                }
            }
            PythonResource::ModuleBytecode(_) => false,
            PythonResource::ModuleBytecodeRequest(_) => false,
            PythonResource::Resource(_) => false,
            PythonResource::DistributionResource(_) => false,
            PythonResource::ExtensionModuleDynamicLibrary(_) => false,
            PythonResource::ExtensionModuleStaticallyLinked(_) => false,
            PythonResource::PathExtension(_) => false,
            PythonResource::EggFile(_) => false,
        }
    }
}
