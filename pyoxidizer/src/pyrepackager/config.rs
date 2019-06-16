// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use itertools::Itertools;
use serde::Deserialize;
use std::path::{Path, PathBuf};

// TOML config file parsing.

#[serde(untagged)]
#[derive(Debug, Deserialize)]
enum ConfigPythonDistribution {
    Local {
        target: String,
        local_path: String,
        sha256: String,
    },
    Url {
        target: String,
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

#[derive(Clone, Debug, Deserialize)]
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
    target: String,
    artifacts_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ConfigPython {
    #[serde(default = "ALL")]
    target: String,
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

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ConfigPythonPackaging {
    #[serde(rename = "setup-py-install")]
    SetupPyInstall {
        #[serde(default = "ALL")]
        target: String,
        package_path: String,
        #[serde(default = "ZERO")]
        optimize_level: i64,
        #[serde(default = "TRUE")]
        include_source: bool,
    },

    #[serde(rename = "stdlib-extensions-policy")]
    StdlibExtensionsPolicy {
        #[serde(default = "ALL")]
        target: String,
        // TODO make this an enum.
        policy: String,
    },

    #[serde(rename = "stdlib-extensions-explicit-includes")]
    StdlibExtensionsExplicitIncludes {
        #[serde(default = "ALL")]
        target: String,
        #[serde(default)]
        includes: Vec<String>,
    },

    #[serde(rename = "stdlib-extensions-explicit-excludes")]
    StdlibExtensionsExplicitExcludes {
        #[serde(default = "ALL")]
        target: String,
        #[serde(default)]
        excludes: Vec<String>,
    },

    #[serde(rename = "stdlib-extension-variant")]
    StdlibExtensionVariant {
        #[serde(default = "ALL")]
        target: String,
        extension: String,
        variant: String,
    },

    #[serde(rename = "stdlib")]
    Stdlib {
        #[serde(default = "ALL")]
        target: String,
        #[serde(default = "ZERO")]
        optimize_level: i64,
        #[serde(default = "TRUE")]
        exclude_test_modules: bool,
        #[serde(default = "TRUE")]
        include_source: bool,
        #[serde(default)]
        include_resources: bool,
    },

    #[serde(rename = "virtualenv")]
    Virtualenv {
        #[serde(default = "ALL")]
        target: String,
        path: String,
        #[serde(default = "ZERO")]
        optimize_level: i64,
        #[serde(default)]
        excludes: Vec<String>,
        #[serde(default = "TRUE")]
        include_source: bool,
    },

    #[serde(rename = "package-root")]
    PackageRoot {
        #[serde(default = "ALL")]
        target: String,
        path: String,
        packages: Vec<String>,
        #[serde(default = "ZERO")]
        optimize_level: i64,
        #[serde(default)]
        excludes: Vec<String>,
        #[serde(default = "TRUE")]
        include_source: bool,
    },

    #[serde(rename = "pip-install-simple")]
    PipInstallSimple {
        #[serde(default = "ALL")]
        target: String,
        package: String,
        #[serde(default = "ZERO")]
        optimize_level: i64,
        #[serde(default = "TRUE")]
        include_source: bool,
    },

    #[serde(rename = "pip-requirements-file")]
    PipRequirementsFile {
        #[serde(default = "ALL")]
        target: String,
        requirements_path: String,
        #[serde(default = "ZERO")]
        optimize_level: i64,
        #[serde(default = "TRUE")]
        include_source: bool,
    },

    #[serde(rename = "filter-include")]
    FilterInclude {
        #[serde(default = "ALL")]
        target: String,

        files: Vec<String>,
        glob_files: Vec<String>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "mode")]
enum ConfigRunMode {
    #[serde(rename = "noop")]
    Noop {
        #[serde(default = "ALL")]
        target: String,
    },
    #[serde(rename = "repl")]
    Repl {
        #[serde(default = "ALL")]
        target: String,
    },
    #[serde(rename = "module")]
    Module {
        #[serde(default = "ALL")]
        target: String,
        module: String,
    },
    #[serde(rename = "eval")]
    Eval {
        #[serde(default = "ALL")]
        target: String,
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
    #[serde(rename = "python_packaging_rule")]
    python_packaging_rules: Vec<ConfigPythonPackaging>,
    #[serde(rename = "embedded_python_run")]
    python_run: Vec<ConfigRunMode>,
}

#[derive(Debug)]
pub struct BuildConfig {
    pub artifacts_path: Option<PathBuf>,
}

#[derive(Debug)]
pub enum PythonDistribution {
    Local { local_path: String, sha256: String },
    Url { url: String, sha256: String },
}

#[derive(Debug)]
pub struct PackagingSetupPyInstall {
    pub path: String,
    pub optimize_level: i64,
    pub include_source: bool,
}

#[derive(Debug)]
pub struct PackagingStdlibExtensionsPolicy {
    // TODO make this an enum.
    pub policy: String,
}

#[derive(Debug)]
pub struct PackagingStdlibExtensionsExplicitIncludes {
    pub includes: Vec<String>,
}

#[derive(Debug)]
pub struct PackagingStdlibExtensionsExplicitExcludes {
    pub excludes: Vec<String>,
}

#[derive(Debug)]
pub struct PackagingStdlibExtensionVariant {
    pub extension: String,
    pub variant: String,
}

#[derive(Debug)]
pub struct PackagingStdlib {
    pub optimize_level: i64,
    pub exclude_test_modules: bool,
    pub include_source: bool,
    pub include_resources: bool,
}

#[derive(Debug)]
pub struct PackagingVirtualenv {
    pub path: String,
    pub optimize_level: i64,
    pub excludes: Vec<String>,
    pub include_source: bool,
}

#[derive(Debug)]
pub struct PackagingPackageRoot {
    pub path: String,
    pub packages: Vec<String>,
    pub optimize_level: i64,
    pub excludes: Vec<String>,
    pub include_source: bool,
}

#[derive(Debug)]
pub struct PackagingPipInstallSimple {
    pub package: String,
    pub optimize_level: i64,
    pub include_source: bool,
}

#[derive(Debug)]
pub struct PackagingPipRequirementsFile {
    // TODO resolve to a PathBuf.
    pub requirements_path: String,
    pub optimize_level: i64,
    pub include_source: bool,
}

#[derive(Debug)]
pub struct PackagingFilterInclude {
    pub files: Vec<String>,
    pub glob_files: Vec<String>,
}

#[derive(Debug)]
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
}

#[derive(Debug)]
pub enum RunMode {
    Noop,
    Repl,
    Module { module: String },
    Eval { code: String },
}

/// Represents a parsed PyOxidizer configuration file.
#[derive(Debug)]
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

/// Parse a PyOxidizer TOML config from raw data.
///
/// Configs are evaluated against a specific build target. Config entries not
/// relevant to the specified target are removed from the final data structure.
pub fn parse_config(data: &[u8], config_path: &Path, target: &str) -> Result<Config, String> {
    let config: ParsedConfig = match toml::from_slice(&data) {
        Ok(v) => v,
        Err(e) => return Err(e.to_string()),
    };

    let origin = config_path
        .parent()
        .ok_or_else(|| "unable to get config parent directory")?
        .canonicalize()
        .or_else(|e| Err(e.to_string()))?
        .display()
        .to_string();

    let mut artifacts_path = None;

    for build_config in config
        .builds
        .iter()
        .filter(|c| c.target == "all" || c.target == target)
    {
        if let Some(ref path) = build_config.artifacts_path {
            artifacts_path = Some(PathBuf::from(path.replace("$ORIGIN", &origin)));
        }
    }

    let build_config = BuildConfig { artifacts_path };

    if config.python_distributions.is_empty() {
        return Err("no [[python_distribution]] sections".to_string());
    }

    let python_distribution = match config
        .python_distributions
        .iter()
        .filter_map(|d| match d {
            ConfigPythonDistribution::Local {
                target: dist_target,
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
                target: dist_target,
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
    let mut program_name = String::from("undefined");
    let mut stdio_encoding_name = None;
    let mut stdio_encoding_errors = None;
    let mut unbuffered_stdio = false;
    let mut filesystem_importer = false;
    let mut sys_paths = Vec::new();
    let mut raw_allocator = RawAllocator::Jemalloc;
    let mut write_modules_directory_env = None;

    for python_config in config
        .python_configs
        .iter()
        .filter(|c| c.target == "all" || c.target == target)
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

    let python_packaging = config
        .python_packaging_rules
        .iter()
        .filter_map(|r| match r {
            ConfigPythonPackaging::FilterInclude {
                target: rule_target,
                files,
                glob_files,
            } => {
                if rule_target == "all" || rule_target == target {
                    Some(PythonPackaging::FilterInclude(PackagingFilterInclude {
                        files: files.clone(),
                        glob_files: glob_files.clone(),
                    }))
                } else {
                    None
                }
            }
            ConfigPythonPackaging::PackageRoot {
                target: rule_target,
                path,
                packages,
                optimize_level,
                excludes,
                include_source,
            } => {
                if rule_target == "all" || rule_target == target {
                    Some(PythonPackaging::PackageRoot(PackagingPackageRoot {
                        path: path.clone(),
                        packages: packages.clone(),
                        optimize_level: *optimize_level,
                        excludes: excludes.clone(),
                        include_source: *include_source,
                    }))
                } else {
                    None
                }
            }
            ConfigPythonPackaging::PipInstallSimple {
                target: rule_target,
                package,
                optimize_level,
                include_source,
            } => {
                if rule_target == "all" || rule_target == target {
                    Some(PythonPackaging::PipInstallSimple(
                        PackagingPipInstallSimple {
                            package: package.clone(),
                            optimize_level: *optimize_level,
                            include_source: *include_source,
                        },
                    ))
                } else {
                    None
                }
            }
            ConfigPythonPackaging::PipRequirementsFile {
                target: rule_target,
                requirements_path,
                optimize_level,
                include_source,
            } => {
                if rule_target == "all" || rule_target == target {
                    Some(PythonPackaging::PipRequirementsFile(
                        PackagingPipRequirementsFile {
                            requirements_path: requirements_path.clone(),
                            optimize_level: *optimize_level,
                            include_source: *include_source,
                        },
                    ))
                } else {
                    None
                }
            }
            ConfigPythonPackaging::SetupPyInstall {
                target: rule_target,
                package_path,
                optimize_level,
                include_source,
            } => {
                if rule_target == "all" || rule_target == target {
                    Some(PythonPackaging::SetupPyInstall(PackagingSetupPyInstall {
                        path: package_path.clone(),
                        optimize_level: *optimize_level,
                        include_source: *include_source,
                    }))
                } else {
                    None
                }
            }
            ConfigPythonPackaging::Stdlib {
                target: rule_target,
                optimize_level,
                exclude_test_modules,
                include_source,
                include_resources,
            } => {
                if rule_target == "all" || rule_target == target {
                    have_stdlib = true;

                    Some(PythonPackaging::Stdlib(PackagingStdlib {
                        optimize_level: *optimize_level,
                        exclude_test_modules: *exclude_test_modules,
                        include_source: *include_source,
                        include_resources: *include_resources,
                    }))
                } else {
                    None
                }
            }
            ConfigPythonPackaging::StdlibExtensionsExplicitExcludes {
                target: rule_target,
                excludes,
            } => {
                if rule_target == "all" || rule_target == target {
                    Some(PythonPackaging::StdlibExtensionsExplicitExcludes(
                        PackagingStdlibExtensionsExplicitExcludes {
                            excludes: excludes.clone(),
                        },
                    ))
                } else {
                    None
                }
            }
            ConfigPythonPackaging::StdlibExtensionsExplicitIncludes {
                target: rule_target,
                includes,
            } => {
                if rule_target == "all" || rule_target == target {
                    Some(PythonPackaging::StdlibExtensionsExplicitIncludes(
                        PackagingStdlibExtensionsExplicitIncludes {
                            includes: includes.clone(),
                        },
                    ))
                } else {
                    None
                }
            }
            ConfigPythonPackaging::StdlibExtensionsPolicy {
                target: rule_target,
                policy,
            } => {
                if rule_target == "all" || rule_target == target {
                    have_stdlib_extensions_policy = true;

                    Some(PythonPackaging::StdlibExtensionsPolicy(
                        PackagingStdlibExtensionsPolicy {
                            policy: policy.clone(),
                        },
                    ))
                } else {
                    None
                }
            }
            ConfigPythonPackaging::StdlibExtensionVariant {
                target: rule_target,
                extension,
                variant,
            } => {
                if rule_target == "all" || rule_target == target {
                    Some(PythonPackaging::StdlibExtensionVariant(
                        PackagingStdlibExtensionVariant {
                            extension: extension.clone(),
                            variant: variant.clone(),
                        },
                    ))
                } else {
                    None
                }
            }
            ConfigPythonPackaging::Virtualenv {
                target: rule_target,
                path,
                optimize_level,
                excludes,
                include_source,
            } => {
                if rule_target == "all" || rule_target == target {
                    Some(PythonPackaging::Virtualenv(PackagingVirtualenv {
                        path: path.clone(),
                        optimize_level: *optimize_level,
                        excludes: excludes.clone(),
                        include_source: *include_source,
                    }))
                } else {
                    None
                }
            }
        })
        .collect_vec();

    if !have_stdlib_extensions_policy {
        return Err(
            "no `type = \"stdlib-extensions-policy\"` entry in `[[python_packaging_rule]]`"
                .to_string(),
        );
    }

    if !have_stdlib {
        return Err("no `type = \"stdlib\"` entry in `[[python_packaging_rule]]`".to_string());
    }

    let mut run = RunMode::Noop {};

    for run_mode in config.python_run.iter().filter_map(|r| match r {
        ConfigRunMode::Eval {
            target: run_target,
            code,
        } => {
            if run_target == "all" || run_target == target {
                Some(RunMode::Eval { code: code.clone() })
            } else {
                None
            }
        }
        ConfigRunMode::Module {
            target: run_target,
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
        ConfigRunMode::Noop { target: run_target } => {
            if run_target == "all" || run_target == target {
                Some(RunMode::Noop)
            } else {
                None
            }
        }
        ConfigRunMode::Repl { target: run_target } => {
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
