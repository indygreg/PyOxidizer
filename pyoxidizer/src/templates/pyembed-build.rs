// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Build script to embed Python in a binary.

The goal of this build script is to emit metadata to tell the build
system how to embed a Python interpreter into a binary.

This is done by printing the content of an artifact (cargo_metadata.txt)
produced by PyOxidizer. This artifact in turn references other build
artifacts.

The following strategies exist for obtaining the build artifacts needed
by this crate:

1. Call `pyoxidizer run-build-script` and use its output verbatim.
2. Call into the PyOxidizer library directly to perform the equivalent
   of `pyoxidizer run-build-script`. (See bottom of file for an example.)
3. Build artifacts out-of-band and consume them manually in this script
   (e.g. by calling `pyoxidizer build` and then reading the generated
   `cargo_metadata.txt` file manually.)
*/

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

// To call into PyOxidizer as a library, replace the code above with something
// like the code below example. Don't forget to add a [[build-dependencies]] entry in the
// Cargo.toml file!
//
// Please note that PyOxidizer has a lot of dependencies and building them all can take
// a while.
/*
use {
    pyoxidizerlib::logging::LoggerContext,
    pyoxidizerlib::project_building::run_from_build,
}

fn main() {
    let logger_context = LoggerContext::default();
    run_from_build(&logger_context.logger, "build.rs", Some("embedded")).unwrap();
}
*/
