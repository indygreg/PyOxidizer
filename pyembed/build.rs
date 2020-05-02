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

use {
    std::env,
    std::path::{Path, PathBuf},
};

/// Build with PyOxidizer artifacts in a directory.
fn build_with_artifacts_in_dir(path: &Path) {
    println!("using pre-built artifacts from {}", path.display());

    // Emit the cargo metadata lines to register libraries for linking.
    let cargo_metadata_path = path.join("cargo_metadata.txt");
    let metadata = std::fs::read_to_string(&cargo_metadata_path)
        .unwrap_or_else(|_| panic!("failed to read {}", cargo_metadata_path.display()));
    println!("{}", metadata);
}

/// Build by calling a `pyoxidizer` executable to generate build artifacts.
fn build_with_pyoxidizer_exe(exe: Option<String>, resolve_target: Option<&str>) {
    let pyoxidizer_exe = if let Some(path) = exe {
        path
    } else {
        "pyoxidizer".to_string()
    };

    let mut args = vec!["run-build-script", "build.rs"];
    if let Some(target) = resolve_target {
        args.push("--target");
        args.push(target);
    }

    match std::process::Command::new(pyoxidizer_exe)
        .args(args)
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

/* UNCOMMENT THE FOLLOWING TO ENABLE BUILDING IN LIBRARY MODE.

use {
    pyoxidizerlib::logging::LoggerContext,
    pyoxidizerlib::project_building::run_from_build,
};

/// Build by calling PyOxidizer natively as a Rust library.
///
/// Uses the build output from config file target `resolve_target` or the
/// default if not set.
fn build_with_pyoxidizer_native(resolve_target: Option<&str>) {
    println!("invoking PyOxidizer natively to build artifacts");
    let logger_context = LoggerContext::default();

    run_from_build(&logger_context.logger, "build.rs", resolve_target).unwrap();
}
*/

fn main() {
    let mut library_mode = "pyembed";

    if env::var("CARGO_FEATURE_BUILD_MODE_STANDALONE").is_ok() {
    } else if env::var("CARGO_FEATURE_BUILD_MODE_PYOXIDIZER_EXE").is_ok() {
        let target = if let Ok(target) = env::var("PYOXIDIZER_BUILD_TARGET") {
            Some(target)
        } else {
            None
        };

        build_with_pyoxidizer_exe(
            env::var("PYOXIDIZER_EXE").ok(),
            if let Some(target) = &target {
                Some(target.as_ref())
            } else {
                None
            },
        );
    } else if env::var("CARGO_FEATURE_BUILD_MODE_PREBUILT_ARTIFACTS").is_ok() {
        let artifact_dir_env = env::var("PYOXIDIZER_ARTIFACT_DIR");

        let artifact_dir_path = match artifact_dir_env {
            Ok(ref v) => PathBuf::from(v),
            Err(_) => {
                let out_dir = env::var("OUT_DIR").unwrap();
                PathBuf::from(&out_dir)
            }
        };

        println!("cargo:rerun-if-env-changed=PYOXIDIZER_ARTIFACT_DIR");
        build_with_artifacts_in_dir(&artifact_dir_path);
    } else if env::var("CARGO_FEATURE_BUILD_MODE_EXTENSION_MODULE").is_ok() {
        library_mode = "extension";
    } else if env::var("CARGO_FEATURE_BUILD_MODE_TEST").is_ok() {
        println!(
            "cargo:rustc-env=PYEMBED_TESTS_DIR={}/src/test",
            env::var("CARGO_MANIFEST_DIR").unwrap()
        );
    } else {
        panic!("build-mode-* feature not set");
    }

    println!("cargo:rustc-cfg=library_mode=\"{}\"", library_mode);
}
