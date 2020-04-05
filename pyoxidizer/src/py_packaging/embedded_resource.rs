// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Embedded Python resources in a binary.
*/

use {
    super::bytecode::{BytecodeCompiler, CompileMode},
    super::filtering::{filter_btreemap, resolve_resource_names_from_files},
    super::resource::{
        has_dunder_file, packages_from_module_name, packages_from_module_names,
        BytecodeOptimizationLevel, DataLocation, PythonExtensionModule,
        PythonModuleBytecodeFromSource, PythonModuleSource, PythonPackageResource,
    },
    super::resources_policy::PythonResourcesPolicy,
    super::standalone_distribution::DistributionExtensionModule,
    crate::app_packaging::resource::{FileContent, FileManifest},
    anyhow::{anyhow, Error, Result},
    python_packed_resources::data::{Resource as EmbeddedResource, ResourceFlavor},
    python_packed_resources::writer::write_embedded_resources_v1,
    slog::{info, warn},
    std::borrow::Cow,
    std::collections::{BTreeMap, BTreeSet, HashMap},
    std::convert::TryFrom,
    std::io::Write,
    std::iter::FromIterator,
    std::path::{Path, PathBuf},
};

/// Represents an embedded Python module resource entry before it is packaged.
///
/// Instances hold the same fields as `EmbeddedResource` except
/// content backing fields is a `DataLocation` instead of `Vec<u8>`, since
/// it may not be available yet.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EmbeddedResourcePythonModulePrePackaged {
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

impl<'a> TryFrom<&EmbeddedResourcePythonModulePrePackaged> for EmbeddedResource<'a, u8> {
    type Error = Error;

    fn try_from(value: &EmbeddedResourcePythonModulePrePackaged) -> Result<Self, Self::Error> {
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
            in_memory_resources: if let Some(resources) = &value.in_memory_resources {
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

enum ModuleLocation {
    InMemory,
    RelativePath(String),
}

enum ResourceLocation {
    InMemory,
    RelativePath,
}

/// Holds state necessary to link an extension module into libpython.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionModuleBuildState {
    /// Extension C initialization function.
    pub init_fn: Option<String>,

    /// Object files to link into produced binary.
    pub link_object_files: Vec<DataLocation>,

    /// Frameworks this extension module needs to link against.
    pub link_frameworks: BTreeSet<String>,

    /// System libraries this extension module needs to link against.
    pub link_system_libraries: BTreeSet<String>,

    /// Static libraries this extension module needs to link against.
    pub link_static_libraries: BTreeSet<String>,

    /// Dynamic libraries this extension module needs to link against.
    pub link_dynamic_libraries: BTreeSet<String>,

    /// Dynamic libraries this extension module needs to link against.
    pub link_external_libraries: BTreeSet<String>,
}

/// Represents Python resources to embed in a binary.
///
/// This collection holds resources before packaging. This type is
/// transformed to `EmbeddedPythonResources` as part of packaging.
#[derive(Debug, Clone)]
pub struct EmbeddedPythonResourcesPrePackaged {
    policy: PythonResourcesPolicy,
    modules: BTreeMap<String, EmbeddedResourcePythonModulePrePackaged>,

    extension_module_states: BTreeMap<String, ExtensionModuleBuildState>,

    extra_files: FileManifest,
}

impl EmbeddedPythonResourcesPrePackaged {
    pub fn new(policy: &PythonResourcesPolicy) -> Self {
        Self {
            policy: policy.clone(),
            modules: BTreeMap::new(),
            extension_module_states: BTreeMap::new(),
            extra_files: FileManifest::default(),
        }
    }

    /// Obtain `SourceModule` in this instance.
    pub fn get_in_memory_module_sources(&self) -> BTreeMap<String, PythonModuleSource> {
        BTreeMap::from_iter(self.modules.iter().filter_map(|(name, module)| {
            if let Some(location) = &module.in_memory_source {
                Some((
                    name.clone(),
                    PythonModuleSource {
                        name: name.clone(),
                        is_package: module.is_package,
                        source: location.clone(),
                    },
                ))
            } else {
                None
            }
        }))
    }

    /// Obtain `BytecodeModule` in this instance.
    pub fn get_in_memory_module_bytecodes(
        &self,
    ) -> BTreeMap<String, PythonModuleBytecodeFromSource> {
        BTreeMap::from_iter(self.modules.iter().filter_map(|(name, module)| {
            if let Some(location) = &module.in_memory_bytecode {
                Some((
                    name.clone(),
                    PythonModuleBytecodeFromSource {
                        name: name.clone(),
                        is_package: module.is_package,
                        source: location.clone(),
                        optimize_level: BytecodeOptimizationLevel::Zero,
                    },
                ))
            } else if let Some(location) = &module.in_memory_bytecode_opt1 {
                Some((
                    name.clone(),
                    PythonModuleBytecodeFromSource {
                        name: name.clone(),
                        is_package: module.is_package,
                        source: location.clone(),
                        optimize_level: BytecodeOptimizationLevel::One,
                    },
                ))
            } else if let Some(location) = &module.in_memory_bytecode_opt2 {
                Some((
                    name.clone(),
                    PythonModuleBytecodeFromSource {
                        name: name.clone(),
                        is_package: module.is_package,
                        source: location.clone(),
                        optimize_level: BytecodeOptimizationLevel::Two,
                    },
                ))
            } else {
                None
            }
        }))
    }

    /// Obtain resource files in this instance.
    pub fn get_in_memory_package_resources(&self) -> BTreeMap<String, BTreeMap<String, Vec<u8>>> {
        BTreeMap::from_iter(self.modules.iter().filter_map(|(name, module)| {
            if let Some(resources) = &module.in_memory_resources {
                Some((
                    name.clone(),
                    BTreeMap::from_iter(resources.iter().map(|(key, value)| {
                        (
                            key.clone(),
                            // TODO should return a DataLocation or Result.
                            value.resolve().expect("resolved resource location"),
                        )
                    })),
                ))
            } else {
                None
            }
        }))
    }

    /// Validate that a resource add in the specified location is allowed.
    fn check_policy(&self, location: ResourceLocation) -> Result<()> {
        match self.policy {
            PythonResourcesPolicy::InMemoryOnly => match location {
                ResourceLocation::InMemory => Ok(()),
                ResourceLocation::RelativePath => Err(anyhow!(
                    "in-memory-only policy does not allow relative path resources"
                )),
            },
            PythonResourcesPolicy::FilesystemRelativeOnly(_) => match location {
                ResourceLocation::InMemory => Err(anyhow!(
                    "filesystem-relative-only policy does not allow in-memory resources"
                )),
                ResourceLocation::RelativePath => Ok(()),
            },
            PythonResourcesPolicy::PreferInMemoryFallbackFilesystemRelative(_) => Ok(()),
        }
    }

    /// Add a source module to the collection of embedded source modules.
    pub fn add_in_memory_module_source(&mut self, module: &PythonModuleSource) -> Result<()> {
        self.check_policy(ResourceLocation::InMemory)?;

        let entry = self.modules.entry(module.name.clone()).or_insert_with(|| {
            EmbeddedResourcePythonModulePrePackaged {
                name: module.name.clone(),
                ..EmbeddedResourcePythonModulePrePackaged::default()
            }
        });
        entry.is_package = module.is_package;
        entry.in_memory_source = Some(module.source.clone());

        self.add_parent_packages(&module.name, ModuleLocation::InMemory, true, None)
    }

    /// Add module source to be loaded from a file on the filesystem relative to the resources.
    pub fn add_relative_path_module_source(
        &mut self,
        module: &PythonModuleSource,
        prefix: &str,
    ) -> Result<()> {
        self.check_policy(ResourceLocation::RelativePath)?;
        let entry = self.modules.entry(module.name.clone()).or_insert_with(|| {
            EmbeddedResourcePythonModulePrePackaged {
                name: module.name.clone(),
                ..EmbeddedResourcePythonModulePrePackaged::default()
            }
        });

        entry.is_package = module.is_package;
        entry.relative_path_module_source = Some(module.resolve_path(prefix));

        module.add_to_file_manifest(&mut self.extra_files, prefix)?;

        self.add_parent_packages(
            &module.name,
            ModuleLocation::RelativePath(prefix.to_string()),
            true,
            None,
        )
    }

    /// Add a bytecode module to the collection of embedded bytecode modules.
    pub fn add_in_memory_module_bytecode(
        &mut self,
        module: &PythonModuleBytecodeFromSource,
    ) -> Result<()> {
        self.check_policy(ResourceLocation::InMemory)?;
        let entry = self.modules.entry(module.name.clone()).or_insert_with(|| {
            EmbeddedResourcePythonModulePrePackaged {
                name: module.name.clone(),
                ..EmbeddedResourcePythonModulePrePackaged::default()
            }
        });

        entry.is_package = module.is_package;

        match module.optimize_level {
            BytecodeOptimizationLevel::Zero => {
                entry.in_memory_bytecode = Some(module.source.clone());
            }
            BytecodeOptimizationLevel::One => {
                entry.in_memory_bytecode_opt1 = Some(module.source.clone());
            }
            BytecodeOptimizationLevel::Two => {
                entry.in_memory_bytecode_opt2 = Some(module.source.clone());
            }
        }

        self.add_parent_packages(
            &module.name,
            ModuleLocation::InMemory,
            false,
            Some(module.optimize_level),
        )
    }

    /// Add a bytecode module to be loaded from the filesystem relative to some entity.
    pub fn add_relative_path_module_bytecode(
        &mut self,
        module: &PythonModuleBytecodeFromSource,
        prefix: &str,
    ) -> Result<()> {
        self.check_policy(ResourceLocation::RelativePath)?;
        let entry = self.modules.entry(module.name.clone()).or_insert_with(|| {
            EmbeddedResourcePythonModulePrePackaged {
                name: module.name.clone(),
                ..EmbeddedResourcePythonModulePrePackaged::default()
            }
        });

        entry.is_package = module.is_package;

        match module.optimize_level {
            BytecodeOptimizationLevel::Zero => {
                entry.relative_path_module_bytecode =
                    Some((prefix.to_string(), module.source.clone()))
            }
            BytecodeOptimizationLevel::One => {
                entry.relative_path_module_bytecode_opt1 =
                    Some((prefix.to_string(), module.source.clone()))
            }
            BytecodeOptimizationLevel::Two => {
                entry.relative_path_module_bytecode_opt2 =
                    Some((prefix.to_string(), module.source.clone()))
            }
        }

        Ok(())
    }

    /// Add resource data.
    ///
    /// Resource data belongs to a Python package and has a name and bytes data.
    pub fn add_in_memory_package_resource(
        &mut self,
        resource: &PythonPackageResource,
    ) -> Result<()> {
        self.check_policy(ResourceLocation::InMemory)?;
        let entry = self
            .modules
            .entry(resource.leaf_package.clone())
            .or_insert_with(|| EmbeddedResourcePythonModulePrePackaged {
                name: resource.leaf_package.clone(),
                ..EmbeddedResourcePythonModulePrePackaged::default()
            });

        // Adding a resource automatically makes the module a package.
        entry.is_package = true;

        if entry.in_memory_resources.is_none() {
            entry.in_memory_resources = Some(BTreeMap::new());
        }

        entry
            .in_memory_resources
            .as_mut()
            .unwrap()
            .insert(resource.relative_name.clone(), resource.data.clone());

        self.add_parent_packages(
            &resource.leaf_package,
            ModuleLocation::InMemory,
            false,
            None,
        )
    }

    /// Add resource data to be loaded from the filesystem.
    pub fn add_relative_path_package_resource(
        &mut self,
        prefix: &str,
        resource: &PythonPackageResource,
    ) -> Result<()> {
        self.check_policy(ResourceLocation::RelativePath)?;
        let entry = self
            .modules
            .entry(resource.leaf_package.clone())
            .or_insert_with(|| EmbeddedResourcePythonModulePrePackaged {
                name: resource.leaf_package.clone(),
                ..EmbeddedResourcePythonModulePrePackaged::default()
            });

        // Adding a resource automatically makes the module a package.
        entry.is_package = true;

        if entry.relative_path_package_resources.is_none() {
            entry.relative_path_package_resources = Some(BTreeMap::new());
        }

        entry
            .relative_path_package_resources
            .as_mut()
            .unwrap()
            .insert(
                resource.relative_name.clone(),
                resource.resolve_path(prefix),
            );

        resource.add_to_file_manifest(&mut self.extra_files, prefix)?;

        self.add_parent_packages(
            &resource.leaf_package,
            ModuleLocation::RelativePath(prefix.to_string()),
            false,
            None,
        )
    }

    /// Add an extension module from a Python distribution to be linked into the binary.
    ///
    /// The extension module will have its object files linked into the produced
    /// `libpython` and the extension module will be registered in the list of
    /// the set of extension modules available for import with Python's *builtin*
    /// importer.
    pub fn add_builtin_distribution_extension_module(
        &mut self,
        module: &DistributionExtensionModule,
    ) -> Result<()> {
        // No policy check because distribution extension modules are special.

        self.extension_module_states.insert(
            module.module.clone(),
            ExtensionModuleBuildState {
                init_fn: module.init_fn.clone(),
                link_object_files: if module.builtin_default {
                    vec![]
                } else {
                    module
                        .object_paths
                        .iter()
                        .map(|p| DataLocation::Path(p.clone()))
                        .collect()
                },
                link_frameworks: BTreeSet::from_iter(module.links.iter().filter_map(|link| {
                    if link.framework {
                        Some(link.name.clone())
                    } else {
                        None
                    }
                })),
                link_system_libraries: BTreeSet::from_iter(module.links.iter().filter_map(
                    |link| {
                        if link.system {
                            Some(link.name.clone())
                        } else {
                            None
                        }
                    },
                )),
                link_static_libraries: BTreeSet::from_iter(module.links.iter().filter_map(
                    |link| {
                        if link.static_path.is_some() {
                            Some(link.name.clone())
                        } else {
                            None
                        }
                    },
                )),
                link_dynamic_libraries: BTreeSet::from_iter(module.links.iter().filter_map(
                    |link| {
                        if link.dynamic_path.is_some() {
                            Some(link.name.clone())
                        } else {
                            None
                        }
                    },
                )),
                link_external_libraries: BTreeSet::new(),
            },
        );

        // TODO should we populate opt1, opt2, source?
        self.add_parent_packages(
            &module.module,
            ModuleLocation::InMemory,
            false,
            Some(BytecodeOptimizationLevel::Zero),
        )
    }

    /// Add a distribution extension module to be loaded from in-memory import.
    pub fn add_in_memory_distribution_extension_module(
        &mut self,
        module: &DistributionExtensionModule,
    ) -> Result<()> {
        self.check_policy(ResourceLocation::InMemory)?;

        if module.shared_library.is_none() {
            return Err(anyhow!("cannot add extension module {} for in-memory loading because it lacks shared library data", module.module));
        }

        let entry = self
            .modules
            .entry(module.module.clone())
            .or_insert_with(|| EmbeddedResourcePythonModulePrePackaged {
                name: module.module.clone(),
                ..EmbeddedResourcePythonModulePrePackaged::default()
            });

        entry.is_package = false;
        entry.in_memory_extension_module_shared_library = Some(DataLocation::Path(
            module.shared_library.as_ref().unwrap().to_path_buf(),
        ));

        for link in &module.links {
            if let Some(shared_library) = &link.dynamic_path {
                // Add a resource holding the shared library data.
                let name = shared_library
                    .file_name()
                    .expect("filename on shared library")
                    .to_string_lossy();

                let resource = self.modules.entry(name.to_string()).or_insert_with(|| {
                    EmbeddedResourcePythonModulePrePackaged {
                        name: name.to_string(),
                        ..EmbeddedResourcePythonModulePrePackaged::default()
                    }
                });

                resource.in_memory_shared_library =
                    Some(DataLocation::Path(shared_library.clone()));

                // And update the extension module entry to record a library dependency.
                self.modules
                    .get_mut(&module.module)
                    .expect("entry for extension module")
                    .shared_library_dependency_names
                    .get_or_insert_with(|| Vec::new())
                    .push(name.to_string());
            }
        }

        self.add_parent_packages(
            &module.module,
            ModuleLocation::InMemory,
            false,
            Some(BytecodeOptimizationLevel::Zero),
        )
    }

    /// Add an extension module from a Python distribution to be loaded from the filesystem as a dynamic library.
    pub fn add_relative_path_distribution_extension_module(
        &mut self,
        prefix: &str,
        module: &DistributionExtensionModule,
    ) -> Result<()> {
        self.check_policy(ResourceLocation::RelativePath)?;

        if module.shared_library.is_none() {
            return Err(anyhow!(
                "cannot add extension module {} as path relative because it lacks a shared library",
                module.module
            ));
        }

        let entry = self
            .modules
            .entry(module.module.clone())
            .or_insert_with(|| EmbeddedResourcePythonModulePrePackaged {
                name: module.module.clone(),
                ..EmbeddedResourcePythonModulePrePackaged::default()
            });

        let prefix_path = PathBuf::from(prefix);
        let extension_path = module.shared_library.as_ref().unwrap();
        let install_path = prefix_path.join(extension_path.file_name().unwrap());
        let extension_data = std::fs::read(&extension_path)?;

        entry.is_package = false;
        entry.relative_path_extension_module_shared_library = Some(install_path.clone());

        self.extra_files.add_file(
            &install_path,
            &FileContent {
                data: extension_data,
                executable: false,
            },
        )?;

        for link in &module.links {
            // Install dynamic library dependencies next to extension module.
            //
            // On Windows, this should "just work" since the opening DLL's directory
            // is searched for dependencies.
            // TODO this logic likely needs to be expanded.
            if let Some(shared_library) = &link.dynamic_path {
                self.extra_files.add_file(
                    &prefix_path.join(shared_library.file_name().unwrap()),
                    &FileContent {
                        data: std::fs::read(&shared_library)?,
                        executable: false,
                    },
                )?;
            }
        }

        self.add_parent_packages(
            &module.module,
            ModuleLocation::RelativePath(prefix.to_string()),
            false,
            None,
        )
    }

    /// Add an extension module to be linked into the binary.
    ///
    /// The object files for the extension module will be linked into the produced
    /// binary and the extension module will be made available for import from
    /// Python's _builtin_ importer.
    pub fn add_builtin_extension_module(&mut self, module: &PythonExtensionModule) -> Result<()> {
        self.check_policy(ResourceLocation::InMemory)?;

        if module.object_file_data.is_empty() {
            return Err(anyhow!(
                "cannot add extension module {} as builtin because it lacks object file data",
                module.name
            ));
        }

        self.extension_module_states.insert(
            module.name.clone(),
            ExtensionModuleBuildState {
                init_fn: module.init_fn.clone(),
                link_object_files: module
                    .object_file_data
                    .iter()
                    .map(|d| DataLocation::Memory(d.clone()))
                    .collect(),
                link_frameworks: BTreeSet::new(),
                link_system_libraries: BTreeSet::new(),
                link_static_libraries: BTreeSet::new(),
                link_dynamic_libraries: BTreeSet::new(),
                link_external_libraries: BTreeSet::from_iter(module.libraries.iter().cloned()),
            },
        );

        let entry = self.modules.entry(module.name.clone()).or_insert_with(|| {
            EmbeddedResourcePythonModulePrePackaged {
                name: module.name.clone(),
                ..EmbeddedResourcePythonModulePrePackaged::default()
            }
        });

        entry.is_package = module.is_package;

        // Add empty bytecode for missing parent packages.
        // TODO should we populate opt1, opt2?
        self.add_parent_packages(
            &module.name,
            ModuleLocation::InMemory,
            false,
            Some(BytecodeOptimizationLevel::Zero),
        )
    }

    /// Add an extension module shared library that should be imported from memory.
    pub fn add_in_memory_extension_module_shared_library(
        &mut self,
        module: &str,
        is_package: bool,
        data: &[u8],
    ) -> Result<()> {
        self.check_policy(ResourceLocation::InMemory)?;
        let entry = self.modules.entry(module.to_string()).or_insert_with(|| {
            EmbeddedResourcePythonModulePrePackaged {
                name: module.to_string(),
                ..EmbeddedResourcePythonModulePrePackaged::default()
            }
        });

        if is_package {
            entry.is_package = true;
        }
        entry.in_memory_extension_module_shared_library = Some(DataLocation::Memory(data.to_vec()));

        // Add empty bytecode for missing parent packages.
        self.add_parent_packages(
            module,
            ModuleLocation::InMemory,
            false,
            Some(BytecodeOptimizationLevel::Zero),
        )

        // TODO add shared library dependencies to be packaged as well.
        // TODO add shared library dependency names.
    }

    /// Add an extension module to be loaded from the filesystem as a dynamic library.
    pub fn add_relative_path_extension_module(
        &mut self,
        em: &PythonExtensionModule,
        prefix: &str,
    ) -> Result<()> {
        self.check_policy(ResourceLocation::RelativePath)?;

        if em.extension_data.is_none() {
            return Err(anyhow!("extension module {} lacks shared library data and cannot be loaded from the filesystem", em.name));
        }

        let entry = self.modules.entry(em.name.clone()).or_insert_with(|| {
            EmbeddedResourcePythonModulePrePackaged {
                name: em.name.clone(),
                ..EmbeddedResourcePythonModulePrePackaged::default()
            }
        });
        entry.is_package = em.is_package;
        entry.relative_path_extension_module_shared_library = Some(em.resolve_path(prefix));

        em.add_to_file_manifest(&mut self.extra_files, prefix)?;

        // TODO add shared library dependencies.

        self.add_parent_packages(
            &em.name,
            ModuleLocation::RelativePath(prefix.to_string()),
            false,
            None,
        )
    }

    /// Filter the entities in this instance against names in files.
    pub fn filter_from_files(
        &mut self,
        logger: &slog::Logger,
        files: &[&Path],
        glob_patterns: &[&str],
    ) -> Result<()> {
        let resource_names = resolve_resource_names_from_files(files, glob_patterns)?;

        warn!(logger, "filtering module entries");
        filter_btreemap(logger, &mut self.modules, &resource_names);
        warn!(logger, "filtering embedded extension modules");
        filter_btreemap(logger, &mut self.extension_module_states, &resource_names);

        Ok(())
    }

    /// Searches for embedded module sources for references to __file__.
    ///
    /// __file__ usage can be problematic for in-memory modules. This method searches
    /// for its occurrences and returns module names having it present.
    pub fn find_dunder_file(&self) -> Result<BTreeSet<String>> {
        let mut res = BTreeSet::new();

        for (name, module) in &self.modules {
            if let Some(location) = &module.in_memory_source {
                if has_dunder_file(&location.resolve()?)? {
                    res.insert(name.clone());
                }
            }

            if let Some(location) = &module.in_memory_bytecode {
                if has_dunder_file(&location.resolve()?)? {
                    res.insert(name.clone());
                }
            }

            if let Some(location) = &module.in_memory_bytecode_opt1 {
                if has_dunder_file(&location.resolve()?)? {
                    res.insert(name.clone());
                }
            }

            if let Some(location) = &module.in_memory_bytecode_opt2 {
                if has_dunder_file(&location.resolve()?)? {
                    res.insert(name.clone());
                }
            }
        }

        Ok(res)
    }

    /// Transform this instance into embedded resources data.
    ///
    /// This method performs actions necessary to produce entities which will allow the
    /// resources to be embedded in a binary.
    pub fn package(
        &self,
        logger: &slog::Logger,
        python_exe: &Path,
    ) -> Result<EmbeddedPythonResources> {
        let mut file_seen = false;
        for module in self.find_dunder_file()? {
            file_seen = true;
            warn!(logger, "warning: {} contains __file__", module);
        }

        if file_seen {
            warn!(logger, "__file__ was encountered in some embedded modules");
            warn!(
                logger,
                "PyOxidizer does not set __file__ and this may create problems at run-time"
            );
            warn!(
                logger,
                "See https://github.com/indygreg/PyOxidizer/issues/69 for more"
            );
        }

        let mut modules = BTreeMap::new();
        let mut extra_files = self.extra_files.clone();

        let mut compiler = BytecodeCompiler::new(&python_exe)?;
        {
            for (name, module) in &self.modules {
                let mut entry = EmbeddedResource::try_from(module)?;

                if let Some(location) = &module.in_memory_bytecode {
                    entry.in_memory_bytecode = Some(Cow::Owned(compiler.compile(
                        &location.resolve()?,
                        &name,
                        BytecodeOptimizationLevel::Zero,
                        CompileMode::Bytecode,
                    )?));
                }

                if let Some(location) = &module.in_memory_bytecode_opt1 {
                    entry.in_memory_bytecode_opt1 = Some(Cow::Owned(compiler.compile(
                        &location.resolve()?,
                        &name,
                        BytecodeOptimizationLevel::One,
                        CompileMode::Bytecode,
                    )?));
                }

                if let Some(location) = &module.in_memory_bytecode_opt2 {
                    entry.in_memory_bytecode_opt2 = Some(Cow::Owned(compiler.compile(
                        &location.resolve()?,
                        &name,
                        BytecodeOptimizationLevel::Two,
                        CompileMode::Bytecode,
                    )?));
                }

                if let Some((prefix, location)) = &module.relative_path_module_bytecode {
                    let module = PythonModuleBytecodeFromSource {
                        name: name.clone(),
                        source: DataLocation::Memory(vec![]),
                        optimize_level: BytecodeOptimizationLevel::Zero,
                        is_package: entry.is_package,
                    };

                    let path = module.resolve_path(prefix);

                    extra_files.add_file(
                        &path,
                        &FileContent {
                            data: compiler.compile(
                                &location.resolve()?,
                                &name,
                                BytecodeOptimizationLevel::Zero,
                                CompileMode::PycUncheckedHash,
                            )?,
                            executable: false,
                        },
                    )?;

                    entry.relative_path_module_bytecode = Some(Cow::Owned(path));
                }

                if let Some((prefix, location)) = &module.relative_path_module_bytecode_opt1 {
                    let module = PythonModuleBytecodeFromSource {
                        name: name.clone(),
                        source: DataLocation::Memory(vec![]),
                        optimize_level: BytecodeOptimizationLevel::One,
                        is_package: entry.is_package,
                    };

                    let path = module.resolve_path(prefix);

                    extra_files.add_file(
                        &path,
                        &FileContent {
                            data: compiler.compile(
                                &location.resolve()?,
                                &name,
                                BytecodeOptimizationLevel::One,
                                CompileMode::PycUncheckedHash,
                            )?,
                            executable: false,
                        },
                    )?;

                    entry.relative_path_module_bytecode_opt1 = Some(Cow::Owned(path));
                }

                if let Some((prefix, location)) = &module.relative_path_module_bytecode_opt2 {
                    let module = PythonModuleBytecodeFromSource {
                        name: name.clone(),
                        source: DataLocation::Memory(vec![]),
                        optimize_level: BytecodeOptimizationLevel::Two,
                        is_package: entry.is_package,
                    };

                    let path = module.resolve_path(prefix);

                    extra_files.add_file(
                        &path,
                        &FileContent {
                            data: compiler.compile(
                                &location.resolve()?,
                                &name,
                                BytecodeOptimizationLevel::Two,
                                CompileMode::PycUncheckedHash,
                            )?,
                            executable: false,
                        },
                    )?;

                    entry.relative_path_module_bytecode_opt1 = Some(Cow::Owned(path));
                }

                modules.insert(name.clone(), entry);
            }
        }

        let mut derived_package_names = packages_from_module_names(modules.keys().cloned());
        derived_package_names.extend(packages_from_module_names(
            self.extension_module_states.keys().cloned(),
        ));

        for package in derived_package_names {
            let entry = modules
                .entry(package.clone())
                .or_insert_with(|| EmbeddedResource {
                    name: Cow::Owned(package.clone()),
                    ..EmbeddedResource::default()
                });

            if !entry.is_package {
                warn!(
                    logger,
                    "package {} not initially detected as such; possible package detection bug",
                    package
                );
                entry.is_package = true;
            }
        }

        Ok(EmbeddedPythonResources {
            resources: modules,
            extra_files,
            extension_modules: self.extension_module_states.clone(),
        })
    }

    fn add_parent_packages(
        &mut self,
        name: &str,
        location: ModuleLocation,
        add_source: bool,
        bytecode_level: Option<BytecodeOptimizationLevel>,
    ) -> Result<()> {
        for package in packages_from_module_name(name) {
            let m = self.modules.entry(package.clone()).or_insert_with(|| {
                EmbeddedResourcePythonModulePrePackaged {
                    name: package.clone(),
                    ..EmbeddedResourcePythonModulePrePackaged::default()
                }
            });

            // All parents are packages by definition.
            m.is_package = true;

            // Add empty source code if told to do so.
            if add_source {
                match location {
                    ModuleLocation::InMemory => {
                        if m.in_memory_source.is_none() {
                            m.in_memory_source = Some(DataLocation::Memory(vec![]));
                        }
                    }
                    ModuleLocation::RelativePath(ref prefix) => {
                        if m.relative_path_module_source.is_none() {
                            let module = PythonModuleSource {
                                name: package.clone(),
                                source: DataLocation::Memory(vec![]),
                                is_package: true,
                            };
                            module.add_to_file_manifest(&mut self.extra_files, prefix)?;
                            m.relative_path_module_source = Some(module.resolve_path(prefix));
                        }
                    }
                }
            }

            if let Some(level) = bytecode_level {
                match level {
                    BytecodeOptimizationLevel::Zero => match location {
                        ModuleLocation::InMemory => {
                            if m.in_memory_bytecode.is_none() {
                                m.in_memory_bytecode = Some(DataLocation::Memory(vec![]));
                            }
                        }
                        ModuleLocation::RelativePath(ref prefix) => {
                            if m.relative_path_module_bytecode.is_none() {
                                m.relative_path_module_bytecode =
                                    Some((prefix.to_string(), DataLocation::Memory(vec![])));
                            }
                        }
                    },
                    BytecodeOptimizationLevel::One => match location {
                        ModuleLocation::InMemory => {
                            if m.in_memory_bytecode_opt1.is_none() {
                                m.in_memory_bytecode_opt1 = Some(DataLocation::Memory(vec![]));
                            }
                        }
                        ModuleLocation::RelativePath(ref prefix) => {
                            if m.relative_path_module_bytecode_opt1.is_none() {
                                m.relative_path_module_bytecode_opt1 =
                                    Some((prefix.to_string(), DataLocation::Memory(vec![])));
                            }
                        }
                    },
                    BytecodeOptimizationLevel::Two => match location {
                        ModuleLocation::InMemory => {
                            if m.in_memory_bytecode_opt2.is_none() {
                                m.in_memory_bytecode_opt2 = Some(DataLocation::Memory(vec![]));
                            }
                        }
                        ModuleLocation::RelativePath(ref prefix) => {
                            if m.relative_path_module_bytecode_opt2.is_none() {
                                m.relative_path_module_bytecode_opt2 =
                                    Some((prefix.to_string(), DataLocation::Memory(vec![])));
                            }
                        }
                    },
                }
            }
        }

        Ok(())
    }
}

/// Holds state necessary to link libpython.
pub struct LibpythonLinkingInfo {
    /// Object files that need to be linked.
    pub object_files: Vec<DataLocation>,

    pub link_libraries: BTreeSet<String>,
    pub link_frameworks: BTreeSet<String>,
    pub link_system_libraries: BTreeSet<String>,
    pub link_libraries_external: BTreeSet<String>,
}

/// Represents Python resources to embed in a binary.
#[derive(Debug, Default, Clone)]
pub struct EmbeddedPythonResources<'a> {
    /// Resources to write to a packed resources data structure.
    resources: BTreeMap<String, EmbeddedResource<'a, u8>>,

    /// Additional files that need to be written out next to the produced binary.
    extra_files: FileManifest,

    /// Holds state needed for adding extension modules to libpython.
    extension_modules: BTreeMap<String, ExtensionModuleBuildState>,
}

impl<'a> EmbeddedPythonResources<'a> {
    /// Write entities defining resources.
    pub fn write_blobs<W: Write>(&self, module_names: &mut W, resources: &mut W) -> Result<()> {
        for name in self.resources.keys() {
            module_names
                .write_all(name.as_bytes())
                .expect("failed to write");
            module_names.write_all(b"\n").expect("failed to write");
        }

        write_embedded_resources_v1(
            &self
                .resources
                .values()
                .cloned()
                .collect::<Vec<EmbeddedResource<'a, u8>>>(),
            resources,
            None,
        )
    }

    /// Obtain a list of built-in extensions.
    ///
    /// The returned list will likely make its way to PyImport_Inittab.
    pub fn builtin_extensions(&self) -> Vec<(String, String)> {
        self.extension_modules
            .iter()
            .filter_map(|(name, state)| {
                if let Some(init_fn) = &state.init_fn {
                    Some((name.clone(), init_fn.clone()))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Obtain a FileManifest of extra files to install relative to the produced binary.
    pub fn extra_install_files(&self) -> Result<FileManifest> {
        let mut res = FileManifest::default();

        res.add_manifest(&self.extra_files)?;

        Ok(res)
    }

    /// Resolve state needed to link a libpython.
    pub fn resolve_libpython_linking_info(
        &self,
        logger: &slog::Logger,
    ) -> Result<LibpythonLinkingInfo> {
        let mut object_files = Vec::new();
        let mut link_libraries = BTreeSet::new();
        let mut link_frameworks = BTreeSet::new();
        let mut link_system_libraries = BTreeSet::new();
        let mut link_libraries_external = BTreeSet::new();

        warn!(
            logger,
            "resolving inputs for {} extension modules...",
            self.extension_modules.len()
        );

        for (name, state) in &self.extension_modules {
            if !state.link_object_files.is_empty() {
                info!(
                    logger,
                    "adding {} object files for {} extension module",
                    state.link_object_files.len(),
                    name
                );
                object_files.extend(state.link_object_files.iter().cloned());
            }

            for framework in &state.link_frameworks {
                warn!(logger, "framework {} required by {}", framework, name);
                link_frameworks.insert(framework.clone());
            }

            for library in &state.link_system_libraries {
                warn!(logger, "system library {} required by {}", library, name);
                link_system_libraries.insert(library.clone());
            }

            for library in &state.link_static_libraries {
                warn!(logger, "static library {} required by {}", library, name);
                link_libraries.insert(library.clone());
            }

            for library in &state.link_dynamic_libraries {
                warn!(logger, "dynamic library {} required by {}", library, name);
                link_libraries.insert(library.clone());
            }

            for library in &state.link_external_libraries {
                warn!(logger, "dynamic library {} required by {}", library, name);
                link_libraries_external.insert(library.clone());
            }
        }

        Ok(LibpythonLinkingInfo {
            object_files,
            link_libraries,
            link_frameworks,
            link_system_libraries,
            link_libraries_external,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_in_memory_source_module() -> Result<()> {
        let mut r = EmbeddedPythonResourcesPrePackaged::new(&PythonResourcesPolicy::InMemoryOnly);
        r.add_in_memory_module_source(&PythonModuleSource {
            name: "foo".to_string(),
            source: DataLocation::Memory(vec![42]),
            is_package: false,
        })?;

        assert!(r.modules.contains_key("foo"));
        assert_eq!(
            r.modules.get("foo"),
            Some(&EmbeddedResourcePythonModulePrePackaged {
                name: "foo".to_string(),
                is_package: false,
                in_memory_source: Some(DataLocation::Memory(vec![42])),
                ..EmbeddedResourcePythonModulePrePackaged::default()
            })
        );

        Ok(())
    }

    #[test]
    fn test_add_relative_path_source_module() -> Result<()> {
        let mut r = EmbeddedPythonResourcesPrePackaged::new(
            &PythonResourcesPolicy::FilesystemRelativeOnly("".to_string()),
        );
        r.add_relative_path_module_source(
            &PythonModuleSource {
                name: "foo".to_string(),
                source: DataLocation::Memory(vec![42]),
                is_package: false,
            },
            "",
        )?;

        assert!(r.modules.contains_key("foo"));
        assert_eq!(
            r.modules.get("foo"),
            Some(&EmbeddedResourcePythonModulePrePackaged {
                name: "foo".to_string(),
                is_package: false,
                relative_path_module_source: Some(PathBuf::from("foo.py")),
                ..EmbeddedResourcePythonModulePrePackaged::default()
            })
        );
        let entries = r
            .extra_files
            .entries()
            .collect::<Vec<(&PathBuf, &FileContent)>>();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, &PathBuf::from("foo.py"));
        assert_eq!(
            entries[0].1,
            &FileContent {
                data: vec![42],
                executable: false
            }
        );

        Ok(())
    }

    #[test]
    fn test_add_in_memory_source_module_parents() -> Result<()> {
        let mut r = EmbeddedPythonResourcesPrePackaged::new(&PythonResourcesPolicy::InMemoryOnly);
        r.add_in_memory_module_source(&PythonModuleSource {
            name: "root.parent.child".to_string(),
            source: DataLocation::Memory(vec![42]),
            is_package: true,
        })?;

        assert_eq!(r.modules.len(), 3);
        assert_eq!(
            r.modules.get("root.parent.child"),
            Some(&EmbeddedResourcePythonModulePrePackaged {
                name: "root.parent.child".to_string(),
                is_package: true,
                in_memory_source: Some(DataLocation::Memory(vec![42])),
                ..EmbeddedResourcePythonModulePrePackaged::default()
            })
        );

        assert_eq!(
            r.modules.get("root.parent"),
            Some(&EmbeddedResourcePythonModulePrePackaged {
                name: "root.parent".to_string(),
                is_package: true,
                in_memory_source: Some(DataLocation::Memory(vec![])),
                ..EmbeddedResourcePythonModulePrePackaged::default()
            })
        );

        assert_eq!(
            r.modules.get("root"),
            Some(&EmbeddedResourcePythonModulePrePackaged {
                name: "root".to_string(),
                is_package: true,
                in_memory_source: Some(DataLocation::Memory(vec![])),
                ..EmbeddedResourcePythonModulePrePackaged::default()
            })
        );

        Ok(())
    }

    #[test]
    fn test_add_in_memory_bytecode_module() -> Result<()> {
        let mut r = EmbeddedPythonResourcesPrePackaged::new(&PythonResourcesPolicy::InMemoryOnly);
        r.add_in_memory_module_bytecode(&PythonModuleBytecodeFromSource {
            name: "foo".to_string(),
            source: DataLocation::Memory(vec![42]),
            optimize_level: BytecodeOptimizationLevel::Zero,
            is_package: false,
        })?;

        assert!(r.modules.contains_key("foo"));
        assert_eq!(
            r.modules.get("foo"),
            Some(&EmbeddedResourcePythonModulePrePackaged {
                name: "foo".to_string(),
                in_memory_bytecode: Some(DataLocation::Memory(vec![42])),
                is_package: false,
                ..EmbeddedResourcePythonModulePrePackaged::default()
            })
        );

        Ok(())
    }

    #[test]
    fn test_add_in_memory_bytecode_module_parents() -> Result<()> {
        let mut r = EmbeddedPythonResourcesPrePackaged::new(&PythonResourcesPolicy::InMemoryOnly);
        r.add_in_memory_module_bytecode(&PythonModuleBytecodeFromSource {
            name: "root.parent.child".to_string(),
            source: DataLocation::Memory(vec![42]),
            optimize_level: BytecodeOptimizationLevel::One,
            is_package: true,
        })?;

        assert_eq!(r.modules.len(), 3);
        assert_eq!(
            r.modules.get("root.parent.child"),
            Some(&EmbeddedResourcePythonModulePrePackaged {
                name: "root.parent.child".to_string(),
                in_memory_bytecode_opt1: Some(DataLocation::Memory(vec![42])),
                is_package: true,
                ..EmbeddedResourcePythonModulePrePackaged::default()
            })
        );
        assert_eq!(
            r.modules.get("root.parent"),
            Some(&EmbeddedResourcePythonModulePrePackaged {
                name: "root.parent".to_string(),
                in_memory_bytecode_opt1: Some(DataLocation::Memory(vec![])),
                is_package: true,
                ..EmbeddedResourcePythonModulePrePackaged::default()
            })
        );
        assert_eq!(
            r.modules.get("root"),
            Some(&EmbeddedResourcePythonModulePrePackaged {
                name: "root".to_string(),
                in_memory_bytecode_opt1: Some(DataLocation::Memory(vec![])),
                is_package: true,
                ..EmbeddedResourcePythonModulePrePackaged::default()
            })
        );

        Ok(())
    }

    #[test]
    fn test_add_in_memory_resource() -> Result<()> {
        let mut r = EmbeddedPythonResourcesPrePackaged::new(&PythonResourcesPolicy::InMemoryOnly);
        r.add_in_memory_package_resource(&PythonPackageResource {
            full_name: "foo/resource.txt".to_string(),
            leaf_package: "foo".to_string(),
            relative_name: "resource.txt".to_string(),
            data: DataLocation::Memory(vec![42]),
        })?;

        assert_eq!(r.modules.len(), 1);
        assert_eq!(
            r.modules.get("foo"),
            Some(&EmbeddedResourcePythonModulePrePackaged {
                name: "foo".to_string(),
                is_package: true,
                in_memory_resources: Some(BTreeMap::from_iter(
                    [("resource.txt".to_string(), DataLocation::Memory(vec![42]))]
                        .iter()
                        .cloned()
                )),
                ..EmbeddedResourcePythonModulePrePackaged::default()
            })
        );

        Ok(())
    }

    #[test]
    fn test_add_distribution_extension_module() -> Result<()> {
        let mut r = EmbeddedPythonResourcesPrePackaged::new(&PythonResourcesPolicy::InMemoryOnly);
        let em = DistributionExtensionModule {
            module: "foo.bar".to_string(),
            init_fn: None,
            builtin_default: false,
            disableable: false,
            object_paths: vec![],
            static_library: None,
            shared_library: None,
            links: vec![],
            required: false,
            variant: "".to_string(),
            licenses: None,
            license_paths: None,
            license_public_domain: None,
        };

        r.add_builtin_distribution_extension_module(&em)?;
        assert_eq!(r.extension_module_states.len(), 1);
        assert_eq!(
            r.extension_module_states.get("foo.bar"),
            Some(&ExtensionModuleBuildState {
                init_fn: None,
                link_object_files: vec![],
                link_frameworks: BTreeSet::new(),
                link_system_libraries: BTreeSet::new(),
                link_static_libraries: BTreeSet::new(),
                link_dynamic_libraries: BTreeSet::new(),
                link_external_libraries: BTreeSet::new()
            })
        );

        assert_eq!(r.modules.len(), 1);
        assert_eq!(
            r.modules.get("foo"),
            Some(&EmbeddedResourcePythonModulePrePackaged {
                name: "foo".to_string(),
                in_memory_bytecode: Some(DataLocation::Memory(vec![])),
                is_package: true,
                ..EmbeddedResourcePythonModulePrePackaged::default()
            })
        );

        Ok(())
    }

    #[test]
    fn test_add_extension_module_data() -> Result<()> {
        let mut r = EmbeddedPythonResourcesPrePackaged::new(&PythonResourcesPolicy::InMemoryOnly);
        let em = PythonExtensionModule {
            name: "foo.bar".to_string(),
            init_fn: Some("".to_string()),
            extension_file_suffix: "".to_string(),
            extension_data: None,
            object_file_data: vec![vec![42]],
            is_package: false,
            libraries: vec![],
            library_dirs: vec![],
        };

        r.add_builtin_extension_module(&em)?;
        assert_eq!(r.extension_module_states.len(), 1);
        assert_eq!(
            r.extension_module_states.get("foo.bar"),
            Some(&ExtensionModuleBuildState {
                init_fn: Some("".to_string()),
                link_object_files: vec![DataLocation::Memory(vec![42])],
                link_frameworks: BTreeSet::new(),
                link_system_libraries: BTreeSet::new(),
                link_static_libraries: BTreeSet::new(),
                link_dynamic_libraries: BTreeSet::new(),
                link_external_libraries: BTreeSet::new()
            })
        );

        assert_eq!(r.modules.len(), 2);
        assert_eq!(
            r.modules.get("foo"),
            Some(&EmbeddedResourcePythonModulePrePackaged {
                name: "foo".to_string(),
                in_memory_bytecode: Some(DataLocation::Memory(vec![])),
                is_package: true,
                ..EmbeddedResourcePythonModulePrePackaged::default()
            })
        );

        Ok(())
    }

    #[test]
    fn test_add_relative_path_extension_module() -> Result<()> {
        let mut r = EmbeddedPythonResourcesPrePackaged::new(
            &PythonResourcesPolicy::FilesystemRelativeOnly("".to_string()),
        );
        let em = PythonExtensionModule {
            name: "foo.bar".to_string(),
            init_fn: Some("PyInit_bar".to_string()),
            extension_file_suffix: ".so".to_string(),
            extension_data: Some(DataLocation::Memory(vec![42])),
            object_file_data: vec![],
            is_package: false,
            libraries: vec![],
            library_dirs: vec![],
        };

        r.add_relative_path_extension_module(&em, "prefix")?;
        assert_eq!(r.modules.len(), 2);
        assert_eq!(
            r.modules.get("foo.bar"),
            Some(&EmbeddedResourcePythonModulePrePackaged {
                name: "foo.bar".to_string(),
                is_package: false,
                relative_path_extension_module_shared_library: Some(PathBuf::from(
                    "prefix/foo/bar.so"
                )),
                ..EmbeddedResourcePythonModulePrePackaged::default()
            })
        );

        let extra_files = r
            .extra_files
            .entries()
            .collect::<Vec<(&PathBuf, &FileContent)>>();
        assert_eq!(extra_files.len(), 1);
        assert_eq!(
            extra_files[0],
            (
                &PathBuf::from("prefix/foo/bar.so"),
                &FileContent {
                    data: vec![42],
                    executable: true
                }
            )
        );

        Ok(())
    }

    #[test]
    fn test_find_dunder_file() -> Result<()> {
        let mut r = EmbeddedPythonResourcesPrePackaged::new(&PythonResourcesPolicy::InMemoryOnly);
        assert_eq!(r.find_dunder_file()?.len(), 0);

        r.add_in_memory_module_source(&PythonModuleSource {
            name: "foo.bar".to_string(),
            source: DataLocation::Memory(vec![]),
            is_package: false,
        })?;
        assert_eq!(r.find_dunder_file()?.len(), 0);

        r.add_in_memory_module_source(&PythonModuleSource {
            name: "baz".to_string(),
            source: DataLocation::Memory(Vec::from("import foo; if __file__ == 'ignored'")),
            is_package: false,
        })?;
        assert_eq!(r.find_dunder_file()?.len(), 1);
        assert!(r.find_dunder_file()?.contains("baz"));

        r.add_in_memory_module_bytecode(&PythonModuleBytecodeFromSource {
            name: "bytecode".to_string(),
            source: DataLocation::Memory(Vec::from("import foo; if __file__")),
            optimize_level: BytecodeOptimizationLevel::Zero,
            is_package: false,
        })?;
        assert_eq!(r.find_dunder_file()?.len(), 2);
        assert!(r.find_dunder_file()?.contains("bytecode"));

        Ok(())
    }
}
