// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    git2::{Commit, Repository},
    std::{
        path::{Path, PathBuf},
        process::Command,
    },
};

/// Canonical Git repository for PyOxidizer.
const CANONICAL_GIT_REPO_URL: &str = "https://github.com/indygreg/PyOxidizer.git";

/// Root Git commit for PyOxidizer.
const ROOT_COMMIT: &str = "b1f95017c897e0fd3ed006aec25b6886196a889d";

fn canonicalize_path(path: &Path) -> Result<PathBuf, std::io::Error> {
    let mut p = path.canonicalize()?;

    // Strip \\?\ prefix on Windows and replace \ with /, which is valid.
    if cfg!(windows) {
        let mut s = p.display().to_string().replace('\\', "/");
        if s.starts_with("//?/") {
            s = s[4..].to_string();
        }

        p = PathBuf::from(s);
    }

    Ok(p)
}

/// Find the root Git commit given a starting Git commit.
///
/// This just walks parents until it gets to a commit without any.
fn find_root_git_commit(commit: Commit) -> Commit {
    let mut current = commit;

    while let Ok(parent) = current.parent(0) {
        current = parent;
    }

    current
}

fn main() {
    let cwd = std::env::current_dir().expect("could not obtain current directory");

    // Allow PYOXIDIZER_BUILD_GIT_URL to define the Git repository URL.
    let git_repo_url = if let Ok(url) = std::env::var("PYOXIDIZER_BUILD_GIT_URL") {
        url
    } else {
        CANONICAL_GIT_REPO_URL.to_string()
    };
    println!("cargo:rerun-if-env-changed=PYOXIDIZER_BUILD_GIT_URL");

    // Allow PYOXIDIZER_BUILD_FORCE_GIT_SOURCE to force use of a Git install source.
    let force_git_source = std::env::var("PYOXIDIZER_BUILD_FORCE_GIT_SOURCE").is_ok();
    println!("cargo:rerun-if-env-changed=PYOXIDIZER_BUILD_FORCE_GIT_SOURCE");

    // Allow PYOXIDIZER_BUILD_FORCE_GIT_COMMIT to override the Git commit.
    let force_git_commit = std::env::var("PYOXIDIZER_BUILD_FORCE_GIT_COMMIT").ok();
    println!("cargo:rerun-if-env-changed=PYOXIDIZER_BUILD_FORCE_GIT_COMMIT");

    let mut git_commit = "".to_string();
    let mut repo_path = "".to_string();

    // Various crates that resolve commits and versions from git shell out to `git`.
    // This isn't reliable, especially on Windows. So we use libgit2 to extract data
    // from the git repo, if present.
    if let Ok(repo) = Repository::discover(&cwd) {
        if let Ok(head_ref) = repo.head() {
            if let Ok(commit) = head_ref.peel_to_commit() {
                let root = find_root_git_commit(commit.clone());

                if root.id().to_string() == ROOT_COMMIT {
                    let path = canonicalize_path(repo.workdir().expect("could not obtain workdir"))
                        .expect("could not canonicalize repo path");

                    repo_path = path.display().to_string();
                    git_commit = commit.id().to_string();
                }
            }
        }
    } else if let Ok(output) = Command::new("sl").arg("root").current_dir(&cwd).output() {
        if output.status.success() {
            repo_path = String::from_utf8(output.stdout)
                .expect("sl root should print UTF-8")
                .trim()
                .to_string();

            if let Ok(output) = Command::new("sl")
                .args(["log", "-r", ".", "-T", "{node}"])
                .current_dir(&cwd)
                .output()
            {
                git_commit =
                    String::from_utf8(output.stdout).expect("sl log output should print UTF-8");
            }
        }
    }

    if force_git_source {
        repo_path = "".to_string();
    }

    if let Some(commit) = force_git_commit {
        git_commit = commit;
    }

    let pkg_version =
        std::env::var("CARGO_PKG_VERSION").expect("could not obtain CARGO_PKG_VERSION");

    let (pyoxidizer_version, git_tag) = if pkg_version.ends_with("-pre") {
        (
            format!(
                "{}-{}",
                pkg_version,
                if git_commit.is_empty() {
                    "UNKNOWN"
                } else {
                    git_commit.as_str()
                }
            ),
            "".to_string(),
        )
    } else {
        (pkg_version.clone(), format!("pyoxidizer/{}", pkg_version))
    };

    println!("cargo:rustc-env=PYOXIDIZER_VERSION={}", pyoxidizer_version);

    println!("cargo:rustc-env=GIT_REPO_PATH={}", repo_path);
    println!("cargo:rustc-env=GIT_REPO_URL={}", git_repo_url);
    println!("cargo:rustc-env=GIT_TAG={}", git_tag);
    println!("cargo:rustc-env=GIT_COMMIT={}", git_commit);

    println!(
        "cargo:rustc-env=TARGET={}",
        std::env::var("TARGET").expect("TARGET not set")
    );
}
