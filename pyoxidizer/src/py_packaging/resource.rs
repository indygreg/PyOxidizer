// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

use super::distribution::ExtensionModule;

/// A Python source module agnostic of location.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceModule {
    pub name: String,
    pub source: Vec<u8>,
    pub is_package: bool,
}

impl SourceModule {
    pub fn as_python_resource(&self) -> PythonResource {
        PythonResource::ModuleSource {
            name: self.name.clone(),
            source: self.source.clone(),
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
        PythonResource::ModuleBytecode {
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
    ModuleBytecode {
        name: String,
        source: Vec<u8>,
        optimize_level: i32,
        is_package: bool,
    },
    Resource {
        package: String,
        name: String,
        data: Vec<u8>,
    },
    BuiltExtensionModule(BuiltExtensionModule),
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
