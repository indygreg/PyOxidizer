// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Defining and manipulating Python distributions.
*/

use {
    super::standalone_distribution::StandaloneDistribution,
    crate::python_distributions::CPYTHON_STANDALONE_BY_TRIPLE,
    anyhow::{anyhow, Context, Result},
    fs2::FileExt,
    sha2::{Digest, Sha256},
    slog::warn,
    std::collections::BTreeMap,
    std::fs,
    std::fs::{create_dir_all, File},
    std::io::Read,
    std::path::{Path, PathBuf},
    url::Url,
    uuid::Uuid,
};

const STDLIB_TEST_PACKAGES: &[&str] = &[
    "bsddb.test",
    "ctypes.test",
    "distutils.tests",
    "email.test",
    "idlelib.idle_test",
    "json.tests",
    "lib-tk.test",
    "lib2to3.tests",
    "sqlite3.test",
    "test",
    "tkinter.test",
    "unittest.test",
];

pub fn is_stdlib_test_package(name: &str) -> bool {
    for package in STDLIB_TEST_PACKAGES {
        let prefix = format!("{}.", package);

        if &name == package || name.starts_with(&prefix) {
            return true;
        }
    }

    false
}

#[derive(Clone, Debug, PartialEq)]
pub enum PythonDistributionLocation {
    Local { local_path: String, sha256: String },
    Url { url: String, sha256: String },
}

/// Represents contents of the config.c/config.c.in file.
#[derive(Debug)]
#[allow(unused)]
pub struct ConfigC {
    pub init_funcs: Vec<String>,
    pub init_mods: BTreeMap<String, String>,
}

/// Multiple threads or processes could race to extract the archive.
/// So we use a lock file to ensure exclusive access.
/// TODO use more granular lock based on the output directory (possibly
/// by putting lock in output directory itself).
pub struct DistributionExtractLock {
    file: std::fs::File,
}

impl DistributionExtractLock {
    pub fn new(extract_dir: &Path) -> Result<Self> {
        let lock_path = extract_dir
            .parent()
            .unwrap()
            .join("distribution-extract-lock");

        let file = File::create(&lock_path)
            .context(format!("could not create {}", lock_path.display()))?;

        file.lock_exclusive()
            .context(format!("failed to obtain lock for {}", lock_path.display()))?;

        Ok(DistributionExtractLock { file })
    }
}

impl Drop for DistributionExtractLock {
    fn drop(&mut self) {
        self.file.unlock().unwrap();
    }
}

fn sha256_path(path: &PathBuf) -> Vec<u8> {
    let mut hasher = Sha256::new();
    let fh = File::open(&path).unwrap();
    let mut reader = std::io::BufReader::new(fh);

    let mut buffer = [0; 32768];

    loop {
        let count = reader.read(&mut buffer).expect("error reading");
        if count == 0 {
            break;
        }
        hasher.input(&buffer[..count]);
    }

    hasher.result().to_vec()
}

pub fn get_http_client() -> reqwest::Result<reqwest::Client> {
    let mut builder = reqwest::ClientBuilder::new();

    for (key, value) in std::env::vars() {
        let key = key.to_lowercase();
        if key.ends_with("_proxy") {
            let end = key.len() - "_proxy".len();
            let schema = &key[..end];

            if let Ok(url) = Url::parse(&value) {
                if let Some(proxy) = match schema {
                    "http" => Some(reqwest::Proxy::http(url.as_str())),
                    "https" => Some(reqwest::Proxy::https(url.as_str())),
                    _ => None,
                } {
                    if let Ok(proxy) = proxy {
                        builder = builder.proxy(proxy);
                    }
                }
            }
        }
    }

    builder.build()
}

/// Ensure a Python distribution at a URL is available in a local directory.
///
/// The path to the downloaded and validated file is returned.
pub fn download_distribution(url: &str, sha256: &str, cache_dir: &Path) -> PathBuf {
    let expected_hash = hex::decode(sha256).expect("could not parse SHA256 hash");
    let u = Url::parse(url).expect("failed to parse URL");

    let basename = u
        .path_segments()
        .expect("cannot be base path")
        .last()
        .expect("could not get final URL path element")
        .to_string();

    let cache_path = cache_dir.join(basename);

    if cache_path.exists() {
        let file_hash = sha256_path(&cache_path);

        // We don't care about timing side-channels from the string compare.
        if file_hash == expected_hash {
            return cache_path;
        }
    }

    let mut data: Vec<u8> = Vec::new();

    println!("downloading {}", u);
    let client = get_http_client().expect("unable to get HTTP client");
    let mut response = client
        .get(u.as_str())
        .send()
        .expect("unable to perform HTTP request");
    response
        .read_to_end(&mut data)
        .expect("unable to download URL");

    let mut hasher = Sha256::new();
    hasher.input(&data);

    let url_hash = hasher.result().to_vec();
    if url_hash != expected_hash {
        panic!("sha256 of Python distribution does not validate");
    }

    let mut temp_cache_path = cache_path.clone();
    temp_cache_path.set_file_name(format!("{}.tmp", Uuid::new_v4()));

    fs::write(&temp_cache_path, data).expect("unable to write file");

    fs::rename(&temp_cache_path, &cache_path)
        .or_else(|e| {
            fs::remove_file(&temp_cache_path).expect("unable to remove temp file");

            if cache_path.exists() {
                download_distribution(url, sha256, cache_dir);
                return Ok(());
            }

            Err(e)
        })
        .expect("unable to rename downloaded file");

    cache_path
}

pub fn copy_local_distribution(path: &PathBuf, sha256: &str, cache_dir: &Path) -> PathBuf {
    let expected_hash = hex::decode(sha256).expect("could not parse SHA256 hash");
    let basename = path.file_name().unwrap().to_str().unwrap().to_string();
    let cache_path = cache_dir.join(basename);

    if cache_path.exists() {
        let file_hash = sha256_path(&cache_path);

        if file_hash == expected_hash {
            println!(
                "existing {} passes SHA-256 integrity check",
                cache_path.display()
            );
            return cache_path;
        }
    }

    let source_hash = sha256_path(&path);

    if source_hash != expected_hash {
        panic!("sha256 of Python distribution does not validate");
    }

    println!("copying {}", path.display());
    std::fs::copy(path, &cache_path).unwrap();

    cache_path
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
pub fn resolve_python_distribution_archive(
    dist: &PythonDistributionLocation,
    cache_dir: &Path,
) -> PathBuf {
    if !cache_dir.exists() {
        create_dir_all(cache_dir).unwrap();
    }

    match dist {
        PythonDistributionLocation::Local { local_path, sha256 } => {
            let p = PathBuf::from(local_path);
            copy_local_distribution(&p, sha256, cache_dir)
        }
        PythonDistributionLocation::Url { url, sha256 } => {
            download_distribution(url, sha256, cache_dir)
        }
    }
}

/// Resolve a parsed distribution from a location and local filesystem path.
///
/// The distribution will be copied and extracted into the destination
/// directory. It will be parsed from the extracted location.
///
/// The created files outlive the returned object.
pub fn resolve_parsed_distribution(
    logger: &slog::Logger,
    location: &PythonDistributionLocation,
    dest_dir: &Path,
) -> Result<StandaloneDistribution> {
    warn!(logger, "resolving Python distribution {:?}", location);
    let path = resolve_python_distribution_archive(location, dest_dir);
    warn!(
        logger,
        "Python distribution available at {}",
        path.display()
    );

    let distribution_hash = match location {
        PythonDistributionLocation::Local { sha256, .. } => sha256,
        PythonDistributionLocation::Url { sha256, .. } => sha256,
    };

    let distribution_path = dest_dir.join(format!("python.{}", distribution_hash));

    StandaloneDistribution::from_path(logger, &path, &distribution_path)
}

/// Resolve the default Python distribution for a build target.
pub fn default_distribution(
    logger: &slog::Logger,
    target: &str,
    dest_dir: &Path,
) -> Result<StandaloneDistribution> {
    let dist = CPYTHON_STANDALONE_BY_TRIPLE
        .get(target)
        .ok_or_else(|| anyhow!("could not find default Python distribution for {}", target))?;

    let location = PythonDistributionLocation::Url {
        url: dist.url.clone(),
        sha256: dist.sha256.clone(),
    };

    resolve_parsed_distribution(logger, &location, dest_dir)
}

#[cfg(test)]
mod tests {
    use {super::*, crate::testutil::*};

    #[test]
    fn test_default_distribution() -> Result<()> {
        let logger = get_logger()?;
        let target = env!("HOST");

        let temp_dir = tempdir::TempDir::new("pyoxidizer-test")?;

        default_distribution(&logger, target, temp_dir.path())?;

        Ok(())
    }
}
