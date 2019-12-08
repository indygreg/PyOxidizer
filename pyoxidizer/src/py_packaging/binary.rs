// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use anyhow::Result;
use slog::warn;
use tempdir::TempDir;

use super::config::{EmbeddedPythonConfig, RunMode};
use super::distribution::ParsedPythonDistribution;
use super::embedded_resource::EmbeddedPythonResourcesPrePackaged;
use super::libpython::{derive_importlib, link_libpython, ImportlibData};

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
    ) -> Result<PythonLibrary> {
        let resources = self.resources.package(&self.distribution.python_exe)?;

        let temp_dir = TempDir::new("pyoxidizer-build-exe")?;
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

        let data = std::fs::read(&library_info.path)?;

        Ok(PythonLibrary {
            pre_built: self.clone(),
            data,
            cargo_metadata,
        })
    }

    /// Generate data embedded in binaries representing Python resource data.
    pub fn build_embedded_blobs(&self) -> Result<EmbeddedResourcesBlobs> {
        let embedded_resources = self.resources.package(&self.distribution.python_exe)?;

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

/// Represents resources to embed Python in a binary.
pub struct EmbeddedPythonBinaryData {
    pub library: PythonLibrary,
    pub importlib: ImportlibData,
    pub resources: EmbeddedResourcesBlobs,
}

impl EmbeddedPythonBinaryData {
    pub fn from_pre_built_python_executable(
        exe: &PreBuiltPythonExecutable,
        logger: &slog::Logger,
        host: &str,
        target: &str,
        opt_level: &str,
    ) -> Result<EmbeddedPythonBinaryData> {
        let library = exe.build_libpython(logger, host, target, opt_level)?;
        let resources = exe.build_embedded_blobs()?;
        warn!(
            logger,
            "deriving custom importlib modules to support in-memory importing"
        );
        let importlib = derive_importlib(&exe.distribution)?;

        Ok(EmbeddedPythonBinaryData {
            library,
            importlib,
            resources,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;

    #[test]
    fn test_build_simple() -> Result<()> {
        let resources = EmbeddedPythonResourcesPrePackaged::default();
        let config = EmbeddedPythonConfig::default();
        let run_mode = RunMode::Noop;

        let pre_built = PreBuiltPythonExecutable {
            distribution: get_default_distribution()?,
            resources,
            config,
            run_mode,
        };

        EmbeddedPythonBinaryData::from_pre_built_python_executable(
            &pre_built,
            &get_logger()?,
            env!("HOST"),
            env!("HOST"),
            "0",
        )?;

        Ok(())
    }
}
