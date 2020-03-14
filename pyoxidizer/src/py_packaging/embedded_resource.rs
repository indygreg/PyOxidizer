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
    python_packed_resources::data::{Resource as EmbeddedResource, ResourceFlavor},
    python_packed_resources::writer::write_embedded_resources_v1,
    slog::{info, warn},
    std::borrow::Cow,
    std::collections::{BTreeMap, BTreeSet, HashMap},
    std::convert::TryFrom,
    std::io::Write,
    std::iter::FromIterator,
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

impl<'a> TryFrom<&EmbeddedResourcePythonModulePrePackaged> for EmbeddedResource<'a, u8> {
    type Error = Error;

    fn try_from(value: &EmbeddedResourcePythonModulePrePackaged) -> Result<Self, Self::Error> {
        Ok(Self {
            flavor: if value.in_memory_extension_module_shared_library.is_some() {
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
                let mut res = Box::new(HashMap::new());
                for (key, location) in resources {
                    res.insert(Cow::Owned(key.clone()), Cow::Owned(location.resolve()?));
                }
                Some(Arc::new(res))
            } else {
                None
            },
            in_memory_package_distribution: if let Some(resources) =
                &value.in_memory_package_distribution
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
            relative_path_module_source: None,
            relative_path_module_bytecode: None,
            relative_path_module_bytecode_opt1: None,
            relative_path_module_bytecode_opt2: None,
            relative_path_extension_module_shared_library: None,
            relative_path_package_resources: None,
            relative_path_package_distribution: None,
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
    pub fn get_in_memory_module_sources(&self) -> BTreeMap<String, SourceModule> {
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
    pub fn get_in_memory_module_bytecodes(&self) -> BTreeMap<String, BytecodeModule> {
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

    /// Obtain `ExtensionModule` in this instance.
    pub fn get_extension_modules(&self) -> BTreeMap<String, ExtensionModule> {
        self.extension_modules.clone()
    }

    /// Obtain `ExtensionModuleData` in this instance.
    pub fn get_extension_module_datas(&self) -> BTreeMap<String, ExtensionModuleData> {
        self.extension_module_datas.clone()
    }

    /// Add a source module to the collection of embedded source modules.
    pub fn add_in_memory_module_source(&mut self, module: &SourceModule) {
        let entry = self.modules.entry(module.name.clone()).or_insert_with(|| {
            EmbeddedResourcePythonModulePrePackaged {
                name: module.name.clone(),
                ..EmbeddedResourcePythonModulePrePackaged::default()
            }
        });
        entry.is_package = module.is_package;
        entry.in_memory_source = Some(module.source.clone());

        self.add_parent_packages(&module.name, true, None);
    }

    /// Add a bytecode module to the collection of embedded bytecode modules.
    pub fn add_in_memory_module_bytecode(&mut self, module: &BytecodeModule) {
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

        self.add_parent_packages(&module.name, false, Some(module.optimize_level));
    }

    /// Add resource data.
    ///
    /// Resource data belongs to a Python package and has a name and bytes data.
    pub fn add_in_memory_package_resource(&mut self, resource: &ResourceData) {
        let entry = self
            .modules
            .entry(resource.package.clone())
            .or_insert_with(|| EmbeddedResourcePythonModulePrePackaged {
                name: resource.package.clone(),
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
            .insert(resource.name.clone(), resource.data.clone());

        self.add_parent_packages(&resource.package, false, None);
    }

    /// Add an extension module.
    pub fn add_extension_module(&mut self, module: &ExtensionModule) {
        self.extension_modules
            .insert(module.module.clone(), module.clone());

        // TODO should we populate opt1, opt2, source?
        self.add_parent_packages(&module.module, false, Some(BytecodeOptimizationLevel::Zero));
    }

    /// Add an extension module.
    pub fn add_extension_module_data(&mut self, module: &ExtensionModuleData) {
        self.extension_module_datas
            .insert(module.name.clone(), module.clone());

        // Add empty bytecode for missing parent packages.
        // TODO should we populate opt1, opt2?
        self.add_parent_packages(&module.name, false, Some(BytecodeOptimizationLevel::Zero));
    }

    /// Add an extension module shared library that should be imported from memory.
    pub fn add_in_memory_extension_module_shared_library(
        &mut self,
        module: &str,
        is_package: bool,
        data: &[u8],
    ) {
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
        self.add_parent_packages(module, false, Some(BytecodeOptimizationLevel::Zero));

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

            extension_modules.insert(name.clone(), em.clone());
        }

        let mut built_extension_modules = BTreeMap::new();
        for (name, em) in &self.extension_module_datas {
            if ignored.contains(name) {
                continue;
            }

            let entry = modules
                .entry(name.clone())
                .or_insert_with(|| EmbeddedResource {
                    name: Cow::Owned(name.clone()),
                    ..EmbeddedResource::default()
                });

            if em.is_package {
                entry.is_package = true;
            }

            built_extension_modules.insert(name.clone(), em.clone());
        }

        let derived_package_names = packages_from_module_names(modules.keys().cloned());

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
            extension_modules,
            built_extension_modules,
        })
    }

    fn add_parent_packages(
        &mut self,
        name: &str,
        add_source: bool,
        bytecode_level: Option<BytecodeOptimizationLevel>,
    ) {
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
            if add_source && m.in_memory_source.is_none() {
                m.in_memory_source = Some(DataLocation::Memory(vec![]));
            }

            if let Some(level) = bytecode_level {
                match level {
                    BytecodeOptimizationLevel::Zero => {
                        if m.in_memory_bytecode.is_none() {
                            m.in_memory_bytecode = Some(DataLocation::Memory(vec![]));
                        }
                    }
                    BytecodeOptimizationLevel::One => {
                        if m.in_memory_bytecode_opt1.is_none() {
                            m.in_memory_bytecode_opt1 = Some(DataLocation::Memory(vec![]));
                        }
                    }
                    BytecodeOptimizationLevel::Two => {
                        if m.in_memory_bytecode_opt2.is_none() {
                            m.in_memory_bytecode_opt2 = Some(DataLocation::Memory(vec![]));
                        }
                    }
                }
            }
        }
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

    // TODO combine the extension module types.
    extension_modules: BTreeMap<String, ExtensionModule>,
    built_extension_modules: BTreeMap<String, ExtensionModuleData>,
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
        let mut res = Vec::new();

        for (name, em) in &self.extension_modules {
            if let Some(init_fn) = &em.init_fn {
                res.push((name.clone(), init_fn.clone()));
            }
        }

        for (name, em) in &self.built_extension_modules {
            if let Some(init_fn) = &em.init_fn {
                res.push((name.clone(), init_fn.clone()));
            }
        }

        res
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

        for (name, em) in &self.extension_modules {
            if em.builtin_default {
                continue;
            }

            info!(
                logger,
                "adding {} object files for {} extension module",
                em.object_paths.len(),
                name
            );

            for path in &em.object_paths {
                object_files.push(DataLocation::Path(path.clone()));
            }

            for entry in &em.links {
                if entry.framework {
                    warn!(logger, "framework {} required by {}", entry.name, name);
                    link_frameworks.insert(entry.name.clone());
                } else if entry.system {
                    warn!(logger, "system library {} required by {}", entry.name, name);
                    link_system_libraries.insert(entry.name.clone());
                } else if let Some(_lib) = &entry.static_path {
                    warn!(logger, "static library {} required by {}", entry.name, name);
                    link_libraries.insert(entry.name.clone());
                } else if let Some(_) = &entry.dynamic_path {
                    warn!(
                        logger,
                        "dynamic library {} required by {}", entry.name, name
                    );
                    link_libraries.insert(entry.name.clone());
                }
            }
        }

        warn!(
            logger,
            "resolving inputs for {} built extension modules...",
            self.built_extension_modules.len()
        );

        for (name, em) in &self.built_extension_modules {
            info!(
                logger,
                "adding {} object files for {} built extension module",
                em.object_file_data.len(),
                name
            );

            for data in &em.object_file_data {
                object_files.push(DataLocation::Memory(data.clone()));
            }

            for library in &em.libraries {
                warn!(logger, "library {} required by {}", library, name);
                link_libraries_external.insert(library.clone());
            }

            // TODO do something with library_dirs.
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
    fn test_add_source_module() {
        let mut r = EmbeddedPythonResourcesPrePackaged::default();
        r.add_in_memory_module_source(&SourceModule {
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
        r.add_in_memory_module_source(&SourceModule {
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
        r.add_in_memory_module_bytecode(&BytecodeModule {
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
        r.add_in_memory_module_bytecode(&BytecodeModule {
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
        r.add_in_memory_package_resource(&ResourceData {
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

        r.add_in_memory_module_source(&SourceModule {
            name: "foo.bar".to_string(),
            source: DataLocation::Memory(vec![]),
            is_package: false,
        });
        assert_eq!(r.find_dunder_file()?.len(), 0);

        r.add_in_memory_module_source(&SourceModule {
            name: "baz".to_string(),
            source: DataLocation::Memory(Vec::from("import foo; if __file__ == 'ignored'")),
            is_package: false,
        });
        assert_eq!(r.find_dunder_file()?.len(), 1);
        assert!(r.find_dunder_file()?.contains("baz"));

        r.add_in_memory_module_bytecode(&BytecodeModule {
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
