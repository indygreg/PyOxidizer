// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Resolve details about the PyOxidizer execution environment.

use {
    crate::project_layout::PyembedLocation,
    anyhow::Result,
    once_cell::sync::Lazy,
    std::{
        env,
        path::{Path, PathBuf},
    },
};

const PACKAGE_VERSION: &str = env!("CARGO_PKG_VERSION");

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
// Remember to update the CI configuration in ci/azure-pipelines-template.yml
// and the `Installing Rust` documentation when this changes.
pub static MINIMUM_RUST_VERSION: Lazy<semver::Version> =
    Lazy::new(|| semver::Version::new(1, 45, 0));

/// Target triples for Linux.
pub static LINUX_TARGET_TRIPLES: Lazy<Vec<&'static str>> =
    Lazy::new(|| vec!["x86_64-unknown-linux-gnu", "x86_64-unknown-linux-musl"]);

/// Target triples for macOS.
pub static MACOS_TARGET_TRIPLES: Lazy<Vec<&'static str>> =
    Lazy::new(|| vec!["x86_64-apple-darwin"]);

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
    let pyoxidizer_source = if let Some(path) = BUILD_GIT_REPO_PATH.as_ref() {
        PyOxidizerSource::LocalPath { path: path.clone() }
    } else {
        GIT_SOURCE.clone()
    };

    Ok(Environment { pyoxidizer_source })
}
