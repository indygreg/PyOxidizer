// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    pyoxidizerlib::logging::LoggerContext,
    pyoxidizerlib::project_building::run_from_build,
    std::env,
    std::path::{Path, PathBuf},
};

/// Build with PyOxidizer artifacts in a directory.
fn build_with_artifacts_in_dir(path: &Path) {
    println!("using pre-built artifacts from {}", path.display());

    // Emit the cargo metadata lines to register libraries for linking.
    let cargo_metadata_path = path.join("cargo_metadata.txt");
    let metadata = std::fs::read_to_string(&cargo_metadata_path)
        .expect(format!("failed to read {}", cargo_metadata_path.display()).as_str());
    println!("{}", metadata);
}

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

        println!("cargo:rerun-if-env-changed=PYOXIDIZER_REUSE_ARTIFACTS");
        println!("cargo:rerun-if-env-changed=PYOXIDIZER_ARTIFACT_DIR");
        build_with_artifacts_in_dir(&artifact_dir_path);
    } else {
        println!("invoking PyOxidizer natively to build artifacts");
        let logger_context = LoggerContext::default();

        let target = if let Ok(target) = env::var("PYOXIDIZER_BUILD_TARGET") {
            Some(target)
        } else {
            None
        };

        let resolve_target = if let Some(target) = &target {
            Some(target.as_ref())
        } else {
            None
        };

        run_from_build(&logger_context.logger, "build.rs", resolve_target).unwrap();
    }
}
