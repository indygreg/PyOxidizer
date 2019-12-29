// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use anyhow::{anyhow, Context, Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::convert::TryFrom;
use std::path::PathBuf;

use super::distribution::ExtensionModule;
use super::fsscan::{is_package_from_path, PythonFileResource};

pub fn packages_from_module_name(module: &str) -> BTreeSet<String> {
    let mut package_names = BTreeSet::new();

    let mut search: &str = &module;

    while let Some(idx) = search.rfind('.') {
        package_names.insert(search[0..idx].to_string());
        search = &search[0..idx];
    }

    package_names
}

pub fn packages_from_module_names<I>(names: I) -> BTreeSet<String>
where
    I: Iterator<Item = String>,
{
    let mut package_names = BTreeSet::new();

    for name in names {
        let mut search: &str = &name;

        while let Some(idx) = search.rfind('.') {
            package_names.insert(search[0..idx].to_string());
            search = &search[0..idx];
        }
    }

    package_names
}

/// A Python source module agnostic of location.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceModule {
    pub name: String,
    pub source: Vec<u8>,
    pub is_package: bool,
}

impl SourceModule {
    pub fn package(&self) -> String {
        if let Some(idx) = self.name.rfind('.') {
            self.name[0..idx].to_string()
        } else {
            self.name.clone()
        }
    }

    pub fn as_python_resource(&self) -> PythonResource {
        PythonResource::ModuleSource {
            name: self.name.clone(),
            source: self.source.clone(),
            is_package: self.is_package,
        }
    }

    /// Convert the instance to a BytecodeModule.
    pub fn as_bytecode_module(&self, optimize_level: BytecodeOptimizationLevel) -> BytecodeModule {
        BytecodeModule {
            name: self.name.clone(),
            source: self.source.clone(),
            optimize_level,
            is_package: self.is_package,
        }
    }
}

/// An optimization level for Python bytecode.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BytecodeOptimizationLevel {
    Zero,
    One,
    Two,
}

impl From<i32> for BytecodeOptimizationLevel {
    fn from(i: i32) -> Self {
        match i {
            0 => BytecodeOptimizationLevel::Zero,
            1 => BytecodeOptimizationLevel::One,
            2 => BytecodeOptimizationLevel::Two,
            _ => panic!("unsupported bytecode optimization level"),
        }
    }
}

impl From<BytecodeOptimizationLevel> for i32 {
    fn from(level: BytecodeOptimizationLevel) -> Self {
        match level {
            BytecodeOptimizationLevel::Zero => 0,
            BytecodeOptimizationLevel::One => 1,
            BytecodeOptimizationLevel::Two => 2,
        }
    }
}

/// Python module bytecode, agnostic of location.
#[derive(Clone, Debug, PartialEq)]
pub struct BytecodeModule {
    pub name: String,
    pub source: Vec<u8>,
    pub optimize_level: BytecodeOptimizationLevel,
    pub is_package: bool,
}

impl BytecodeModule {
    pub fn as_python_resource(&self) -> PythonResource {
        PythonResource::ModuleBytecodeRequest {
            name: self.name.clone(),
            source: self.source.clone(),
            optimize_level: match self.optimize_level {
                BytecodeOptimizationLevel::Zero => 0,
                BytecodeOptimizationLevel::One => 1,
                BytecodeOptimizationLevel::Two => 2,
            },
            is_package: self.is_package,
        }
    }
}

/// Python package resource data, agnostic of storage location.
#[derive(Clone, Debug, PartialEq)]
pub struct ResourceData {
    pub package: String,
    pub name: String,
    pub data: Vec<u8>,
}

impl ResourceData {
    pub fn as_python_resource(&self) -> PythonResource {
        PythonResource::Resource {
            package: self.package.clone(),
            name: self.name.clone(),
            data: self.data.clone(),
        }
    }
}

/// Represents an extension module built during packaging.
///
/// This is like a light version of `ExtensionModule`.
#[derive(Clone, Debug)]
pub struct BuiltExtensionModule {
    pub name: String,
    pub init_fn: String,
    pub object_file_data: Vec<Vec<u8>>,
    pub is_package: bool,
    pub libraries: Vec<String>,
    pub library_dirs: Vec<PathBuf>,
}

/// Represents a resource to make available to the Python interpreter.
#[derive(Debug)]
pub enum PythonResource {
    ExtensionModule {
        name: String,
        module: ExtensionModule,
    },
    ModuleSource {
        name: String,
        source: Vec<u8>,
        is_package: bool,
    },
    ModuleBytecodeRequest {
        name: String,
        source: Vec<u8>,
        optimize_level: i32,
        is_package: bool,
    },
    ModuleBytecode {
        name: String,
        bytecode: Vec<u8>,
        optimize_level: BytecodeOptimizationLevel,
        is_package: bool,
    },
    Resource {
        package: String,
        name: String,
        data: Vec<u8>,
    },
    BuiltExtensionModule(BuiltExtensionModule),
}

impl TryFrom<&PythonFileResource> for PythonResource {
    type Error = Error;

    fn try_from(resource: &PythonFileResource) -> Result<PythonResource> {
        match resource {
            PythonFileResource::Source {
                full_name, path, ..
            } => {
                let source =
                    std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;

                Ok(PythonResource::ModuleSource {
                    name: full_name.clone(),
                    source,
                    is_package: is_package_from_path(&path),
                })
            }

            PythonFileResource::Bytecode {
                full_name, path, ..
            } => {
                let bytecode =
                    std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;

                // First 16 bytes are a validation header.
                let bytecode = bytecode[16..bytecode.len()].to_vec();

                Ok(PythonResource::ModuleBytecode {
                    name: full_name.clone(),
                    bytecode,
                    optimize_level: BytecodeOptimizationLevel::Zero,
                    is_package: is_package_from_path(&path),
                })
            }

            PythonFileResource::BytecodeOpt1 {
                full_name, path, ..
            } => {
                let bytecode =
                    std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;

                // First 16 bytes are a validation header.
                let bytecode = bytecode[16..bytecode.len()].to_vec();

                Ok(PythonResource::ModuleBytecode {
                    name: full_name.clone(),
                    bytecode,
                    optimize_level: BytecodeOptimizationLevel::One,
                    is_package: is_package_from_path(&path),
                })
            }

            PythonFileResource::BytecodeOpt2 {
                full_name, path, ..
            } => {
                let bytecode =
                    std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;

                // First 16 bytes are a validation header.
                let bytecode = bytecode[16..bytecode.len()].to_vec();

                Ok(PythonResource::ModuleBytecode {
                    name: full_name.clone(),
                    bytecode,
                    optimize_level: BytecodeOptimizationLevel::Two,
                    is_package: is_package_from_path(&path),
                })
            }

            PythonFileResource::Resource(resource) => {
                let path = &(resource.path);
                let data =
                    std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;

                Ok(PythonResource::Resource {
                    package: resource.package.clone(),
                    name: resource.stem.clone(),
                    data,
                })
            }

            PythonFileResource::ExtensionModule { .. } => {
                Err(anyhow!("converting ExtensionModule not yet supported"))
            }

            PythonFileResource::EggFile { .. } => {
                Err(anyhow!("converting egg files not yet supported"))
            }

            PythonFileResource::PthFile { .. } => {
                Err(anyhow!("converting pth files not yet supported"))
            }

            PythonFileResource::Other { .. } => {
                Err(anyhow!("converting other files not yet supported"))
            }
        }
    }
}

impl PythonResource {
    pub fn is_in_packages(&self, packages: &Vec<String>) -> bool {
        let name = match self {
            PythonResource::ModuleSource { name, .. } => name,
            PythonResource::ModuleBytecode { name, .. } => name,
            PythonResource::ModuleBytecodeRequest { name, .. } => name,
            PythonResource::Resource { package, .. } => package,
            PythonResource::BuiltExtensionModule(em) => &em.name,
            PythonResource::ExtensionModule { name, .. } => name,
        };

        for package in packages {
            if packages_from_module_name(&name).contains(package) {
                return true;
            }
        }

        false
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PackagedModuleSource {
    pub source: Vec<u8>,
    pub is_package: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PackagedModuleBytecode {
    pub bytecode: Vec<u8>,
    pub is_package: bool,
}

/// Represents resources to install in an app-relative location.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AppRelativeResources {
    pub module_sources: BTreeMap<String, PackagedModuleSource>,
    pub module_bytecodes: BTreeMap<String, PackagedModuleBytecode>,
    pub resources: BTreeMap<String, BTreeMap<String, Vec<u8>>>,
}

impl AppRelativeResources {
    pub fn package_names(&self) -> BTreeSet<String> {
        let mut packages = packages_from_module_names(self.module_sources.keys().cloned());
        packages.extend(packages_from_module_names(
            self.module_bytecodes.keys().cloned(),
        ));

        packages
    }
}
