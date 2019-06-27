// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::super::environment::canonicalize_path;
use serde::Deserialize;
use std::path::{Path, PathBuf};

// TOML config file parsing.

#[serde(untagged)]
#[derive(Debug, Deserialize)]
enum ConfigPythonDistribution {
    Local {
        build_target: String,
        local_path: String,
        sha256: String,
    },
    Url {
        build_target: String,
        url: String,
        sha256: String,
    },
}

#[allow(non_snake_case)]
fn TRUE() -> bool {
    true
}

#[allow(non_snake_case)]
fn ZERO() -> i64 {
    0
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub enum RawAllocator {
    #[serde(rename = "jemalloc")]
    Jemalloc,
    #[serde(rename = "rust")]
    Rust,
    #[serde(rename = "system")]
    System,
}

#[allow(non_snake_case)]
fn ALL() -> String {
    "all".to_string()
}

#[derive(Debug, Deserialize)]
struct ConfigBuild {
    #[serde(default = "ALL")]
    build_target: String,
    application_name: Option<String>,
    build_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ConfigPython {
    #[serde(default = "ALL")]
    build_target: String,
    dont_write_bytecode: Option<bool>,
    ignore_environment: Option<bool>,
    no_site: Option<bool>,
    no_user_site_directory: Option<bool>,
    optimize_level: Option<i64>,
    program_name: Option<String>,
    stdio_encoding: Option<String>,
    unbuffered_stdio: Option<bool>,
    filesystem_importer: Option<bool>,
    sys_paths: Option<Vec<String>>,
    raw_allocator: Option<RawAllocator>,
    write_modules_directory_env: Option<String>,
}

#[allow(non_snake_case)]
fn EMBEDDED() -> String {
    "embedded".to_string()
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ConfigPythonPackaging {
    #[serde(rename = "setup-py-install")]
    SetupPyInstall {
        #[serde(default = "ALL")]
        build_target: String,
        package_path: String,
        #[serde(default = "ZERO")]
        optimize_level: i64,
        #[serde(default = "TRUE")]
        include_source: bool,
        #[serde(default = "EMBEDDED")]
        install_location: String,
    },

    #[serde(rename = "stdlib-extensions-policy")]
    StdlibExtensionsPolicy {
        #[serde(default = "ALL")]
        build_target: String,
        // TODO make this an enum.
        policy: String,
    },

    #[serde(rename = "stdlib-extensions-explicit-includes")]
    StdlibExtensionsExplicitIncludes {
        #[serde(default = "ALL")]
        build_target: String,
        #[serde(default)]
        includes: Vec<String>,
    },

    #[serde(rename = "stdlib-extensions-explicit-excludes")]
    StdlibExtensionsExplicitExcludes {
        #[serde(default = "ALL")]
        build_target: String,
        #[serde(default)]
        excludes: Vec<String>,
    },

    #[serde(rename = "stdlib-extension-variant")]
    StdlibExtensionVariant {
        #[serde(default = "ALL")]
        build_target: String,
        extension: String,
        variant: String,
    },

    #[serde(rename = "stdlib")]
    Stdlib {
        #[serde(default = "ALL")]
        build_target: String,
        #[serde(default = "ZERO")]
        optimize_level: i64,
        #[serde(default = "TRUE")]
        exclude_test_modules: bool,
        #[serde(default = "TRUE")]
        include_source: bool,
        #[serde(default)]
        include_resources: bool,
        #[serde(default = "EMBEDDED")]
        install_location: String,
    },

    #[serde(rename = "virtualenv")]
    Virtualenv {
        #[serde(default = "ALL")]
        build_target: String,
        path: String,
        #[serde(default = "ZERO")]
        optimize_level: i64,
        #[serde(default)]
        excludes: Vec<String>,
        #[serde(default = "TRUE")]
        include_source: bool,
        #[serde(default = "EMBEDDED")]
        install_location: String,
    },

    #[serde(rename = "package-root")]
    PackageRoot {
        #[serde(default = "ALL")]
        build_target: String,
        path: String,
        packages: Vec<String>,
        #[serde(default = "ZERO")]
        optimize_level: i64,
        #[serde(default)]
        excludes: Vec<String>,
        #[serde(default = "TRUE")]
        include_source: bool,
        #[serde(default = "EMBEDDED")]
        install_location: String,
    },

    #[serde(rename = "pip-install-simple")]
    PipInstallSimple {
        #[serde(default = "ALL")]
        build_target: String,
        package: String,
        #[serde(default = "ZERO")]
        optimize_level: i64,
        #[serde(default)]
        excludes: Vec<String>,
        #[serde(default = "TRUE")]
        include_source: bool,
        #[serde(default = "EMBEDDED")]
        install_location: String,
    },

    #[serde(rename = "pip-requirements-file")]
    PipRequirementsFile {
        #[serde(default = "ALL")]
        build_target: String,
        requirements_path: String,
        #[serde(default = "ZERO")]
        optimize_level: i64,
        #[serde(default = "TRUE")]
        include_source: bool,
        #[serde(default = "EMBEDDED")]
        install_location: String,
    },

    #[serde(rename = "filter-include")]
    FilterInclude {
        #[serde(default = "ALL")]
        build_target: String,

        files: Vec<String>,
        glob_files: Vec<String>,
    },

    #[serde(rename = "write-license-files")]
    WriteLicenseFiles {
        #[serde(default = "ALL")]
        build_target: String,

        path: String,
    },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "mode")]
enum ConfigRunMode {
    #[serde(rename = "noop")]
    Noop {
        #[serde(default = "ALL")]
        build_target: String,
    },
    #[serde(rename = "repl")]
    Repl {
        #[serde(default = "ALL")]
        build_target: String,
    },
    #[serde(rename = "module")]
    Module {
        #[serde(default = "ALL")]
        build_target: String,
        module: String,
    },
    #[serde(rename = "eval")]
    Eval {
        #[serde(default = "ALL")]
        build_target: String,
        code: String,
    },
}

#[derive(Debug, Deserialize)]
struct ParsedConfig {
    #[serde(default, rename = "build")]
    builds: Vec<ConfigBuild>,
    #[serde(default, rename = "python_distribution")]
    python_distributions: Vec<ConfigPythonDistribution>,
    #[serde(default, rename = "embedded_python_config")]
    python_configs: Vec<ConfigPython>,
    #[serde(rename = "packaging_rule")]
    packaging_rules: Vec<ConfigPythonPackaging>,
    #[serde(rename = "embedded_python_run")]
    python_run: Vec<ConfigRunMode>,
}

#[derive(Clone, Debug)]
pub struct BuildConfig {
    pub application_name: String,
    pub build_path: PathBuf,
}

#[derive(Clone, Debug)]
pub enum PythonDistribution {
    Local { local_path: String, sha256: String },
    Url { url: String, sha256: String },
}

#[derive(Clone, Debug)]
pub enum InstallLocation {
    Embedded,
    AppRelative { path: String },
}

#[derive(Clone, Debug)]
pub struct PackagingSetupPyInstall {
    pub path: String,
    pub optimize_level: i64,
    pub include_source: bool,
    pub install_location: InstallLocation,
}

#[derive(Clone, Debug)]
pub struct PackagingStdlibExtensionsPolicy {
    // TODO make this an enum.
    pub policy: String,
}

#[derive(Clone, Debug)]
pub struct PackagingStdlibExtensionsExplicitIncludes {
    pub includes: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct PackagingStdlibExtensionsExplicitExcludes {
    pub excludes: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct PackagingStdlibExtensionVariant {
    pub extension: String,
    pub variant: String,
}

#[derive(Clone, Debug)]
pub struct PackagingStdlib {
    pub optimize_level: i64,
    pub exclude_test_modules: bool,
    pub include_source: bool,
    pub include_resources: bool,
    pub install_location: InstallLocation,
}

#[derive(Clone, Debug)]
pub struct PackagingVirtualenv {
    pub path: String,
    pub optimize_level: i64,
    pub excludes: Vec<String>,
    pub include_source: bool,
    pub install_location: InstallLocation,
}

#[derive(Clone, Debug)]
pub struct PackagingPackageRoot {
    pub path: String,
    pub packages: Vec<String>,
    pub optimize_level: i64,
    pub excludes: Vec<String>,
    pub include_source: bool,
    pub install_location: InstallLocation,
}

#[derive(Clone, Debug)]
pub struct PackagingPipInstallSimple {
    pub package: String,
    pub optimize_level: i64,
    pub excludes: Vec<String>,
    pub include_source: bool,
    pub install_location: InstallLocation,
}

#[derive(Clone, Debug)]
pub struct PackagingPipRequirementsFile {
    // TODO resolve to a PathBuf.
    pub requirements_path: String,
    pub optimize_level: i64,
    pub include_source: bool,
    pub install_location: InstallLocation,
}

#[derive(Clone, Debug)]
pub struct PackagingFilterInclude {
    pub files: Vec<String>,
    pub glob_files: Vec<String>,
}

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug)]
pub enum RunMode {
    Noop,
    Repl,
    Module { module: String },
    Eval { code: String },
}

/// Represents a parsed PyOxidizer configuration file.
#[derive(Clone, Debug)]
pub struct Config {
    pub config_path: PathBuf,
    pub build_config: BuildConfig,
    pub dont_write_bytecode: bool,
    pub ignore_environment: bool,
    pub no_site: bool,
    pub no_user_site_directory: bool,
    pub optimize_level: i64,
    pub program_name: String,
    pub python_distribution: PythonDistribution,
    pub stdio_encoding_name: Option<String>,
    pub stdio_encoding_errors: Option<String>,
    pub unbuffered_stdio: bool,
    pub python_packaging: Vec<PythonPackaging>,
    pub run: RunMode,
    pub filesystem_importer: bool,
    pub sys_paths: Vec<String>,
    pub raw_allocator: RawAllocator,
    pub write_modules_directory_env: Option<String>,
}

fn resolve_install_location(value: &str) -> Result<InstallLocation, String> {
    if value == "embedded" {
        Ok(InstallLocation::Embedded)
    } else if value.starts_with("app-relative:") {
        let path = value[13..value.len()].to_string();

        Ok(InstallLocation::AppRelative { path })
    } else {
        Err(format!("invalid install_location: {}", value))
    }
}

/// Parse a PyOxidizer TOML config from raw data.
///
/// Configs are evaluated against a specific build target. Config entries not
/// relevant to the specified target are removed from the final data structure.
pub fn parse_config(data: &[u8], config_path: &Path, target: &str) -> Result<Config, String> {
    let config: ParsedConfig = match toml::from_slice(&data) {
        Ok(v) => v,
        Err(e) => return Err(e.to_string()),
    };

    let origin = canonicalize_path(
        config_path
            .parent()
            .ok_or_else(|| "unable to get config parent directory")?,
    )
    .or_else(|e| Err(e.to_string()))?
    .display()
    .to_string();

    let mut application_name = None;
    let mut build_path = PathBuf::from(&origin).join("build");

    for build_config in config
        .builds
        .iter()
        .filter(|c| c.build_target == "all" || c.build_target == target)
    {
        if let Some(ref name) = build_config.application_name {
            application_name = Some(name.clone());
        }

        if let Some(ref path) = build_config.build_path {
            build_path = PathBuf::from(path.replace("$ORIGIN", &origin));
        }
    }

    if application_name.is_none() {
        return Err("no [[build]] application_name defined".to_string());
    }

    let build_config = BuildConfig {
        application_name: application_name.clone().unwrap(),
        build_path,
    };

    if config.python_distributions.is_empty() {
        return Err("no [[python_distribution]] sections".to_string());
    }

    let python_distribution = match config
        .python_distributions
        .iter()
        .filter_map(|d| match d {
            ConfigPythonDistribution::Local {
                build_target: dist_target,
                local_path,
                sha256,
            } => {
                if dist_target == target {
                    Some(PythonDistribution::Local {
                        local_path: local_path.clone(),
                        sha256: sha256.clone(),
                    })
                } else {
                    None
                }
            }

            ConfigPythonDistribution::Url {
                build_target: dist_target,
                url,
                sha256,
            } => {
                if dist_target == target {
                    Some(PythonDistribution::Url {
                        url: url.clone(),
                        sha256: sha256.clone(),
                    })
                } else {
                    None
                }
            }
        })
        .next()
    {
        Some(v) => v,
        None => {
            return Err(format!(
                "no suitable Python distributions found for target {}",
                target
            ))
        }
    };

    let mut dont_write_bytecode = true;
    let mut ignore_environment = true;
    let mut no_site = true;
    let mut no_user_site_directory = true;
    let mut optimize_level = 0;
    let mut program_name = application_name.unwrap();
    let mut stdio_encoding_name = None;
    let mut stdio_encoding_errors = None;
    let mut unbuffered_stdio = false;
    let mut filesystem_importer = false;
    let mut sys_paths = Vec::new();
    let mut raw_allocator = if target == "x86_64-pc-windows-msvc" {
        RawAllocator::System
    } else {
        RawAllocator::Jemalloc
    };
    let mut write_modules_directory_env = None;

    for python_config in config
        .python_configs
        .iter()
        .filter(|c| c.build_target == "all" || c.build_target == target)
    {
        if let Some(v) = python_config.dont_write_bytecode {
            dont_write_bytecode = v;
        }

        if let Some(v) = python_config.ignore_environment {
            ignore_environment = v;
        }

        if let Some(v) = python_config.no_site {
            no_site = v;
        }

        if let Some(v) = python_config.no_user_site_directory {
            no_user_site_directory = v;
        }

        if let Some(v) = python_config.optimize_level {
            optimize_level = match v {
                0 => 0,
                1 => 1,
                2 => 2,
                value => {
                    return Err(format!(
                        "illegal optimize_level {}; value must be 0, 1, or 2",
                        value
                    ))
                }
            };
        }

        if let Some(ref v) = python_config.program_name {
            program_name = v.clone();
        }

        if let Some(ref v) = python_config.stdio_encoding {
            let values: Vec<&str> = v.split(':').collect();
            stdio_encoding_name = Some(values[0].to_string());
            stdio_encoding_errors = Some(values[1].to_string());
        }

        if let Some(v) = python_config.unbuffered_stdio {
            unbuffered_stdio = v;
        }

        if let Some(v) = python_config.filesystem_importer {
            filesystem_importer = v;
        }

        if let Some(ref v) = python_config.sys_paths {
            sys_paths = v.clone();
        }

        if let Some(ref v) = python_config.raw_allocator {
            raw_allocator = v.clone();
        }

        if let Some(ref v) = python_config.write_modules_directory_env {
            write_modules_directory_env = Some(v.clone());
        }
    }

    let mut have_stdlib_extensions_policy = false;
    let mut have_stdlib = false;

    let python_packaging: Result<Vec<Option<PythonPackaging>>, String> = config
        .packaging_rules
        .iter()
        .map(|r| match r {
            ConfigPythonPackaging::FilterInclude {
                build_target: rule_target,
                files,
                glob_files,
            } => {
                if rule_target == "all" || rule_target == target {
                    Ok(Some(PythonPackaging::FilterInclude(
                        PackagingFilterInclude {
                            files: files.clone(),
                            glob_files: glob_files.clone(),
                        },
                    )))
                } else {
                    Ok(None)
                }
            }
            ConfigPythonPackaging::PackageRoot {
                build_target: rule_target,
                path,
                packages,
                optimize_level,
                excludes,
                include_source,
                install_location,
            } => {
                if rule_target == "all" || rule_target == target {
                    Ok(Some(PythonPackaging::PackageRoot(PackagingPackageRoot {
                        path: path.clone(),
                        packages: packages.clone(),
                        optimize_level: *optimize_level,
                        excludes: excludes.clone(),
                        include_source: *include_source,
                        install_location: resolve_install_location(&install_location)?,
                    })))
                } else {
                    Ok(None)
                }
            }
            ConfigPythonPackaging::PipInstallSimple {
                build_target: rule_target,
                package,
                optimize_level,
                excludes,
                include_source,
                install_location,
            } => {
                if rule_target == "all" || rule_target == target {
                    Ok(Some(PythonPackaging::PipInstallSimple(
                        PackagingPipInstallSimple {
                            package: package.clone(),
                            optimize_level: *optimize_level,
                            excludes: excludes.clone(),
                            include_source: *include_source,
                            install_location: resolve_install_location(&install_location)?,
                        },
                    )))
                } else {
                    Ok(None)
                }
            }
            ConfigPythonPackaging::PipRequirementsFile {
                build_target: rule_target,
                requirements_path,
                optimize_level,
                include_source,
                install_location,
            } => {
                if rule_target == "all" || rule_target == target {
                    Ok(Some(PythonPackaging::PipRequirementsFile(
                        PackagingPipRequirementsFile {
                            requirements_path: requirements_path.clone(),
                            optimize_level: *optimize_level,
                            include_source: *include_source,
                            install_location: resolve_install_location(&install_location)?,
                        },
                    )))
                } else {
                    Ok(None)
                }
            }
            ConfigPythonPackaging::SetupPyInstall {
                build_target: rule_target,
                package_path,
                optimize_level,
                include_source,
                install_location,
            } => {
                if rule_target == "all" || rule_target == target {
                    Ok(Some(PythonPackaging::SetupPyInstall(
                        PackagingSetupPyInstall {
                            path: package_path.clone(),
                            optimize_level: *optimize_level,
                            include_source: *include_source,
                            install_location: resolve_install_location(&install_location)?,
                        },
                    )))
                } else {
                    Ok(None)
                }
            }
            ConfigPythonPackaging::Stdlib {
                build_target: rule_target,
                optimize_level,
                exclude_test_modules,
                include_source,
                include_resources,
                install_location,
            } => {
                if rule_target == "all" || rule_target == target {
                    have_stdlib = true;

                    Ok(Some(PythonPackaging::Stdlib(PackagingStdlib {
                        optimize_level: *optimize_level,
                        exclude_test_modules: *exclude_test_modules,
                        include_source: *include_source,
                        include_resources: *include_resources,
                        install_location: resolve_install_location(&install_location)?,
                    })))
                } else {
                    Ok(None)
                }
            }
            ConfigPythonPackaging::StdlibExtensionsExplicitExcludes {
                build_target: rule_target,
                excludes,
            } => {
                if rule_target == "all" || rule_target == target {
                    Ok(Some(PythonPackaging::StdlibExtensionsExplicitExcludes(
                        PackagingStdlibExtensionsExplicitExcludes {
                            excludes: excludes.clone(),
                        },
                    )))
                } else {
                    Ok(None)
                }
            }
            ConfigPythonPackaging::StdlibExtensionsExplicitIncludes {
                build_target: rule_target,
                includes,
            } => {
                if rule_target == "all" || rule_target == target {
                    Ok(Some(PythonPackaging::StdlibExtensionsExplicitIncludes(
                        PackagingStdlibExtensionsExplicitIncludes {
                            includes: includes.clone(),
                        },
                    )))
                } else {
                    Ok(None)
                }
            }
            ConfigPythonPackaging::StdlibExtensionsPolicy {
                build_target: rule_target,
                policy,
            } => {
                if rule_target == "all" || rule_target == target {
                    have_stdlib_extensions_policy = true;

                    Ok(Some(PythonPackaging::StdlibExtensionsPolicy(
                        PackagingStdlibExtensionsPolicy {
                            policy: policy.clone(),
                        },
                    )))
                } else {
                    Ok(None)
                }
            }
            ConfigPythonPackaging::StdlibExtensionVariant {
                build_target: rule_target,
                extension,
                variant,
            } => {
                if rule_target == "all" || rule_target == target {
                    Ok(Some(PythonPackaging::StdlibExtensionVariant(
                        PackagingStdlibExtensionVariant {
                            extension: extension.clone(),
                            variant: variant.clone(),
                        },
                    )))
                } else {
                    Ok(None)
                }
            }
            ConfigPythonPackaging::Virtualenv {
                build_target: rule_target,
                path,
                optimize_level,
                excludes,
                include_source,
                install_location,
            } => {
                if rule_target == "all" || rule_target == target {
                    Ok(Some(PythonPackaging::Virtualenv(PackagingVirtualenv {
                        path: path.clone(),
                        optimize_level: *optimize_level,
                        excludes: excludes.clone(),
                        include_source: *include_source,
                        install_location: resolve_install_location(&install_location)?,
                    })))
                } else {
                    Ok(None)
                }
            }
            ConfigPythonPackaging::WriteLicenseFiles {
                build_target: rule_target,
                path,
            } => {
                if rule_target == "all" || rule_target == target {
                    Ok(Some(PythonPackaging::WriteLicenseFiles(
                        PackagingWriteLicenseFiles { path: path.clone() },
                    )))
                } else {
                    Ok(None)
                }
            }
        })
        .collect();

    let python_packaging: Vec<PythonPackaging> = python_packaging?
        .clone()
        .iter()
        // .clone() is needed to avoid move out of borrowed content. There's surely
        // a better way to do this. But it isn't performance critical, so low
        // priority.
        .filter_map(|v| v.clone())
        .collect();

    if !have_stdlib_extensions_policy {
        return Err(
            "no `type = \"stdlib-extensions-policy\"` entry in `[[packaging_rule]]`".to_string(),
        );
    }

    if !have_stdlib {
        return Err("no `type = \"stdlib\"` entry in `[[packaging_rule]]`".to_string());
    }

    let mut run = RunMode::Noop {};

    for run_mode in config.python_run.iter().filter_map(|r| match r {
        ConfigRunMode::Eval {
            build_target: run_target,
            code,
        } => {
            if run_target == "all" || run_target == target {
                Some(RunMode::Eval { code: code.clone() })
            } else {
                None
            }
        }
        ConfigRunMode::Module {
            build_target: run_target,
            module,
        } => {
            if run_target == "all" || run_target == target {
                Some(RunMode::Module {
                    module: module.clone(),
                })
            } else {
                None
            }
        }
        ConfigRunMode::Noop {
            build_target: run_target,
        } => {
            if run_target == "all" || run_target == target {
                Some(RunMode::Noop)
            } else {
                None
            }
        }
        ConfigRunMode::Repl {
            build_target: run_target,
        } => {
            if run_target == "all" || run_target == target {
                Some(RunMode::Repl)
            } else {
                None
            }
        }
    }) {
        run = run_mode;
    }

    filesystem_importer = filesystem_importer || !sys_paths.is_empty();

    Ok(Config {
        config_path: config_path.to_path_buf(),
        build_config,
        dont_write_bytecode,
        ignore_environment,
        no_site,
        no_user_site_directory,
        optimize_level,
        program_name,
        python_distribution,
        stdio_encoding_name,
        stdio_encoding_errors,
        unbuffered_stdio,
        python_packaging,
        run,
        filesystem_importer,
        sys_paths,
        raw_allocator,
        write_modules_directory_env,
    })
}
