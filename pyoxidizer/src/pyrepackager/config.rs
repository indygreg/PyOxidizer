// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use itertools::Itertools;
use serde::Deserialize;

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

#[derive(Debug, Deserialize)]
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
struct ConfigPython {
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

    #[serde(rename = "filter-file-include")]
    FilterFileInclude {
        #[serde(default = "ALL")]
        target: String,
        path: String,
    },

    #[serde(rename = "filter-files-include")]
    FilterFilesInclude {
        #[serde(default = "ALL")]
        target: String,
        glob: String,
    },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "mode")]
pub enum RunMode {
    #[serde(rename = "noop")]
    Noop {},
    #[serde(rename = "repl")]
    Repl {},
    #[serde(rename = "module")]
    Module { module: String },
    #[serde(rename = "eval")]
    Eval { code: String },
}

#[derive(Debug, Deserialize)]
struct ParsedConfig {
    #[serde(default, rename = "python_distribution")]
    python_distributions: Vec<ConfigPythonDistribution>,
    python_config: ConfigPython,
    python_packages: Vec<ConfigPythonPackaging>,
    python_run: RunMode,
}

#[derive(Debug)]
pub enum PythonDistribution {
    Local { local_path: String, sha256: String },
    Url { url: String, sha256: String },
}

#[derive(Debug)]
pub enum PythonPackaging {
    StdlibExtensionsPolicy {
        // TODO make this an enum.
        policy: String,
    },

    StdlibExtensionsExplicitIncludes {
        includes: Vec<String>,
    },

    StdlibExtensionsExplicitExcludes {
        excludes: Vec<String>,
    },

    StdlibExtensionVariant {
        extension: String,
        variant: String,
    },

    Stdlib {
        optimize_level: i64,
        exclude_test_modules: bool,
        include_source: bool,
    },

    Virtualenv {
        path: String,
        optimize_level: i64,
        excludes: Vec<String>,
        include_source: bool,
    },

    PackageRoot {
        path: String,
        packages: Vec<String>,
        optimize_level: i64,
        excludes: Vec<String>,
        include_source: bool,
    },

    PipInstallSimple {
        package: String,
        optimize_level: i64,
        include_source: bool,
    },

    FilterFileInclude {
        path: String,
    },

    FilterFilesInclude {
        glob: String,
    },
}

#[derive(Debug)]
pub struct Config {
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
pub fn parse_config(data: &[u8], target: &str) -> Config {
    let config: ParsedConfig = toml::from_slice(&data).unwrap();

    if config.python_distributions.is_empty() {
        panic!("no [[python_distribution]] sections");
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
        None => panic!(
            "no suitable Python distributions found for target {}",
            target
        ),
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

    if let Some(v) = config.python_config.dont_write_bytecode {
        dont_write_bytecode = v;
    }

    if let Some(v) = config.python_config.ignore_environment {
        ignore_environment = v;
    }

    if let Some(v) = config.python_config.no_site {
        no_site = v;
    }

    if let Some(v) = config.python_config.no_user_site_directory {
        no_user_site_directory = v;
    }

    if let Some(v) = config.python_config.optimize_level {
        optimize_level = match v {
            0 => 0,
            1 => 1,
            2 => 2,
            value => panic!("illegal optimize_level {}; value must be 0, 1, or 2", value),
        };
    }

    if let Some(v) = config.python_config.program_name {
        program_name = v;
    }

    if let Some(v) = config.python_config.stdio_encoding {
        let values: Vec<&str> = v.split(':').collect();
        stdio_encoding_name = Some(values[0].to_string());
        stdio_encoding_errors = Some(values[1].to_string());
    }

    if let Some(v) = config.python_config.unbuffered_stdio {
        unbuffered_stdio = v;
    }

    if let Some(v) = config.python_config.filesystem_importer {
        filesystem_importer = v;
    }

    if let Some(v) = config.python_config.sys_paths {
        sys_paths = v.clone();
    }

    if let Some(v) = config.python_config.raw_allocator {
        raw_allocator = v;
    }

    if let Some(v) = config.python_config.write_modules_directory_env {
        write_modules_directory_env = Some(v);
    }

    let mut have_stdlib_extensions_policy = false;
    let mut have_stdlib = false;

    let python_packaging = config
        .python_packages
        .iter()
        .filter_map(|r| match r {
            ConfigPythonPackaging::FilterFileInclude {
                target: rule_target,
                path,
            } => {
                if rule_target == "all" || rule_target == target {
                    Some(PythonPackaging::FilterFileInclude { path: path.clone() })
                } else {
                    None
                }
            }
            ConfigPythonPackaging::FilterFilesInclude {
                target: rule_target,
                glob,
            } => {
                if rule_target == "all" || rule_target == target {
                    Some(PythonPackaging::FilterFilesInclude { glob: glob.clone() })
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
                    Some(PythonPackaging::PackageRoot {
                        path: path.clone(),
                        packages: packages.clone(),
                        optimize_level: *optimize_level,
                        excludes: excludes.clone(),
                        include_source: *include_source,
                    })
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
                    Some(PythonPackaging::PipInstallSimple {
                        package: package.clone(),
                        optimize_level: *optimize_level,
                        include_source: *include_source,
                    })
                } else {
                    None
                }
            }
            ConfigPythonPackaging::Stdlib {
                target: rule_target,
                optimize_level,
                exclude_test_modules,
                include_source,
            } => {
                if rule_target == "all" || rule_target == target {
                    have_stdlib = true;

                    Some(PythonPackaging::Stdlib {
                        optimize_level: *optimize_level,
                        exclude_test_modules: *exclude_test_modules,
                        include_source: *include_source,
                    })
                } else {
                    None
                }
            }
            ConfigPythonPackaging::StdlibExtensionsExplicitExcludes {
                target: rule_target,
                excludes,
            } => {
                if rule_target == "all" || rule_target == target {
                    Some(PythonPackaging::StdlibExtensionsExplicitExcludes {
                        excludes: excludes.clone(),
                    })
                } else {
                    None
                }
            }
            ConfigPythonPackaging::StdlibExtensionsExplicitIncludes {
                target: rule_target,
                includes,
            } => {
                if rule_target == "all" || rule_target == target {
                    Some(PythonPackaging::StdlibExtensionsExplicitIncludes {
                        includes: includes.clone(),
                    })
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

                    Some(PythonPackaging::StdlibExtensionsPolicy {
                        policy: policy.clone(),
                    })
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
                    Some(PythonPackaging::StdlibExtensionVariant {
                        extension: extension.clone(),
                        variant: variant.clone(),
                    })
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
                    Some(PythonPackaging::Virtualenv {
                        path: path.clone(),
                        optimize_level: *optimize_level,
                        excludes: excludes.clone(),
                        include_source: *include_source,
                    })
                } else {
                    None
                }
            }
        })
        .collect_vec();

    if !have_stdlib_extensions_policy {
        panic!("no `type = \"stdlib-extensions-policy\"` entry in `[[python_packages]]`");
    }

    if !have_stdlib {
        panic!("no `type = \"stdlib\"` entry in `[[python_packages]]`");
    }

    filesystem_importer = filesystem_importer || !sys_paths.is_empty();

    Config {
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
        run: config.python_run,
        filesystem_importer,
        sys_paths,
        raw_allocator,
        write_modules_directory_env,
    }
}
