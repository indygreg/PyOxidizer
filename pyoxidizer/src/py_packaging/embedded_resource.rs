// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Embedded Python resources in a binary.
*/

use {
    super::filtering::{filter_btreemap, resolve_resource_names_from_files},
    super::standalone_distribution::DistributionExtensionModule,
    crate::app_packaging::resource::{FileContent, FileManifest},
    anyhow::{anyhow, Result},
    python_packaging::bytecode::{BytecodeCompiler, CompileMode},
    python_packaging::module_util::{packages_from_module_name, packages_from_module_names},
    python_packaging::python_source::has_dunder_file,
    python_packaging::resource::{
        BytecodeOptimizationLevel, DataLocation, PythonExtensionModule,
        PythonModuleBytecodeFromSource, PythonModuleSource, PythonPackageDistributionResource,
        PythonPackageResource,
    },
    python_packaging::resource_collection::{PrePackagedResource, PythonResourcesPolicy},
    python_packed_resources::data::{Resource, ResourceFlavor},
    python_packed_resources::writer::write_embedded_resources_v1,
    slog::{info, warn},
    std::borrow::Cow,
    std::collections::{BTreeMap, BTreeSet},
    std::convert::TryFrom,
    std::io::Write,
    std::iter::FromIterator,
    std::path::{Path, PathBuf},
};

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
pub struct PrePackagedResources {
    policy: PythonResourcesPolicy,
    resources: BTreeMap<String, PrePackagedResource>,
    cache_tag: String,

    extension_module_states: BTreeMap<String, ExtensionModuleBuildState>,
}

impl PrePackagedResources {
    pub fn new(policy: &PythonResourcesPolicy, cache_tag: &str) -> Self {
        Self {
            policy: policy.clone(),
            resources: BTreeMap::new(),
            cache_tag: cache_tag.to_string(),
            extension_module_states: BTreeMap::new(),
        }
    }

    /// Obtain `SourceModule` in this instance.
    pub fn get_in_memory_module_sources(&self) -> BTreeMap<String, PythonModuleSource> {
        BTreeMap::from_iter(self.resources.iter().filter_map(|(name, module)| {
            if let Some(location) = &module.in_memory_source {
                Some((
                    name.clone(),
                    PythonModuleSource {
                        name: name.clone(),
                        is_package: module.is_package,
                        source: location.clone(),
                        cache_tag: self.cache_tag.clone(),
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
        BTreeMap::from_iter(self.resources.iter().filter_map(|(name, module)| {
            if let Some(location) = &module.in_memory_bytecode {
                Some((
                    name.clone(),
                    PythonModuleBytecodeFromSource {
                        name: name.clone(),
                        is_package: module.is_package,
                        source: location.clone(),
                        optimize_level: BytecodeOptimizationLevel::Zero,
                        cache_tag: self.cache_tag.clone(),
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
                        cache_tag: self.cache_tag.clone(),
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
                        cache_tag: self.cache_tag.clone(),
                    },
                ))
            } else {
                None
            }
        }))
    }

    /// Obtain resource files in this instance.
    pub fn get_in_memory_package_resources(&self) -> BTreeMap<String, BTreeMap<String, Vec<u8>>> {
        BTreeMap::from_iter(self.resources.iter().filter_map(|(name, module)| {
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

        let entry = self
            .resources
            .entry(module.name.clone())
            .or_insert_with(|| PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: module.name.clone(),
                ..PrePackagedResource::default()
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
        let entry = self
            .resources
            .entry(module.name.clone())
            .or_insert_with(|| PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: module.name.clone(),
                ..PrePackagedResource::default()
            });

        entry.is_package = module.is_package;
        entry.relative_path_module_source = Some((
            prefix.to_string(),
            module.resolve_path(prefix),
            module.source.clone(),
        ));

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
        let entry = self
            .resources
            .entry(module.name.clone())
            .or_insert_with(|| PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: module.name.clone(),
                ..PrePackagedResource::default()
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
        let entry = self
            .resources
            .entry(module.name.clone())
            .or_insert_with(|| PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: module.name.clone(),
                ..PrePackagedResource::default()
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
            .resources
            .entry(resource.leaf_package.clone())
            .or_insert_with(|| PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: resource.leaf_package.clone(),
                ..PrePackagedResource::default()
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
            .resources
            .entry(resource.leaf_package.clone())
            .or_insert_with(|| PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: resource.leaf_package.clone(),
                ..PrePackagedResource::default()
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
                (
                    prefix.to_string(),
                    resource.resolve_path(prefix),
                    resource.data.clone(),
                ),
            );

        self.add_parent_packages(
            &resource.leaf_package,
            ModuleLocation::RelativePath(prefix.to_string()),
            false,
            None,
        )
    }

    /// Add a package distribution resource to be loaded from memory.
    pub fn add_in_memory_package_distribution_resource(
        &mut self,
        resource: &PythonPackageDistributionResource,
    ) -> Result<()> {
        self.check_policy(ResourceLocation::InMemory)?;

        let entry = self
            .resources
            .entry(resource.package.clone())
            .or_insert_with(|| PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: resource.package.clone(),
                ..PrePackagedResource::default()
            });

        // A distribution resource makes the entity a package.
        entry.is_package = true;

        if entry.in_memory_distribution_resources.is_none() {
            entry.in_memory_distribution_resources = Some(BTreeMap::new());
        }

        entry
            .in_memory_distribution_resources
            .as_mut()
            .unwrap()
            .insert(resource.name.clone(), resource.data.clone());

        Ok(())
    }

    pub fn add_relative_path_package_distribution_resource(
        &mut self,
        prefix: &str,
        resource: &PythonPackageDistributionResource,
    ) -> Result<()> {
        self.check_policy(ResourceLocation::RelativePath)?;
        let entry = self
            .resources
            .entry(resource.package.clone())
            .or_insert_with(|| PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: resource.package.clone(),
                ..PrePackagedResource::default()
            });

        entry.is_package = true;

        if entry.relative_path_distribution_resources.is_none() {
            entry.relative_path_distribution_resources = Some(BTreeMap::new());
        }

        entry
            .relative_path_distribution_resources
            .as_mut()
            .unwrap()
            .insert(
                resource.name.clone(),
                (
                    prefix.to_string(),
                    resource.resolve_path(prefix),
                    resource.data.clone(),
                ),
            );

        Ok(())
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
            .resources
            .entry(module.module.clone())
            .or_insert_with(|| PrePackagedResource {
                flavor: ResourceFlavor::Extension,
                name: module.module.clone(),
                ..PrePackagedResource::default()
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

                let resource =
                    self.resources
                        .entry(name.to_string())
                        .or_insert_with(|| PrePackagedResource {
                            flavor: ResourceFlavor::SharedLibrary,
                            name: name.to_string(),
                            ..PrePackagedResource::default()
                        });

                resource.in_memory_shared_library =
                    Some(DataLocation::Path(shared_library.clone()));

                // And update the extension module entry to record a library dependency.
                self.resources
                    .get_mut(&module.module)
                    .expect("entry for extension module")
                    .shared_library_dependency_names
                    .get_or_insert_with(Vec::new)
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
            .resources
            .entry(module.module.clone())
            .or_insert_with(|| PrePackagedResource {
                flavor: ResourceFlavor::Extension,
                name: module.module.clone(),
                ..PrePackagedResource::default()
            });

        let prefix_path = PathBuf::from(prefix);
        let extension_path = module.shared_library.clone().unwrap();
        let install_path = prefix_path.join(extension_path.file_name().unwrap());

        entry.is_package = false;
        entry.relative_path_extension_module_shared_library = Some((
            prefix.to_string(),
            install_path,
            DataLocation::Path(extension_path),
        ));

        for link in &module.links {
            // Install dynamic library dependencies next to extension module.
            //
            // On Windows, this should "just work" since the opening DLL's directory
            // is searched for dependencies.
            // TODO this logic likely needs to be expanded.
            if let Some(shared_library) = &link.dynamic_path {
                let resource = self.resources.entry(link.name.clone()).or_insert_with(|| {
                    PrePackagedResource {
                        flavor: ResourceFlavor::SharedLibrary,
                        name: link.name.clone(),
                        ..PrePackagedResource::default()
                    }
                });

                resource.relative_path_shared_library = Some((
                    prefix.to_string(),
                    prefix_path.join(shared_library.file_name().unwrap()),
                    DataLocation::Path(shared_library.clone()),
                ));
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

        let entry = self
            .resources
            .entry(module.name.clone())
            .or_insert_with(|| PrePackagedResource {
                flavor: ResourceFlavor::BuiltinExtensionModule,
                name: module.name.clone(),
                ..PrePackagedResource::default()
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
        let entry =
            self.resources
                .entry(module.to_string())
                .or_insert_with(|| PrePackagedResource {
                    flavor: ResourceFlavor::Extension,
                    name: module.to_string(),
                    ..PrePackagedResource::default()
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

        let entry = self
            .resources
            .entry(em.name.clone())
            .or_insert_with(|| PrePackagedResource {
                flavor: ResourceFlavor::Extension,
                name: em.name.clone(),
                ..PrePackagedResource::default()
            });
        entry.is_package = em.is_package;
        entry.relative_path_extension_module_shared_library = Some((
            prefix.to_string(),
            em.resolve_path(prefix),
            em.extension_data.as_ref().unwrap().clone(),
        ));

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
        filter_btreemap(logger, &mut self.resources, &resource_names);
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

        for (name, module) in &self.resources {
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

    fn derive_extra_files(&self) -> Result<FileManifest> {
        let mut m = FileManifest::default();

        for resource in self.resources.values() {
            if let Some((_, path, location)) = &resource.relative_path_module_source {
                m.add_file(
                    path,
                    &FileContent {
                        data: location.resolve()?,
                        executable: false,
                    },
                )?;
            }

            if let Some((_, path, location)) =
                &resource.relative_path_extension_module_shared_library
            {
                m.add_file(
                    path,
                    &FileContent {
                        data: location.resolve()?,
                        executable: true,
                    },
                )?;
            }

            if let Some(resources) = &resource.relative_path_package_resources {
                for (_, path, location) in resources.values() {
                    m.add_file(
                        path,
                        &FileContent {
                            data: location.resolve()?,
                            executable: false,
                        },
                    )?;
                }
            }

            if let Some(resources) = &resource.relative_path_distribution_resources {
                for (_, path, location) in resources.values() {
                    m.add_file(
                        path,
                        &FileContent {
                            data: location.resolve()?,
                            executable: false,
                        },
                    )?;
                }
            }

            if let Some((_, path, location)) = &resource.relative_path_shared_library {
                m.add_file(
                    path,
                    &FileContent {
                        data: location.resolve()?,
                        executable: true,
                    },
                )?;
            }
        }

        Ok(m)
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

        let mut resources = BTreeMap::new();
        let mut extra_files = self.derive_extra_files()?;

        let mut compiler = BytecodeCompiler::new(&python_exe)?;
        {
            for (name, module) in &self.resources {
                let mut entry = Resource::try_from(module)?;

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
                        cache_tag: self.cache_tag.clone(),
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
                        cache_tag: self.cache_tag.clone(),
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
                        cache_tag: self.cache_tag.clone(),
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

                resources.insert(name.clone(), entry);
            }
        }

        let mut derived_package_names = packages_from_module_names(resources.keys().cloned());
        derived_package_names.extend(packages_from_module_names(
            self.extension_module_states.keys().cloned(),
        ));

        for package in derived_package_names {
            let entry = resources
                .entry(package.clone())
                .or_insert_with(|| Resource {
                    name: Cow::Owned(package.clone()),
                    ..Resource::default()
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
            resources,
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
            let m = self
                .resources
                .entry(package.clone())
                .or_insert_with(|| PrePackagedResource {
                    flavor: ResourceFlavor::Module,
                    name: package.clone(),
                    ..PrePackagedResource::default()
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
                                cache_tag: self.cache_tag.clone(),
                            };
                            m.relative_path_module_source = Some((
                                prefix.to_string(),
                                module.resolve_path(prefix),
                                module.source,
                            ));
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
    resources: BTreeMap<String, Resource<'a, u8>>,

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
                .collect::<Vec<Resource<'a, u8>>>(),
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

    const DEFAULT_CACHE_TAG: &str = "cpython-37";

    #[test]
    fn test_add_in_memory_source_module() -> Result<()> {
        let mut r =
            PrePackagedResources::new(&PythonResourcesPolicy::InMemoryOnly, DEFAULT_CACHE_TAG);
        r.add_in_memory_module_source(&PythonModuleSource {
            name: "foo".to_string(),
            source: DataLocation::Memory(vec![42]),
            is_package: false,
            cache_tag: DEFAULT_CACHE_TAG.to_string(),
        })?;

        assert!(r.resources.contains_key("foo"));
        assert_eq!(
            r.resources.get("foo"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: "foo".to_string(),
                is_package: false,
                in_memory_source: Some(DataLocation::Memory(vec![42])),
                ..PrePackagedResource::default()
            })
        );

        Ok(())
    }

    #[test]
    fn test_add_relative_path_source_module() -> Result<()> {
        let mut r = PrePackagedResources::new(
            &PythonResourcesPolicy::FilesystemRelativeOnly("".to_string()),
            DEFAULT_CACHE_TAG,
        );
        r.add_relative_path_module_source(
            &PythonModuleSource {
                name: "foo".to_string(),
                source: DataLocation::Memory(vec![42]),
                is_package: false,
                cache_tag: DEFAULT_CACHE_TAG.to_string(),
            },
            "",
        )?;

        assert!(r.resources.contains_key("foo"));
        assert_eq!(
            r.resources.get("foo"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: "foo".to_string(),
                is_package: false,
                relative_path_module_source: Some((
                    "".to_string(),
                    PathBuf::from("foo.py"),
                    DataLocation::Memory(vec![42])
                )),
                ..PrePackagedResource::default()
            })
        );
        let m = r.derive_extra_files()?;
        let entries = m.entries().collect::<Vec<(&PathBuf, &FileContent)>>();
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
        let mut r =
            PrePackagedResources::new(&PythonResourcesPolicy::InMemoryOnly, DEFAULT_CACHE_TAG);
        r.add_in_memory_module_source(&PythonModuleSource {
            name: "root.parent.child".to_string(),
            source: DataLocation::Memory(vec![42]),
            is_package: true,
            cache_tag: DEFAULT_CACHE_TAG.to_string(),
        })?;

        assert_eq!(r.resources.len(), 3);
        assert_eq!(
            r.resources.get("root.parent.child"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: "root.parent.child".to_string(),
                is_package: true,
                in_memory_source: Some(DataLocation::Memory(vec![42])),
                ..PrePackagedResource::default()
            })
        );

        assert_eq!(
            r.resources.get("root.parent"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: "root.parent".to_string(),
                is_package: true,
                in_memory_source: Some(DataLocation::Memory(vec![])),
                ..PrePackagedResource::default()
            })
        );

        assert_eq!(
            r.resources.get("root"),
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
    fn test_add_in_memory_bytecode_module() -> Result<()> {
        let mut r =
            PrePackagedResources::new(&PythonResourcesPolicy::InMemoryOnly, DEFAULT_CACHE_TAG);
        r.add_in_memory_module_bytecode(&PythonModuleBytecodeFromSource {
            name: "foo".to_string(),
            source: DataLocation::Memory(vec![42]),
            optimize_level: BytecodeOptimizationLevel::Zero,
            is_package: false,
            cache_tag: DEFAULT_CACHE_TAG.to_string(),
        })?;

        assert!(r.resources.contains_key("foo"));
        assert_eq!(
            r.resources.get("foo"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: "foo".to_string(),
                in_memory_bytecode: Some(DataLocation::Memory(vec![42])),
                is_package: false,
                ..PrePackagedResource::default()
            })
        );

        Ok(())
    }

    #[test]
    fn test_add_in_memory_bytecode_module_parents() -> Result<()> {
        let mut r =
            PrePackagedResources::new(&PythonResourcesPolicy::InMemoryOnly, DEFAULT_CACHE_TAG);
        r.add_in_memory_module_bytecode(&PythonModuleBytecodeFromSource {
            name: "root.parent.child".to_string(),
            source: DataLocation::Memory(vec![42]),
            optimize_level: BytecodeOptimizationLevel::One,
            is_package: true,
            cache_tag: DEFAULT_CACHE_TAG.to_string(),
        })?;

        assert_eq!(r.resources.len(), 3);
        assert_eq!(
            r.resources.get("root.parent.child"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: "root.parent.child".to_string(),
                in_memory_bytecode_opt1: Some(DataLocation::Memory(vec![42])),
                is_package: true,
                ..PrePackagedResource::default()
            })
        );
        assert_eq!(
            r.resources.get("root.parent"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: "root.parent".to_string(),
                in_memory_bytecode_opt1: Some(DataLocation::Memory(vec![])),
                is_package: true,
                ..PrePackagedResource::default()
            })
        );
        assert_eq!(
            r.resources.get("root"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: "root".to_string(),
                in_memory_bytecode_opt1: Some(DataLocation::Memory(vec![])),
                is_package: true,
                ..PrePackagedResource::default()
            })
        );

        Ok(())
    }

    #[test]
    fn test_add_in_memory_resource() -> Result<()> {
        let mut r =
            PrePackagedResources::new(&PythonResourcesPolicy::InMemoryOnly, DEFAULT_CACHE_TAG);
        r.add_in_memory_package_resource(&PythonPackageResource {
            full_name: "foo/resource.txt".to_string(),
            leaf_package: "foo".to_string(),
            relative_name: "resource.txt".to_string(),
            data: DataLocation::Memory(vec![42]),
        })?;

        assert_eq!(r.resources.len(), 1);
        assert_eq!(
            r.resources.get("foo"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: "foo".to_string(),
                is_package: true,
                in_memory_resources: Some(BTreeMap::from_iter(
                    [("resource.txt".to_string(), DataLocation::Memory(vec![42]))]
                        .iter()
                        .cloned()
                )),
                ..PrePackagedResource::default()
            })
        );

        Ok(())
    }

    #[test]
    fn test_add_distribution_extension_module() -> Result<()> {
        let mut r =
            PrePackagedResources::new(&PythonResourcesPolicy::InMemoryOnly, DEFAULT_CACHE_TAG);
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

        assert_eq!(r.resources.len(), 1);
        assert_eq!(
            r.resources.get("foo"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: "foo".to_string(),
                in_memory_bytecode: Some(DataLocation::Memory(vec![])),
                is_package: true,
                ..PrePackagedResource::default()
            })
        );

        Ok(())
    }

    #[test]
    fn test_add_extension_module_data() -> Result<()> {
        let mut r =
            PrePackagedResources::new(&PythonResourcesPolicy::InMemoryOnly, DEFAULT_CACHE_TAG);
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

        assert_eq!(r.resources.len(), 2);
        assert_eq!(
            r.resources.get("foo"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: "foo".to_string(),
                in_memory_bytecode: Some(DataLocation::Memory(vec![])),
                is_package: true,
                ..PrePackagedResource::default()
            })
        );

        Ok(())
    }

    #[test]
    fn test_add_relative_path_extension_module() -> Result<()> {
        let mut r = PrePackagedResources::new(
            &PythonResourcesPolicy::FilesystemRelativeOnly("".to_string()),
            DEFAULT_CACHE_TAG,
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
        assert_eq!(r.resources.len(), 2);
        assert_eq!(
            r.resources.get("foo.bar"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::Extension,
                name: "foo.bar".to_string(),
                is_package: false,
                relative_path_extension_module_shared_library: Some((
                    "prefix".to_string(),
                    PathBuf::from("prefix/foo/bar.so"),
                    DataLocation::Memory(vec![42])
                )),
                ..PrePackagedResource::default()
            })
        );

        let m = r.derive_extra_files()?;
        let extra_files = m.entries().collect::<Vec<(&PathBuf, &FileContent)>>();
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
        let mut r =
            PrePackagedResources::new(&PythonResourcesPolicy::InMemoryOnly, DEFAULT_CACHE_TAG);
        assert_eq!(r.find_dunder_file()?.len(), 0);

        r.add_in_memory_module_source(&PythonModuleSource {
            name: "foo.bar".to_string(),
            source: DataLocation::Memory(vec![]),
            is_package: false,
            cache_tag: DEFAULT_CACHE_TAG.to_string(),
        })?;
        assert_eq!(r.find_dunder_file()?.len(), 0);

        r.add_in_memory_module_source(&PythonModuleSource {
            name: "baz".to_string(),
            source: DataLocation::Memory(Vec::from("import foo; if __file__ == 'ignored'")),
            is_package: false,
            cache_tag: DEFAULT_CACHE_TAG.to_string(),
        })?;
        assert_eq!(r.find_dunder_file()?.len(), 1);
        assert!(r.find_dunder_file()?.contains("baz"));

        r.add_in_memory_module_bytecode(&PythonModuleBytecodeFromSource {
            name: "bytecode".to_string(),
            source: DataLocation::Memory(Vec::from("import foo; if __file__")),
            optimize_level: BytecodeOptimizationLevel::Zero,
            is_package: false,
            cache_tag: DEFAULT_CACHE_TAG.to_string(),
        })?;
        assert_eq!(r.find_dunder_file()?.len(), 2);
        assert!(r.find_dunder_file()?.contains("bytecode"));

        Ok(())
    }
}
