// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::app_packaging::config::{eval_starlark_config_file, find_pyoxidizer_config_file_env},
    crate::environment::MINIMUM_RUST_VERSION,
    crate::project_layout::initialize_project,
    crate::py_packaging::binary::{EmbeddedPythonBinaryData, PreBuiltPythonExecutable},
    crate::py_packaging::config::RawAllocator,
    anyhow::{anyhow, Context, Result},
    slog::warn,
    std::env,
    std::fs::create_dir_all,
    std::path::{Path, PathBuf},
};

/// Build an executable embedding Python using an existing Rust project.
///
/// The path to the produced executable is returned.
#[allow(clippy::too_many_arguments)]
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

/// Build a Python executable using a temporary Rust project.
///
/// Returns the binary data constituting the built executable.
pub fn build_python_executable(
    logger: &slog::Logger,
    bin_name: &str,
    exe: &PreBuiltPythonExecutable,
    host: &str,
    target: &str,
    opt_level: &str,
    release: bool,
) -> Result<(String, Vec<u8>)> {
    let temp_dir = tempdir::TempDir::new("pyoxidizer")?;

    // Directory needs to have name of project.
    let project_path = temp_dir.path().join(bin_name);
    let build_path = temp_dir.path().join("build");
    let artifacts_path = temp_dir.path().join("artifacts");

    initialize_project(&project_path, None, &[])?;

    let exe_path = build_executable_with_rust_project(
        logger,
        &project_path,
        bin_name,
        exe,
        &build_path,
        &artifacts_path,
        host,
        target,
        opt_level,
        release,
    )?;

    let data = std::fs::read(&exe_path)?;
    let filename = exe_path.file_name().unwrap().to_string_lossy().to_string();

    Ok((filename, data))
}

/// Runs packaging/embedding from the context of a Rust build script.
///
/// This function should be called by the build script for the package
/// that wishes to embed a Python interpreter/application. When called,
/// a PyOxidizer configuration file is found and read. The configuration
/// is then applied to the current build. This involves obtaining a
/// Python distribution to embed (possibly by downloading it from the Internet),
/// analyzing the contents of that distribution, extracting relevant files
/// from the distribution, compiling Python bytecode, and generating
/// resources required to build the ``pyembed`` crate/modules.
///
/// If everything works as planned, this whole process should be largely
/// invisible and the calling application will have an embedded Python
/// interpreter when it is built.
pub fn run_from_build(logger: &slog::Logger, build_script: &str) {
    // Adding our our rerun-if-changed lines will overwrite the default, so
    // we need to emit the build script name explicitly.
    println!("cargo:rerun-if-changed={}", build_script);

    println!("cargo:rerun-if-env-changed=PYOXIDIZER_CONFIG");

    // TODO use these variables?
    //let host = env::var("HOST").expect("HOST not defined");
    let target = env::var("TARGET").expect("TARGET not defined");
    //let opt_level = env::var("OPT_LEVEL").expect("OPT_LEVEL not defined");
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not found");
    let profile = env::var("PROFILE").expect("PROFILE not defined");

    //let project_path = PathBuf::from(&manifest_dir);

    let config_path = match find_pyoxidizer_config_file_env(logger, &PathBuf::from(manifest_dir)) {
        Some(v) => v,
        None => panic!("Could not find PyOxidizer config file"),
    };

    if !config_path.exists() {
        panic!("PyOxidizer config file does not exist");
    }

    let dest_dir = match env::var("PYOXIDIZER_ARTIFACT_DIR") {
        Ok(ref v) => PathBuf::from(v),
        Err(_) => PathBuf::from(env::var("OUT_DIR").unwrap()),
    };

    eval_starlark_config_file(
        logger,
        &config_path,
        &target,
        profile == "release",
        false,
        Some(&dest_dir),
        Some(Vec::new()),
    )
    .unwrap();

    let cargo_metadata = dest_dir.join("cargo_metadata.txt");
    let content = std::fs::read(&cargo_metadata).unwrap();
    let content = String::from_utf8(content).unwrap();
    print!("{}", content);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::py_packaging::binary::tests::get_prebuilt;
    use crate::testutil::*;

    #[test]
    fn test_empty_project() -> Result<()> {
        let logger = get_logger()?;
        let pre_built = get_prebuilt(&logger)?;

        build_python_executable(
            &logger,
            "myapp",
            &pre_built,
            env!("HOST"),
            env!("HOST"),
            "0",
            false,
        )?;

        Ok(())
    }
}
