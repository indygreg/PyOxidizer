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

#[derive(Debug, Deserialize)]
struct PythonConfig {
    dont_write_bytecode: Option<bool>,
    ignore_environment: Option<bool>,
    no_site: Option<bool>,
    no_user_site_directory: Option<bool>,
    optimize_level: Option<i64>,
    program_name: Option<String>,
    stdio_encoding: Option<String>,
    unbuffered_stdio: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct PythonPackaging {
    module_paths: Option<toml::value::Array>,
    optimize_level: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct ParsedConfig {
    python_distribution: PythonDistribution,
    python_config: PythonConfig,
    python_packaging: PythonPackaging,
}

#[derive(Debug)]
pub struct Config {
    pub dont_write_bytecode: bool,
    pub ignore_environment: bool,
    pub no_site: bool,
    pub no_user_site_directory: bool,
    pub optimize_level: i64,
    pub package_module_paths: Vec<PathBuf>,
    pub package_optimize_level: i64,
    pub program_name: String,
    pub python_distribution_path: Option<String>,
    pub python_distribution_url: Option<String>,
    pub python_distribution_sha256: String,
    pub stdio_encoding_name: Option<String>,
    pub stdio_encoding_errors: Option<String>,
    pub unbuffered_stdio: bool,
}

pub fn parse_config(data: &Vec<u8>) -> Config {
    let config: ParsedConfig = toml::from_slice(&data).unwrap();

    let dont_write_bytecode = match config.python_config.dont_write_bytecode {
        Some(value) => value,
        None => true,
    };

    let ignore_environment = match config.python_config.ignore_environment {
        Some(value) => value,
        None => true,
    };

    let optimize_level = match config.python_config.optimize_level {
        Some(0) => 0,
        Some(1) => 1,
        Some(2) => 2,
        Some(value) => panic!("illegal optimize_level {}; value must be 0, 1, or 2", value),
        None => 0,
    };

    let no_site = match config.python_config.no_site {
        Some(value) => value,
        None => true,
    };

    let no_user_site_directory = match config.python_config.no_user_site_directory {
        Some(value) => value,
        None => true,
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
        },
        None => (None, None),
    };

    let unbuffered_stdio = match config.python_config.unbuffered_stdio {
        Some(value) => value,
        None => false,
    };

    let package_module_paths = match config.python_packaging.module_paths {
        Some(value) => {
            value.iter().map(|p| PathBuf::from(p.as_str().unwrap())).collect()
        },
        None => Vec::new(),
    };

    let package_optimize_level = match config.python_packaging.optimize_level {
        Some(0) => 0,
        Some(1) => 1,
        Some(2) => 2,
        Some(value) => panic!("illegal optimize_level {}: value must be 0, 1 or 2", value),
        None => 0,
    };

    Config {
        dont_write_bytecode,
        ignore_environment,
        no_site,
        no_user_site_directory,
        optimize_level,
        package_module_paths,
        package_optimize_level,
        program_name,
        python_distribution_path,
        python_distribution_url,
        python_distribution_sha256,
        stdio_encoding_name,
        stdio_encoding_errors,
        unbuffered_stdio,
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
        },
        None => match &config.python_distribution_url {
            Some(url) => {
                let url = Url::parse(url).expect("failed to parse URL");
                url.path_segments().expect("cannot be base path").last().expect("could not get last element").to_string()
            },
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
        },
        None => match &config.python_distribution_url {
            Some(url) => {
                let mut data: Vec<u8> = Vec::new();

                let mut response = reqwest::get(url).expect("unable to perform HTTP request");
                response.read_to_end(&mut data).expect("unable to download URL");

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
