// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Resolve details about the PyOxidizer execution environment.

use {
    crate::project_layout::PyembedLocation,
    anyhow::{anyhow, Result},
    git2::{Commit, Repository},
    lazy_static::lazy_static,
    std::env,
    std::path::{Path, PathBuf},
};

pub const PYOXIDIZER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Canonical Git repository for PyOxidizer.
const CANONICAL_GIT_REPO_URL: &str = "https://github.com/indygreg/PyOxidizer.git";

/// Root Git commit for PyOxidizer.
const ROOT_COMMIT: &str = "b1f95017c897e0fd3ed006aec25b6886196a889d";

/// Git commit this build of PyOxidizer was produced with.
pub const BUILD_GIT_COMMIT: &str = env!("VERGEN_SHA");

/// Semantic version for this build of PyOxidizer. Can correspond to a Git
/// tag or version string from Cargo.toml.
pub const BUILD_SEMVER: &str = env!("VERGEN_SEMVER");

/// Semantic version for this build of PyOxidizer. Usually of form
/// <tag>-<count>-<short sha>.
pub const BUILD_SEMVER_LIGHTWEIGHT: &str = env!("VERGEN_SEMVER_LIGHTWEIGHT");

lazy_static! {
    /// Minimum version of Rust required to build PyOxidizer applications.
    pub static ref MINIMUM_RUST_VERSION: semver::Version = semver::Version::new(1, 36, 0);
}

/// Find the root Git commit given a starting Git commit.
///
/// This just walks parents until it gets to a commit without any.
fn find_root_git_commit(commit: Commit) -> Commit {
    let mut current = commit;

    while current.parent_count() != 0 {
        current = current.parents().next().unwrap();
    }

    current
}

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

/// Describes the location of the PyOxidizer source files.
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

/// Describes the PyOxidizer run-time environment.
pub struct Environment {
    /// Where a copy of PyOxidizer can be obtained from.
    pub pyoxidizer_source: PyOxidizerSource,

    /// Semantic version string for PyOxidizer.
    pub pyoxidizer_semver: String,
}

impl Environment {
    /// Determine the location of the pyembed crate given a run-time environment.
    ///
    /// If running from a PyOxidizer Git repository, we reference the pyembed
    /// crate within the PyOxidizer Git repository. Otherwise we use the pyembed
    /// crate from the package registry.
    ///
    /// There is room to reference a Git repository+commit. But this isn't implemented
    /// yet.
    pub fn as_pyembed_location(&self) -> PyembedLocation {
        match &self.pyoxidizer_source {
            PyOxidizerSource::LocalPath { path } => {
                PyembedLocation::Path(canonicalize_path(&path.join("pyembed")).unwrap())
            }
            PyOxidizerSource::GitUrl { .. } => {
                PyembedLocation::Version(PYOXIDIZER_VERSION.to_string())
            }
        }
    }
}

/// Obtain a PyOxidizerSource pointing to the GitUrl this binary was built with.
pub fn built_git_url() -> PyOxidizerSource {
    let commit = match BUILD_GIT_COMMIT {
        // Can happen when not run from a Git checkout (such as installing
        // from a crate).
        "" => None,
        // Can happen if `git` is not available at build time.
        "UNKNOWN" => None,
        value => Some(value.to_string()),
    };

    // Commit and tag should be mutually exclusive. BUILD_SEMVER could be
    // derived by a Git tag in some circumstances. More commonly it is
    // derived from Cargo.toml. The Git tags have ``v`` prefixes.
    let tag = if commit.is_some() {
        None
    } else if !BUILD_SEMVER.starts_with('v') {
        Some("v".to_string() + BUILD_SEMVER)
    } else {
        Some(BUILD_SEMVER.to_string())
    };

    PyOxidizerSource::GitUrl {
        url: CANONICAL_GIT_REPO_URL.to_owned(),
        commit,
        tag,
    }
}

pub fn resolve_environment() -> Result<Environment> {
    let exe_path = PathBuf::from(
        env::current_exe()?
            .parent()
            .ok_or_else(|| anyhow!("could not resolve parent of current exe"))?,
    );

    let pyoxidizer_semver = BUILD_SEMVER.to_string();

    let pyoxidizer_source = match Repository::discover(&exe_path) {
        Ok(repo) => {
            let head = repo.head().unwrap();
            let commit = head.peel_to_commit().unwrap();
            let root = find_root_git_commit(commit.clone());

            if root.id().to_string() == ROOT_COMMIT {
                PyOxidizerSource::LocalPath {
                    path: canonicalize_path(
                        repo.workdir()
                            .ok_or_else(|| anyhow!("unable to resolve Git workdir"))?,
                    )?,
                }
            } else {
                // The pyoxidizer binary is in a directory that is in a Git repo that isn't
                // pyoxidizer's. This could happen if running `pyoxidizer` from another
                // project's Git repository. This commonly happens when running
                // pyoxidizer as a library from a build script. Fall back to
                // returning info embedded in the build.
                built_git_url()
            }
        }
        Err(_) => {
            // We're not running from a Git repo. Point to the canonical repo for the Git commit
            // baked into the binary.
            // TODO detect builds from forks via build.rs environment variable.
            built_git_url()
        }
    };

    Ok(Environment {
        pyoxidizer_source,
        pyoxidizer_semver,
    })
}
