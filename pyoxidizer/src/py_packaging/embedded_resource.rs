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
        has_dunder_file, packages_from_module_name, packages_from_module_names, BytecodeModule,
        BytecodeOptimizationLevel, DataLocation, ExtensionModuleData, ResourceData, SourceModule,
    },
    super::standalone_distribution::ExtensionModule,
    anyhow::{Error, Result},
    lazy_static::lazy_static,
    python_packed_resources::writer::{write_embedded_resources_v1, EmbeddedResource},
    slog::warn,
    std::collections::{BTreeMap, BTreeSet},
    std::convert::TryFrom,
    std::io::Write,
    std::iter::FromIterator,
    std::path::Path,
};

lazy_static! {
    /// Python extension modules that should never be included.
    ///
    /// Ideally this data structure doesn't exist. But there are some problems
    /// with various extensions on various targets.
    pub static ref OS_IGNORE_EXTENSIONS: Vec<&'static str> = {
        let mut v = Vec::new();

        if cfg!(target_os = "linux") {
            // Linking issues.
            v.push("_crypt");

            // Linking issues.
            v.push("nis");
        }

        else if cfg!(target_os = "macos") {
            // curses and readline have linking issues.
            v.push("_curses");
            v.push("_curses_panel");
            v.push("readline");
        }

        v
    };
}

/// Represents an embedded Python module resource entry before it is packaged.
///
/// Instances hold the same fields as `EmbeddedResourcePythonModule` except
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
    pub in_memory_package_distribution: Option<BTreeMap<String, DataLocation>>,
    pub in_memory_shared_library: Option<DataLocation>,
    pub shared_library_dependency_names: Option<Vec<String>>,
}

impl TryFrom<&EmbeddedResourcePythonModulePrePackaged> for EmbeddedResource {
    type Error = Error;

    fn try_from(value: &EmbeddedResourcePythonModulePrePackaged) -> Result<Self, Self::Error> {
        Ok(Self {
            name: value.name.clone(),
            is_package: value.is_package,
            is_namespace_package: value.is_namespace_package,
            in_memory_source: if let Some(location) = &value.in_memory_source {
                Some(location.resolve()?)
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
                Some(location.resolve()?)
            } else {
                None
            },
            in_memory_resources: if let Some(resources) = &value.in_memory_resources {
                let mut res = BTreeMap::new();
                for (key, location) in resources {
                    res.insert(key.clone(), location.resolve()?);
                }
                Some(res)
            } else {
                None
            },
            in_memory_package_distribution: if let Some(resources) =
                &value.in_memory_package_distribution
            {
                let mut res = BTreeMap::new();
                for (key, location) in resources {
                    res.insert(key.clone(), location.resolve()?);
                }
                Some(res)
            } else {
                None
            },
            in_memory_shared_library: if let Some(location) = &value.in_memory_shared_library {
                Some(location.resolve()?)
            } else {
                None
            },
            shared_library_dependency_names: if let Some(names) =
                &value.shared_library_dependency_names
            {
                Some(names.clone())
            } else {
                None
            },
        })
    }
}

/// Represents Python resources to embed in a binary.
///
/// This collection holds resources before packaging. This type is
/// transformed to `EmbeddedPythonResources` as part of packaging.
#[derive(Debug, Default, Clone)]
pub struct EmbeddedPythonResourcesPrePackaged {
    modules: BTreeMap<String, EmbeddedResourcePythonModulePrePackaged>,

    // TODO combine into single extension module type.
    extension_modules: BTreeMap<String, ExtensionModule>,
    extension_module_datas: BTreeMap<String, ExtensionModuleData>,
}

impl EmbeddedPythonResourcesPrePackaged {
    /// Obtain `SourceModule` in this instance.
    pub fn get_source_modules(&self) -> BTreeMap<String, SourceModule> {
        BTreeMap::from_iter(self.modules.iter().filter_map(|(name, module)| {
            if let Some(location) = &module.in_memory_source {
                Some((
                    name.clone(),
                    SourceModule {
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
    pub fn get_bytecode_modules(&self) -> BTreeMap<String, BytecodeModule> {
        BTreeMap::from_iter(self.modules.iter().filter_map(|(name, module)| {
            if let Some(location) = &module.in_memory_bytecode {
                Some((
                    name.clone(),
                    BytecodeModule {
                        name: name.clone(),
                        is_package: module.is_package,
                        source: location.clone(),
                        optimize_level: BytecodeOptimizationLevel::Zero,
                    },
                ))
            } else if let Some(location) = &module.in_memory_bytecode_opt1 {
                Some((
                    name.clone(),
                    BytecodeModule {
                        name: name.clone(),
                        is_package: module.is_package,
                        source: location.clone(),
                        optimize_level: BytecodeOptimizationLevel::One,
                    },
                ))
            } else if let Some(location) = &module.in_memory_bytecode_opt2 {
                Some((
                    name.clone(),
                    BytecodeModule {
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
    pub fn get_resources(&self) -> BTreeMap<String, BTreeMap<String, Vec<u8>>> {
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

    /// Obtain `ExtensionModule` in this instance.
    pub fn get_extension_modules(&self) -> BTreeMap<String, ExtensionModule> {
        self.extension_modules.clone()
    }

    /// Obtain `ExtensionModuleData` in this instance.
    pub fn get_extension_module_datas(&self) -> BTreeMap<String, ExtensionModuleData> {
        self.extension_module_datas.clone()
    }

    /// Add a source module to the collection of embedded source modules.
    pub fn add_source_module(&mut self, module: &SourceModule) {
        if !self.modules.contains_key(&module.name) {
            self.modules.insert(
                module.name.clone(),
                EmbeddedResourcePythonModulePrePackaged {
                    name: module.name.clone(),
                    ..EmbeddedResourcePythonModulePrePackaged::default()
                },
            );
        }

        let mut entry = self.modules.get_mut(&module.name).unwrap();

        entry.is_package = module.is_package;
        entry.in_memory_source = Some(module.source.clone());

        // Automatically insert empty modules for missing parent packages.
        for package in packages_from_module_name(&module.name) {
            if !self.modules.contains_key(&package) {
                self.modules.insert(
                    package.clone(),
                    EmbeddedResourcePythonModulePrePackaged {
                        name: package.clone(),
                        is_package: true,
                        in_memory_source: Some(DataLocation::Memory(vec![])),
                        ..EmbeddedResourcePythonModulePrePackaged::default()
                    },
                );
            }
        }
    }

    /// Add a bytecode module to the collection of embedded bytecode modules.
    pub fn add_bytecode_module(&mut self, module: &BytecodeModule) {
        if !self.modules.contains_key(&module.name) {
            self.modules.insert(
                module.name.clone(),
                EmbeddedResourcePythonModulePrePackaged {
                    name: module.name.clone(),
                    ..EmbeddedResourcePythonModulePrePackaged::default()
                },
            );
        }

        let mut entry = self.modules.get_mut(&module.name).unwrap();
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

        // Automatically insert empty modules for missing parent packages.
        for package in packages_from_module_name(&module.name) {
            if !self.modules.contains_key(&package) {
                let mut entry = EmbeddedResourcePythonModulePrePackaged {
                    name: package.clone(),
                    is_package: true,
                    ..EmbeddedResourcePythonModulePrePackaged::default()
                };

                match module.optimize_level {
                    BytecodeOptimizationLevel::Zero => {
                        entry.in_memory_bytecode = Some(DataLocation::Memory(vec![]));
                    }
                    BytecodeOptimizationLevel::One => {
                        entry.in_memory_bytecode_opt1 = Some(DataLocation::Memory(vec![]));
                    }
                    BytecodeOptimizationLevel::Two => {
                        entry.in_memory_bytecode_opt2 = Some(DataLocation::Memory(vec![]));
                    }
                }

                self.modules.insert(package.clone(), entry);
            }
        }
    }

    /// Add resource data.
    ///
    /// Resource data belongs to a Python package and has a name and bytes data.
    pub fn add_resource(&mut self, resource: &ResourceData) {
        if !self.modules.contains_key(&resource.package) {
            self.modules.insert(
                resource.package.clone(),
                EmbeddedResourcePythonModulePrePackaged {
                    name: resource.package.clone(),
                    ..EmbeddedResourcePythonModulePrePackaged::default()
                },
            );
        }

        let mut entry = self.modules.get_mut(&resource.package).unwrap();
        entry.is_package = true;

        if entry.in_memory_resources.is_none() {
            entry.in_memory_resources = Some(BTreeMap::new());
        }

        entry
            .in_memory_resources
            .as_mut()
            .unwrap()
            .insert(resource.name.clone(), resource.data.clone());
    }

    /// Add an extension module.
    pub fn add_extension_module(&mut self, module: &ExtensionModule) {
        self.extension_modules
            .insert(module.module.clone(), module.clone());

        // Add empty bytecode for missing parent packages.
        for package in packages_from_module_name(&module.module) {
            if !self.modules.contains_key(&package) {
                self.modules.insert(
                    package.clone(),
                    EmbeddedResourcePythonModulePrePackaged {
                        name: package.clone(),
                        is_package: true,
                        // TODO should we populate opt1, opt2?
                        in_memory_bytecode: Some(DataLocation::Memory(vec![])),
                        ..EmbeddedResourcePythonModulePrePackaged::default()
                    },
                );
            }

            let mut entry = self.modules.get_mut(&package).unwrap();
            entry.is_package = true;
        }
    }

    /// Add an extension module.
    pub fn add_extension_module_data(&mut self, module: &ExtensionModuleData) {
        self.extension_module_datas
            .insert(module.name.clone(), module.clone());

        // Add empty bytecode for missing parent packages.
        for package in packages_from_module_name(&module.name) {
            if !self.modules.contains_key(&package) {
                self.modules.insert(
                    package.clone(),
                    EmbeddedResourcePythonModulePrePackaged {
                        name: package.clone(),
                        is_package: true,
                        // TODO should we populate opt1, opt2?
                        in_memory_bytecode: Some(DataLocation::Memory(vec![])),
                        ..EmbeddedResourcePythonModulePrePackaged::default()
                    },
                );
            }

            let mut entry = self.modules.get_mut(&package).unwrap();
            entry.is_package = true;
        }
    }

    /// Add an extension module shared library that should be imported from memory.
    pub fn add_in_memory_extension_module_shared_library(
        &mut self,
        module: &str,
        is_package: bool,
        data: &[u8],
    ) {
        if !self.modules.contains_key(module) {
            self.modules.insert(
                module.to_string(),
                EmbeddedResourcePythonModulePrePackaged {
                    name: module.to_string(),
                    ..EmbeddedResourcePythonModulePrePackaged::default()
                },
            );
        }

        let mut entry = self.modules.get_mut(module).unwrap();
        if is_package {
            entry.is_package = true;
        }
        entry.in_memory_extension_module_shared_library = Some(DataLocation::Memory(data.to_vec()));

        // Add empty bytecode for missing parent packages.
        for package in packages_from_module_name(module) {
            if !self.modules.contains_key(&package) {
                self.modules.insert(
                    package.clone(),
                    EmbeddedResourcePythonModulePrePackaged {
                        name: package.clone(),
                        // TODO should we populate opt1, opt2?
                        in_memory_bytecode: Some(DataLocation::Memory(vec![])),
                        ..EmbeddedResourcePythonModulePrePackaged::default()
                    },
                );
            }

            let mut entry = self.modules.get_mut(&package).unwrap();
            entry.is_package = true;
        }

        // TODO add shared library dependencies to be packaged as well.
        // TODO add shared library dependency names.
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
        filter_btreemap(logger, &mut self.extension_modules, &resource_names);

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

        let mut compiler = BytecodeCompiler::new(&python_exe)?;
        {
            for (name, module) in &self.modules {
                let mut entry = EmbeddedResource::try_from(module)?;

                if let Some(location) = &module.in_memory_bytecode {
                    entry.in_memory_bytecode = Some(compiler.compile(
                        &location.resolve()?,
                        &name,
                        BytecodeOptimizationLevel::Zero,
                        CompileMode::Bytecode,
                    )?);
                }

                if let Some(location) = &module.in_memory_bytecode_opt1 {
                    entry.in_memory_bytecode_opt1 = Some(compiler.compile(
                        &location.resolve()?,
                        &name,
                        BytecodeOptimizationLevel::One,
                        CompileMode::Bytecode,
                    )?);
                }

                if let Some(location) = &module.in_memory_bytecode_opt2 {
                    entry.in_memory_bytecode_opt2 = Some(compiler.compile(
                        &location.resolve()?,
                        &name,
                        BytecodeOptimizationLevel::Two,
                        CompileMode::Bytecode,
                    )?);
                }

                modules.insert(name.clone(), entry);
            }
        }

        let ignored = OS_IGNORE_EXTENSIONS
            .iter()
            .map(|k| (*k).to_string())
            .collect::<Vec<String>>();

        let mut extension_modules = BTreeMap::new();
        for (name, em) in &self.extension_modules {
            if ignored.contains(name) {
                continue;
            }

            if !modules.contains_key(name) {
                modules.insert(
                    name.clone(),
                    EmbeddedResource {
                        name: name.clone(),
                        ..EmbeddedResource::default()
                    },
                );
            }

            extension_modules.insert(name.clone(), em.clone());
        }

        let mut built_extension_modules = BTreeMap::new();
        for (name, em) in &self.extension_module_datas {
            if ignored.contains(name) {
                continue;
            }

            if !modules.contains_key(name) {
                modules.insert(
                    name.clone(),
                    EmbeddedResource {
                        name: name.clone(),
                        ..EmbeddedResource::default()
                    },
                );
            }

            let mut entry = modules.get_mut(name).unwrap();

            if em.is_package {
                entry.is_package = true;
            }

            built_extension_modules.insert(name.clone(), em.clone());
        }

        let derived_package_names = packages_from_module_names(modules.keys().cloned());

        for package in derived_package_names {
            if !modules.contains_key(&package) {
                modules.insert(
                    package.clone(),
                    EmbeddedResource {
                        name: package.clone(),
                        ..EmbeddedResource::default()
                    },
                );
            }

            let mut entry = modules.get_mut(&package).unwrap();

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
            modules,
            extension_modules,
            built_extension_modules,
        })
    }
}

/// Represents Python resources to embed in a binary.
#[derive(Debug, Default, Clone)]
pub struct EmbeddedPythonResources {
    /// Python modules described by an embeddable resource.
    pub modules: BTreeMap<String, EmbeddedResource>,

    // TODO combine the extension module types.
    pub extension_modules: BTreeMap<String, ExtensionModule>,
    pub built_extension_modules: BTreeMap<String, ExtensionModuleData>,
}

impl EmbeddedPythonResources {
    pub fn write_blobs<W: Write>(&self, module_names: &mut W, resources: &mut W) {
        for name in self.modules.keys() {
            module_names
                .write_all(name.as_bytes())
                .expect("failed to write");
            module_names.write_all(b"\n").expect("failed to write");
        }

        write_embedded_resources_v1(
            &self
                .modules
                .values()
                .cloned()
                .collect::<Vec<EmbeddedResource>>(),
            resources,
            None,
        )
        .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_source_module() {
        let mut r = EmbeddedPythonResourcesPrePackaged::default();
        r.add_source_module(&SourceModule {
            name: "foo".to_string(),
            source: DataLocation::Memory(vec![42]),
            is_package: false,
        });

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
    }

    #[test]
    fn test_add_source_module_parents() {
        let mut r = EmbeddedPythonResourcesPrePackaged::default();
        r.add_source_module(&SourceModule {
            name: "root.parent.child".to_string(),
            source: DataLocation::Memory(vec![42]),
            is_package: true,
        });

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
    }

    #[test]
    fn test_add_bytecode_module() {
        let mut r = EmbeddedPythonResourcesPrePackaged::default();
        r.add_bytecode_module(&BytecodeModule {
            name: "foo".to_string(),
            source: DataLocation::Memory(vec![42]),
            optimize_level: BytecodeOptimizationLevel::Zero,
            is_package: false,
        });

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
    }

    #[test]
    fn test_add_bytecode_module_parents() {
        let mut r = EmbeddedPythonResourcesPrePackaged::default();
        r.add_bytecode_module(&BytecodeModule {
            name: "root.parent.child".to_string(),
            source: DataLocation::Memory(vec![42]),
            optimize_level: BytecodeOptimizationLevel::One,
            is_package: true,
        });

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
    }

    #[test]
    fn test_add_resource() {
        let mut r = EmbeddedPythonResourcesPrePackaged::default();
        r.add_resource(&ResourceData {
            package: "foo".to_string(),
            name: "resource.txt".to_string(),
            data: DataLocation::Memory(vec![42]),
        });

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
    }

    #[test]
    fn test_add_extension_module() {
        let mut r = EmbeddedPythonResourcesPrePackaged::default();
        let em = ExtensionModule {
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

        r.add_extension_module(&em);
        assert_eq!(r.extension_modules.len(), 1);
        assert_eq!(r.extension_modules.get("foo.bar"), Some(&em));

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
    }

    #[test]
    fn test_add_extension_module_data() {
        let mut r = EmbeddedPythonResourcesPrePackaged::default();
        let em = ExtensionModuleData {
            name: "foo.bar".to_string(),
            init_fn: Some("".to_string()),
            extension_file_suffix: "".to_string(),
            extension_data: None,
            object_file_data: vec![],
            is_package: false,
            libraries: vec![],
            library_dirs: vec![],
        };

        r.add_extension_module_data(&em);
        assert_eq!(r.extension_module_datas.len(), 1);
        assert_eq!(r.extension_module_datas.get("foo.bar"), Some(&em));

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
    }

    #[test]
    fn test_find_dunder_file() -> Result<()> {
        let mut r = EmbeddedPythonResourcesPrePackaged::default();
        assert_eq!(r.find_dunder_file()?.len(), 0);

        r.add_source_module(&SourceModule {
            name: "foo.bar".to_string(),
            source: DataLocation::Memory(vec![]),
            is_package: false,
        });
        assert_eq!(r.find_dunder_file()?.len(), 0);

        r.add_source_module(&SourceModule {
            name: "baz".to_string(),
            source: DataLocation::Memory(Vec::from("import foo; if __file__ == 'ignored'")),
            is_package: false,
        });
        assert_eq!(r.find_dunder_file()?.len(), 1);
        assert!(r.find_dunder_file()?.contains("baz"));

        r.add_bytecode_module(&BytecodeModule {
            name: "bytecode".to_string(),
            source: DataLocation::Memory(Vec::from("import foo; if __file__")),
            optimize_level: BytecodeOptimizationLevel::Zero,
            is_package: false,
        });
        assert_eq!(r.find_dunder_file()?.len(), 2);
        assert!(r.find_dunder_file()?.contains("bytecode"));

        Ok(())
    }
}
