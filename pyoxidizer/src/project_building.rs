// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{
        environment::{canonicalize_path, Environment, RustEnvironment},
        licensing::{licenses_from_cargo_manifest, log_licensing_info},
        project_layout::initialize_project,
        py_packaging::{
            binary::{LibpythonLinkMode, PythonBinaryBuilder},
            distribution::AppleSdkInfo,
            embedding::{EmbeddedPythonContext, DEFAULT_PYTHON_CONFIG_FILENAME},
        },
        starlark::eval::{EvaluationContext, EvaluationContextBuilder},
    },
    anyhow::{anyhow, Context, Result},
    apple_sdk::AppleSdk,
    duct::cmd,
    log::warn,
    starlark_dialect_build_targets::ResolvedTarget,
    std::{
        collections::HashMap,
        fs::create_dir_all,
        io::{BufRead, BufReader},
        path::{Path, PathBuf},
    },
};

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
pub fn find_pyoxidizer_config_file_env(start_dir: &Path) -> Option<PathBuf> {
    if let Ok(path) = std::env::var("PYOXIDIZER_CONFIG") {
        warn!(
            "using PyOxidizer config file from PYOXIDIZER_CONFIG: {}",
            path
        );
        return Some(PathBuf::from(path));
    }

    if let Ok(path) = std::env::var("OUT_DIR") {
        warn!("looking for config file in ancestry of {}", path);
        let res = find_pyoxidizer_config_file(Path::new(&path));
        if res.is_some() {
            return res;
        }
    }

    find_pyoxidizer_config_file(start_dir)
}

/// Describes an environment and settings used to build a project.
pub struct BuildEnvironment {
    /// Describes the Rust toolchain we're using.
    pub rust_environment: RustEnvironment,

    /// Environment variables to use in build processes.
    ///
    /// This contains a copy of environment variables that were present at
    /// object creation time, it isn't just a supplemental list.
    pub environment_vars: HashMap<String, String>,
}

impl BuildEnvironment {
    /// Construct a new build environment performing validation of requirements.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        env: &Environment,
        target_triple: &str,
        artifacts_path: &Path,
        pyo3_config_path: impl AsRef<Path>,
        libpython_link_mode: LibpythonLinkMode,
        apple_sdk_info: Option<&AppleSdkInfo>,
    ) -> Result<Self> {
        let rust_environment = env
            .ensure_rust_toolchain(Some(target_triple))
            .context("ensuring Rust toolchain available")?;

        let mut envs = std::env::vars().collect::<HashMap<_, _>>();

        // Tells any invoked pyoxidizer process where to write build artifacts.
        envs.insert(
            "PYOXIDIZER_ARTIFACT_DIR".to_string(),
            artifacts_path.display().to_string(),
        );

        // Tells any invoked pyoxidizer process to reuse artifacts if they are up to date.
        envs.insert("PYOXIDIZER_REUSE_ARTIFACTS".to_string(), "1".to_string());

        // Give PyO3 an explicit configuration file to use. This bypasses the dynamic interpreter
        // probing that PyO3's build script normally performs.
        envs.insert(
            "PYO3_CONFIG_FILE".to_string(),
            pyo3_config_path.as_ref().display().to_string(),
        );

        // When targeting Apple platforms and using Apple SDKs, you can very
        // easily run into SDK and toolchain compatibility issues when your
        // local SDK or toolchain is older than the one used to produce the
        // Python distribution. For example, if the macosx10.15 SDK is used to
        // produce the Python distribution and you are using an older version
        // of Clang that can't parse version 4 .tbd files, the linker will fail
        // to find which dylibs contain symbols (because mach-o must encode the
        // name of a dylib containing weakly linked symbols) and you'll get a
        // linker error for unresolved symbols. See
        // https://github.com/indygreg/PyOxidizer/issues/373 for a thorough
        // discussion on this topic.
        //
        // Here, we validate that the local SDK being used is >= the version used
        // by the Python distribution.
        // TODO validate minimum Clang/linker version as well.
        if target_triple.contains("-apple-") {
            let sdk_info = apple_sdk_info.ok_or_else(|| {
                anyhow!("targeting Apple platform but Apple SDK info not available")
            })?;

            let sdk = env
                .resolve_apple_sdk(sdk_info)
                .context("resolving Apple SDK")?;

            let deployment_target_name = sdk.supported_targets.get(&sdk_info.platform).ok_or_else(|| {
                anyhow!("could not find settings for target {} (this shouldn't happen)", &sdk_info.platform)
            })?.deployment_target_setting_name.clone().unwrap_or_else(|| {
                warn!("Apple SDK does not define deployment target name; assuming MACOSX_DEPLOYMENT_TARGET");
                warn!("(If you see this message, the SDK you are attempting to use may be too old and build failures may occur.)");
                "MACOSX_DEPLOYMENT_TARGET".to_string()
            });

            // SDKROOT will instruct rustc and potentially other tools to use exactly this SDK.
            envs.insert("SDKROOT".to_string(), sdk.path().display().to_string());

            // This (e.g. MACOSX_DEPLOYMENT_TARGET) will instruct compilers to target a specific
            // minimum version of the target platform. We respect an explicit value if one
            // is given.
            if envs.get(&deployment_target_name).is_none() {
                envs.insert(deployment_target_name, sdk_info.deployment_target.clone());
            }
        }

        let mut rust_flags = vec![];

        // Windows standalone_static distributions require the non-DLL CRT.
        // This requires telling Rust to use the static CRT.
        //
        // In addition, these distributions also have some symbols defined in
        // multiple object files. See https://github.com/indygreg/python-build-standalone/issues/71.
        // This can lead to a linker error unless we suppress it via /FORCE:MULTIPLE.
        // This workaround is not ideal.
        // TODO remove /FORCE:MULTIPLE once the distributions eliminate duplicate
        // symbols.
        if target_triple.contains("-windows-") && libpython_link_mode == LibpythonLinkMode::Static {
            rust_flags.extend(
                [
                    "-C".to_string(),
                    "target-feature=+crt-static".to_string(),
                    "-C".to_string(),
                    "link-args=/FORCE:MULTIPLE".to_string(),
                ]
                .iter()
                .map(|x| x.to_string()),
            );
        }

        if !rust_flags.is_empty() {
            let extra_flags = rust_flags.join(" ");

            envs.insert(
                "RUSTFLAGS".to_string(),
                if let Some(value) = envs.get("RUSTFLAGS") {
                    format!("{} {}", extra_flags, value)
                } else {
                    extra_flags
                },
            );
        }

        // We want cargo to use the rustc from our resolved Rust environment. So
        // always set RUSTC to force it.
        envs.insert(
            "RUSTC".to_string(),
            format!("{}", rust_environment.rustc_exe.display()),
        );

        Ok(Self {
            rust_environment,
            environment_vars: envs,
        })
    }
}

/// Derive cargo features for project building.
pub fn cargo_features(exe: &dyn PythonBinaryBuilder) -> Vec<&str> {
    let mut res = vec!["build-mode-prebuilt-artifacts"];

    if exe.requires_jemalloc() {
        res.push("global-allocator-jemalloc");
        res.push("allocator-jemalloc");
    }
    if exe.requires_mimalloc() {
        res.push("global-allocator-mimalloc");
        res.push("allocator-mimalloc");
    }
    if exe.requires_snmalloc() {
        res.push("global-allocator-snmalloc");
        res.push("allocator-snmalloc");
    }

    res
}

/// Holds results from building an executable.
pub struct BuiltExecutable<'a> {
    /// Path to built executable file.
    pub exe_path: Option<PathBuf>,

    /// File name of executable.
    pub exe_name: String,

    /// Holds raw content of built executable.
    pub exe_data: Vec<u8>,

    /// Holds state generated from building.
    pub binary_data: EmbeddedPythonContext<'a>,
}

/// Build an executable embedding Python using an existing Rust project.
///
/// The path to the produced executable is returned.
#[allow(clippy::too_many_arguments)]
pub fn build_executable_with_rust_project<'a>(
    env: &Environment,
    project_path: &Path,
    bin_name: &str,
    exe: &'a (dyn PythonBinaryBuilder + 'a),
    build_path: &Path,
    artifacts_path: &Path,
    target_triple: &str,
    opt_level: &str,
    release: bool,
    locked: bool,
    include_self_license: bool,
) -> Result<BuiltExecutable<'a>> {
    create_dir_all(&artifacts_path).context("creating directory for PyOxidizer build artifacts")?;

    // Derive and write the artifacts needed to build a binary embedding Python.
    let mut embedded_data = exe
        .to_embedded_python_context(env, opt_level)
        .context("obtaining embedded python context")?;
    embedded_data
        .write_files(artifacts_path)
        .context("writing embedded python context files")?;

    let build_env = BuildEnvironment::new(
        env,
        exe.target_triple(),
        artifacts_path,
        embedded_data.pyo3_config_path(&artifacts_path),
        exe.libpython_link_mode(),
        exe.apple_sdk_info(),
    )
    .context("resolving build environment")?;

    warn!(
        "building with Rust {}",
        build_env.rust_environment.rust_version.semver
    );

    let target_base_path = build_path.join("target");
    let target_triple_base_path =
        target_base_path
            .join(target_triple)
            .join(if release { "release" } else { "debug" });

    let mut args = vec!["build", "--target", target_triple];

    let target_dir = target_base_path.display().to_string();
    args.push("--target-dir");
    args.push(&target_dir);

    args.push("--bin");
    args.push(bin_name);

    if locked {
        args.push("--locked");
    }

    if release {
        args.push("--release");
    }

    args.push("--no-default-features");

    let features = cargo_features(exe).join(" ");

    if !features.is_empty() {
        args.push("--features");
        args.push(&features);
    }

    // TODO force cargo to colorize output under certain circumstances?
    let command = cmd(&build_env.rust_environment.cargo_exe, &args)
        .dir(&project_path)
        .full_env(&build_env.environment_vars)
        .stderr_to_stdout()
        .reader()
        .context("invoking cargo command")?;
    {
        let reader = BufReader::new(&command);
        for line in reader.lines() {
            warn!("{}", line.context("reading cargo output")?);
        }
    }
    let output = command
        .try_wait()
        .context("waiting on cargo process")?
        .ok_or_else(|| anyhow!("unable to wait on command"))?;
    if !output.status.success() {
        return Err(anyhow!("cargo build failed"));
    }

    let exe_name = if target_triple.contains("pc-windows") {
        format!("{}.exe", bin_name)
    } else {
        bin_name.to_string()
    };

    let exe_path = target_triple_base_path.join(&exe_name);

    if !exe_path.exists() {
        return Err(anyhow!("{} does not exist", exe_path.display()));
    }

    let exe_data =
        std::fs::read(&exe_path).with_context(|| format!("reading {}", exe_path.display()))?;
    let exe_name = exe_path.file_name().unwrap().to_string_lossy().to_string();

    // Construct unified licensing info by combining the Python licensing metadata
    // with the dynamically derived licensing info for Rust crates from the Cargo manifest.
    for component in licenses_from_cargo_manifest(
        project_path.join("Cargo.toml"),
        false,
        cargo_features(exe),
        Some(target_triple),
        Some(&build_env.rust_environment.cargo_exe),
        include_self_license,
    )?
    .into_components()
    {
        embedded_data.add_licensed_component(component)?;
    }

    // Inform user about licensing info.
    log_licensing_info(embedded_data.licensing());

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
pub fn build_python_executable<'a>(
    env: &Environment,
    bin_name: &str,
    exe: &'a (dyn PythonBinaryBuilder + 'a),
    target_triple: &str,
    opt_level: &str,
    release: bool,
) -> Result<BuiltExecutable<'a>> {
    let cargo_exe = env
        .ensure_rust_toolchain(Some(target_triple))
        .context("resolving Rust toolchain")?
        .cargo_exe;

    let temp_dir = tempfile::Builder::new()
        .prefix("pyoxidizer")
        .tempdir()
        .context("creating temp directory")?;

    // Directory needs to have name of project.
    let project_path = temp_dir.path().join(bin_name);
    let build_path = temp_dir.path().join("build");
    let artifacts_path = temp_dir.path().join("artifacts");

    initialize_project(
        &env.pyoxidizer_source,
        &project_path,
        &cargo_exe,
        None,
        &[],
        exe.windows_subsystem(),
    )
    .context("initializing project")?;

    let mut build = build_executable_with_rust_project(
        env,
        &project_path,
        bin_name,
        exe,
        &build_path,
        &artifacts_path,
        target_triple,
        opt_level,
        release,
        // Always build with locked because we crated a Cargo.lock with the
        // Rust project we just created.
        true,
        // Don't include license for self because the Rust project is temporary and its
        // licensing isn't material.
        false,
    )
    .context("building executable with Rust project")?;

    // Blank out the path since it is in the temporary directory.
    build.exe_path = None;

    Ok(build)
}

/// Build artifacts needed by the pyembed crate.
///
/// This will resolve `resolve_target` or the default then build it. Built
/// artifacts (if any) are written to `artifacts_path`.
#[allow(clippy::too_many_arguments)]
pub fn build_pyembed_artifacts(
    env: &Environment,
    config_path: &Path,
    artifacts_path: &Path,
    resolve_target: Option<&str>,
    extra_vars: HashMap<String, Option<String>>,
    target_triple: &str,
    release: bool,
    verbose: bool,
) -> Result<()> {
    create_dir_all(artifacts_path)?;

    let artifacts_path = canonicalize_path(artifacts_path)?;

    if artifacts_current(config_path, &artifacts_path) {
        return Ok(());
    }

    let mut context: EvaluationContext =
        EvaluationContextBuilder::new(env, config_path, target_triple.to_string())
            .extra_vars(extra_vars)
            .release(release)
            .verbose(verbose)
            .resolve_target_optional(resolve_target)
            .build_script_mode(true)
            .try_into()?;

    context.evaluate_file(config_path)?;

    // TODO should we honor only the specified target if one is given?
    for target in context.targets_to_resolve()? {
        let resolved: ResolvedTarget = context.build_resolved_target(&target)?;

        // Presence of the generated default python config file implies this is a valid
        // artifacts directory.
        let default_python_config = resolved.output_path.join(DEFAULT_PYTHON_CONFIG_FILENAME);
        if !default_python_config.exists() {
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

        return Ok(());
    }

    Err(anyhow!(
        "unable to find generated {}; did you specify the correct target to resolve?",
        DEFAULT_PYTHON_CONFIG_FILENAME
    ))
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
    env: &Environment,
    build_script: &str,
    resolve_target: Option<&str>,
    extra_vars: HashMap<String, Option<String>>,
) -> Result<()> {
    // Adding our our rerun-if-changed lines will overwrite the default, so
    // we need to emit the build script name explicitly.
    println!("cargo:rerun-if-changed={}", build_script);

    println!("cargo:rerun-if-env-changed=PYOXIDIZER_CONFIG");

    // TODO use these variables?
    //let host = std::env::var("HOST").expect("HOST not defined");
    let target = std::env::var("TARGET").context("TARGET")?;
    //let opt_level = std::env::var("OPT_LEVEL").expect("OPT_LEVEL not defined");
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").context("CARGO_MANIFEST_DIR")?;
    let profile = std::env::var("PROFILE").context("PROFILE")?;

    //let project_path = PathBuf::from(&manifest_dir);

    let config_path = match find_pyoxidizer_config_file_env(&PathBuf::from(manifest_dir)) {
        Some(v) => v,
        None => panic!("Could not find PyOxidizer config file"),
    };

    if !config_path.exists() {
        panic!("PyOxidizer config file does not exist");
    }

    println!("cargo:rerun-if-changed={}", config_path.display());

    let dest_dir = match std::env::var("PYOXIDIZER_ARTIFACT_DIR") {
        Ok(ref v) => PathBuf::from(v),
        Err(_) => PathBuf::from(std::env::var("OUT_DIR").context("OUT_DIR")?),
    };

    build_pyembed_artifacts(
        env,
        &config_path,
        &dest_dir,
        resolve_target,
        extra_vars,
        &target,
        profile == "release",
        false,
    )?;

    let default_python_config_path = dest_dir.join(DEFAULT_PYTHON_CONFIG_FILENAME);
    println!(
        "cargo:rustc-env=DEFAULT_PYTHON_CONFIG_RS={}",
        default_python_config_path.display()
    );

    Ok(())
}

fn dependency_current(path: &Path, built_time: std::time::SystemTime) -> bool {
    match path.metadata() {
        Ok(md) => match md.modified() {
            Ok(t) => {
                if t > built_time {
                    warn!("building artifacts because {} changed", path.display());
                    false
                } else {
                    true
                }
            }
            Err(_) => {
                warn!("error resolving mtime of {}", path.display());
                false
            }
        },
        Err(_) => {
            warn!("error resolving metadata of {}", path.display());
            false
        }
    }
}

/// Determines whether PyOxidizer artifacts are current.
fn artifacts_current(config_path: &Path, artifacts_path: &Path) -> bool {
    let python_config_path = artifacts_path.join(DEFAULT_PYTHON_CONFIG_FILENAME);

    if !python_config_path.exists() {
        warn!("no existing PyOxidizer artifacts found");
        return false;
    }

    // We assume the mtime of the metadata file is the built time. If we
    // encounter any modified times newer than that file, we're not up to date.
    let built_time = match python_config_path.metadata() {
        Ok(md) => match md.modified() {
            Ok(t) => t,
            Err(_) => {
                warn!(
                    "error determining mtime of {}",
                    python_config_path.display()
                );
                return false;
            }
        },
        Err(_) => {
            warn!(
                "error resolving metadata of {}",
                python_config_path.display()
            );
            return false;
        }
    };

    let current_exe = std::env::current_exe().expect("unable to determine current exe");
    if !dependency_current(&current_exe, built_time) {
        return false;
    }

    if !dependency_current(config_path, built_time) {
        return false;
    }

    // TODO detect config file change.
    true
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::{
            environment::default_target_triple,
            py_packaging::standalone_builder::tests::StandalonePythonExecutableBuilderOptions,
            testutil::*,
        },
        python_packaging::interpreter::MemoryAllocatorBackend,
    };

    #[cfg(target_env = "msvc")]
    use crate::py_packaging::distribution::DistributionFlavor;

    #[test]
    fn test_empty_project() -> Result<()> {
        let env = get_env()?;
        let options = StandalonePythonExecutableBuilderOptions::default();
        let pre_built = options.new_builder()?;

        build_python_executable(
            &env,
            "myapp",
            pre_built.as_ref(),
            default_target_triple(),
            "0",
            false,
        )?;

        Ok(())
    }

    // Skip on aarch64-apple-darwin because we don't have 3.8 builds.
    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    #[test]
    fn test_empty_project_python_38() -> Result<()> {
        let env = get_env()?;
        let options = StandalonePythonExecutableBuilderOptions {
            distribution_version: Some("3.8".to_string()),
            ..Default::default()
        };
        let pre_built = options.new_builder()?;

        build_python_executable(
            &env,
            "myapp",
            pre_built.as_ref(),
            default_target_triple(),
            "0",
            false,
        )?;

        Ok(())
    }

    #[test]
    fn test_empty_project_python_310() -> Result<()> {
        let env = get_env()?;
        let options = StandalonePythonExecutableBuilderOptions {
            distribution_version: Some("3.10".to_string()),
            ..Default::default()
        };
        let pre_built = options.new_builder()?;

        build_python_executable(
            &env,
            "myapp",
            pre_built.as_ref(),
            default_target_triple(),
            "0",
            false,
        )?;

        Ok(())
    }

    #[test]
    fn test_empty_project_system_rust() -> Result<()> {
        let mut env = get_env()?;
        env.unmanage_rust()?;
        let options = StandalonePythonExecutableBuilderOptions::default();
        let pre_built = options.new_builder()?;

        build_python_executable(
            &env,
            "myapp",
            pre_built.as_ref(),
            default_target_triple(),
            "0",
            false,
        )?;

        Ok(())
    }

    #[test]
    #[cfg(target_env = "msvc")]
    fn test_empty_project_standalone_static() -> Result<()> {
        let env = get_env()?;
        let options = StandalonePythonExecutableBuilderOptions {
            distribution_flavor: DistributionFlavor::StandaloneStatic,
            ..Default::default()
        };
        let pre_built = options.new_builder()?;

        build_python_executable(
            &env,
            "myapp",
            pre_built.as_ref(),
            default_target_triple(),
            "0",
            false,
        )?;

        Ok(())
    }

    #[test]
    #[cfg(target_env = "msvc")]
    fn test_empty_project_standalone_static_38() -> Result<()> {
        let env = get_env()?;
        let options = StandalonePythonExecutableBuilderOptions {
            distribution_version: Some("3.8".to_string()),
            distribution_flavor: DistributionFlavor::StandaloneStatic,
            ..Default::default()
        };
        let pre_built = options.new_builder()?;

        build_python_executable(
            &env,
            "myapp",
            pre_built.as_ref(),
            default_target_triple(),
            "0",
            false,
        )?;

        Ok(())
    }

    #[test]
    #[cfg(target_env = "msvc")]
    fn test_empty_project_standalone_static_310() -> Result<()> {
        let env = get_env()?;
        let options = StandalonePythonExecutableBuilderOptions {
            distribution_version: Some("3.10".to_string()),
            distribution_flavor: DistributionFlavor::StandaloneStatic,
            ..Default::default()
        };
        let pre_built = options.new_builder()?;

        build_python_executable(
            &env,
            "myapp",
            pre_built.as_ref(),
            default_target_triple(),
            "0",
            false,
        )?;

        Ok(())
    }

    #[test]
    // Not supported on Windows.
    #[cfg(not(target_env = "msvc"))]
    fn test_allocator_jemalloc() -> Result<()> {
        let env = get_env()?;

        let mut options = StandalonePythonExecutableBuilderOptions::default();
        options.config.allocator_backend = MemoryAllocatorBackend::Jemalloc;

        let pre_built = options.new_builder()?;

        build_python_executable(
            &env,
            "myapp",
            pre_built.as_ref(),
            default_target_triple(),
            "0",
            false,
        )?;

        Ok(())
    }

    #[test]
    fn test_allocator_mimalloc() -> Result<()> {
        // cmake required to build.
        if cfg!(windows) {
            eprintln!("skipping on Windows due to build sensitivity");
            return Ok(());
        }

        let env = get_env()?;

        let mut options = StandalonePythonExecutableBuilderOptions::default();
        options.config.allocator_backend = MemoryAllocatorBackend::Mimalloc;

        let pre_built = options.new_builder()?;

        build_python_executable(
            &env,
            "myapp",
            pre_built.as_ref(),
            default_target_triple(),
            "0",
            false,
        )?;

        Ok(())
    }

    #[test]
    fn test_allocator_snmalloc() -> Result<()> {
        // cmake required to build.
        if cfg!(windows) {
            eprintln!("skipping on Windows due to build sensitivity");
            return Ok(());
        }

        let env = get_env()?;

        let mut options = StandalonePythonExecutableBuilderOptions::default();
        options.config.allocator_backend = MemoryAllocatorBackend::Snmalloc;

        let pre_built = options.new_builder()?;

        build_python_executable(
            &env,
            "myapp",
            pre_built.as_ref(),
            default_target_triple(),
            "0",
            false,
        )?;

        Ok(())
    }
}
