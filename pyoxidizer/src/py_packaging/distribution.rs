// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Defining and manipulating Python distributions.
*/

use {
    super::{
        binary::{LibpythonLinkMode, PythonBinaryBuilder},
        config::PyembedPythonInterpreterConfig,
        standalone_distribution::StandaloneDistribution,
    },
    crate::python_distributions::PYTHON_DISTRIBUTIONS,
    anyhow::{anyhow, Context, Result},
    fs2::FileExt,
    python_packaging::{
        bytecode::PythonBytecodeCompiler, module_util::PythonModuleSuffixes,
        policy::PythonPackagingPolicy, resource::PythonResource,
    },
    sha2::{Digest, Sha256},
    slog::warn,
    std::{
        collections::HashMap,
        convert::TryFrom,
        fs,
        fs::{create_dir_all, File},
        io::Read,
        ops::DerefMut,
        path::{Path, PathBuf},
        sync::{Arc, Mutex},
    },
    tugger_common::http::get_http_client,
    tugger_file_manifest::FileData,
    url::Url,
    uuid::Uuid,
};

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

/// Describes Apple SDK build/targeting.
#[derive(Clone, Debug, PartialEq)]
pub struct AppleSdkInfo {
    /// Canonical name of Apple SDK used.
    pub canonical_name: String,
    /// Name of SDK platform being targeted.
    pub platform: String,
    /// Version of Apple SDK used.
    pub version: String,
    /// Deployment target version used.
    pub deployment_target: String,
}

/// Describes a generic Python distribution.
pub trait PythonDistribution {
    /// Clone self into a Box'ed trait object.
    fn clone_trait(&self) -> Arc<dyn PythonDistribution>;

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

    /// Obtain Python packages in the standard library that provide tests.
    fn stdlib_test_packages(&self) -> Vec<String>;

    /// Obtain Apple SDK settings for this distribution.
    fn apple_sdk_info(&self) -> Option<&AppleSdkInfo>;

    /// Create a `PythonBytecodeCompiler` from this instance.
    fn create_bytecode_compiler(&self) -> Result<Box<dyn PythonBytecodeCompiler>>;

    /// Construct a `PythonPackagingPolicy` derived from this instance.
    fn create_packaging_policy(&self) -> Result<PythonPackagingPolicy>;

    /// Construct an `EmbeddedPythonConfig` derived from this instance.
    fn create_python_interpreter_config(&self) -> Result<PyembedPythonInterpreterConfig>;

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
        config: &PyembedPythonInterpreterConfig,
        host_distribution: Option<Arc<dyn PythonDistribution>>,
    ) -> Result<Box<dyn PythonBinaryBuilder>>;

    /// Obtain `PythonResource` instances for every resource in this distribution.
    fn python_resources<'a>(&self) -> Vec<PythonResource<'a>>;

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

    /// Whether this distribution supports loading shared libraries from memory.
    ///
    /// This effectively answers whether we can embed a shared library into an
    /// executable and load it without having to materialize it on a filesystem.
    fn supports_in_memory_shared_library_loading(&self) -> bool;

    /// Determine whether a named module is in a known standard library test package.
    fn is_stdlib_test_package(&self, name: &str) -> bool {
        for package in self.stdlib_test_packages() {
            let prefix = format!("{}.", package);

            if name == package || name.starts_with(&prefix) {
                return true;
            }
        }

        false
    }

    /// Obtain support files for tcl/tk.
    ///
    /// The returned list of files contains relative file names and the locations
    /// of file content. If the files are installed in a new directory, it should
    /// be possible to use that directory joined with `tcl_library_path_directory`
    /// as the value of `TCL_LIBRARY`.
    fn tcl_files(&self) -> Result<Vec<(PathBuf, FileData)>>;

    /// The name of the directory to use for `TCL_LIBRARY`
    fn tcl_library_path_directory(&self) -> Option<String>;
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

fn sha256_path(path: &Path) -> Vec<u8> {
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

pub fn copy_local_distribution(path: &Path, sha256: &str, cache_dir: &Path) -> Result<PathBuf> {
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

impl TryFrom<&str> for DistributionFlavor {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "standalone" => Ok(Self::Standalone),
            "standalone_static" | "standalone-static" => Ok(Self::StandaloneStatic),
            "standalone_dynamic" | "standalone-dynamic" => Ok(Self::StandaloneDynamic),
            _ => Err(format!("distribution flavor {} not recognized", value)),
        }
    }
}

type DistributionCacheKey = (PathBuf, PythonDistributionLocation);
type DistributionCacheValue = Arc<Mutex<Option<Arc<StandaloneDistribution>>>>;

/// Holds references to resolved PythonDistribution instances.
#[derive(Debug)]
pub struct DistributionCache {
    cache: Mutex<HashMap<DistributionCacheKey, DistributionCacheValue>>,
    default_dest_dir: Option<PathBuf>,
}

impl DistributionCache {
    pub fn new(default_dest_dir: Option<&Path>) -> Self {
        Self {
            cache: Mutex::new(HashMap::new()),
            default_dest_dir: default_dest_dir.clone().map(|x| x.to_path_buf()),
        }
    }

    /// Resolve a `PythonDistribution` given its source and storage locations.
    pub fn resolve_distribution(
        &self,
        logger: &slog::Logger,
        location: &PythonDistributionLocation,
        dest_dir: Option<&Path>,
    ) -> Result<Arc<StandaloneDistribution>> {
        let dest_dir = if let Some(p) = dest_dir {
            p
        } else if let Some(p) = &self.default_dest_dir {
            p
        } else {
            return Err(anyhow!("no destination directory available"));
        };

        let key = (dest_dir.to_path_buf(), location.clone());

        // This logic is whack. Surely there's a cleaner way to do this...
        //
        // The general problem is instances of this type are Send + Sync. And
        // we do rely on multiple threads simultaneously accessing it. This
        // occurs in tests for example, which use a global/static instance to
        // cache resolved distributions to drastically reduce CPU overhead.
        //
        // We need a Mutex of some kind around the HashMap to allow
        // multi-threaded access. But if that was the only Mutex that existed, we'd
        // need to hold the Mutex while any thread was resolving a distribution
        // and this would prevent multi-threaded distribution resolving.
        //
        // Or we could release that Mutex after a missing lookup and then each
        // thread would race to resolve the distribution and insert. That's fine,
        // but it results in redundancy and wasted CPU (several minutes worth for
        // debug builds in the test harness).
        //
        // What we do instead is have HashMap values be Arc<Mutex<Option<T>>>.
        // We then perform a 2 phase lookup.
        //
        // In the 1st lock, we lock the entire HashMap and do the key lookup.
        // If it exists, we clone the Arc<T>. Else if it is missing, we insert
        // a new key with `None` and return a clone of its Arc<T>. Either way,
        // we have a handle on the Arc<Mutex<Option<T>>> in a populated. We then
        // release the outer HashMap lock.
        //
        // We then lock the inner entry. With that lock hold, we return a clone
        // of its `Some(T)` entry immediately or proceed to populate it. Only 1
        // thread can hold this lock, ensuring only 1 thread performs the
        // value resolution. Multiple threads can resolve different keys in
        // parallel. By other threads will be blocked resolving a single key.

        let entry = {
            let mut lock = self
                .cache
                .lock()
                .map_err(|e| anyhow!("cannot obtain distribution cache lock: {}", e))?;

            if let Some(value) = lock.get(&key) {
                value.clone()
            } else {
                let value = Arc::new(Mutex::new(None));
                lock.insert(key.clone(), value.clone());

                value
            }
        };

        let mut lock = entry
            .lock()
            .map_err(|e| anyhow!("cannot obtain distribution lock: {}", e))?;

        let value = lock.deref_mut();

        if let Some(dist) = value {
            Ok(dist.clone())
        } else {
            let dist = Arc::new(StandaloneDistribution::from_location(
                logger, location, &dest_dir,
            )?);

            lock.replace(dist.clone());

            Ok(dist)
        }
    }
}

/// Obtain a `PythonDistribution` implementation of a flavor and from a location.
///
/// The distribution will be written to `dest_dir`.
#[allow(unused)]
pub fn resolve_distribution(
    logger: &slog::Logger,
    location: &PythonDistributionLocation,
    dest_dir: &Path,
) -> Result<Box<dyn PythonDistribution>> {
    Ok(Box::new(StandaloneDistribution::from_location(
        logger, &location, dest_dir,
    )?) as Box<dyn PythonDistribution>)
}

/// Resolve the location of the default Python distribution of a given flavor and build target.
pub fn default_distribution_location(
    flavor: &DistributionFlavor,
    target: &str,
    python_major_minor_version: Option<&str>,
) -> Result<PythonDistributionLocation> {
    let dist = PYTHON_DISTRIBUTIONS
        .find_distribution(target, flavor, python_major_minor_version)
        .ok_or_else(|| anyhow!("could not find default Python distribution for {}", target))?;

    Ok(dist.location)
}

#[cfg(test)]
mod tests {
    use {super::*, crate::testutil::*};

    #[test]
    fn test_all_standalone_distributions() -> Result<()> {
        assert!(!get_all_standalone_distributions()?.is_empty());

        Ok(())
    }
}
