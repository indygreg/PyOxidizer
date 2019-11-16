// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

use super::distribution::ExtensionModule;
use crate::pyrepackager::config::InstallLocation;

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

#[derive(Debug)]
pub enum ResourceAction {
    Add,
    Remove,
}

/// Represents the packaging location for a resource.
#[derive(Clone, Debug)]
pub enum ResourceLocation {
    /// Embed the resource in the binary.
    Embedded,

    /// Install the resource in a path relative to the produced binary.
    AppRelative { path: String },
}

impl ResourceLocation {
    pub fn new(v: &InstallLocation) -> Self {
        match v {
            InstallLocation::Embedded => ResourceLocation::Embedded,
            InstallLocation::AppRelative { path } => {
                ResourceLocation::AppRelative { path: path.clone() }
            }
        }
    }
}

#[derive(Debug)]
pub struct PythonResourceAction {
    pub action: ResourceAction,
    pub location: ResourceLocation,
    pub resource: PythonResource,
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
