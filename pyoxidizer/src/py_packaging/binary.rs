// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use anyhow::Result;
use slog::warn;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use tempdir::TempDir;

use super::config::{EmbeddedPythonConfig, RunMode};
use super::distribution::ParsedPythonDistribution;
use super::embedded_resource::EmbeddedPythonResourcesPrePackaged;
use super::libpython::{derive_importlib, link_libpython, ImportlibData};
use super::pyembed::{derive_python_config, write_data_rs};

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
        )?;

        let mut cargo_metadata: Vec<String> = Vec::new();
        cargo_metadata.extend(library_info.cargo_metadata);

        let libpython_data = std::fs::read(&library_info.libpython_path)?;
        let libpyembeddedconfig_data = std::fs::read(&library_info.libpyembeddedconfig_path)?;

        Ok(PythonLibrary {
            pre_built: self.clone(),
            libpython_data,
            libpyembeddedconfig_data,
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
    pub libpython_data: Vec<u8>,
    pub libpyembeddedconfig_data: Vec<u8>,
    pub cargo_metadata: Vec<String>,
}

/// Represents serialized data embedded in binaries for loading Python resources.
pub struct EmbeddedResourcesBlobs {
    pub module_names: Vec<u8>,
    pub modules: Vec<u8>,
    pub resources: Vec<u8>,
}

/// Holds filesystem paths to resources required to build a binary embedding Python.
pub struct EmbeddedPythonBinaryPaths {
    pub importlib_bootstrap: PathBuf,
    pub importlib_bootstrap_external: PathBuf,
    pub module_names: PathBuf,
    pub py_modules: PathBuf,
    pub resources: PathBuf,
    pub libpython: PathBuf,
    pub libpyembeddedconfig: PathBuf,
    pub config_rs: PathBuf,
    pub cargo_metadata: PathBuf,
}

/// Represents resources to embed Python in a binary.
pub struct EmbeddedPythonBinaryData {
    pub config: EmbeddedPythonConfig,
    pub run_mode: RunMode,
    pub library: PythonLibrary,
    pub importlib: ImportlibData,
    pub resources: EmbeddedResourcesBlobs,
    pub host: String,
    pub target: String,
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
            config: exe.config.clone(),
            run_mode: exe.run_mode.clone(),
            library,
            importlib,
            resources,
            host: host.to_string(),
            target: target.to_string(),
        })
    }

    /// Write out files needed to link a binary.
    pub fn write_files(&self, dest_dir: &Path) -> Result<EmbeddedPythonBinaryPaths> {
        let importlib_bootstrap = dest_dir.join("importlib_bootstrap");
        let mut fh = File::create(&importlib_bootstrap)?;
        fh.write_all(&self.importlib.bootstrap_bytecode)?;

        let importlib_bootstrap_external = dest_dir.join("importlib_bootstrap_external");
        let mut fh = File::create(&importlib_bootstrap_external)?;
        fh.write_all(&self.importlib.bootstrap_external_bytecode)?;

        let module_names = dest_dir.join("py-module-names");
        let mut fh = File::create(&module_names)?;
        fh.write_all(&self.resources.module_names)?;

        let py_modules = dest_dir.join("py-modules");
        let mut fh = File::create(&py_modules)?;
        fh.write_all(&self.resources.modules)?;

        let resources = dest_dir.join("python-resources");
        let mut fh = File::create(&resources)?;
        fh.write_all(&self.resources.resources)?;

        let libpython = dest_dir.join("libpythonXY.a");
        let mut fh = File::create(&libpython)?;
        fh.write_all(&self.library.libpython_data)?;

        let libpyembeddedconfig = dest_dir.join("libpyembeddedconfig.a");
        let mut fh = File::create(&libpyembeddedconfig)?;
        fh.write_all(&self.library.libpyembeddedconfig_data)?;

        let config_rs_data = derive_python_config(
            &self.config,
            &self.run_mode,
            &importlib_bootstrap,
            &importlib_bootstrap_external,
            &py_modules,
            &resources,
        );
        let config_rs = dest_dir.join("data.rs");
        write_data_rs(&config_rs, &config_rs_data)?;

        let mut cargo_metadata_lines = Vec::new();
        cargo_metadata_lines.extend(self.library.cargo_metadata.clone());

        // Tell Cargo where libpythonXY is located.
        cargo_metadata_lines.push(format!(
            "cargo:rustc-link-search=native={}",
            dest_dir.display()
        ));

        // Give pyembed the path to the config file.
        cargo_metadata_lines.push(format!(
            "cargo:rustc-env=PYEMBED_DATA_RS_PATH={}",
            config_rs.display()
        ));

        let cargo_metadata = dest_dir.join("cargo_metadata.txt");
        let mut fh = File::create(&cargo_metadata)?;
        fh.write_all(cargo_metadata_lines.join("\n").as_bytes())?;

        Ok(EmbeddedPythonBinaryPaths {
            importlib_bootstrap,
            importlib_bootstrap_external,
            module_names,
            py_modules,
            resources,
            libpython,
            libpyembeddedconfig,
            config_rs,
            cargo_metadata,
        })
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::testutil::*;

    pub fn get_prebuilt() -> Result<PreBuiltPythonExecutable> {
        let resources = EmbeddedPythonResourcesPrePackaged::default();
        let config = EmbeddedPythonConfig::default();
        let run_mode = RunMode::Noop;

        Ok(PreBuiltPythonExecutable {
            distribution: get_default_distribution()?,
            resources,
            config,
            run_mode,
        })
    }

    pub fn get_embedded() -> Result<EmbeddedPythonBinaryData> {
        EmbeddedPythonBinaryData::from_pre_built_python_executable(
            &get_prebuilt()?,
            &get_logger()?,
            env!("HOST"),
            env!("HOST"),
            "0",
        )
    }

    #[test]
    fn test_write_embedded_files() -> Result<()> {
        let embedded = get_embedded()?;
        let temp_dir = tempdir::TempDir::new("pyoxidizer-test")?;

        embedded.write_files(temp_dir.path())?;

        Ok(())
    }

    #[test]
    fn test_build_simple() -> Result<()> {
        get_embedded()?;

        Ok(())
    }
}
