// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Defines primitives representing Python resources.
*/

use {
    crate::app_packaging::resource::{FileContent, FileManifest},
    anyhow::Result,
    python_packaging::module_util::{packages_from_module_name, resolve_path_for_module},
    python_packaging::resource::{
        PythonEggFile, PythonExtensionModule, PythonModuleBytecode, PythonModuleBytecodeFromSource,
        PythonModuleSource, PythonPackageDistributionResource, PythonPackageResource,
        PythonPathExtension,
    },
};

pub trait ToPythonResource {
    /// Converts the type to a `PythonResource` instance.
    fn to_python_resource(&self) -> PythonResource;
}

pub trait AddToFileManifest {
    /// Add the object to a FileManifest instance.
    fn add_to_file_manifest(&self, manifest: &mut FileManifest, prefix: &str) -> Result<()>;
}

impl ToPythonResource for PythonModuleSource {
    fn to_python_resource(&self) -> PythonResource {
        PythonResource::ModuleSource(self.clone())
    }
}

impl AddToFileManifest for PythonModuleSource {
    fn add_to_file_manifest(&self, manifest: &mut FileManifest, prefix: &str) -> Result<()> {
        let content = FileContent {
            data: self.source.resolve()?,
            executable: false,
        };

        manifest.add_file(&self.resolve_path(prefix), &content)?;

        for package in packages_from_module_name(&self.name) {
            let package_path = resolve_path_for_module(prefix, &package, true, None);

            if !manifest.has_path(&package_path) {
                manifest.add_file(
                    &package_path,
                    &FileContent {
                        data: vec![],
                        executable: false,
                    },
                )?;
            }
        }

        Ok(())
    }
}

impl ToPythonResource for PythonModuleBytecodeFromSource {
    fn to_python_resource(&self) -> PythonResource {
        PythonResource::ModuleBytecodeRequest(self.clone())
    }
}

impl ToPythonResource for PythonModuleBytecode {
    fn to_python_resource(&self) -> PythonResource {
        PythonResource::ModuleBytecode(self.clone())
    }
}

impl ToPythonResource for PythonPackageResource {
    fn to_python_resource(&self) -> PythonResource {
        PythonResource::Resource(self.clone())
    }
}

impl AddToFileManifest for PythonPackageResource {
    fn add_to_file_manifest(&self, manifest: &mut FileManifest, prefix: &str) -> Result<()> {
        let dest_path = self.resolve_path(prefix);

        manifest.add_file(
            &dest_path,
            &FileContent {
                data: self.data.resolve()?,
                executable: false,
            },
        )
    }
}

impl ToPythonResource for PythonPackageDistributionResource {
    fn to_python_resource(&self) -> PythonResource {
        PythonResource::DistributionResource(self.clone())
    }
}

impl AddToFileManifest for PythonPackageDistributionResource {
    fn add_to_file_manifest(&self, manifest: &mut FileManifest, prefix: &str) -> Result<()> {
        let dest_path = self.resolve_path(prefix);

        manifest.add_file(
            &dest_path,
            &FileContent {
                data: self.data.resolve()?,
                executable: false,
            },
        )
    }
}

impl AddToFileManifest for PythonExtensionModule {
    fn add_to_file_manifest(&self, manifest: &mut FileManifest, prefix: &str) -> Result<()> {
        if let Some(data) = &self.extension_data {
            manifest.add_file(
                &self.resolve_path(prefix),
                &FileContent {
                    data: data.resolve()?,
                    executable: true,
                },
            )
        } else {
            Ok(())
        }
    }
}

/// Represents a resource that can be read by Python somehow.
#[derive(Clone, Debug, PartialEq)]
pub enum PythonResource {
    /// A module defined by source code.
    ModuleSource(PythonModuleSource),
    /// A module defined by a request to generate bytecode from source.
    ModuleBytecodeRequest(PythonModuleBytecodeFromSource),
    /// A module defined by existing bytecode.
    ModuleBytecode(PythonModuleBytecode),
    /// A non-module resource file.
    Resource(PythonPackageResource),
    /// A file in a Python package distribution metadata collection.
    DistributionResource(PythonPackageDistributionResource),
    /// An extension module that is represented by a dynamic library.
    ExtensionModuleDynamicLibrary(PythonExtensionModule),
    /// An extension module that was built from source and can be statically linked.
    ExtensionModuleStaticallyLinked(PythonExtensionModule),
    /// A self-contained Python egg.
    EggFile(PythonEggFile),
    /// A path extension.
    PathExtension(PythonPathExtension),
}

impl PythonResource {
    /// Resolves the fully qualified resource name.
    pub fn full_name(&self) -> String {
        match self {
            PythonResource::ModuleSource(m) => m.name.clone(),
            PythonResource::ModuleBytecode(m) => m.name.clone(),
            PythonResource::ModuleBytecodeRequest(m) => m.name.clone(),
            PythonResource::Resource(resource) => {
                format!("{}.{}", resource.leaf_package, resource.relative_name)
            }
            PythonResource::DistributionResource(resource) => {
                format!("{}:{}", resource.package, resource.name)
            }
            PythonResource::ExtensionModuleDynamicLibrary(em) => em.name.clone(),
            PythonResource::ExtensionModuleStaticallyLinked(em) => em.name.clone(),
            PythonResource::EggFile(_) => "".to_string(),
            PythonResource::PathExtension(_) => "".to_string(),
        }
    }

    pub fn is_in_packages(&self, packages: &[String]) -> bool {
        let name = match self {
            PythonResource::ModuleSource(m) => &m.name,
            PythonResource::ModuleBytecode(m) => &m.name,
            PythonResource::ModuleBytecodeRequest(m) => &m.name,
            PythonResource::Resource(resource) => &resource.leaf_package,
            PythonResource::DistributionResource(resource) => &resource.package,
            PythonResource::ExtensionModuleDynamicLibrary(em) => &em.name,
            PythonResource::ExtensionModuleStaticallyLinked(em) => &em.name,
            PythonResource::EggFile(_) => return false,
            PythonResource::PathExtension(_) => return false,
        };

        for package in packages {
            // Even though the entity may not be marked as a package, we allow exact
            // name matches through the filter because this makes sense for filtering.
            // The package annotation is really only useful to influence file layout,
            // when __init__.py files need to be materialized.
            if name == package || packages_from_module_name(&name).contains(package) {
                return true;
            }
        }

        false
    }

    /// Create a new instance that is guaranteed to be backed by memory.
    pub fn to_memory(&self) -> Result<Self> {
        Ok(match self {
            PythonResource::ModuleSource(m) => PythonResource::ModuleSource(m.to_memory()?),
            PythonResource::ModuleBytecode(m) => PythonResource::ModuleBytecode(m.to_memory()?),
            PythonResource::ModuleBytecodeRequest(m) => {
                PythonResource::ModuleBytecodeRequest(m.to_memory()?)
            }
            PythonResource::Resource(r) => PythonResource::Resource(r.to_memory()?),
            PythonResource::DistributionResource(r) => {
                PythonResource::DistributionResource(r.to_memory()?)
            }
            PythonResource::ExtensionModuleDynamicLibrary(m) => {
                PythonResource::ExtensionModuleDynamicLibrary(m.to_memory()?)
            }
            PythonResource::ExtensionModuleStaticallyLinked(m) => {
                PythonResource::ExtensionModuleStaticallyLinked(m.to_memory()?)
            }
            PythonResource::EggFile(e) => PythonResource::EggFile(e.to_memory()?),
            PythonResource::PathExtension(e) => PythonResource::PathExtension(e.to_memory()?),
        })
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        itertools::Itertools,
        python_packaging::resource::{BytecodeOptimizationLevel, DataLocation},
        std::path::PathBuf,
    };

    const DEFAULT_CACHE_TAG: &str = "cpython-37";

    #[test]
    fn test_source_module_add_to_manifest_top_level() -> Result<()> {
        let mut m = FileManifest::default();

        PythonModuleSource {
            name: "foo".to_string(),
            source: DataLocation::Memory(vec![]),
            is_package: false,
            cache_tag: DEFAULT_CACHE_TAG.to_string(),
        }
        .add_to_file_manifest(&mut m, ".")?;

        PythonModuleSource {
            name: "bar".to_string(),
            source: DataLocation::Memory(vec![]),
            is_package: false,
            cache_tag: DEFAULT_CACHE_TAG.to_string(),
        }
        .add_to_file_manifest(&mut m, ".")?;

        let entries = m.entries().collect_vec();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].0, &PathBuf::from("./bar.py"));
        assert_eq!(entries[1].0, &PathBuf::from("./foo.py"));

        Ok(())
    }

    #[test]
    fn test_source_module_add_to_manifest_top_level_package() -> Result<()> {
        let mut m = FileManifest::default();

        PythonModuleSource {
            name: "foo".to_string(),
            source: DataLocation::Memory(vec![]),
            is_package: true,
            cache_tag: DEFAULT_CACHE_TAG.to_string(),
        }
        .add_to_file_manifest(&mut m, ".")?;

        let entries = m.entries().collect_vec();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, &PathBuf::from("./foo/__init__.py"));

        Ok(())
    }

    #[test]
    fn test_source_module_add_to_manifest_missing_parent() -> Result<()> {
        let mut m = FileManifest::default();

        PythonModuleSource {
            name: "root.parent.child".to_string(),
            source: DataLocation::Memory(vec![]),
            is_package: false,
            cache_tag: DEFAULT_CACHE_TAG.to_string(),
        }
        .add_to_file_manifest(&mut m, ".")?;

        let entries = m.entries().collect_vec();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].0, &PathBuf::from("./root/__init__.py"));
        assert_eq!(entries[1].0, &PathBuf::from("./root/parent/__init__.py"));
        assert_eq!(entries[2].0, &PathBuf::from("./root/parent/child.py"));

        Ok(())
    }

    #[test]
    fn test_source_module_add_to_manifest_missing_parent_package() -> Result<()> {
        let mut m = FileManifest::default();

        PythonModuleSource {
            name: "root.parent.child".to_string(),
            source: DataLocation::Memory(vec![]),
            is_package: true,
            cache_tag: DEFAULT_CACHE_TAG.to_string(),
        }
        .add_to_file_manifest(&mut m, ".")?;

        let entries = m.entries().collect_vec();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].0, &PathBuf::from("./root/__init__.py"));
        assert_eq!(entries[1].0, &PathBuf::from("./root/parent/__init__.py"));
        assert_eq!(
            entries[2].0,
            &PathBuf::from("./root/parent/child/__init__.py")
        );

        Ok(())
    }

    #[test]
    fn test_is_in_packages() {
        let source = PythonResource::ModuleSource(PythonModuleSource {
            name: "foo".to_string(),
            source: DataLocation::Memory(vec![]),
            is_package: false,
            cache_tag: DEFAULT_CACHE_TAG.to_string(),
        });
        assert!(source.is_in_packages(&["foo".to_string()]));
        assert!(!source.is_in_packages(&[]));
        assert!(!source.is_in_packages(&["bar".to_string()]));

        let bytecode = PythonResource::ModuleBytecode(PythonModuleBytecode {
            name: "foo".to_string(),
            bytecode: DataLocation::Memory(vec![]),
            optimize_level: BytecodeOptimizationLevel::Zero,
            is_package: false,
        });
        assert!(bytecode.is_in_packages(&["foo".to_string()]));
        assert!(!bytecode.is_in_packages(&[]));
        assert!(!bytecode.is_in_packages(&["bar".to_string()]));
    }
}
