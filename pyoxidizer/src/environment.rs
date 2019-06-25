// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Resolve details about the PyOxidizer execution environment.

use git2::{Commit, Repository};
use std::env;
use std::path::{Path, PathBuf};

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

pub fn resolve_environment() -> Result<Environment, &'static str> {
    let exe_path = PathBuf::from(
        env::current_exe()
            .or_else(|_| Err("could not resolve current exe"))?
            .parent()
            .ok_or_else(|| "could not resolve parent of current exe")?,
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
                            .ok_or_else(|| "unable to resolve Git workdir")?,
                    )
                    .or_else(|_| Err("unable to canonicalize path"))?,
                }
            } else {
                // The pyoxidizer binary is in a directory that is in a Git repo that isn't
                // pyoxidizer's. That's really weird. While this could occur, treat as a fatal
                // error for now.
                return Err(
                    "pyoxidizer binary is in a Git repository that is not pyoxidizer; \
                     refusing to continue; if you would like this feature, please file \
                     an issue for it at https://github.com/indygreg/PyOxidizer/issues/",
                );
            }
        }
        Err(_) => {
            // We're not running from a Git repo. Point to the canonical repo for the Git commit
            // baked into the binary.
            // TODO detect builds from forks via build.rs environment variable.

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
            } else {
                if !BUILD_SEMVER.starts_with("v") {
                    Some("v".to_string() + BUILD_SEMVER)
                } else {
                    Some(BUILD_SEMVER.to_string())
                }
            };

            PyOxidizerSource::GitUrl {
                url: CANONICAL_GIT_REPO_URL.to_owned(),
                commit,
                tag,
            }
        }
    };

    Ok(Environment {
        pyoxidizer_source,
        pyoxidizer_semver,
    })
}
