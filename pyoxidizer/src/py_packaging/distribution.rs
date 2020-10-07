// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Defining and manipulating Python distributions.
*/

use {
    super::{
        binary::{LibpythonLinkMode, PythonBinaryBuilder},
        config::EmbeddedPythonConfig,
        standalone_distribution::StandaloneDistribution,
    },
    crate::python_distributions::PYTHON_DISTRIBUTIONS,
    anyhow::{anyhow, Context, Result},
    fs2::FileExt,
    python_packaging::{
        bytecode::PythonBytecodeCompiler,
        module_util::PythonModuleSuffixes,
        policy::PythonPackagingPolicy,
        resource::{PythonExtensionModule, PythonModuleSource, PythonPackageResource},
    },
    sha2::{Digest, Sha256},
    slog::warn,
    std::{
        collections::HashMap,
        fs,
        fs::{create_dir_all, File},
        io::Read,
        path::{Path, PathBuf},
        sync::Arc,
    },
    url::Url,
    uuid::Uuid,
};

// TODO denote test packages in Python distribution.
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

/// Denotes how a binary should link libpython.
#[derive(Clone, Debug, PartialEq)]
pub enum BinaryLibpythonLinkMode {
    /// Use default link mode semantics.
    Default,
    /// Statically link libpython into the binary.
    Static,
    /// Binary should dynamically link libpython.
    Dynamic,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum PythonDistributionLocation {
    Local { local_path: String, sha256: String },
    Url { url: String, sha256: String },
}

/// Describes an obtainable Python distribution.
#[derive(Clone, Debug, PartialEq)]
pub struct PythonDistributionRecord {
    /// X.Y major.minor version of Python.
    pub python_major_minor_version: String,

    /// Where the distribution can be obtained from.
    pub location: PythonDistributionLocation,

    /// Rust target triple this distribution runs on.
    pub target_triple: String,

    /// Whether the distribution can load prebuilt extension modules.
    pub supports_prebuilt_extension_modules: bool,
}

/// Describes a generic Python distribution.
pub trait PythonDistribution {
    /// Clone self into a Box'ed trait object.
    fn clone_box(&self) -> Box<dyn PythonDistribution>;

    /// The Rust machine triple this distribution runs on.
    fn target_triple(&self) -> &str;

    /// Rust target triples on which this distribution's binaries can run.
    ///
    /// For example, an x86 distribution might advertise that it can run on
    /// 64-bit host triples.
    ///
    /// `target_triple()` is always in the result.
    fn compatible_host_triples(&self) -> Vec<String>;

    /// Obtain the filesystem path to a `python` executable for this distribution.
    fn python_exe_path(&self) -> &Path;

    /// Obtain the full Python version string.
    fn python_version(&self) -> &str;

    /// Obtain the X.Y Python version component. e.g. `3.7`.
    fn python_major_minor_version(&self) -> String;

    /// Obtain the full Python implementation name. e.g. `cpython`.
    fn python_implementation(&self) -> &str;

    /// Obtain the short Python implementation name. e.g. `cp`
    fn python_implementation_short(&self) -> &str;

    /// Obtain the PEP 425 Python tag. e.g. `cp38`.
    fn python_tag(&self) -> &str;

    /// Obtain the PEP 425 Python ABI tag. e.g. `cp38d`.
    fn python_abi_tag(&self) -> Option<&str>;

    /// Obtain the Python platform tag.
    fn python_platform_tag(&self) -> &str;

    /// Obtain the Python platform tag used to indicate compatibility.
    ///
    /// This is similar to the platform tag. But where `python_platform_tag()`
    /// exposes the raw value like `linux-x86_64`, this is the normalized
    /// value that can be used by tools like `pip`. e.g. `manylinux2014_x86_64`.
    fn python_platform_compatibility_tag(&self) -> &str;

    /// Obtain the cache tag to apply to Python bytecode modules.
    fn cache_tag(&self) -> &str;

    /// Obtain file suffixes for various Python module flavors.
    fn python_module_suffixes(&self) -> Result<PythonModuleSuffixes>;

    /// Create a `PythonBytecodeCompiler` from this instance.
    fn create_bytecode_compiler(&self) -> Result<Box<dyn PythonBytecodeCompiler>>;

    /// Construct a `PythonPackagingPolicy` derived from this instance.
    fn create_packaging_policy(&self) -> Result<PythonPackagingPolicy>;

    /// Obtain a `PythonBinaryBuilder` for constructing an executable embedding Python.
    ///
    /// This method is how you start the process of creating a new executable file
    /// from a Python distribution. Using the returned `PythonBinaryBuilder` instance,
    /// you can manipulate resources, etc and then eventually build a new executable
    /// with it.
    #[allow(clippy::too_many_arguments)]
    fn as_python_executable_builder(
        &self,
        logger: &slog::Logger,
        host_triple: &str,
        target_triple: &str,
        name: &str,
        libpython_link_mode: BinaryLibpythonLinkMode,
        policy: &PythonPackagingPolicy,
        config: &EmbeddedPythonConfig,
        host_distribution: Option<Arc<Box<dyn PythonDistribution>>>,
    ) -> Result<Box<dyn PythonBinaryBuilder>>;

    /// Obtain `PythonExtensionModule` instances present in this distribution.
    ///
    /// Multiple variants of the same extension module may be returned.
    fn iter_extension_modules<'a>(
        &'a self,
    ) -> Box<dyn Iterator<Item = &'a PythonExtensionModule> + 'a>;

    /// Obtain `SourceModule` instances present in this distribution.
    fn source_modules(&self) -> Result<Vec<PythonModuleSource>>;

    /// Obtain `ResourceData` instances present in this distribution.
    fn resource_datas(&self) -> Result<Vec<PythonPackageResource>>;

    /// Ensure pip is available to run in the distribution.
    ///
    /// Returns the path to a `pip` executable.
    fn ensure_pip(&self, logger: &slog::Logger) -> Result<PathBuf>;

    /// Resolve a `distutils` installation used for building Python packages.
    ///
    /// Some distributions may need to use a modified `distutils` to coerce builds to work
    /// as PyOxidizer desires. This method is used to realize such a `distutils` installation.
    ///
    /// Note that we pass in an explicit libpython link mode because the link mode
    /// we care about may differ from the link mode of the distribution itself (as some
    /// distributions support multiple link modes).
    ///
    /// The return is a map of environment variables to set in the build environment.
    fn resolve_distutils(
        &self,
        logger: &slog::Logger,
        libpython_link_mode: LibpythonLinkMode,
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
        hasher.update(&buffer[..count]);
    }

    hasher.finalize().to_vec()
}

pub fn get_http_client() -> reqwest::Result<reqwest::blocking::Client> {
    let mut builder = reqwest::blocking::ClientBuilder::new();

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
    hasher.update(&data);

    let url_hash = hasher.finalize().to_vec();
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

    let distribution_path = distributions_dir.join(format!("python.{}", &distribution_hash[0..12]));

    Ok((path, distribution_path))
}

/// Describes the flavor of a distribution.
#[derive(Debug, PartialEq)]
pub enum DistributionFlavor {
    /// Distributions coming from the `python-build-standalone` project.
    Standalone,

    /// Statically linked distributions coming from the `python-build-standalone` project.
    StandaloneStatic,

    /// Dynamically linked distributions coming from the `python-build-standalone` project.
    StandaloneDynamic,
}

impl Default for DistributionFlavor {
    fn default() -> Self {
        DistributionFlavor::Standalone
    }
}

/// Obtain a `PythonDistribution` implementation of a flavor and from a location.
///
/// The distribution will be written to `dest_dir`.
pub fn resolve_distribution(
    logger: &slog::Logger,
    flavor: &DistributionFlavor,
    location: &PythonDistributionLocation,
    dest_dir: &Path,
) -> Result<Box<dyn PythonDistribution>> {
    // TODO is there a way we can define PythonDistribution::from_location()
    Ok(match flavor {
        DistributionFlavor::Standalone => Box::new(StandaloneDistribution::from_location(
            logger, &location, dest_dir,
        )?) as Box<dyn PythonDistribution>,

        DistributionFlavor::StandaloneStatic => Box::new(StandaloneDistribution::from_location(
            logger, &location, dest_dir,
        )?) as Box<dyn PythonDistribution>,

        DistributionFlavor::StandaloneDynamic => Box::new(StandaloneDistribution::from_location(
            logger, &location, dest_dir,
        )?) as Box<dyn PythonDistribution>,
    })
}

/// Resolve the location of the default Python distribution of a given flavor and build target.
pub fn default_distribution_location(
    flavor: &DistributionFlavor,
    target: &str,
) -> Result<PythonDistributionLocation> {
    let dist = PYTHON_DISTRIBUTIONS
        .find_distribution(target, flavor, None)
        .ok_or_else(|| anyhow!("could not find default Python distribution for {}", target))?;

    Ok(dist.location)
}

/// Resolve the default Python distribution for a build target.
///
/// `flavor` is the high-level type of distribution.
/// `target` is a Rust target triple the distribution should target.
/// `dest_dir` is a directory to extract the distribution to. The distribution will
/// be extracted to a child directory of this path.
#[allow(unused)]
pub fn default_distribution(
    logger: &slog::Logger,
    flavor: &DistributionFlavor,
    target: &str,
    dest_dir: &Path,
) -> Result<Box<dyn PythonDistribution>> {
    let location = default_distribution_location(flavor, target)?;

    resolve_distribution(logger, flavor, &location, dest_dir)
}

#[cfg(test)]
mod tests {
    use {super::*, crate::testutil::*};

    #[test]
    fn test_default_distribution() -> Result<()> {
        let logger = get_logger()?;
        let target = env!("HOST");

        let temp_dir = tempdir::TempDir::new("pyoxidizer-test")?;

        default_distribution(
            &logger,
            &DistributionFlavor::Standalone,
            target,
            temp_dir.path(),
        )?;

        Ok(())
    }

    #[test]
    fn test_all_standalone_distributions() -> Result<()> {
        assert!(!get_all_standalone_distributions()?.is_empty());

        Ok(())
    }
}
