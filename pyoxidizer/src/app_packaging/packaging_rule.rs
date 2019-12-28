// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::collections::BTreeSet;

use super::config::{InstallLocation, PythonPackaging};
use crate::py_packaging::distribution::ParsedPythonDistribution;
use crate::py_packaging::resource::{AppRelativeResources, PythonResource};

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

impl AppRelativeResources {
    pub fn package_names(&self) -> BTreeSet<String> {
        let mut packages = packages_from_module_names(self.module_sources.keys().cloned());
        packages.extend(packages_from_module_names(
            self.module_bytecodes.keys().cloned(),
        ));

        packages
    }
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

/// Resolves a Python packaging rule to resources to package.
pub fn resolve_python_packaging(
    logger: &slog::Logger,
    package: &PythonPackaging,
    dist: &ParsedPythonDistribution,
) -> Vec<PythonResourceAction> {
    match package {
        PythonPackaging::WriteLicenseFiles(_) => Vec::new(),

        // This is a no-op because it can only be handled at a higher level.
        PythonPackaging::FilterInclude(_) => Vec::new(),
    }
}
