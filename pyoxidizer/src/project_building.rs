// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use anyhow::{anyhow, Context, Result};
use slog::warn;
use std::fs::create_dir_all;
use std::path::{Path, PathBuf};

use crate::environment::MINIMUM_RUST_VERSION;
use crate::py_packaging::binary::{EmbeddedPythonBinaryData, PreBuiltPythonExecutable};
use crate::py_packaging::config::RawAllocator;

/// Build an executable embedding Python using an existing Rust project.
///
/// The path to the produced executable is returned.
pub fn build_executable_with_rust_project(
    logger: &slog::Logger,
    project_path: &Path,
    bin_name: &str,
    exe: &PreBuiltPythonExecutable,
    build_path: &Path,
    artifacts_path: &Path,
    host: &str,
    target: &str,
    opt_level: &str,
    release: bool,
) -> Result<PathBuf> {
    create_dir_all(&artifacts_path)
        .with_context(|| "creating directory for PyOxidizer build artifacts")?;

    // Derive and write the artifacts needed to build a binary embedding Python.
    let embedded_data = EmbeddedPythonBinaryData::from_pre_built_python_executable(
        &exe, logger, host, target, opt_level,
    )?;
    embedded_data.write_files(&artifacts_path)?;

    let rust_version = rustc_version::version()?;
    if rust_version.lt(&MINIMUM_RUST_VERSION) {
        return Err(anyhow!(
            "PyOxidizer requires Rust {}; version {} found",
            *MINIMUM_RUST_VERSION,
            rust_version
        ));
    }
    warn!(logger, "building with Rust {}", rust_version);

    let target_base_path = build_path.join("target");
    let target_triple_base_path =
        target_base_path
            .join(target)
            .join(if release { "release" } else { "debug" });

    let mut args = Vec::new();
    args.push("build");
    args.push("--target");
    args.push(target);

    let target_dir = target_base_path.display().to_string();
    args.push("--target-dir");
    args.push(&target_dir);

    args.push("--bin");
    args.push(bin_name);

    if release {
        args.push("--release");
    }

    if exe.config.raw_allocator == RawAllocator::Jemalloc {
        args.push("--features");
        args.push("jemalloc");
    }

    let mut envs = Vec::new();
    envs.push((
        "PYOXIDIZER_ARTIFACT_DIR",
        artifacts_path.display().to_string(),
    ));
    envs.push(("PYOXIDIZER_REUSE_ARTIFACTS", "1".to_string()));

    // Set PYTHON_SYS_EXECUTABLE so python3-sys uses our distribution's Python to configure
    // itself.
    let python_exe_path = &exe.distribution.python_exe;
    envs.push((
        "PYTHON_SYS_EXECUTABLE",
        python_exe_path.display().to_string(),
    ));

    // static-nobundle link kind requires nightly Rust compiler until
    // https://github.com/rust-lang/rust/issues/37403 is resolved.
    if cfg!(windows) {
        envs.push(("RUSTC_BOOTSTRAP", "1".to_string()));
    }

    let status = std::process::Command::new("cargo")
        .args(args)
        .current_dir(&project_path)
        .envs(envs)
        .status()?;

    if !status.success() {
        return Err(anyhow!("cargo build failed"));
    }

    let exe_name = if target.contains("pc-windows") {
        format!("{}.exe", bin_name)
    } else {
        bin_name.to_string()
    };

    let exe_path = target_triple_base_path.join(&exe_name);

    if !exe_path.exists() {
        return Err(anyhow!("{} does not exist", exe_path.display()));
    }

    Ok(exe_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::py_packaging::binary::tests::get_prebuilt;
    use crate::testutil::*;

    #[test]
    fn test_empty_project() -> Result<()> {
        let temp_dir = tempdir::TempDir::new("pyoxidizer-test")?;
        let project_path = temp_dir.path().join("myapp");

        crate::projectmgmt::init(&project_path.display().to_string(), None, &[])?;

        let logger = get_logger()?;
        let pre_built = get_prebuilt(&logger)?;

        let build_path = project_path.join("build");
        let artifacts_path = build_path.join("artifacts");

        build_executable_with_rust_project(
            &logger,
            &project_path,
            "myapp",
            &pre_built,
            &build_path,
            &artifacts_path,
            env!("HOST"),
            env!("HOST"),
            "0",
            false,
        )?;

        Ok(())
    }
}
