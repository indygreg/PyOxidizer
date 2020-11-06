// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use git2::Repository;

/// Canonical Git repository for PyOxidizer.
const CANONICAL_GIT_REPO_URL: &str = "https://github.com/indygreg/PyOxidizer.git";

fn main() {
    let cwd = std::env::current_dir().expect("could not obtain current directory");

    // Various crates that resolve commits and versions from git shell out to `git`.
    // This isn't reliable, especially on Windows. So we use libgit2 to extract data
    // from the git repo, if present.
    let git_commit = if let Ok(repo) = Repository::discover(&cwd) {
        if let Ok(head_ref) = repo.head() {
            if let Ok(commit) = head_ref.peel_to_commit() {
                Some(format!("{}", commit.id()))
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    let pkg_version =
        std::env::var("CARGO_PKG_VERSION").expect("could not obtain CARGO_PKG_VERSION");

    let (pyoxidizer_version, git_tag) = if pkg_version.ends_with("-pre") {
        (
            format!(
                "{}-{}",
                pkg_version,
                git_commit.clone().unwrap_or_else(|| "UNKNOWN".to_string())
            ),
            "".to_string(),
        )
    } else {
        (pkg_version.clone(), format!("v{}", pkg_version))
    };

    println!("cargo:rustc-env=PYOXIDIZER_VERSION={}", pyoxidizer_version);

    // TODO detect builds from forks via build.rs environment variable.
    println!("cargo:rustc-env=GIT_REPO_URL={}", CANONICAL_GIT_REPO_URL);
    println!("cargo:rustc-env=GIT_TAG={}", git_tag);

    println!(
        "cargo:rustc-env=GIT_COMMIT={}",
        match git_commit {
            Some(commit) => commit,
            None => "UNKNOWN".to_string(),
        }
    );

    println!(
        "cargo:rustc-env=HOST={}",
        std::env::var("HOST").expect("HOST not set")
    );
}
