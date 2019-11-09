// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub enum RawAllocator {
    Jemalloc,
    Rust,
    System,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BuildConfig {
    pub application_name: String,
    pub build_path: PathBuf,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PythonDistribution {
    Local { local_path: String, sha256: String },
    Url { url: String, sha256: String },
}

#[derive(Clone, Debug, PartialEq)]
pub enum InstallLocation {
    Embedded,
    AppRelative { path: String },
}

#[derive(Clone, Debug, PartialEq)]
pub struct PackagingSetupPyInstall {
    pub path: String,
    pub extra_env: HashMap<String, String>,
    pub extra_global_arguments: Vec<String>,
    pub optimize_level: i64,
    pub include_source: bool,
    pub install_location: InstallLocation,
    pub excludes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PackagingStdlibExtensionsPolicy {
    // TODO make this an enum.
    pub policy: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PackagingStdlibExtensionsExplicitIncludes {
    pub includes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PackagingStdlibExtensionsExplicitExcludes {
    pub excludes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PackagingStdlibExtensionVariant {
    pub extension: String,
    pub variant: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PackagingStdlib {
    pub optimize_level: i64,
    pub exclude_test_modules: bool,
    pub include_source: bool,
    pub include_resources: bool,
    pub install_location: InstallLocation,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PackagingVirtualenv {
    pub path: String,
    pub optimize_level: i64,
    pub excludes: Vec<String>,
    pub include_source: bool,
    pub install_location: InstallLocation,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PackagingPackageRoot {
    pub path: String,
    pub packages: Vec<String>,
    pub optimize_level: i64,
    pub excludes: Vec<String>,
    pub include_source: bool,
    pub install_location: InstallLocation,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PackagingPipInstallSimple {
    pub package: String,
    pub extra_env: HashMap<String, String>,
    pub optimize_level: i64,
    pub excludes: Vec<String>,
    pub include_source: bool,
    pub install_location: InstallLocation,
    pub extra_args: Option<Vec<String>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PackagingPipRequirementsFile {
    // TODO resolve to a PathBuf.
    pub requirements_path: String,
    pub extra_env: HashMap<String, String>,
    pub optimize_level: i64,
    pub include_source: bool,
    pub install_location: InstallLocation,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PackagingFilterInclude {
    pub files: Vec<String>,
    pub glob_files: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PackagingWriteLicenseFiles {
    pub path: String,
}

#[derive(Clone, Debug)]
pub enum PythonPackaging {
    SetupPyInstall(PackagingSetupPyInstall),
    StdlibExtensionsPolicy(PackagingStdlibExtensionsPolicy),
    StdlibExtensionsExplicitIncludes(PackagingStdlibExtensionsExplicitIncludes),
    StdlibExtensionsExplicitExcludes(PackagingStdlibExtensionsExplicitExcludes),
    StdlibExtensionVariant(PackagingStdlibExtensionVariant),
    Stdlib(PackagingStdlib),
    Virtualenv(PackagingVirtualenv),
    PackageRoot(PackagingPackageRoot),
    PipInstallSimple(PackagingPipInstallSimple),
    PipRequirementsFile(PackagingPipRequirementsFile),
    FilterInclude(PackagingFilterInclude),
    WriteLicenseFiles(PackagingWriteLicenseFiles),
}

#[derive(Clone, Debug, PartialEq)]
pub enum RunMode {
    Noop,
    Repl,
    Module { module: String },
    Eval { code: String },
}

#[derive(Clone, Debug)]
pub struct DistributionTarball {
    pub path_prefix: Option<String>,
}

#[derive(Clone, Debug)]
pub struct DistributionWixInstaller {
    pub msi_upgrade_code_x86: Option<String>,
    pub msi_upgrade_code_amd64: Option<String>,
    pub bundle_upgrade_code: Option<String>,
}

/// Represents a distribution rule.
#[derive(Clone, Debug)]
pub enum Distribution {
    Tarball(DistributionTarball),
    WixInstaller(DistributionWixInstaller),
}

/// How the `terminfo` database is resolved at run-time.
#[derive(Clone, Debug, PartialEq)]
pub enum TerminfoResolution {
    Dynamic,
    None,
    Static(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmbeddedPythonConfig {
    pub dont_write_bytecode: bool,
    pub ignore_environment: bool,
    pub no_site: bool,
    pub no_user_site_directory: bool,
    pub optimize_level: i64,
    pub stdio_encoding_name: Option<String>,
    pub stdio_encoding_errors: Option<String>,
    pub unbuffered_stdio: bool,
    pub filesystem_importer: bool,
    pub sys_frozen: bool,
    pub sys_meipass: bool,
    pub sys_paths: Vec<String>,
    pub raw_allocator: RawAllocator,
    pub terminfo_resolution: TerminfoResolution,
    pub write_modules_directory_env: Option<String>,
}

/// Represents a parsed PyOxidizer configuration file.
#[derive(Clone, Debug)]
pub struct Config {
    pub config_path: PathBuf,
    pub build_config: BuildConfig,
    pub embedded_python_config: EmbeddedPythonConfig,
    pub python_distribution: PythonDistribution,
    pub python_packaging: Vec<PythonPackaging>,
    pub run: RunMode,
    pub distributions: Vec<Distribution>,
}

pub fn resolve_install_location(value: &str) -> Result<InstallLocation, String> {
    if value == "embedded" {
        Ok(InstallLocation::Embedded)
    } else if value.starts_with("app-relative:") {
        let path = value[13..value.len()].to_string();

        Ok(InstallLocation::AppRelative { path })
    } else {
        Err(format!("invalid install_location: {}", value))
    }
}

pub fn default_raw_allocator(target: &str) -> RawAllocator {
    if target == "x86_64-pc-windows-msvc" {
        RawAllocator::System
    } else {
        RawAllocator::Jemalloc
    }
}

pub fn eval_starlark_config_file(path: &Path, build_target: &str) -> Result<Config, String> {
    let parent = path
        .parent()
        .ok_or("unable to resolve parent directory of config".to_string())?;

    let context = super::super::starlark::env::EnvironmentContext {
        cwd: parent.to_path_buf(),
        config_path: path.to_path_buf(),
        build_target: build_target.to_string(),
    };

    let res = crate::starlark::eval::evaluate_file(path, &context).or_else(|d| Err(d.message))?;

    let config = res
        .env
        .get("CONFIG")
        .or_else(|_| Err("CONFIG not assigned".to_string()))?;

    if config.get_type() != "Config" {
        return Err(format!(
            "CONFIG must be type Config; got type {}",
            config.get_type()
        ));
    }

    Ok(config.downcast_apply(|x: &crate::starlark::config::Config| -> Config { x.config.clone() }))
}
