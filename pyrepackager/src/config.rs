// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use sha2::{Digest, Sha256};
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use url::Url;

// TOML config file parsing.
#[derive(Debug, Deserialize)]
struct PythonDistribution {
    local_path: Option<String>,
    url: Option<String>,
    sha256: String,
}

#[allow(non_snake_case)]
fn TRUE() -> bool {
    true
}

#[allow(non_snake_case)]
fn FALSE() -> bool {
    false
}

#[allow(non_snake_case)]
fn ZERO() -> i64 {
    0
}

#[derive(Debug, Deserialize)]
struct PythonConfig {
    #[serde(default = "TRUE")]
    dont_write_bytecode: bool,
    #[serde(default = "TRUE")]
    ignore_environment: bool,
    #[serde(default = "TRUE")]
    no_site: bool,
    #[serde(default = "TRUE")]
    no_user_site_directory: bool,
    #[serde(default = "ZERO")]
    optimize_level: i64,
    program_name: Option<String>,
    stdio_encoding: Option<String>,
    #[serde(default = "FALSE")]
    unbuffered_stdio: bool,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "policy")]
pub enum PythonExtensions {
    #[serde(rename = "all")]
    All {},
    #[serde(rename = "none")]
    None {},
    #[serde(rename = "no-libraries")]
    NoLibraries {},
    #[serde(rename = "explicit-includes")]
    ExplicitIncludes {
        #[serde(default)]
        includes: Vec<String>,
    },
    #[serde(rename = "explicit-excludes")]
    ExplicitExcludes {
        #[serde(default)]
        excludes: Vec<String>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum PythonPackaging {
    #[serde(rename = "stdlib")]
    Stdlib {
        #[serde(default = "ZERO")]
        optimize_level: i64,
        #[serde(default = "TRUE")]
        exclude_test_modules: bool,
        #[serde(default = "TRUE")]
        include_source: bool,
    },
    #[serde(rename = "virtualenv")]
    Virtualenv {
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
        path: String,
        packages: Vec<String>,
        #[serde(default = "ZERO")]
        optimize_level: i64,
        #[serde(default)]
        excludes: Vec<String>,
        #[serde(default = "TRUE")]
        include_source: bool,
    },
    #[serde(rename = "filter-file-include")]
    FilterFileInclude {
        path: String,
    },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "mode")]
pub enum RunMode {
    #[serde(rename = "repl")]
    Repl {},
    #[serde(rename = "module")]
    Module { module: String },
    #[serde(rename = "eval")]
    Eval { code: String },
}

#[derive(Debug, Deserialize)]
struct ParsedConfig {
    python_distribution: PythonDistribution,
    python_config: PythonConfig,
    python_extensions: PythonExtensions,
    python_packages: Vec<PythonPackaging>,
    python_run: RunMode,
}

#[derive(Debug)]
pub struct Config {
    pub dont_write_bytecode: bool,
    pub ignore_environment: bool,
    pub no_site: bool,
    pub no_user_site_directory: bool,
    pub optimize_level: i64,
    pub program_name: String,
    pub python_distribution_path: Option<String>,
    pub python_distribution_url: Option<String>,
    pub python_distribution_sha256: String,
    pub stdio_encoding_name: Option<String>,
    pub stdio_encoding_errors: Option<String>,
    pub unbuffered_stdio: bool,
    pub python_extensions: PythonExtensions,
    pub python_packaging: Vec<PythonPackaging>,
    pub run: RunMode,
}

pub fn parse_config(data: &Vec<u8>) -> Config {
    let config: ParsedConfig = toml::from_slice(&data).unwrap();

    let optimize_level = match config.python_config.optimize_level {
        0 => 0,
        1 => 1,
        2 => 2,
        value => panic!("illegal optimize_level {}; value must be 0, 1, or 2", value),
    };

    let program_name = match config.python_config.program_name {
        Some(value) => value,
        None => String::from("undefined"),
    };

    let python_distribution_path = config.python_distribution.local_path;
    let python_distribution_url = config.python_distribution.url;
    let python_distribution_sha256 = config.python_distribution.sha256;

    let (stdio_encoding_name, stdio_encoding_errors) = match config.python_config.stdio_encoding {
        Some(value) => {
            let values: Vec<&str> = value.split(":").collect();
            (Some(values[0].to_string()), Some(values[1].to_string()))
        }
        None => (None, None),
    };

    let mut have_stdlib = false;

    for packaging in &config.python_packages {
        match packaging {
            PythonPackaging::Stdlib { .. } => {
                have_stdlib = true;
            }
            _ => {}
        }
    }

    if !have_stdlib {
        panic!("no `type = \"stdlib\"` entry in `[[python_packages]]`");
    }

    Config {
        dont_write_bytecode: config.python_config.dont_write_bytecode,
        ignore_environment: config.python_config.ignore_environment,
        no_site: config.python_config.no_site,
        no_user_site_directory: config.python_config.no_user_site_directory,
        optimize_level,
        program_name,
        python_distribution_path,
        python_distribution_url,
        python_distribution_sha256,
        stdio_encoding_name,
        stdio_encoding_errors,
        unbuffered_stdio: config.python_config.unbuffered_stdio,
        python_extensions: config.python_extensions,
        python_packaging: config.python_packages,
        run: config.python_run,
    }
}

/// Obtain a local Path for a Python distribution tar archive.
///
/// Takes a parsed config and a cache directory as input. Usually the cache
/// directory is the OUT_DIR for the invocation of a Cargo build script.
/// A Python distribution will be fetched according to the configuration and a
/// copy of the archive placed in ``cache_dir``. If the archive already exists
/// in ``cache_dir``, it will be verified and returned.
///
/// Local filesystem paths are preferred over remote URLs if both are defined.
pub fn resolve_python_distribution_archive(config: &Config, cache_dir: &Path) -> PathBuf {
    let expected_hash = hex::decode(&config.python_distribution_sha256).unwrap();

    let basename = match &config.python_distribution_path {
        Some(path) => {
            let p = Path::new(path);
            p.file_name().unwrap().to_str().unwrap().to_string()
        }
        None => match &config.python_distribution_url {
            Some(url) => {
                let url = Url::parse(url).expect("failed to parse URL");
                url.path_segments()
                    .expect("cannot be base path")
                    .last()
                    .expect("could not get last element")
                    .to_string()
            }
            None => panic!("neither local path nor URL defined for distribution"),
        },
    };

    let cache_path = cache_dir.join(basename);

    if cache_path.exists() {
        let mut hasher = Sha256::new();
        let mut fh = File::open(&cache_path).unwrap();
        let mut data = Vec::new();
        fh.read_to_end(&mut data).unwrap();
        hasher.input(data);

        let file_hash = hasher.result().to_vec();

        // We don't care about timing side-channels from the string compare.
        if file_hash == expected_hash {
            return cache_path;
        }
    }

    match &config.python_distribution_path {
        Some(path) => {
            let mut hasher = Sha256::new();
            let mut fh = File::open(path).unwrap();
            let mut data = Vec::new();
            fh.read_to_end(&mut data).unwrap();
            hasher.input(data);

            let file_hash = hasher.result().to_vec();

            if file_hash != expected_hash {
                panic!("sha256 of Python distribution does not validate");
            }

            std::fs::copy(path, &cache_path).unwrap();
            cache_path
        }
        None => match &config.python_distribution_url {
            Some(url) => {
                let mut data: Vec<u8> = Vec::new();

                let mut response = reqwest::get(url).expect("unable to perform HTTP request");
                response
                    .read_to_end(&mut data)
                    .expect("unable to download URL");

                let mut hasher = Sha256::new();
                hasher.input(&data);

                let url_hash = hasher.result().to_vec();
                if url_hash != expected_hash {
                    panic!("sha256 of Python distribution does not validate");
                }

                fs::write(&cache_path, data).expect("unable to write file");
                cache_path
            }
            None => panic!("expected distribution path or URL"),
        },
    }
}
