// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Resolve details about the PyOxidizer execution environment.

use {
    crate::project_layout::PyembedLocation,
    anyhow::{anyhow, Result},
    git2::{Commit, Repository},
    lazy_static::lazy_static,
    std::{
        env,
        path::{Path, PathBuf},
    },
};

const PACKAGE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// URL of Git repository we were built from.
const GIT_REPO_URL: &str = env!("GIT_REPO_URL");

/// Root Git commit for PyOxidizer.
const ROOT_COMMIT: &str = "b1f95017c897e0fd3ed006aec25b6886196a889d";

/// Version string of PyOxidizer.
pub const PYOXIDIZER_VERSION: &str = env!("PYOXIDIZER_VERSION");

lazy_static! {
    /// Git commit this build of PyOxidizer was produced with.
    pub static ref BUILD_GIT_COMMIT: Option<String> = {
        match env!("GIT_COMMIT") {
            // Can happen when not run from a Git checkout (such as installing
            // from a crate).
            "" => None,
            // Can happen if build script could not find Git repository.
            "UNKNOWN" => None,
            value => Some(value.to_string()),
        }
    };

    /// The Git tag we are built against.
    pub static ref BUILD_GIT_TAG: Option<String> = {
        let tag = env!("GIT_TAG");
        if tag.is_empty() {
            None
        } else {
            Some(tag.to_string())
        }
    };

    /// Defines the source of this install from Git data embedded in the binary.
    pub static ref GIT_SOURCE: PyOxidizerSource = {
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
    };

    /// Minimum version of Rust required to build PyOxidizer applications.
    ///
    // Remember to update the CI configuration in ci/azure-pipelines-template.yml
    // when this changes.
    pub static ref MINIMUM_RUST_VERSION: semver::Version = semver::Version::new(1, 41, 0);

    /// Target triples for Linux.
    pub static ref LINUX_TARGET_TRIPLES: Vec<&'static str> = vec![
        "x86_64-unknown-linux-gnu",
        "x86_64-unknown-linux-musl",
    ];

    /// Target triples for macOS.
    pub static ref MACOS_TARGET_TRIPLES: Vec<&'static str> = vec![
        "x86_64-apple-darwin",
    ];

    /// Target triples for Windows.
    pub static ref WINDOWS_TARGET_TRIPLES: Vec<&'static str> = vec![
        "i686-pc-windows-gnu",
        "i686-pc-windows-msvc",
        "x86_64-pc-windows-gnu",
        "x86_64-pc-windows-msvc",
    ];
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

/// Describes the PyOxidizer run-time environment.
pub struct Environment {
    /// Where a copy of PyOxidizer can be obtained from.
    pub pyoxidizer_source: PyOxidizerSource,
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
            PyOxidizerSource::GitUrl { url, commit, .. } => match commit {
                Some(commit) => PyembedLocation::Git(url.clone(), commit.clone()),
                None => PyembedLocation::Version(PACKAGE_VERSION.to_string()),
            },
        }
    }

    /// Obtain a string to be used as the long form version info for the executable.
    pub fn version_long(&self) -> String {
        format!(
            "{}\ncommit: {}\nsource: {}\npyembed crate location: {}",
            PACKAGE_VERSION,
            if let Some(commit) = BUILD_GIT_COMMIT.as_ref() {
                commit.as_str()
            } else {
                "unknown"
            },
            match &self.pyoxidizer_source {
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

pub fn resolve_environment() -> Result<Environment> {
    let exe_path = PathBuf::from(
        env::current_exe()?
            .parent()
            .ok_or_else(|| anyhow!("could not resolve parent of current exe"))?,
    );

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
                GIT_SOURCE.clone()
            }
        }
        Err(_) => {
            // We're not running from a Git repo. Point to the canonical repo for the Git commit
            // baked into the binary.
            GIT_SOURCE.clone()
        }
    };

    Ok(Environment { pyoxidizer_source })
}
