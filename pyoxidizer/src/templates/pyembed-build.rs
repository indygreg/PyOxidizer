// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::env;
use std::path::PathBuf;
use std::process;

/// Path to pyoxidizer executable this file was created with.
const DEFAULT_PYOXIDIZER_EXE: &str = r#"{{{pyoxidizer_exe}}}"#;

fn main() {
    // We support using pre-built artifacts, in which case we emit the
    // cargo metadata lines from the "original" build to "register" the
    // artifacts with this cargo invocation.
    if env::var("PYOXIDIZER_REUSE_ARTIFACTS").is_ok() {
        let artifact_dir_env = env::var("PYOXIDIZER_ARTIFACT_DIR");

        let artifact_dir_path = match artifact_dir_env {
            Ok(ref v) => PathBuf::from(v),
            Err(_) => {
                let out_dir = env::var("OUT_DIR").unwrap();
                PathBuf::from(&out_dir)
            }
        };

        println!(
            "using pre-built artifacts from {}",
            artifact_dir_path.display()
        );

        println!("cargo:rerun-if-env-changed=PYOXIDIZER_REUSE_ARTIFACTS");
        println!("cargo:rerun-if-env-changed=PYOXIDIZER_ARTIFACT_DIR");

        // Emit the cargo metadata lines to register libraries for linking.
        let cargo_metadata_path = artifact_dir_path.join("cargo_metadata.txt");
        let metadata = std::fs::read_to_string(&cargo_metadata_path)
            .expect(format!("failed to read {}", cargo_metadata_path.display()).as_str());
        println!("{}", metadata);
    } else {
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
            .arg("--target")
            .arg("embedded")
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
}
