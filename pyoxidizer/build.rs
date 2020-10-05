// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    git2::Repository,
    vergen::{generate_cargo_keys, ConstantsFlags},
};

fn main() {
    let cwd = std::env::current_dir().expect("could not obtain current directory");

    // vergen uses `git` to find Git information. But `git` isn't available in
    // all environments. Let's use libgit2 to reliably find Git info.
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

    println!(
        "cargo:rustc-env=GIT_COMMIT={}",
        match git_commit {
            Some(commit) => commit,
            None => "UNKNOWN".to_string(),
        }
    );

    generate_cargo_keys(ConstantsFlags::all()).expect("error running vergen");

    println!(
        "cargo:rustc-env=HOST={}",
        std::env::var("HOST").expect("HOST not set")
    );
}
