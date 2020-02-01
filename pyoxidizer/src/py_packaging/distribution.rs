// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Defining and manipulating Python distributions.
*/

use {
    super::binary::PythonBinaryBuilder,
    super::bytecode::BytecodeCompiler,
    super::config::EmbeddedPythonConfig,
    super::embedded_resource::EmbeddedPythonResourcesPrePackaged,
    super::libpython::ImportlibBytecode,
    super::resource::{ResourceData, SourceModule},
    super::standalone_distribution::{ExtensionModule, StandaloneDistribution},
    crate::python_distributions::CPYTHON_STANDALONE_BY_TRIPLE,
    anyhow::{anyhow, Context, Result},
    fs2::FileExt,
    sha2::{Digest, Sha256},
    slog::warn,
    std::collections::HashMap,
    std::convert::TryFrom,
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

/// Denotes methods to filter extension modules.
#[derive(Clone, Debug, PartialEq)]
pub enum ExtensionModuleFilter {
    Minimal,
    All,
    NoLibraries,
    NoGPL,
}

impl TryFrom<&str> for ExtensionModuleFilter {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "minimal" => Ok(ExtensionModuleFilter::Minimal),
            "all" => Ok(ExtensionModuleFilter::All),
            "no-libraries" => Ok(ExtensionModuleFilter::NoLibraries),
            "no-gpl" => Ok(ExtensionModuleFilter::NoGPL),
            t => Err(format!("{} is not a valid extension module filter", t)),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PythonDistributionLocation {
    Local { local_path: String, sha256: String },
    Url { url: String, sha256: String },
}

/// Describes a generic Python distribution.
pub trait PythonDistribution {
    /// Obtain an instance from a source location and destination directory tree.
    ///
    /// The distribution will be obtained and extracted into a directory under
    /// ``distributions_dir``. Those files will outlive the returned instance.
    fn from_location(
        logger: &slog::Logger,
        location: &PythonDistributionLocation,
        distributions_dir: &Path,
    ) -> Result<Box<Self>>
    where
        Self: Sized;

    /// Obtain the filesystem path to a `python` executable for this distribution.
    fn python_exe_path(&self) -> &Path;

    /// Obtain the X.Y Python version component. e.g. `3.7`.
    fn python_major_minor_version(&self) -> String;

    /// Create a `BytecodeCompiler` from this instance.
    fn create_bytecode_compiler(&self) -> Result<BytecodeCompiler>;

    /// Resolve `importlib` bytecode.
    ///
    /// The returned struct holds Python bytecode which will be turned into
    /// a frozen module. This bytecode registers the PyOxidizer memory importer.
    ///
    /// The bytecode should be compiled from modified sources of the
    /// corresponding Python modules.
    fn resolve_importlib_bytecode(&self) -> Result<ImportlibBytecode>;

    /// Obtain a `PythonBinaryBuilder` for constructing an executable.
    #[allow(clippy::too_many_arguments)]
    fn as_python_executable_builder(
        &self,
        logger: &slog::Logger,
        name: &str,
        config: &EmbeddedPythonConfig,
        extension_module_filter: &ExtensionModuleFilter,
        preferred_extension_module_variants: Option<HashMap<String, String>>,
        include_sources: bool,
        include_resources: bool,
        include_test: bool,
    ) -> Result<Box<dyn PythonBinaryBuilder>>;

    /// Obtain extension modules matching a specified filter and variant selection preferences.
    fn filter_extension_modules(
        &self,
        logger: &slog::Logger,
        filter: &ExtensionModuleFilter,
        preferred_variants: Option<HashMap<String, String>>,
    ) -> Result<Vec<ExtensionModule>>;

    /// Obtain `SourceModule` instances present in this distribution.
    fn source_modules(&self) -> Result<Vec<SourceModule>>;

    /// Obtain `ResourceData` instances present in this distribution.
    fn resource_datas(&self) -> Result<Vec<ResourceData>>;

    /// Obtain an `EmbeddedPythonResourcesPrePackaged` from this instance.
    fn as_embedded_python_resources_pre_packaged(
        &self,
        logger: &slog::Logger,
        extension_module_filter: &ExtensionModuleFilter,
        preferred_extension_module_variants: Option<HashMap<String, String>>,
        include_sources: bool,
        include_resources: bool,
        include_test: bool,
    ) -> Result<EmbeddedPythonResourcesPrePackaged>;

    /// Ensure pip is available to run in the distribution.
    ///
    /// Returns the path to a `pip` executable.
    fn ensure_pip(&self, logger: &slog::Logger) -> Result<PathBuf>;

    /// Resolve a `distutils` installation used for building Python packages.
    ///
    /// Some distributions may need to use a modified `distutils` to coerce builds to work
    /// as PyOxidizer desires. This method is used to realize such a `distutils` installation.
    ///
    /// The return is a map of environment variables to set in the build environment.
    fn resolve_distutils(
        &self,
        logger: &slog::Logger,
        dest_dir: &Path,
        extra_python_paths: &[&Path],
    ) -> Result<HashMap<String, String>>;
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
pub fn download_distribution(url: &str, sha256: &str, cache_dir: &Path) -> Result<PathBuf> {
    let expected_hash = hex::decode(sha256)?;
    let u = Url::parse(url)?;

    let basename = u
        .path_segments()
        .expect("cannot be base path")
        .last()
        .unwrap()
        .to_string();

    let cache_path = cache_dir.join(basename);

    if cache_path.exists() {
        let file_hash = sha256_path(&cache_path);

        // We don't care about timing side-channels from the string compare.
        if file_hash == expected_hash {
            return Ok(cache_path);
        }
    }

    let mut data: Vec<u8> = Vec::new();

    println!("downloading {}", u);
    let client = get_http_client()?;
    let mut response = client.get(u.as_str()).send()?;
    response.read_to_end(&mut data)?;

    let mut hasher = Sha256::new();
    hasher.input(&data);

    let url_hash = hasher.result().to_vec();
    if url_hash != expected_hash {
        return Err(anyhow!("sha256 of Python distribution does not validate"));
    }

    let mut temp_cache_path = cache_path.clone();
    temp_cache_path.set_file_name(format!("{}.tmp", Uuid::new_v4()));

    fs::write(&temp_cache_path, data).context("unable to write distribution file")?;

    fs::rename(&temp_cache_path, &cache_path)
        .or_else(|e| -> Result<()> {
            fs::remove_file(&temp_cache_path)
                .context("unable to remove temporary distribution file")?;

            if cache_path.exists() {
                download_distribution(url, sha256, cache_dir)?;
                return Ok(());
            }

            Err(e.into())
        })
        .context("unable to rename downloaded distribution file")?;

    Ok(cache_path)
}

pub fn copy_local_distribution(path: &PathBuf, sha256: &str, cache_dir: &Path) -> Result<PathBuf> {
    let expected_hash = hex::decode(sha256)?;
    let basename = path.file_name().unwrap().to_str().unwrap().to_string();
    let cache_path = cache_dir.join(basename);

    if cache_path.exists() {
        let file_hash = sha256_path(&cache_path);

        if file_hash == expected_hash {
            println!(
                "existing {} passes SHA-256 integrity check",
                cache_path.display()
            );
            return Ok(cache_path);
        }
    }

    let source_hash = sha256_path(&path);

    if source_hash != expected_hash {
        return Err(anyhow!("sha256 of Python distribution does not validate"));
    }

    println!("copying {}", path.display());
    std::fs::copy(path, &cache_path)?;

    Ok(cache_path)
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
) -> Result<PathBuf> {
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

/// Resolve a Python distribution archive.
///
/// Returns a tuple of (archive path, extract directory).
pub fn resolve_python_distribution_from_location(
    logger: &slog::Logger,
    location: &PythonDistributionLocation,
    distributions_dir: &Path,
) -> Result<(PathBuf, PathBuf)> {
    warn!(logger, "resolving Python distribution {:?}", location);
    let path = resolve_python_distribution_archive(location, distributions_dir)?;
    warn!(
        logger,
        "Python distribution available at {}",
        path.display()
    );

    let distribution_hash = match location {
        PythonDistributionLocation::Local { sha256, .. } => sha256,
        PythonDistributionLocation::Url { sha256, .. } => sha256,
    };

    let distribution_path = distributions_dir.join(format!("python.{}", distribution_hash));

    Ok((path, distribution_path))
}

/// Resolve the default Python distribution for a build target.
pub fn default_distribution(
    logger: &slog::Logger,
    target: &str,
    dest_dir: &Path,
) -> Result<Box<StandaloneDistribution>> {
    let dist = CPYTHON_STANDALONE_BY_TRIPLE
        .get(target)
        .ok_or_else(|| anyhow!("could not find default Python distribution for {}", target))?;

    let location = PythonDistributionLocation::Url {
        url: dist.url.clone(),
        sha256: dist.sha256.clone(),
    };

    StandaloneDistribution::from_location(logger, &location, dest_dir)
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
