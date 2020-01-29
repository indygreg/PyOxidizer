// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Embedded Python resources in a binary.
*/

use {
    super::bytecode::{BytecodeCompiler, CompileMode},
    super::distribution::is_stdlib_test_package,
    super::filtering::{filter_btreemap, resolve_resource_names_from_files},
    super::resource::{
        packages_from_module_name, packages_from_module_names, BytecodeModule,
        BytecodeOptimizationLevel, DataLocation, ExtensionModuleData, PackagedModuleBytecode,
        PackagedModuleSource, ResourceData, SourceModule,
    },
    super::standalone_distribution::{
        ExtensionModule, ExtensionModuleFilter, ParsedPythonDistribution,
    },
    anyhow::Result,
    byteorder::{LittleEndian, WriteBytesExt},
    lazy_static::lazy_static,
    slog::warn,
    std::collections::{BTreeMap, BTreeSet, HashMap},
    std::io::Write,
    std::path::Path,
    std::sync::Arc,
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

/// Represents Python resources to embed in a binary.
///
/// This collection holds resources before packaging. This type is
/// transformed to `EmbeddedPythonResources` as part of packaging.
#[derive(Debug, Default, Clone)]
pub struct EmbeddedPythonResourcesPrePackaged {
    pub source_modules: BTreeMap<String, SourceModule>,
    pub bytecode_modules: BTreeMap<String, BytecodeModule>,
    pub resources: BTreeMap<String, BTreeMap<String, Vec<u8>>>,
    // TODO combine into single extension module type.
    pub extension_modules: BTreeMap<String, ExtensionModule>,
    pub extension_module_datas: BTreeMap<String, ExtensionModuleData>,
}

impl EmbeddedPythonResourcesPrePackaged {
    pub fn from_distribution(
        logger: &slog::Logger,
        distribution: Arc<ParsedPythonDistribution>,
        extension_module_filter: &ExtensionModuleFilter,
        preferred_extension_module_variants: Option<HashMap<String, String>>,
        include_sources: bool,
        include_resources: bool,
        include_test: bool,
    ) -> Result<EmbeddedPythonResourcesPrePackaged> {
        let mut embedded = EmbeddedPythonResourcesPrePackaged::default();

        for ext in distribution.filter_extension_modules(
            logger,
            extension_module_filter,
            preferred_extension_module_variants,
        )? {
            embedded.add_extension_module(&ext);
        }

        for source in distribution.source_modules()? {
            if !include_test && is_stdlib_test_package(&source.package()) {
                continue;
            }

            if include_sources {
                embedded.add_source_module(&source);
            }

            embedded
                .add_bytecode_module(&source.as_bytecode_module(BytecodeOptimizationLevel::Zero));
        }

        if include_resources {
            for resource in distribution.resources_data()? {
                if !include_test && is_stdlib_test_package(&resource.package) {
                    continue;
                }

                embedded.add_resource(&resource);
            }
        }

        Ok(embedded)
    }

    /// Add a source module to the collection of embedded source modules.
    pub fn add_source_module(&mut self, module: &SourceModule) {
        self.source_modules
            .insert(module.name.clone(), module.clone());

        // Automatically insert empty modules for missing parent packages.
        for package in packages_from_module_name(&module.name) {
            if !self.source_modules.contains_key(&package) {
                self.source_modules.insert(
                    package.clone(),
                    SourceModule {
                        name: package,
                        source: DataLocation::Memory(vec![]),
                        is_package: true,
                    },
                );
            }
        }
    }

    /// Add a bytecode module to the collection of embedded bytecode modules.
    pub fn add_bytecode_module(&mut self, module: &BytecodeModule) {
        self.bytecode_modules
            .insert(module.name.clone(), module.clone());

        // Automatically insert empty modules for missing parent packages.
        for package in packages_from_module_name(&module.name) {
            if !self.bytecode_modules.contains_key(&package) {
                self.bytecode_modules.insert(
                    package.clone(),
                    BytecodeModule {
                        name: package,
                        source: DataLocation::Memory(vec![]),
                        optimize_level: module.optimize_level,
                        is_package: true,
                    },
                );
            }
        }
    }

    /// Add resource data.
    ///
    /// Resource data belongs to a Python package and has a name and bytes data.
    pub fn add_resource(&mut self, resource: &ResourceData) {
        if !self.resources.contains_key(&resource.package) {
            self.resources
                .insert(resource.package.clone(), BTreeMap::new());
        }

        let inner = self.resources.get_mut(&resource.package).unwrap();
        inner.insert(resource.name.clone(), resource.data.resolve().unwrap());
    }

    /// Add an extension module.
    pub fn add_extension_module(&mut self, module: &ExtensionModule) {
        self.extension_modules
            .insert(module.module.clone(), module.clone());

        // Add empty bytecode for missing parent packages.
        // TODO should we choose source if we only have a specific module flavor?
        for package in packages_from_module_name(&module.module) {
            if !self.bytecode_modules.contains_key(&package) {
                self.bytecode_modules.insert(
                    package.clone(),
                    BytecodeModule {
                        name: package,
                        source: DataLocation::Memory(vec![]),
                        optimize_level: BytecodeOptimizationLevel::Zero,
                        is_package: true,
                    },
                );
            }
        }
    }

    /// Add an extension module.
    pub fn add_extension_module_data(&mut self, module: &ExtensionModuleData) {
        self.extension_module_datas
            .insert(module.name.clone(), module.clone());

        // Add empty bytecode for missing parent packages.
        // TODO should we choose source if we only have a specific module flavor?
        for package in packages_from_module_name(&module.name) {
            if !self.bytecode_modules.contains_key(&package) {
                self.bytecode_modules.insert(
                    package.clone(),
                    BytecodeModule {
                        name: package,
                        source: DataLocation::Memory(vec![]),
                        optimize_level: BytecodeOptimizationLevel::Zero,
                        is_package: true,
                    },
                );
            }
        }
    }

    /// Filter the entities in this instance against names in files.
    pub fn filter_from_files(
        &mut self,
        logger: &slog::Logger,
        files: &[&Path],
        glob_patterns: &[&str],
    ) -> Result<()> {
        let resource_names = resolve_resource_names_from_files(files, glob_patterns)?;

        warn!(logger, "filtering embedded extension modules");
        filter_btreemap(logger, &mut self.extension_modules, &resource_names);
        warn!(logger, "filtering embedded module sources");
        filter_btreemap(logger, &mut self.source_modules, &resource_names);
        warn!(logger, "filtering embedded module bytecode");
        filter_btreemap(logger, &mut self.bytecode_modules, &resource_names);
        warn!(logger, "filtering embedded resources");
        filter_btreemap(logger, &mut self.resources, &resource_names);

        Ok(())
    }

    /// Searches for embedded module sources for references to __file__.
    ///
    /// __file__ usage can be problematic for in-memory modules. This method searches
    /// for its occurrences and returns module names having it present.
    pub fn find_dunder_file(&self) -> Result<BTreeSet<String>> {
        let mut res = BTreeSet::new();

        for (name, module) in &self.source_modules {
            if module.has_dunder_file()? {
                res.insert(name.clone());
            }
        }

        for (name, module) in &self.bytecode_modules {
            if module.has_dunder_file()? {
                res.insert(name.clone());
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

        let mut all_modules = BTreeSet::new();
        let mut all_packages = BTreeSet::new();

        let mut module_sources = BTreeMap::new();

        for (name, module) in &self.source_modules {
            all_modules.insert(name.clone());
            if module.is_package {
                all_packages.insert(name.clone());
            }

            module_sources.insert(
                name.clone(),
                PackagedModuleSource {
                    source: module.source.resolve()?,
                    is_package: module.is_package,
                },
            );
        }

        let mut module_bytecodes = BTreeMap::new();
        {
            let mut compiler = BytecodeCompiler::new(&python_exe)?;

            for (name, request) in &self.bytecode_modules {
                let bytecode = compiler.compile(
                    &request.source.resolve()?,
                    &request.name,
                    request.optimize_level,
                    CompileMode::Bytecode,
                )?;

                all_modules.insert(name.clone());
                if request.is_package {
                    all_packages.insert(name.clone());
                }

                module_bytecodes.insert(
                    name.clone(),
                    PackagedModuleBytecode {
                        bytecode,
                        is_package: request.is_package,
                    },
                );
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

            all_modules.insert(name.clone());

            extension_modules.insert(name.clone(), em.clone());
        }

        let mut built_extension_modules = BTreeMap::new();
        for (name, em) in &self.extension_module_datas {
            if ignored.contains(name) {
                continue;
            }

            all_modules.insert(name.clone());
            if em.is_package {
                all_packages.insert(em.name.clone());
            }

            built_extension_modules.insert(name.clone(), em.clone());
        }

        let derived_package_names = packages_from_module_names(all_modules.iter().cloned());

        for package in derived_package_names {
            if !all_packages.contains(&package) {
                warn!(
                    logger,
                    "package {} not initially detected as such; possible package detection bug",
                    package
                );
                all_packages.insert(package);
            }
        }

        let resources = self
            .resources
            .iter()
            .filter_map(|(package, values)| {
                if !all_packages.contains(package) {
                    warn!(
                        logger,
                        "package {} does not exist; excluding resources: {:?}",
                        package,
                        values.keys()
                    );
                    None
                } else {
                    Some((package.clone(), values.clone()))
                }
            })
            .collect();

        Ok(EmbeddedPythonResources {
            module_sources,
            module_bytecodes,
            all_modules,
            all_packages,
            resources,
            extension_modules,
            built_extension_modules,
        })
    }
}

/// Represents Python resources to embed in a binary.
#[derive(Debug, Default, Clone)]
pub struct EmbeddedPythonResources {
    pub module_sources: BTreeMap<String, PackagedModuleSource>,
    pub module_bytecodes: BTreeMap<String, PackagedModuleBytecode>,
    pub all_modules: BTreeSet<String>,
    pub all_packages: BTreeSet<String>,
    pub resources: BTreeMap<String, BTreeMap<String, Vec<u8>>>,
    // TODO combine the extension module types.
    pub extension_modules: BTreeMap<String, ExtensionModule>,
    pub built_extension_modules: BTreeMap<String, ExtensionModuleData>,
}

/// Represents a single module's data record.
pub struct ModuleEntry {
    pub name: String,
    pub is_package: bool,
    pub source: Option<Vec<u8>>,
    pub bytecode: Option<Vec<u8>>,
}

/// Represents an ordered collection of module entries.
pub type ModuleEntries = Vec<ModuleEntry>;

impl EmbeddedPythonResources {
    /// Obtain records for all modules in this resources collection.
    pub fn modules_records(&self) -> ModuleEntries {
        let mut records = ModuleEntries::new();

        for name in &self.all_modules {
            let source = self.module_sources.get(name);
            let bytecode = self.module_bytecodes.get(name);

            records.push(ModuleEntry {
                name: name.clone(),
                is_package: self.all_packages.contains(name),
                source: match source {
                    Some(value) => Some(value.source.clone()),
                    None => None,
                },
                bytecode: match bytecode {
                    Some(value) => Some(value.bytecode.clone()),
                    None => None,
                },
            });
        }

        records
    }

    pub fn write_blobs<W: Write>(&self, module_names: &mut W, modules: &mut W, resources: &mut W) {
        for name in &self.all_modules {
            module_names
                .write_all(name.as_bytes())
                .expect("failed to write");
            module_names.write_all(b"\n").expect("failed to write");
        }

        write_modules_entries(modules, &self.modules_records()).unwrap();

        write_resources_entries(resources, &self.resources).unwrap();
    }

    pub fn embedded_extension_module_names(&self) -> BTreeSet<String> {
        let mut res = BTreeSet::new();

        for name in self.extension_modules.keys() {
            res.insert(name.clone());
        }
        for name in self.built_extension_modules.keys() {
            res.insert(name.clone());
        }

        res
    }
}

/// Serialize a ModulesEntries to a writer.
///
/// See the documentation in the `pyembed` crate for the data format.
pub fn write_modules_entries<W: Write>(mut dest: W, entries: &[ModuleEntry]) -> Result<()> {
    dest.write_u32::<LittleEndian>(entries.len() as u32)?;

    for entry in entries.iter() {
        let name_bytes = entry.name.as_bytes();
        dest.write_u32::<LittleEndian>(name_bytes.len() as u32)?;
        dest.write_u32::<LittleEndian>(if let Some(ref v) = entry.source {
            v.len() as u32
        } else {
            0
        })?;
        dest.write_u32::<LittleEndian>(if let Some(ref v) = entry.bytecode {
            v.len() as u32
        } else {
            0
        })?;

        let mut flags = 0;
        if entry.is_package {
            flags |= 1;
        }

        dest.write_u32::<LittleEndian>(flags)?;
    }

    for entry in entries.iter() {
        let name_bytes = entry.name.as_bytes();
        dest.write_all(name_bytes)?;
    }

    for entry in entries.iter() {
        if let Some(ref v) = entry.source {
            dest.write_all(v.as_slice())?;
        }
    }

    for entry in entries.iter() {
        if let Some(ref v) = entry.bytecode {
            dest.write_all(v.as_slice())?;
        }
    }

    Ok(())
}

/// Serializes resource data to a writer.
///
/// See the documentation in the `pyembed` crate for the data format.
pub fn write_resources_entries<W: Write>(
    dest: &mut W,
    entries: &BTreeMap<String, BTreeMap<String, Vec<u8>>>,
) -> Result<()> {
    dest.write_u32::<LittleEndian>(entries.len() as u32)?;

    // All the numeric index data is written in pass 1.
    for (package, resources) in entries {
        let package_bytes = package.as_bytes();

        dest.write_u32::<LittleEndian>(package_bytes.len() as u32)?;
        dest.write_u32::<LittleEndian>(resources.len() as u32)?;

        for (name, value) in resources {
            let name_bytes = name.as_bytes();

            dest.write_u32::<LittleEndian>(name_bytes.len() as u32)?;
            dest.write_u32::<LittleEndian>(value.len() as u32)?;
        }
    }

    // All the name strings are written in pass 2.
    for (package, resources) in entries {
        dest.write_all(package.as_bytes())?;

        for name in resources.keys() {
            dest.write_all(name.as_bytes())?;
        }
    }

    // All the resource data is written in pass 3.
    for resources in entries.values() {
        for value in resources.values() {
            dest.write_all(value.as_slice())?;
        }
    }

    Ok(())
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

        assert!(r.source_modules.contains_key("foo"));
        assert_eq!(
            r.source_modules.get("foo"),
            Some(&SourceModule {
                name: "foo".to_string(),
                source: DataLocation::Memory(vec![42]),
                is_package: false,
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

        assert_eq!(r.source_modules.len(), 3);
        assert_eq!(
            r.source_modules.get("root.parent.child"),
            Some(&SourceModule {
                name: "root.parent.child".to_string(),
                source: DataLocation::Memory(vec![42]),
                is_package: true,
            })
        );
        assert_eq!(
            r.source_modules.get("root.parent"),
            Some(&SourceModule {
                name: "root.parent".to_string(),
                source: DataLocation::Memory(vec![]),
                is_package: true,
            })
        );
        assert_eq!(
            r.source_modules.get("root"),
            Some(&SourceModule {
                name: "root".to_string(),
                source: DataLocation::Memory(vec![]),
                is_package: true,
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

        assert!(r.bytecode_modules.contains_key("foo"));
        assert_eq!(
            r.bytecode_modules.get("foo"),
            Some(&BytecodeModule {
                name: "foo".to_string(),
                source: DataLocation::Memory(vec![42]),
                optimize_level: BytecodeOptimizationLevel::Zero,
                is_package: false,
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

        assert_eq!(r.bytecode_modules.len(), 3);
        assert_eq!(
            r.bytecode_modules.get("root.parent.child"),
            Some(&BytecodeModule {
                name: "root.parent.child".to_string(),
                source: DataLocation::Memory(vec![42]),
                optimize_level: BytecodeOptimizationLevel::One,
                is_package: true,
            })
        );
        assert_eq!(
            r.bytecode_modules.get("root.parent"),
            Some(&BytecodeModule {
                name: "root.parent".to_string(),
                source: DataLocation::Memory(vec![]),
                optimize_level: BytecodeOptimizationLevel::One,
                is_package: true,
            })
        );
        assert_eq!(
            r.bytecode_modules.get("root"),
            Some(&BytecodeModule {
                name: "root".to_string(),
                source: DataLocation::Memory(vec![]),
                optimize_level: BytecodeOptimizationLevel::One,
                is_package: true,
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

        assert_eq!(r.resources.len(), 1);
        assert!(r.resources.contains_key("foo"));

        let foo = r.resources.get("foo").unwrap();
        assert_eq!(foo.len(), 1);
        assert_eq!(foo.get("resource.txt"), Some(&vec![42]));
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

        assert_eq!(r.bytecode_modules.len(), 1);
        assert_eq!(
            r.bytecode_modules.get("foo"),
            Some(&BytecodeModule {
                name: "foo".to_string(),
                source: DataLocation::Memory(vec![]),
                optimize_level: BytecodeOptimizationLevel::Zero,
                is_package: true
            })
        );
    }

    #[test]
    fn test_add_extension_module_data() {
        let mut r = EmbeddedPythonResourcesPrePackaged::default();
        let em = ExtensionModuleData {
            name: "foo.bar".to_string(),
            init_fn: "".to_string(),
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

        assert_eq!(r.bytecode_modules.len(), 1);
        assert_eq!(
            r.bytecode_modules.get("foo"),
            Some(&BytecodeModule {
                name: "foo".to_string(),
                source: DataLocation::Memory(vec![]),
                optimize_level: BytecodeOptimizationLevel::Zero,
                is_package: true,
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
