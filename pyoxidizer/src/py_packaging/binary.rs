// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use slog::warn;
use tempdir::TempDir;

use super::config::{EmbeddedPythonConfig, RunMode};
use super::distribution::ParsedPythonDistribution;
use super::embedded_resource::EmbeddedPythonResourcesPrePackaged;
use super::libpython::link_libpython;

/// A self-contained Python executable before it is compiled.
#[derive(Debug, Clone)]
pub struct PreBuiltPythonExecutable {
    pub distribution: ParsedPythonDistribution,
    pub resources: EmbeddedPythonResourcesPrePackaged,
    pub config: EmbeddedPythonConfig,
    pub run_mode: RunMode,
}

impl PreBuiltPythonExecutable {
    /// Build a Python library suitable for linking.
    ///
    /// This will take the underlying distribution, resources, and
    /// configuration and produce a new executable binary.
    pub fn build_libpython(
        &self,
        logger: &slog::Logger,
        host: &str,
        target: &str,
        opt_level: &str,
    ) -> Result<PythonLibrary, String> {
        let resources = self
            .resources
            .package(&self.distribution.python_exe)
            .or_else(|e| Err(e.to_string()))?;

        let temp_dir = TempDir::new("pyoxidizer-build-exe").or_else(|e| Err(e.to_string()))?;
        let temp_dir_path = temp_dir.path();

        warn!(
            logger,
            "generating custom link library containing Python..."
        );
        let library_info = link_libpython(
            logger,
            &self.distribution,
            &resources,
            &temp_dir_path,
            host,
            target,
            opt_level,
        );

        let mut cargo_metadata: Vec<String> = Vec::new();
        cargo_metadata.extend(library_info.cargo_metadata);

        let data = std::fs::read(&library_info.path).or_else(|e| Err(e.to_string()))?;

        Ok(PythonLibrary {
            pre_built: self.clone(),
            data,
            cargo_metadata,
        })
    }

    /// Generate data embedded in binaries representing Python resource data.
    pub fn build_embedded_blobs(&self) -> Result<EmbeddedResourcesBlobs, String> {
        let embedded_resources = self
            .resources
            .package(&self.distribution.python_exe)
            .or_else(|e| Err(e.to_string()))?;

        let mut module_names = Vec::new();
        let mut modules = Vec::new();
        let mut resources = Vec::new();

        embedded_resources.write_blobs(&mut module_names, &mut modules, &mut resources);

        Ok(EmbeddedResourcesBlobs {
            module_names,
            modules,
            resources,
        })
    }
}

/// A self-contained Python executable after it is built.
pub struct PythonLibrary {
    pub pre_built: PreBuiltPythonExecutable,
    pub data: Vec<u8>,
    pub cargo_metadata: Vec<String>,
}

/// Represents serialized data embedded in binaries for loading Python resources.
pub struct EmbeddedResourcesBlobs {
    pub module_names: Vec<u8>,
    pub modules: Vec<u8>,
    pub resources: Vec<u8>,
}
