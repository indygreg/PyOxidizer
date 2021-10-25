// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Resolve details about the PyOxidizer execution environment.

use {
    crate::{project_layout::PyembedLocation, py_packaging::distribution::AppleSdkInfo},
    anyhow::{anyhow, Context, Result},
    once_cell::sync::Lazy,
    semver::Version,
    slog::{info, warn},
    std::{
        env,
        ops::Deref,
        path::{Path, PathBuf},
        sync::{Arc, RwLock},
    },
    tugger_apple::{find_command_line_tools_sdks, find_default_developer_sdks, AppleSdk},
    tugger_rust_toolchain::install_rust_toolchain,
};

/// Version string of PyOxidizer's crate from its Cargo.toml.
const PYOXIDIZER_CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Version string of pyembed crate from its Cargo.toml.
const PYEMBED_CRATE_VERSION: &str = "0.18.0";

/// URL of Git repository we were built from.
const GIT_REPO_URL: &str = env!("GIT_REPO_URL");

/// Version string of PyOxidizer.
pub const PYOXIDIZER_VERSION: &str = env!("PYOXIDIZER_VERSION");

/// Filesystem path to Git repository we were built from.
///
/// Will be None if a path is defined in the environment but not present.
pub static BUILD_GIT_REPO_PATH: Lazy<Option<PathBuf>> = Lazy::new(|| {
    match env!("GIT_REPO_PATH") {
        "" => None,
        value => {
            let path = PathBuf::from(value);

            // There is a potential for false positives here. e.g. shared checkout
            // directories. But hopefully that should be rare.
            if path.exists() {
                Some(path)
            } else {
                None
            }
        }
    }
});

/// Git commit this build of PyOxidizer was produced with.
pub static BUILD_GIT_COMMIT: Lazy<Option<String>> = Lazy::new(|| {
    match env!("GIT_COMMIT") {
        // Can happen when not run from a Git checkout (such as installing
        // from a crate).
        "" => None,
        value => Some(value.to_string()),
    }
});

/// The Git tag we are built against.
pub static BUILD_GIT_TAG: Lazy<Option<String>> = Lazy::new(|| {
    let tag = env!("GIT_TAG");
    if tag.is_empty() {
        None
    } else {
        Some(tag.to_string())
    }
});

/// Defines the source of this install from Git data embedded in the binary.
pub static GIT_SOURCE: Lazy<PyOxidizerSource> = Lazy::new(|| {
    let commit = BUILD_GIT_COMMIT.clone();

    // Commit and tag should be mutually exclusive.
    let tag = if commit.is_some() || BUILD_GIT_TAG.is_none() {
        None
    } else {
        BUILD_GIT_TAG.clone()
    };

    PyOxidizerSource::GitUrl {
        url: GIT_REPO_URL.to_owned(),
        commit,
        tag,
    }
});

/// Minimum version of Rust required to build PyOxidizer applications.
///
// Remember to update the CI configuration in .github/workflows/
// and the `Installing Rust` documentation when this changes.
pub static MINIMUM_RUST_VERSION: Lazy<semver::Version> =
    Lazy::new(|| semver::Version::new(1, 56, 0));

/// Version of Rust toolchain to use for our managed Rust install.
pub const RUST_TOOLCHAIN_VERSION: &str = "1.56.0";

/// Target triples for Linux.
pub static LINUX_TARGET_TRIPLES: Lazy<Vec<&'static str>> =
    Lazy::new(|| vec!["x86_64-unknown-linux-gnu", "x86_64-unknown-linux-musl"]);

/// Target triples for macOS.
pub static MACOS_TARGET_TRIPLES: Lazy<Vec<&'static str>> =
    Lazy::new(|| vec!["aarch64-apple-darwin", "x86_64-apple-darwin"]);

/// Target triples for Windows.
pub static WINDOWS_TARGET_TRIPLES: Lazy<Vec<&'static str>> = Lazy::new(|| {
    vec![
        "i686-pc-windows-gnu",
        "i686-pc-windows-msvc",
        "x86_64-pc-windows-gnu",
        "x86_64-pc-windows-msvc",
    ]
});

pub fn canonicalize_path(path: &Path) -> Result<PathBuf, std::io::Error> {
    let mut p = path.canonicalize()?;

    // Strip \\?\ prefix on Windows and replace \ with /, which is valid.
    if cfg!(windows) {
        let mut s = p.display().to_string().replace("\\", "/");
        if s.starts_with("//?/") {
            s = s[4..].to_string();
        }

        p = PathBuf::from(s);
    }

    Ok(p)
}

/// The default target triple to build for.
///
/// This typically matches the triple of the current binary. But in some
/// cases we remap to a more generic target.
pub fn default_target_triple() -> &'static str {
    match env!("TARGET") {
        // Release binaries are typically musl. But Linux GNU is a more
        // user friendly target to build for. So we perform this mapping.
        "x86_64-unknown-linux-musl" => "x86_64-unknown-linux-gnu",
        v => v,
    }
}

/// Describes the location of the PyOxidizer source files.
#[derive(Clone, Debug)]
pub enum PyOxidizerSource {
    /// A local filesystem path.
    LocalPath { path: PathBuf },

    /// A Git repository somewhere. Defined by a Git remote URL and a commit string.
    GitUrl {
        url: String,
        commit: Option<String>,
        tag: Option<String>,
    },
}

impl Default for PyOxidizerSource {
    fn default() -> Self {
        if let Some(path) = BUILD_GIT_REPO_PATH.as_ref() {
            Self::LocalPath { path: path.clone() }
        } else {
            GIT_SOURCE.clone()
        }
    }
}

impl PyOxidizerSource {
    /// Determine the location of the pyembed crate given a run-time environment.
    ///
    /// If not a pre-release version, we always reference the plain version,
    /// presumably available in a registry.
    ///
    /// If it is a pre-release version, we reference a local filesystem path, if
    /// available, or a specific commit in this project's Git repository.
    pub fn as_pyembed_location(&self) -> PyembedLocation {
        // Pre-release version only available via path or Git references.
        if PYEMBED_CRATE_VERSION.ends_with("-pre") {
            match self {
                PyOxidizerSource::LocalPath { path } => {
                    PyembedLocation::Path(canonicalize_path(&path.join("pyembed")).unwrap())
                }
                PyOxidizerSource::GitUrl { url, commit, tag } => {
                    if let Some(tag) = tag {
                        PyembedLocation::Git(url.clone(), tag.clone())
                    } else if let Some(commit) = commit {
                        PyembedLocation::Git(url.clone(), commit.clone())
                    } else {
                        // We shouldn't get here. But who knows what's possible.
                        PyembedLocation::Git(url.clone(), "main".to_string())
                    }
                }
            }
        } else {
            // Published version is always a plain version reference to the registry.
            PyembedLocation::Version(PYEMBED_CRATE_VERSION.to_string())
        }
    }

    /// Obtain a string to be used as the long form version info for the executable.
    pub fn version_long(&self) -> String {
        format!(
            "{}\ncommit: {}\nsource: {}\npyembed crate location: {}",
            PYOXIDIZER_CRATE_VERSION,
            if let Some(commit) = BUILD_GIT_COMMIT.as_ref() {
                commit.as_str()
            } else {
                "unknown"
            },
            match self {
                PyOxidizerSource::LocalPath { path } => {
                    format!("{}", path.display())
                }
                PyOxidizerSource::GitUrl { url, .. } => {
                    url.clone()
                }
            },
            self.as_pyembed_location().cargo_manifest_fields(),
        )
    }
}

/// Describes the PyOxidizer run-time environment.
#[derive(Clone, Debug)]
pub struct Environment {
    /// Where a copy of PyOxidizer can be obtained from.
    pub pyoxidizer_source: PyOxidizerSource,

    /// Directory to use for caching things.
    cache_dir: PathBuf,

    /// Whether we should use a Rust installation we manage ourselves.
    managed_rust: bool,

    /// Rust environment to use.
    ///
    /// Cached because lookups may be expensive.
    rust_environment: Arc<RwLock<Option<RustEnvironment>>>,
}

impl Environment {
    /// Obtain a new instance.
    pub fn new() -> Result<Self> {
        let pyoxidizer_source = PyOxidizerSource::default();

        let cache_dir = if let Ok(p) = std::env::var("PYOXIDIZER_CACHE_DIR") {
            PathBuf::from(p)
        } else if let Some(cache_dir) = dirs::cache_dir() {
            cache_dir.join("pyoxidizer")
        } else {
            dirs::home_dir().ok_or_else(|| anyhow!("could not resolve home dir as part of resolving PyOxidizer cache directory"))?.join(".pyoxidizer").join("cache")
        };

        let managed_rust = std::env::var("PYOXIDIZER_SYSTEM_RUST").is_err();

        Ok(Self {
            pyoxidizer_source,
            cache_dir,
            managed_rust,
            rust_environment: Arc::new(RwLock::new(None)),
        })
    }

    /// Cache directory for PyOxidizer to use.
    ///
    /// The cache is per-user but multi-process.
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// Directory to use for storing Python distributions.
    pub fn python_distributions_dir(&self) -> PathBuf {
        self.cache_dir.join("python_distributions")
    }

    /// Directory to hold Rust toolchains.
    pub fn rust_dir(&self) -> PathBuf {
        self.cache_dir.join("rust")
    }

    /// Do not use a managed Rust.
    ///
    /// When called, [self.ensure_rust_toolchain()] will attempt to locate a
    /// Rust install on the system rather than manage it itself.
    pub fn unmanage_rust(&mut self) -> Result<()> {
        self.managed_rust = false;
        self.rust_environment
            .write()
            .map_err(|e| anyhow!("unable to lock cached rust environment for writing: {}", e))?
            .take();

        Ok(())
    }

    /// Find an executable of the given name.
    ///
    /// Resolves to `Some(T)` if an executable was found or `None` if not.
    ///
    /// Errors if there were problems searching for executables.
    pub fn find_executable(&self, name: &str) -> which::Result<Option<PathBuf>> {
        match which::which(name) {
            Ok(p) => Ok(Some(p)),
            Err(which::Error::CannotFindBinaryPath) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Ensure a Rust toolchain suitable for building is available.
    pub fn ensure_rust_toolchain(
        &self,
        logger: &slog::Logger,
        target_triple: Option<&str>,
    ) -> Result<RustEnvironment> {
        let mut cached = self
            .rust_environment
            .write()
            .map_err(|e| anyhow!("failed to acquire rust environment lock: {}", e))?;

        if cached.is_none() {
            warn!(
                logger,
                "ensuring Rust toolchain {} is available", RUST_TOOLCHAIN_VERSION,
            );

            let rust_env = if self.managed_rust {
                let target_triple = target_triple.unwrap_or_else(|| default_target_triple());

                let toolchain = install_rust_toolchain(
                    logger,
                    RUST_TOOLCHAIN_VERSION,
                    default_target_triple(),
                    &[target_triple],
                    &self.rust_dir(),
                    Some(&self.rust_dir()),
                )?;

                RustEnvironment {
                    cargo_exe: toolchain.cargo_path,
                    rustc_exe: toolchain.rustc_path.clone(),
                    rust_version: rustc_version::VersionMeta::for_command(
                        std::process::Command::new(toolchain.rustc_path),
                    )?,
                }
            } else {
                self.system_rust_environment()?
            };

            cached.replace(rust_env);
        }

        Ok(cached
            .deref()
            .as_ref()
            .expect("should have been populated above")
            .clone())
    }

    /// Obtain the path to a `rustc` executable.
    ///
    /// This respects the `RUSTC` environment variable.
    ///
    /// Not exposed as public because we want all consumers of rustc to go
    /// through validation logic in [self.rust_environment()].
    fn rustc_exe(&self) -> which::Result<Option<PathBuf>> {
        if let Some(v) = std::env::var_os("RUSTC") {
            let p = PathBuf::from(v);

            if p.exists() {
                Ok(Some(p))
            } else {
                Err(which::Error::BadAbsolutePath)
            }
        } else {
            self.find_executable("rustc")
        }
    }

    /// Obtain the path to a `cargo` executable.
    ///
    /// Not exposed as public because we want all consumers of cargo to
    /// go through validation logic in [self.rust_environment()].
    fn cargo_exe(&self) -> which::Result<Option<PathBuf>> {
        self.find_executable("cargo")
    }

    /// Return information about the system's Rust toolchain.
    ///
    /// This attempts to locate a Rust toolchain suitable for use with
    /// PyOxidizer. If a toolchain could not be found or doesn't meet the
    /// requirements, an error occurs.
    fn system_rust_environment(&self) -> Result<RustEnvironment> {
        let cargo_exe = self
            .cargo_exe()
            .context("finding cargo executable")?
            .ok_or_else(|| anyhow!("cargo executable not found; is Rust installed and in PATH?"))?;

        let rustc_exe = self
            .rustc_exe()
            .context("finding rustc executable")?
            .ok_or_else(|| anyhow!("rustc executable not found; is Rust installed and in PATH?"))?;

        let rust_version =
            rustc_version::VersionMeta::for_command(std::process::Command::new(&rustc_exe))
                .context("resolving rustc version")?;

        if rust_version.semver.lt(&MINIMUM_RUST_VERSION) {
            return Err(anyhow!(
                "PyOxidizer requires Rust {}; {} is version {}",
                *MINIMUM_RUST_VERSION,
                rustc_exe.display(),
                rust_version.semver
            ));
        }

        Ok(RustEnvironment {
            cargo_exe,
            rustc_exe,
            rust_version,
        })
    }

    /// Attempt to resolve an appropriate Apple SDK to use given settings.
    pub fn resolve_apple_sdk(
        &self,
        logger: &slog::Logger,
        sdk_info: &AppleSdkInfo,
    ) -> Result<AppleSdk> {
        let platform = &sdk_info.platform;
        let minimum_version = &sdk_info.version;
        let deployment_target = &sdk_info.deployment_target;

        let sdk = if let Ok(sdk_root) = std::env::var("SDKROOT") {
            warn!(logger, "SDKROOT defined; using Apple SDK at {}", &sdk_root);

            let sdk = AppleSdk::from_directory(&PathBuf::from(&sdk_root)).with_context(|| {
                format!("resolving Apple SDK at {} as defined by SDKROOT", sdk_root)
            })?;

            // The environment variable may force the use of an SDK that otherwise
            // wouldn't be allowed by our platform, minimum version, or deployment
            // target criteria.
            //
            // This is a hard failure when the platform or deployment target isn't
            // compatible because this would just lead to compile errors.
            //
            // Failure to meet minimum version requirements is a soft warning because
            // sometimes things will work. e.g. you may get lucky being able to compile
            // with a 11.2 SDK even if an 11.3 SDK is wanted.

            if let Some(support_targets) = sdk.supported_targets.get(platform) {
                if !support_targets
                    .valid_deployment_targets
                    .contains(deployment_target)
                {
                    return Err(anyhow!(
                        "SDKROOT defined SDK does not support deployment target {} (supported: {}); refusing to proceed. (Try setting SDKROOT to an older or newer SDK depending on the failure or unset to use automatic SDK discovery.)",
                        deployment_target,
                        support_targets.valid_deployment_targets.join(", ")
                    ));
                }
            } else {
                return Err(anyhow!(
                    "SDKROOT defined SDK does not support target platform {}; refusing to proceed. (Set SDKROOT to the appropriate Apple platform SDK or unset to use automatic SDK discovery.)",
                    platform
                ));
            }

            let minimum_semver = Version::parse(&format!("{}.0", minimum_version))?;

            if sdk
                .version_as_semver()
                .context("resolving Apple SDK version")?
                < minimum_semver
            {
                warn!(
                    logger,
                    "WARNING: SDKROOT defined Apple SDK does not meet minimum version requirement of {}; build errors or unexpected behavior may occur",
                    minimum_version
                );
            }

            sdk
        } else {
            warn!(
                logger,
                "locating Apple SDK {}{}+ supporting {}{}",
                platform,
                minimum_version,
                platform,
                deployment_target
            );

            resolve_apple_sdk(logger, platform, minimum_version, deployment_target)
                .context("resolving Apple SDK")?
        };

        warn!(
            logger,
            "using SDK {} ({} targeting {}{})",
            sdk.path.display(),
            sdk.name,
            platform,
            deployment_target
        );

        Ok(sdk)
    }
}

/// Represents an available Rust toolchain.
#[derive(Clone, Debug)]
pub struct RustEnvironment {
    /// Path to `cargo` executable.
    pub cargo_exe: PathBuf,

    /// Path to `rustc` executable.
    pub rustc_exe: PathBuf,

    /// Describes rustc version info.
    pub rust_version: rustc_version::VersionMeta,
}

/// Resolve an appropriate Apple SDK to use.
///
/// Given an Apple `platform`, locate an Apple SDK that is of least
/// `minimum_version` and supports targeting `deployment_target`, which is likely
/// an OS version string.
pub fn resolve_apple_sdk(
    logger: &slog::Logger,
    platform: &str,
    minimum_version: &str,
    deployment_target: &str,
) -> Result<AppleSdk> {
    if minimum_version.split('.').count() != 2 {
        return Err(anyhow!(
            "expected X.Y minimum Apple SDK version; got {}",
            minimum_version
        ));
    }

    let minimum_semver = Version::parse(&format!("{}.0", minimum_version))?;

    let mut sdks = find_default_developer_sdks()
        .context("discovering Apple SDKs (default developer directory)")?;
    if let Some(extra_sdks) =
        find_command_line_tools_sdks().context("discovering Apple SDKs (command line tools)")?
    {
        sdks.extend(extra_sdks);
    }

    let target_sdks = sdks
        .iter()
        .filter(|sdk| !sdk.is_symlink && sdk.supported_targets.contains_key(platform))
        .collect::<Vec<_>>();

    info!(
        logger,
        "found {} total Apple SDKs; {} support {}",
        sdks.len(),
        target_sdks.len(),
        platform,
    );

    let mut candidate_sdks = target_sdks
        .into_iter()
        .filter(|sdk| {
            let version = match sdk.version_as_semver() {
                Ok(v) => v,
                Err(_) => return false,
            };

            if version < minimum_semver {
                info!(
                    logger,
                    "ignoring SDK {} because it is too old ({} < {})",
                    sdk.path.display(),
                    sdk.version,
                    minimum_version
                );

                false
            } else if !sdk
                .supported_targets
                .get(platform)
                // Safe because key was validated above.
                .unwrap()
                .valid_deployment_targets
                .contains(&deployment_target.to_string())
            {
                info!(
                    logger,
                    "ignoring SDK {} because it doesn't support deployment target {}",
                    sdk.path.display(),
                    deployment_target
                );

                false
            } else {
                true
            }
        })
        .collect::<Vec<_>>();
    candidate_sdks.sort_by(|a, b| {
        b.version_as_semver()
            .unwrap()
            .cmp(&a.version_as_semver().unwrap())
    });

    if candidate_sdks.is_empty() {
        Err(anyhow!(
            "unable to find suitable Apple SDK supporting {}{} or newer",
            platform,
            minimum_version
        ))
    } else {
        info!(
            logger,
            "found {} suitable Apple SDKs ({})",
            candidate_sdks.len(),
            candidate_sdks
                .iter()
                .map(|sdk| sdk.name.clone())
                .collect::<Vec<_>>()
                .join(" ")
        );

        Ok(candidate_sdks[0].clone())
    }
}
