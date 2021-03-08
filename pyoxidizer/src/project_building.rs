// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{
        environment::{canonicalize_path, MINIMUM_RUST_VERSION},
        project_layout::initialize_project,
        py_packaging::{
            binary::{EmbeddedPythonContext, LibpythonLinkMode, PythonBinaryBuilder},
            distribution::AppleSdkInfo,
        },
        starlark::eval::{EvaluationContext, EvaluationContextBuilder},
    },
    anyhow::{anyhow, Context, Result},
    duct::cmd,
    semver::Version,
    slog::{info, warn},
    starlark_dialect_build_targets::ResolvedTarget,
    std::{
        collections::HashMap,
        convert::TryInto,
        env,
        fs::create_dir_all,
        io::{BufRead, BufReader},
        path::{Path, PathBuf},
    },
    tugger_apple::{find_command_line_tools_sdks, find_default_developer_sdks, AppleSdk},
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

/// Resolve an appropriate Apple SDK to use.
pub fn resolve_apple_sdk(
    logger: &slog::Logger,
    platform: &str,
    minimum_version: &str,
    deployment_target: &str,
) -> Result<AppleSdk> {
    if minimum_version.split('.').count() != 2 {
        return Err(anyhow!(
            "expected X.Y minimum Apple SDK version; got {}",
            minimum_version
        ));
    }

    let minimum_semver = Version::parse(&format!("{}.0", minimum_version))?;

    let mut sdks = find_default_developer_sdks()
        .context("discovering Apple SDKs (default developer directory)")?;
    if let Some(extra_sdks) =
        find_command_line_tools_sdks().context("discovering Apple SDKs (command line tools)")?
    {
        sdks.extend(extra_sdks);
    }

    let target_sdks = sdks
        .iter()
        .filter(|sdk| !sdk.is_symlink && sdk.supported_targets.contains_key(platform))
        .collect::<Vec<_>>();

    info!(
        logger,
        "found {} total Apple SDKs; {} support {}",
        sdks.len(),
        target_sdks.len(),
        platform,
    );

    let mut candidate_sdks = target_sdks
        .into_iter()
        .filter(|sdk| {
            let version = match sdk.version_as_semver() {
                Ok(v) => v,
                Err(_) => return false,
            };

            if version < minimum_semver {
                info!(
                    logger,
                    "ignoring SDK {} because it is too old ({} < {})",
                    sdk.path.display(),
                    sdk.version,
                    minimum_version
                );

                false
            } else if !sdk
                .supported_targets
                .get(platform)
                // Safe because key was validated above.
                .unwrap()
                .valid_deployment_targets
                .contains(&deployment_target.to_string())
            {
                info!(
                    logger,
                    "ignoring SDK {} because it doesn't support deployment target {}",
                    sdk.path.display(),
                    deployment_target
                );

                false
            } else {
                true
            }
        })
        .collect::<Vec<_>>();
    candidate_sdks.sort_by(|a, b| {
        b.version_as_semver()
            .unwrap()
            .cmp(&a.version_as_semver().unwrap())
    });

    if candidate_sdks.is_empty() {
        Err(anyhow!(
            "unable to find suitable Apple SDK supporting {}{} or newer",
            platform,
            minimum_version
        ))
    } else {
        info!(
            logger,
            "found {} suitable Apple SDKs ({})",
            candidate_sdks.len(),
            candidate_sdks
                .iter()
                .map(|sdk| sdk.name.clone())
                .collect::<Vec<_>>()
                .join(" ")
        );

        Ok(candidate_sdks[0].clone())
    }
}

/// Describes an environment and settings used to build a project.
pub struct BuildEnvironment {
    /// Path to cargo executable to run.
    pub cargo_exe: String,

    /// Version of Rust being used.
    pub rust_version: Version,

    /// Environment variables to use in build processes.
    ///
    /// This contains a copy of environment variables that were present at
    /// object creation time, it isn't just a supplemental list.
    pub environment_vars: HashMap<String, String>,
}

impl BuildEnvironment {
    /// Construct a new build environment performing validation of requirements.
    pub fn new(
        logger: &slog::Logger,
        target_triple: &str,
        artifacts_path: &Path,
        target_python_path: &Path,
        libpython_link_mode: LibpythonLinkMode,
        libpython_filename: Option<&Path>,
        apple_sdk_info: Option<&AppleSdkInfo>,
    ) -> Result<Self> {
        let rust_version = rustc_version::version()?;
        if rust_version.lt(&MINIMUM_RUST_VERSION) {
            return Err(anyhow!(
                "PyOxidizer requires Rust {}; version {} found",
                *MINIMUM_RUST_VERSION,
                rust_version
            ));
        }

        let mut envs = std::env::vars().collect::<HashMap<_, _>>();

        // Tells any invoked pyoxidizer process where to write build artifacts.
        envs.insert(
            "PYOXIDIZER_ARTIFACT_DIR".to_string(),
            artifacts_path.display().to_string(),
        );

        // Tells any invoked pyoxidizer process to reuse artifacts if they are up to date.
        envs.insert("PYOXIDIZER_REUSE_ARTIFACTS".to_string(), "1".to_string());

        // Set PYTHON_SYS_EXECUTABLE so python3-sys uses our distribution's Python to configure
        // itself.
        // TODO the build environment requiring use of target arch executable prevents
        // cross-compiling. We should be able to pass in all state without having to
        // run an executable in a build script.
        envs.insert(
            "PYTHON_SYS_EXECUTABLE".to_string(),
            target_python_path.display().to_string(),
        );

        let mut rust_flags = vec![];

        // If linking against an existing dynamic library on Windows, add the path to that
        // library so the linker can find it.
        if let Some(libpython_filename) = libpython_filename {
            if target_triple.contains("-windows-") {
                let libpython_dir = libpython_filename
                    .parent()
                    .ok_or_else(|| anyhow!("unable to find parent directory of python DLL"))?;

                rust_flags.push(format!("-L{}", libpython_dir.display()));
            }
        }

        // static-nobundle link kind requires nightly Rust compiler until
        // https://github.com/rust-lang/rust/issues/37403 is resolved.
        if target_triple.contains("-windows-") {
            envs.insert("RUSTC_BOOTSTRAP".to_string(), "1".to_string());
        }

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

            let platform = &sdk_info.platform;
            let minimum_version = &sdk_info.version;
            let deployment_target = &sdk_info.deployment_target;

            // Respect the SDKROOT environment variable.
            let sdk = if let Some(sdk_root) = envs.get("SDKROOT") {
                warn!(logger, "SDKROOT defined; using Apple SDK at {}", sdk_root);
                AppleSdk::from_directory(&PathBuf::from(sdk_root)).with_context(|| {
                    format!("resolving SDK at {} as defined via SDKROOT", sdk_root)
                })?
            } else {
                warn!(
                    logger,
                    "locating Apple SDK {}{}+ supporting {}{}",
                    platform,
                    minimum_version,
                    platform,
                    deployment_target
                );

                resolve_apple_sdk(logger, platform, minimum_version, deployment_target)
                    .context("resolving Apple SDK")?
            };

            warn!(
                logger,
                "using SDK {} ({} targeting {}{})",
                sdk.path.display(),
                sdk.name,
                platform,
                deployment_target
            );

            let deployment_target_name = sdk.supported_targets.get(platform).ok_or_else(|| {
                anyhow!("could not find settings for target {} (this shouldn't happen)", platform)
            })?.deployment_target_setting_name.as_ref().ok_or_else(|| {
                anyhow!("unable to identify deployment target environment variable for {} (please report this bug)", platform)
            })?;

            // SDKROOT will instruct rustc and potentially other tools to use exactly this SDK.
            envs.insert("SDKROOT".to_string(), sdk.path.display().to_string());

            // This (e.g. MACOSX_DEPLOYMENT_TARGET) will instruct compilers to target a specific
            // minimum version of the target platform. We respect an explicit value if one
            // is given.
            if envs.get(deployment_target_name).is_none() {
                envs.insert(
                    deployment_target_name.to_string(),
                    deployment_target.to_string(),
                );
            }
        }

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

        Ok(Self {
            cargo_exe: "cargo".to_string(),
            rust_version,
            environment_vars: envs,
        })
    }
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
    logger: &slog::Logger,
    project_path: &Path,
    bin_name: &str,
    exe: &'a (dyn PythonBinaryBuilder + 'a),
    build_path: &Path,
    artifacts_path: &Path,
    target: &str,
    opt_level: &str,
    release: bool,
) -> Result<BuiltExecutable<'a>> {
    create_dir_all(&artifacts_path)
        .with_context(|| "creating directory for PyOxidizer build artifacts")?;

    // Derive and write the artifacts needed to build a binary embedding Python.
    let embedded_data = exe.to_embedded_python_context(logger, opt_level)?;
    embedded_data.write_files(&artifacts_path)?;

    let build_env = BuildEnvironment::new(
        logger,
        exe.target_triple(),
        artifacts_path,
        exe.target_python_exe_path(),
        exe.libpython_link_mode(),
        embedded_data.linking_info.libpython_filename.as_deref(),
        exe.apple_sdk_info(),
    )
    .context("resolving build environment")?;

    warn!(logger, "building with Rust {}", build_env.rust_version);

    let target_base_path = build_path.join("target");
    let target_triple_base_path =
        target_base_path
            .join(target)
            .join(if release { "release" } else { "debug" });

    let mut args = vec!["build", "--target", target];

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
        features.push("global-allocator-jemalloc");
        features.push("allocator-jemalloc");
    }
    if exe.requires_mimalloc() {
        features.push("global-allocator-mimalloc");
        features.push("allocator-mimalloc");
    }
    if exe.requires_snmalloc() {
        features.push("global-allocator-snmalloc");
        features.push("allocator-snmalloc");
    }

    let features = features.join(" ");

    if !features.is_empty() {
        args.push("--features");
        args.push(&features);
    }

    // TODO force cargo to colorize output under certain circumstances?
    let command = cmd(build_env.cargo_exe, &args)
        .dir(&project_path)
        .full_env(&build_env.environment_vars)
        .stderr_to_stdout()
        .reader()
        .context("invoking cargo command")?;
    {
        let reader = BufReader::new(&command);
        for line in reader.lines() {
            warn!(logger, "{}", line?);
        }
    }
    let output = command
        .try_wait()?
        .ok_or_else(|| anyhow!("unable to wait on command"))?;
    if !output.status.success() {
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
pub fn build_python_executable<'a>(
    logger: &slog::Logger,
    bin_name: &str,
    exe: &'a (dyn PythonBinaryBuilder + 'a),
    target: &str,
    opt_level: &str,
    release: bool,
) -> Result<BuiltExecutable<'a>> {
    let env = crate::environment::resolve_environment()?;
    let pyembed_location = env.as_pyembed_location();

    let temp_dir = tempfile::Builder::new().prefix("pyoxidizer").tempdir()?;

    // Directory needs to have name of project.
    let project_path = temp_dir.path().join(bin_name);
    let build_path = temp_dir.path().join("build");
    let artifacts_path = temp_dir.path().join("artifacts");

    initialize_project(
        &project_path,
        &pyembed_location,
        None,
        &[],
        exe.windows_subsystem(),
    )?;

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

    let mut context: EvaluationContext = EvaluationContextBuilder::new(
        logger.clone(),
        config_path.to_path_buf(),
        target_triple.to_string(),
    )
    .release(release)
    .verbose(verbose)
    .resolve_target_optional(resolve_target)
    .build_script_mode(true)
    .try_into()?;

    context.evaluate_file(config_path)?;

    // TODO should we honor only the specified target if one is given?
    for target in context.targets_to_resolve()? {
        let resolved: ResolvedTarget = context.build_resolved_target(&target)?;

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
        crate::{
            py_packaging::standalone_builder::tests::StandalonePythonExecutableBuilderOptions,
            testutil::*,
        },
        python_packaging::interpreter::MemoryAllocatorBackend,
    };

    #[cfg(target_env = "msvc")]
    use crate::py_packaging::distribution::DistributionFlavor;

    #[test]
    fn test_empty_project() -> Result<()> {
        let logger = get_logger()?;
        let options = StandalonePythonExecutableBuilderOptions::default();
        let pre_built = options.new_builder()?;

        build_python_executable(
            &logger,
            "myapp",
            pre_built.as_ref(),
            env!("HOST"),
            "0",
            false,
        )?;

        Ok(())
    }

    // Skip on aarch64-apple-darwin because we don't have 3.8 builds.
    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    #[test]
    fn test_empty_project_python_38() -> Result<()> {
        let logger = get_logger()?;
        let options = StandalonePythonExecutableBuilderOptions {
            distribution_version: Some("3.8".to_string()),
            ..Default::default()
        };
        let pre_built = options.new_builder()?;

        build_python_executable(
            &logger,
            "myapp",
            pre_built.as_ref(),
            env!("HOST"),
            "0",
            false,
        )?;

        Ok(())
    }

    #[test]
    #[cfg(target_env = "msvc")]
    fn test_empty_project_standalone_static() -> Result<()> {
        let logger = get_logger()?;
        let options = StandalonePythonExecutableBuilderOptions {
            distribution_flavor: DistributionFlavor::StandaloneStatic,
            ..Default::default()
        };
        let pre_built = options.new_builder()?;

        build_python_executable(
            &logger,
            "myapp",
            pre_built.as_ref(),
            env!("HOST"),
            "0",
            false,
        )?;

        Ok(())
    }

    #[test]
    #[cfg(target_env = "msvc")]
    fn test_empty_project_standalone_static_38() -> Result<()> {
        let logger = get_logger()?;
        let options = StandalonePythonExecutableBuilderOptions {
            distribution_version: Some("3.8".to_string()),
            distribution_flavor: DistributionFlavor::StandaloneStatic,
            ..Default::default()
        };
        let pre_built = options.new_builder()?;

        build_python_executable(
            &logger,
            "myapp",
            pre_built.as_ref(),
            env!("HOST"),
            "0",
            false,
        )?;

        Ok(())
    }

    #[test]
    // Not supported on Windows.
    #[cfg(not(target_env = "msvc"))]
    fn test_allocator_jemalloc() -> Result<()> {
        let logger = get_logger()?;

        let mut options = StandalonePythonExecutableBuilderOptions::default();
        options.config.allocator_backend = MemoryAllocatorBackend::Jemalloc;

        let pre_built = options.new_builder()?;

        build_python_executable(
            &logger,
            "myapp",
            pre_built.as_ref(),
            env!("HOST"),
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

        let logger = get_logger()?;

        let mut options = StandalonePythonExecutableBuilderOptions::default();
        options.config.allocator_backend = MemoryAllocatorBackend::Mimalloc;

        let pre_built = options.new_builder()?;

        build_python_executable(
            &logger,
            "myapp",
            pre_built.as_ref(),
            env!("HOST"),
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

        let logger = get_logger()?;

        let mut options = StandalonePythonExecutableBuilderOptions::default();
        options.config.allocator_backend = MemoryAllocatorBackend::Snmalloc;

        let pre_built = options.new_builder()?;

        build_python_executable(
            &logger,
            "myapp",
            pre_built.as_ref(),
            env!("HOST"),
            "0",
            false,
        )?;

        Ok(())
    }
}
