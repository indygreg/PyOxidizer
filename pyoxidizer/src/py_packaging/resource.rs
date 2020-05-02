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
    python_packaging::python_source::has_dunder_file,
    python_packaging::resource::{
        BytecodeOptimizationLevel, DataLocation, PythonEggFile, PythonModuleBytecode,
        PythonModuleBytecodeFromSource,
    },
    std::path::PathBuf,
};

pub trait ToPythonResource {
    /// Converts the type to a `PythonResource` instance.
    fn to_python_resource(&self) -> PythonResource;
}

/// A Python module defined via source code.
#[derive(Clone, Debug, PartialEq)]
pub struct PythonModuleSource {
    /// The fully qualified Python module name.
    pub name: String,
    /// Python source code.
    pub source: DataLocation,
    /// Whether this module is also a package.
    pub is_package: bool,
    /// Tag to apply to bytecode files.
    ///
    /// e.g. `cpython-37`.
    pub cache_tag: String,
}

impl PythonModuleSource {
    pub fn to_memory(&self) -> Result<Self> {
        Ok(Self {
            name: self.name.clone(),
            source: self.source.to_memory()?,
            is_package: self.is_package,
            cache_tag: self.cache_tag.clone(),
        })
    }

    /// Resolve the package containing this module.
    ///
    /// If this module is a package, returns the name of self.
    pub fn package(&self) -> String {
        if self.is_package {
            self.name.clone()
        } else if let Some(idx) = self.name.rfind('.') {
            self.name[0..idx].to_string()
        } else {
            self.name.clone()
        }
    }

    /// Convert the instance to a BytecodeModule.
    pub fn as_bytecode_module(
        &self,
        optimize_level: BytecodeOptimizationLevel,
    ) -> PythonModuleBytecodeFromSource {
        PythonModuleBytecodeFromSource {
            name: self.name.clone(),
            source: self.source.clone(),
            optimize_level,
            is_package: self.is_package,
            cache_tag: self.cache_tag.clone(),
        }
    }

    /// Resolve the filesystem path for this source module.
    pub fn resolve_path(&self, prefix: &str) -> PathBuf {
        resolve_path_for_module(prefix, &self.name, self.is_package, None)
    }

    /// Add this source module to a `FileManifest`.
    ///
    /// The reference added to `FileManifest` is a copy of this instance and won't
    /// reflect modification made to this instance.
    pub fn add_to_file_manifest(&self, manifest: &mut FileManifest, prefix: &str) -> Result<()> {
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

    /// Whether the source code for this module has __file__
    pub fn has_dunder_file(&self) -> Result<bool> {
        has_dunder_file(&self.source.resolve()?)
    }
}

impl ToPythonResource for PythonModuleSource {
    fn to_python_resource(&self) -> PythonResource {
        PythonResource::ModuleSource(self.clone())
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

/// Python package resource data, agnostic of storage location.
#[derive(Clone, Debug, PartialEq)]
pub struct PythonPackageResource {
    /// The full relative path to this resource from a library root.
    pub full_name: String,
    /// The leaf-most Python package this resource belongs to.
    pub leaf_package: String,
    /// The relative path within `leaf_package` to this resource.
    pub relative_name: String,
    /// Location of resource data.
    pub data: DataLocation,
}

impl PythonPackageResource {
    pub fn to_memory(&self) -> Result<Self> {
        Ok(Self {
            full_name: self.full_name.clone(),
            leaf_package: self.leaf_package.clone(),
            relative_name: self.relative_name.clone(),
            data: self.data.to_memory()?,
        })
    }

    pub fn symbolic_name(&self) -> String {
        format!("{}:{}", self.leaf_package, self.relative_name)
    }

    /// Resolve filesystem path to this bytecode.
    pub fn resolve_path(&self, prefix: &str) -> PathBuf {
        PathBuf::from(prefix).join(&self.full_name)
    }

    pub fn add_to_file_manifest(&self, manifest: &mut FileManifest, prefix: &str) -> Result<()> {
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

impl ToPythonResource for PythonPackageResource {
    fn to_python_resource(&self) -> PythonResource {
        PythonResource::Resource(self.clone())
    }
}

/// Represents where a Python package distribution resource is materialized.
#[derive(Clone, Debug, PartialEq)]
pub enum PythonPackageDistributionResourceFlavor {
    /// In a .dist-info directory.
    DistInfo,

    /// In a .egg-info directory.
    EggInfo,
}

/// Represents a file defining Python package metadata.
///
/// Instances of this correspond to files in a `<package>-<version>.dist-info`
/// or `.egg-info` directory.
///
/// In terms of `importlib.metadata` terminology, instances correspond to
/// files in a `Distribution`.
#[derive(Clone, Debug, PartialEq)]
pub struct PythonPackageDistributionResource {
    /// Where the resource is materialized.
    pub location: PythonPackageDistributionResourceFlavor,

    /// The name of the Python package this resource is associated with.
    pub package: String,

    /// Version string of Python package.
    pub version: String,

    /// Name of this resource within the distribution.
    ///
    /// Corresponds to the file name in the `.dist-info` directory for this
    /// package distribution.
    pub name: String,

    /// The raw content of the distribution resource.
    pub data: DataLocation,
}

impl PythonPackageDistributionResource {
    pub fn to_memory(&self) -> Result<Self> {
        Ok(Self {
            location: self.location.clone(),
            package: self.package.clone(),
            version: self.version.clone(),
            name: self.name.clone(),
            data: self.data.to_memory()?,
        })
    }

    /// Resolve filesystem path to this resource file.
    pub fn resolve_path(&self, prefix: &str) -> PathBuf {
        let p = match self.location {
            PythonPackageDistributionResourceFlavor::DistInfo => {
                format!("{}-{}.dist-info", self.package, self.version)
            }
            PythonPackageDistributionResourceFlavor::EggInfo => {
                format!("{}-{}.egg-info", self.package, self.version)
            }
        };

        PathBuf::from(prefix).join(p).join(&self.name)
    }

    pub fn add_to_file_manifest(&self, manifest: &mut FileManifest, prefix: &str) -> Result<()> {
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

/// Represents an extension module that can be packaged.
///
/// This is like a light version of `ExtensionModule`.
#[derive(Clone, Debug, PartialEq)]
pub struct PythonExtensionModule {
    /// The module name this extension module is providing.
    pub name: String,
    /// Name of the C function initializing this extension module.
    pub init_fn: Option<String>,
    /// Filename suffix to use when writing extension module data.
    pub extension_file_suffix: String,
    /// File data for linked extension module.
    pub extension_data: Option<DataLocation>,
    /// File data for object files linked together to produce this extension module.
    pub object_file_data: Vec<Vec<u8>>,
    /// Whether this extension module is a package.
    pub is_package: bool,
    /// Names of libraries that we need to link when building extension module.
    pub libraries: Vec<String>,
    /// Paths to directories holding libraries needed for extension module.
    pub library_dirs: Vec<PathBuf>,
}

impl PythonExtensionModule {
    pub fn to_memory(&self) -> Result<Self> {
        Ok(Self {
            name: self.name.clone(),
            init_fn: self.init_fn.clone(),
            extension_file_suffix: self.extension_file_suffix.clone(),
            extension_data: if let Some(data) = &self.extension_data {
                Some(data.to_memory()?)
            } else {
                None
            },
            object_file_data: self.object_file_data.clone(),
            is_package: self.is_package,
            libraries: self.libraries.clone(),
            library_dirs: self.library_dirs.clone(),
        })
    }

    /// The file name (without parent components) this extension module should be
    /// realized with.
    pub fn file_name(&self) -> String {
        if let Some(idx) = self.name.rfind('.') {
            let name = &self.name[idx + 1..self.name.len()];
            format!("{}{}", name, self.extension_file_suffix)
        } else {
            format!("{}{}", self.name, self.extension_file_suffix)
        }
    }

    /// Resolve the filesystem path for this extension module.
    pub fn resolve_path(&self, prefix: &str) -> PathBuf {
        let mut path = PathBuf::from(prefix);
        path.extend(self.package_parts());
        path.push(self.file_name());

        path
    }

    /// Returns the part strings constituting the package name.
    pub fn package_parts(&self) -> Vec<String> {
        if let Some(idx) = self.name.rfind('.') {
            let prefix = &self.name[0..idx];
            prefix.split('.').map(|x| x.to_string()).collect()
        } else {
            Vec::new()
        }
    }

    pub fn add_to_file_manifest(&self, manifest: &mut FileManifest, prefix: &str) -> Result<()> {
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

/// Represents a Python path extension.
///
/// i.e. a .pth file.
#[derive(Clone, Debug, PartialEq)]
pub struct PythonPathExtension {
    /// Content of the .pth file.
    pub data: DataLocation,
}

impl PythonPathExtension {
    pub fn to_memory(&self) -> Result<Self> {
        Ok(Self {
            data: self.data.to_memory()?,
        })
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
    use {super::*, itertools::Itertools};

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
