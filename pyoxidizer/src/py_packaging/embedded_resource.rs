// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use anyhow::Result;
use byteorder::{LittleEndian, WriteBytesExt};
use lazy_static::lazy_static;
use slog::warn;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Write;
use std::iter::FromIterator;
use std::path::Path;
use std::sync::Arc;

use super::bytecode::{BytecodeCompiler, CompileMode};
use super::distribution::{
    is_stdlib_test_package, ExtensionModule, ExtensionModuleFilter, ParsedPythonDistribution,
};
use super::filtering::{filter_btreemap, resolve_resource_names_from_files};
use super::resource::{
    BytecodeModule, BytecodeOptimizationLevel, ExtensionModuleData, PackagedModuleBytecode,
    PackagedModuleSource, ResourceData, SourceModule,
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
        ) {
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
    }

    /// Add a bytecode module to the collection of embedded bytecode modules.
    pub fn add_bytecode_module(&mut self, module: &BytecodeModule) {
        self.bytecode_modules
            .insert(module.name.clone(), module.clone());
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
    }

    /// Add an extension module.
    pub fn add_extension_module_data(&mut self, module: &ExtensionModuleData) {
        self.extension_module_datas
            .insert(module.name.clone(), module.clone());
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

    pub fn package(&self, python_exe: &Path) -> Result<EmbeddedPythonResources> {
        let mut all_modules = BTreeSet::new();
        let mut all_packages = BTreeSet::new();

        let module_sources = BTreeMap::from_iter(self.source_modules.iter().map(|(k, v)| {
            all_modules.insert(k.clone());
            if v.is_package {
                all_packages.insert(k.clone());
            }

            (
                k.clone(),
                PackagedModuleSource {
                    // TODO avoid this unwrap
                    source: v.source.resolve().unwrap(),
                    is_package: v.is_package,
                },
            )
        }));

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

        let resources = self.resources.clone();
        all_packages.extend(resources.keys().cloned());

        let ignored = OS_IGNORE_EXTENSIONS
            .iter()
            .map(|k| (*k).to_string())
            .collect::<Vec<String>>();

        let extension_modules =
            BTreeMap::from_iter(self.extension_modules.iter().filter_map(|(k, v)| {
                if ignored.contains(k) {
                    None
                } else {
                    Some((k.clone(), v.clone()))
                }
            }));

        let built_extension_modules =
            BTreeMap::from_iter(self.extension_module_datas.iter().filter_map(|(k, v)| {
                if ignored.contains(k) {
                    None
                } else {
                    Some((k.clone(), v.clone()))
                }
            }));

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
