// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::environment::{canonicalize_path, MINIMUM_RUST_VERSION},
    crate::project_layout::initialize_project,
    crate::py_packaging::binary::{EmbeddedPythonBinaryData, PythonBinaryBuilder},
    crate::starlark::eval::{eval_starlark_config_file, EvalResult},
    crate::starlark::target::ResolvedTarget,
    anyhow::{anyhow, Context, Result},
    slog::warn,
    std::env,
    std::fs::create_dir_all,
    std::path::{Path, PathBuf},
};

pub const HOST: &str = env!("HOST");

/// Find a pyoxidizer.toml configuration file by walking directory ancestry.
pub fn find_pyoxidizer_config_file(start_dir: &Path) -> Option<PathBuf> {
    for test_dir in start_dir.ancestors() {
        let candidate = test_dir.to_path_buf().join("pyoxidizer.bzl");

        if candidate.exists() {
            return Some(candidate);
        }
    }

    None
}

/// Find a PyOxidizer configuration file from walking the filesystem or an
/// environment variable override.
///
/// We first honor the `PYOXIDIZER_CONFIG` environment variable. This allows
/// explicit control over an exact file to use.
///
/// We then try scanning ancestor directories of `OUT_DIR`. This variable is
/// populated by Cargo to contain the output directory for build artifacts
/// for this crate. The assumption here is that this code is running from
/// the `pyembed` build script or as `pyoxidizer`. In the latter, `OUT_DIR`
/// should not be set. In the former, the crate that is building `pyembed`
/// likely has a config file and `OUT_DIR` is in that crate. This doesn't
/// always hold. But until Cargo starts passing an environment variable
/// defining the path of the main or calling manifest being built, it is
/// the best we can do.
///
/// If none of the above find a config file, we fall back to traversing ancestors
/// of `start_dir`.
pub fn find_pyoxidizer_config_file_env(logger: &slog::Logger, start_dir: &Path) -> Option<PathBuf> {
    if let Ok(path) = env::var("PYOXIDIZER_CONFIG") {
        warn!(
            logger,
            "using PyOxidizer config file from PYOXIDIZER_CONFIG: {}", path
        );
        return Some(PathBuf::from(path));
    }

    if let Ok(path) = env::var("OUT_DIR") {
        warn!(logger, "looking for config file in ancestry of {}", path);
        let res = find_pyoxidizer_config_file(&Path::new(&path));
        if res.is_some() {
            return res;
        }
    }

    find_pyoxidizer_config_file(start_dir)
}

/// Holds results from building an executable.
pub struct BuiltExecutable {
    /// Path to built executable file.
    pub exe_path: Option<PathBuf>,

    /// File name of executable.
    pub exe_name: String,

    /// Holds raw content of built executable.
    pub exe_data: Vec<u8>,

    /// Holds state generated from building.
    pub binary_data: EmbeddedPythonBinaryData,
}

/// Build an executable embedding Python using an existing Rust project.
///
/// The path to the produced executable is returned.
#[allow(clippy::too_many_arguments)]
pub fn build_executable_with_rust_project(
    logger: &slog::Logger,
    project_path: &Path,
    bin_name: &str,
    exe: &dyn PythonBinaryBuilder,
    build_path: &Path,
    artifacts_path: &Path,
    target: &str,
    opt_level: &str,
    release: bool,
) -> Result<BuiltExecutable> {
    create_dir_all(&artifacts_path)
        .with_context(|| "creating directory for PyOxidizer build artifacts")?;

    // Derive and write the artifacts needed to build a binary embedding Python.
    let embedded_data = exe.as_embedded_python_binary_data(logger, opt_level)?;
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

    args.push("--no-default-features");
    let mut features = vec!["build-mode-prebuilt-artifacts"];

    // If we have a real libpython, let cpython crate link against it. Otherwise
    // leave symbols unresolved, as we'll provide them.
    features.push(if embedded_data.linking_info.libpython_filename.is_some() {
        "cpython-link-default"
    } else {
        "cpython-link-unresolved-static"
    });

    if exe.requires_jemalloc() {
        features.push("jemalloc");
    }

    let features = features.join(" ");

    if !features.is_empty() {
        args.push("--features");
        args.push(&features);
    }

    let mut envs = Vec::new();
    envs.push((
        "PYOXIDIZER_ARTIFACT_DIR",
        artifacts_path.display().to_string(),
    ));
    envs.push(("PYOXIDIZER_REUSE_ARTIFACTS", "1".to_string()));

    // Set PYTHON_SYS_EXECUTABLE so python3-sys uses our distribution's Python to configure
    // itself.
    let python_exe_path = exe.python_exe_path();
    envs.push((
        "PYTHON_SYS_EXECUTABLE",
        python_exe_path.display().to_string(),
    ));

    // If linking against an existing dynamic library on Windows, add the path to that
    // library to an environment variable so link.exe can find it.
    if let Some(libpython_filename) = &embedded_data.linking_info.libpython_filename {
        if cfg!(windows) {
            let libpython_dir = libpython_filename
                .parent()
                .ok_or_else(|| anyhow!("unable to find parent directory of python DLL"))?;

            envs.push((
                "LIB",
                if let Ok(lib) = std::env::var("LIB") {
                    format!("{};{}", lib, libpython_dir.display())
                } else {
                    format!("{}", libpython_dir.display())
                },
            ));
        }
    }

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

    let exe_data = std::fs::read(&exe_path)?;
    let exe_name = exe_path.file_name().unwrap().to_string_lossy().to_string();

    Ok(BuiltExecutable {
        exe_path: Some(exe_path),
        exe_name,
        exe_data,
        binary_data: embedded_data,
    })
}

/// Build a Python executable using a temporary Rust project.
///
/// Returns the binary data constituting the built executable.
pub fn build_python_executable(
    logger: &slog::Logger,
    bin_name: &str,
    exe: &dyn PythonBinaryBuilder,
    target: &str,
    opt_level: &str,
    release: bool,
) -> Result<BuiltExecutable> {
    let env = crate::environment::resolve_environment()?;
    let pyembed_location = env.as_pyembed_location();

    let temp_dir = tempdir::TempDir::new("pyoxidizer")?;

    // Directory needs to have name of project.
    let project_path = temp_dir.path().join(bin_name);
    let build_path = temp_dir.path().join("build");
    let artifacts_path = temp_dir.path().join("artifacts");

    initialize_project(&project_path, &pyembed_location, None, &[])?;

    let mut build = build_executable_with_rust_project(
        logger,
        &project_path,
        bin_name,
        exe,
        &build_path,
        &artifacts_path,
        target,
        opt_level,
        release,
    )?;

    // Blank out the path since it is in the temporary directory.
    build.exe_path = None;

    Ok(build)
}

/// Build artifacts needed by the pyembed crate.
///
/// This will resolve `resolve_target` or the default then build it. Built
/// artifacts (if any) are written to `artifacts_path`.
pub fn build_pyembed_artifacts(
    logger: &slog::Logger,
    config_path: &Path,
    artifacts_path: &Path,
    resolve_target: Option<&str>,
    target_triple: &str,
    release: bool,
    verbose: bool,
) -> Result<()> {
    create_dir_all(artifacts_path)?;

    let artifacts_path = canonicalize_path(artifacts_path)?;

    if artifacts_current(logger, config_path, &artifacts_path) {
        return Ok(());
    }

    let mut res: EvalResult = eval_starlark_config_file(
        logger,
        config_path,
        target_triple,
        release,
        verbose,
        if let Some(target) = resolve_target {
            Some(vec![target.to_string()])
        } else {
            None
        },
        true,
    )?;

    // TODO should we honor only the specified target if one is given?
    for target in res.context.targets_to_resolve() {
        let resolved: ResolvedTarget = res.context.build_resolved_target(&target)?;

        let cargo_metadata = resolved.output_path.join("cargo_metadata.txt");

        if !cargo_metadata.exists() {
            continue;
        }

        for p in std::fs::read_dir(&resolved.output_path).context(format!(
            "reading directory {}",
            &resolved.output_path.display()
        ))? {
            let p = p?;

            let dest_path = artifacts_path.join(p.file_name());
            std::fs::copy(&p.path(), &dest_path).context(format!(
                "copying {} to {}",
                p.path().display(),
                dest_path.display()
            ))?;
        }

        // TODO should we normalize paths to pyoxidizer build directory in cargo_metadata.txt
        // with the new artifacts directory?

        return Ok(());
    }

    Err(anyhow!("unable to find generated cargo_metadata.txt; did you specify the correct target to resolve?"))
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
///
/// Receives a logger for receiving log messages, the path to the Rust
/// build script invoking us, and an optional named target in the config
/// file to resolve.
///
/// For this to work as expected, the target resolved in the config file must
/// return a `PythonEmbeddeResources` starlark type.
pub fn run_from_build(
    logger: &slog::Logger,
    build_script: &str,
    resolve_target: Option<&str>,
) -> Result<()> {
    // Adding our our rerun-if-changed lines will overwrite the default, so
    // we need to emit the build script name explicitly.
    println!("cargo:rerun-if-changed={}", build_script);

    println!("cargo:rerun-if-env-changed=PYOXIDIZER_CONFIG");

    // TODO use these variables?
    //let host = env::var("HOST").expect("HOST not defined");
    let target = env::var("TARGET").context("TARGET")?;
    //let opt_level = env::var("OPT_LEVEL").expect("OPT_LEVEL not defined");
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").context("CARGO_MANIFEST_DIR")?;
    let profile = env::var("PROFILE").context("PROFILE")?;

    //let project_path = PathBuf::from(&manifest_dir);

    let config_path = match find_pyoxidizer_config_file_env(logger, &PathBuf::from(manifest_dir)) {
        Some(v) => v,
        None => panic!("Could not find PyOxidizer config file"),
    };

    if !config_path.exists() {
        panic!("PyOxidizer config file does not exist");
    }

    println!("cargo:rerun-if-changed={}", config_path.display());

    let dest_dir = match env::var("PYOXIDIZER_ARTIFACT_DIR") {
        Ok(ref v) => PathBuf::from(v),
        Err(_) => PathBuf::from(env::var("OUT_DIR").context("OUT_DIR")?),
    };

    build_pyembed_artifacts(
        logger,
        &config_path,
        &dest_dir,
        resolve_target,
        &target,
        profile == "release",
        false,
    )?;

    let cargo_metadata = dest_dir.join("cargo_metadata.txt");

    let content =
        std::fs::read(&cargo_metadata).context(format!("reading {}", cargo_metadata.display()))?;
    let content = String::from_utf8(content).context("converting cargo_metadata.txt to string")?;
    println!("{}", content);

    Ok(())
}

fn dependency_current(
    logger: &slog::Logger,
    path: &Path,
    built_time: std::time::SystemTime,
) -> bool {
    match path.metadata() {
        Ok(md) => match md.modified() {
            Ok(t) => {
                if t > built_time {
                    warn!(
                        logger,
                        "building artifacts because {} changed",
                        path.display()
                    );
                    false
                } else {
                    true
                }
            }
            Err(_) => {
                warn!(logger, "error resolving mtime of {}", path.display());
                false
            }
        },
        Err(_) => {
            warn!(logger, "error resolving metadata of {}", path.display());
            false
        }
    }
}

/// Determines whether PyOxidizer artifacts are current.
fn artifacts_current(logger: &slog::Logger, config_path: &Path, artifacts_path: &Path) -> bool {
    let metadata_path = artifacts_path.join("cargo_metadata.txt");

    if !metadata_path.exists() {
        warn!(logger, "no existing PyOxidizer artifacts found");
        return false;
    }

    // We assume the mtime of the metadata file is the built time. If we
    // encounter any modified times newer than that file, we're not up to date.
    let built_time = match metadata_path.metadata() {
        Ok(md) => match md.modified() {
            Ok(t) => t,
            Err(_) => {
                warn!(
                    logger,
                    "error determining mtime of {}",
                    metadata_path.display()
                );
                return false;
            }
        },
        Err(_) => {
            warn!(
                logger,
                "error resolving metadata of {}",
                metadata_path.display()
            );
            return false;
        }
    };

    let metadata_data = match std::fs::read_to_string(&metadata_path) {
        Ok(data) => data,
        Err(_) => {
            warn!(logger, "error reading {}", metadata_path.display());
            return false;
        }
    };

    for line in metadata_data.split('\n') {
        if line.starts_with("cargo:rerun-if-changed=") {
            let path = PathBuf::from(&line[23..line.len()]);

            if !dependency_current(logger, &path, built_time) {
                return false;
            }
        }
    }

    let current_exe = std::env::current_exe().expect("unable to determine current exe");
    if !dependency_current(logger, &current_exe, built_time) {
        return false;
    }

    if !dependency_current(logger, config_path, built_time) {
        return false;
    }

    // TODO detect config file change.
    true
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::py_packaging::standalone_distribution::tests::get_standalone_executable_builder,
        crate::testutil::*,
    };

    #[test]
    fn test_empty_project() -> Result<()> {
        let logger = get_logger()?;
        let pre_built = get_standalone_executable_builder(&logger)?;

        build_python_executable(&logger, "myapp", &pre_built, env!("HOST"), "0", false)?;

        Ok(())
    }
}
