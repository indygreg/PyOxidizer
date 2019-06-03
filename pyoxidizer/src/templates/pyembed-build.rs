// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::env;
use std::path::PathBuf;
use std::process;

/// Path to pyoxidizer executable this file was created with.
const DEFAULT_PYOXIDIZER_EXE: &str = "{{{pyoxidizer_exe}}}";

fn main() {
    let pyoxidizer_exe = match env::var("PYOXIDIZER_EXE") {
        Ok(value) => value,
        Err(_) => DEFAULT_PYOXIDIZER_EXE.to_string(),
    };

    let pyoxidizer_path = PathBuf::from(&pyoxidizer_exe);

    if !pyoxidizer_path.exists() {
        panic!("pyoxidizer executable does not exist: {}", &pyoxidizer_exe);
    }

    match process::Command::new(&pyoxidizer_exe)
        .arg("run-build-script")
        .arg("build.rs")
        .status()
    {
        Ok(status) => {
            if !status.success() {
                panic!("`pyoxidizer run-build-script` failed");
            }
        }
        Err(e) => panic!("`pyoxidizer run-build-script` failed: {}", e.to_string()),
    }
}
