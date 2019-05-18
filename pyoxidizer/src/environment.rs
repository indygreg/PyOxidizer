// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use git2::{Commit, Repository};
use std::env;
use std::path::PathBuf;

// Root Git commit for PyOxidizer.
const ROOT_COMMIT: &str = "b1f95017c897e0fd3ed006aec25b6886196a889d";

pub fn find_root_git_commit(commit: Commit) -> Commit {
    let mut current = commit;

    while current.parent_count() != 0 {
        current = current.parents().next().unwrap();
    }

    current
}

/// Describes the PyOxidizer run-time environment.
pub struct Environment {
    pub pyoxidizer_repo_path: Option<PathBuf>,
    pub pyoxidizer_commit: Option<String>,
}

pub fn resolve_environment() -> Environment {
    let exe_path = PathBuf::from(env::current_exe().unwrap().parent().unwrap());

    let (repo_path, commit) = match Repository::discover(&exe_path) {
        Ok(repo) => {
            let head = repo.head().unwrap();
            let commit = head.peel_to_commit().unwrap();
            let root = find_root_git_commit(commit.clone());

            if root.id().to_string() == ROOT_COMMIT {
                (
                    Some(repo.workdir().unwrap().to_path_buf()),
                    Some(commit.id().to_string()),
                )
            } else {
                (None, None)
            }
        }
        Err(_) => (None, None),
    };

    Environment {
        pyoxidizer_repo_path: repo_path,
        pyoxidizer_commit: commit,
    }
}
