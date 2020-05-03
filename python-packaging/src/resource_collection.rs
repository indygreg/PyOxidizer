// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Functionality for collecting Python resources. */

use {
    crate::module_util::{packages_from_module_name, resolve_path_for_module},
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
    pub flavor: ResourceFlavor,
    pub name: String,
    pub is_package: bool,
    pub is_namespace_package: bool,
    pub in_memory_source: Option<DataLocation>,
    pub in_memory_bytecode_source: Option<DataLocation>,
    pub in_memory_bytecode_opt1_source: Option<DataLocation>,
    pub in_memory_bytecode_opt2_source: Option<DataLocation>,
    pub in_memory_extension_module_shared_library: Option<DataLocation>,
    pub in_memory_resources: Option<BTreeMap<String, DataLocation>>,
    pub in_memory_distribution_resources: Option<BTreeMap<String, DataLocation>>,
    pub in_memory_shared_library: Option<DataLocation>,
    pub shared_library_dependency_names: Option<Vec<String>>,
    // (prefix, source code)
    pub relative_path_module_source: Option<(String, DataLocation)>,
    // (prefix, bytecode tag, source code)
    pub relative_path_bytecode_source: Option<(String, String, DataLocation)>,
    pub relative_path_bytecode_opt1_source: Option<(String, String, DataLocation)>,
    pub relative_path_bytecode_opt2_source: Option<(String, String, DataLocation)>,
    // (prefix, path, data)
    pub relative_path_extension_module_shared_library: Option<(String, PathBuf, DataLocation)>,
    pub relative_path_package_resources: Option<BTreeMap<String, (String, PathBuf, DataLocation)>>,
    pub relative_path_distribution_resources:
        Option<BTreeMap<String, (String, PathBuf, DataLocation)>>,
    pub relative_path_shared_library: Option<(String, DataLocation)>,
}

impl<'a> TryFrom<&PrePackagedResource> for Resource<'a, u8> {
    type Error = Error;

    fn try_from(value: &PrePackagedResource) -> Result<Self, Self::Error> {
        Ok(Self {
            flavor: value.flavor,
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
            relative_path_module_source: if let Some((prefix, _)) =
                &value.relative_path_module_source
            {
                Some(Cow::Owned(resolve_path_for_module(
                    prefix,
                    &value.name,
                    value.is_package,
                    None,
                )))
            } else {
                None
            },
            // Data is stored as source that must be compiled. These fields will be populated as
            // part of packaging, as necessary.
            relative_path_module_bytecode: None,
            relative_path_module_bytecode_opt1: None,
            relative_path_module_bytecode_opt2: None,
            relative_path_extension_module_shared_library: if let Some((_, path, _)) =
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
                for (key, (_, path, _)) in resources {
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
                for (key, (_, path, _)) in resources {
                    res.insert(Cow::Owned(key.clone()), Cow::Owned(path.clone()));
                }
                Some(res)
            } else {
                None
            },
        })
    }
}

impl PrePackagedResource {
    /// Derive additional file installs to perform for filesystem-based resources.
    ///
    /// Returns 3-tuples denoting the relative resource path, data to materialize there,
    /// whether the file should be executable.
    pub fn derive_file_installs(&self) -> Result<Vec<(PathBuf, &DataLocation, bool)>> {
        let mut res = Vec::new();

        if let Some((prefix, location)) = &self.relative_path_module_source {
            res.push((
                resolve_path_for_module(prefix, &self.name, self.is_package, None),
                location,
                false,
            ));
        }

        if let Some((_, path, location)) = &self.relative_path_extension_module_shared_library {
            res.push((path.clone(), location, true));
        }

        if let Some(resources) = &self.relative_path_package_resources {
            for (_, path, location) in resources.values() {
                res.push((path.clone(), location, false));
            }
        }

        if let Some(resources) = &self.relative_path_distribution_resources {
            for (_, path, location) in resources.values() {
                res.push((path.clone(), location, false));
            }
        }

        if let Some((prefix, location)) = &self.relative_path_shared_library {
            res.push((PathBuf::from(prefix).join(&self.name), location, true));
        }

        Ok(res)
    }
}

/// Fill in missing data on parent packages.
///
/// When resources are added, their parent packages could be missing
/// data. If we simply materialized the child resources without the
/// parents, Python's importer would get confused due to the missing
/// resources.
///
/// This function fills in the blanks in our resources state.
///
/// The way this works is that if a child resource has data in
/// a particular field, we populate that field in all its parent
/// packages. If a corresponding fields is already populated, we
/// copy its data as well.
pub fn populate_parent_packages(
    resources: &mut BTreeMap<String, PrePackagedResource>,
) -> Result<()> {
    let original_resources = resources
        .iter()
        .filter_map(|(k, v)| {
            let emit = match v.flavor {
                ResourceFlavor::BuiltinExtensionModule => true,
                ResourceFlavor::Extension => true,
                ResourceFlavor::FrozenModule => true,
                ResourceFlavor::Module => true,
                ResourceFlavor::None => false,
                ResourceFlavor::SharedLibrary => false,
            };

            if emit {
                Some((k.to_owned(), v.to_owned()))
            } else {
                None
            }
        })
        .collect::<Vec<(String, PrePackagedResource)>>();

    for (name, original) in original_resources {
        for package in packages_from_module_name(&name) {
            let entry = resources
                .entry(package.clone())
                .or_insert_with(|| PrePackagedResource {
                    flavor: ResourceFlavor::Module,
                    name: package,
                    ..PrePackagedResource::default()
                });

            // Parents must be packages by definition.
            entry.is_package = true;

            // We want to materialize bytecode on parent packages no matter
            // what. If the original resource has a variant of bytecode in a
            // location, we materialize that variant on parents. We take
            // the source from the parent resource, if present. Otherwise
            // defaulting to empty.
            if original.in_memory_bytecode_source.is_some()
                && entry.in_memory_bytecode_source.is_none()
            {
                entry.in_memory_bytecode_source =
                    Some(if let Some(source) = &entry.in_memory_source {
                        source.clone()
                    } else {
                        DataLocation::Memory(vec![])
                    });
            }
            if original.in_memory_bytecode_opt1_source.is_some()
                && entry.in_memory_bytecode_opt1_source.is_none()
            {
                entry.in_memory_bytecode_opt1_source =
                    Some(if let Some(source) = &entry.in_memory_source {
                        source.clone()
                    } else {
                        DataLocation::Memory(vec![])
                    });
            }
            if original.in_memory_bytecode_opt2_source.is_some()
                && entry.in_memory_bytecode_opt2_source.is_none()
            {
                entry.in_memory_bytecode_opt2_source =
                    Some(if let Some(source) = &entry.in_memory_source {
                        source.clone()
                    } else {
                        DataLocation::Memory(vec![])
                    });
            }

            if let Some((prefix, cache_tag, _)) = &original.relative_path_bytecode_source {
                if entry.relative_path_bytecode_source.is_none() {
                    entry.relative_path_bytecode_source = Some((
                        prefix.clone(),
                        cache_tag.clone(),
                        if let Some((_, location)) = &entry.relative_path_module_source {
                            location.clone()
                        } else {
                            DataLocation::Memory(vec![])
                        },
                    ));
                }
            }

            if let Some((prefix, cache_tag, _)) = &original.relative_path_bytecode_opt1_source {
                if entry.relative_path_bytecode_opt1_source.is_none() {
                    entry.relative_path_bytecode_opt1_source = Some((
                        prefix.clone(),
                        cache_tag.clone(),
                        if let Some((_, location)) = &entry.relative_path_module_source {
                            location.clone()
                        } else {
                            DataLocation::Memory(vec![])
                        },
                    ));
                }
            }

            if let Some((prefix, cache_tag, _)) = &original.relative_path_bytecode_opt2_source {
                if entry.relative_path_bytecode_opt2_source.is_none() {
                    entry.relative_path_bytecode_opt2_source = Some((
                        prefix.clone(),
                        cache_tag.clone(),
                        if let Some((_, location)) = &entry.relative_path_module_source {
                            location.clone()
                        } else {
                            DataLocation::Memory(vec![])
                        },
                    ));
                }
            }

            // If the child had path-based source, we need to materialize source as well.
            if let Some((prefix, _)) = &original.relative_path_module_source {
                entry
                    .relative_path_module_source
                    .get_or_insert_with(|| (prefix.clone(), DataLocation::Memory(vec![])));
            }

            // Ditto for in-memory source.
            if original.in_memory_source.is_some() {
                entry
                    .in_memory_source
                    .get_or_insert(DataLocation::Memory(vec![]));
            }
        }
    }

    Ok(())
}

/// Describes the location of a Python resource.
pub enum ResourceLocation {
    /// Resource is loaded from memory.
    InMemory,
    /// Resource is loaded from a relative filesystem path.
    RelativePath,
}

/// Type used to collect Python resources to they can be serialized.
///
/// We often want to turn Python resource primitives (module source,
/// bytecode, etc) into a collection of ``Resource`` so they can be
/// serialized to the *Python packed resources* format. This type
/// exists to facilitate doing this.
#[derive(Debug, Clone)]
pub struct PythonResourceCollector {
    // TODO remove pub once functionality ported from PyOxidizer.
    pub policy: PythonResourcesPolicy,
    pub resources: BTreeMap<String, PrePackagedResource>,
    pub cache_tag: String,
}

impl PythonResourceCollector {
    /// Construct a new instance of the collector.
    ///
    /// The instance is associated with a resources policy to validate that
    /// added resources conform with rules.
    ///
    /// We also pass a Python bytecode cache tag, which is used to
    /// derive filenames.
    pub fn new(policy: &PythonResourcesPolicy, cache_tag: &str) -> Self {
        Self {
            policy: policy.clone(),
            resources: BTreeMap::new(),
            cache_tag: cache_tag.to_string(),
        }
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

    #[test]
    fn test_populate_parent_packages_in_memory_source() -> Result<()> {
        let mut h = BTreeMap::new();
        h.insert(
            "root.parent.child".to_string(),
            PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: "root.parent.child".to_string(),
                in_memory_source: Some(DataLocation::Memory(vec![42])),
                is_package: true,
                ..PrePackagedResource::default()
            },
        );

        populate_parent_packages(&mut h)?;

        assert_eq!(h.len(), 3);
        assert_eq!(
            h.get("root.parent"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: "root.parent".to_string(),
                is_package: true,
                in_memory_source: Some(DataLocation::Memory(vec![])),
                ..PrePackagedResource::default()
            })
        );
        assert_eq!(
            h.get("root"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: "root".to_string(),
                is_package: true,
                in_memory_source: Some(DataLocation::Memory(vec![])),
                ..PrePackagedResource::default()
            })
        );

        Ok(())
    }

    #[test]
    fn test_populate_parent_packages_relative_path_source() -> Result<()> {
        let mut h = BTreeMap::new();
        h.insert(
            "root.parent.child".to_string(),
            PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: "root.parent.child".to_string(),
                relative_path_module_source: Some((
                    "prefix".to_string(),
                    DataLocation::Memory(vec![42]),
                )),
                is_package: true,
                ..PrePackagedResource::default()
            },
        );

        populate_parent_packages(&mut h)?;

        assert_eq!(h.len(), 3);
        assert_eq!(
            h.get("root.parent"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: "root.parent".to_string(),
                is_package: true,
                relative_path_module_source: Some((
                    "prefix".to_string(),
                    DataLocation::Memory(vec![])
                )),
                ..PrePackagedResource::default()
            })
        );
        assert_eq!(
            h.get("root"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: "root".to_string(),
                is_package: true,
                relative_path_module_source: Some((
                    "prefix".to_string(),
                    DataLocation::Memory(vec![])
                )),
                ..PrePackagedResource::default()
            })
        );

        Ok(())
    }

    #[test]
    fn test_populate_parent_packages_in_memory_bytecode() -> Result<()> {
        let mut h = BTreeMap::new();
        h.insert(
            "root.parent.child".to_string(),
            PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: "root.parent.child".to_string(),
                in_memory_bytecode_source: Some(DataLocation::Memory(vec![42])),
                is_package: true,
                ..PrePackagedResource::default()
            },
        );

        populate_parent_packages(&mut h)?;

        assert_eq!(h.len(), 3);
        assert_eq!(
            h.get("root.parent"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: "root.parent".to_string(),
                is_package: true,
                in_memory_bytecode_source: Some(DataLocation::Memory(vec![])),
                ..PrePackagedResource::default()
            })
        );
        assert_eq!(
            h.get("root"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: "root".to_string(),
                is_package: true,
                in_memory_bytecode_source: Some(DataLocation::Memory(vec![])),
                ..PrePackagedResource::default()
            })
        );

        Ok(())
    }

    #[test]
    fn test_populate_parent_packages_distribution_extension_module() -> Result<()> {
        let mut h = BTreeMap::new();
        h.insert(
            "foo.bar".to_string(),
            PrePackagedResource {
                flavor: ResourceFlavor::BuiltinExtensionModule,
                name: "foo.bar".to_string(),
                relative_path_extension_module_shared_library: Some((
                    "prefix".to_string(),
                    PathBuf::from("prefix/foo/bar.so"),
                    DataLocation::Memory(vec![42]),
                )),
                ..PrePackagedResource::default()
            },
        );

        populate_parent_packages(&mut h)?;

        assert_eq!(
            h.get("foo"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: "foo".to_string(),
                is_package: true,
                ..PrePackagedResource::default()
            })
        );

        Ok(())
    }

    #[test]
    fn test_populate_parent_packages_relative_extension_module() -> Result<()> {
        let mut h = BTreeMap::new();
        h.insert(
            "foo.bar".to_string(),
            PrePackagedResource {
                flavor: ResourceFlavor::Extension,
                name: "foo.bar".to_string(),
                relative_path_extension_module_shared_library: Some((
                    "prefix".to_string(),
                    PathBuf::from("prefix/foo/bar.so"),
                    DataLocation::Memory(vec![42]),
                )),
                ..PrePackagedResource::default()
            },
        );

        populate_parent_packages(&mut h)?;

        assert_eq!(h.len(), 2);

        assert_eq!(
            h.get("foo"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: "foo".to_string(),
                is_package: true,
                ..PrePackagedResource::default()
            })
        );

        Ok(())
    }
}
