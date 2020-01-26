// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Manage PyOxidizer projects.

use {
    super::environment::canonicalize_path,
    crate::app_packaging::config::{eval_starlark_config_file, find_pyoxidizer_config_file_env},
    crate::project_building::run_from_build,
    crate::project_layout::{initialize_project, write_new_pyoxidizer_config_file},
    crate::py_packaging::distribution::analyze_python_distribution_tar_zst,
    crate::starlark::eval::EvalResult,
    anyhow::{anyhow, Result},
    slog::warn,
    std::fs::create_dir_all,
    std::io::{Cursor, Read},
    std::path::{Path, PathBuf},
};

/// Attempt to resolve the default Rust target for a build.
pub fn default_target() -> Result<String> {
    // TODO derive these more intelligently.
    if cfg!(target_os = "linux") {
        Ok("x86_64-unknown-linux-gnu".to_string())
    } else if cfg!(target_os = "windows") {
        Ok("x86_64-pc-windows-msvc".to_string())
    } else if cfg!(target_os = "macos") {
        Ok("x86_64-apple-darwin".to_string())
    } else {
        Err(anyhow!("unable to resolve target"))
    }
}

pub fn resolve_target(target: Option<&str>) -> Result<String> {
    if let Some(s) = target {
        Ok(s.to_string())
    } else {
        default_target()
    }
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

pub fn list_targets(logger: &slog::Logger, project_path: &Path) -> Result<()> {
    let config_path = find_pyoxidizer_config_file_env(logger, project_path).ok_or_else(|| {
        anyhow!(
            "unable to find PyOxidizder config file at {}",
            project_path.display()
        )
    })?;

    let target_triple = default_target()?;
    let res = eval_starlark_config_file(
        logger,
        &config_path,
        &target_triple,
        false,
        false,
        None,
        Some(Vec::new()),
    )?;

    if res.context.default_target.is_none() {
        println!("(no targets defined)");
        return Ok(());
    }

    for target in res.context.targets.keys() {
        let prefix = if Some(target.clone()) == res.context.default_target {
            "*"
        } else {
            ""
        };
        println!("{}{}", prefix, target);
    }

    Ok(())
}

/// Build PyOxidizer artifacts for a project.
fn build_pyoxidizer_artifacts(
    logger: &slog::Logger,
    config_path: &Path,
    artifacts_path: &Path,
    target_triple: &str,
    release: bool,
    verbose: bool,
) -> Result<()> {
    create_dir_all(artifacts_path)?;

    let artifacts_path = canonicalize_path(artifacts_path)?;

    if !artifacts_current(logger, config_path, &artifacts_path) {
        eval_starlark_config_file(
            logger,
            config_path,
            target_triple,
            release,
            verbose,
            Some(&artifacts_path),
            Some(Vec::new()),
        )?;
    }

    Ok(())
}

/// Build a PyOxidizer enabled project.
///
/// This is a glorified wrapper around `cargo build`. Our goal is to get the
/// output from repackaging to give the user something for debugging.
pub fn build(
    logger: &slog::Logger,
    project_path: &Path,
    target_triple: Option<&str>,
    resolve_targets: Option<Vec<String>>,
    release: bool,
    verbose: bool,
) -> Result<()> {
    let config_path = find_pyoxidizer_config_file_env(logger, project_path).ok_or_else(|| {
        anyhow!(
            "unable to find PyOxidizer config file at {}",
            project_path.display()
        )
    })?;
    let target_triple = resolve_target(target_triple)?;

    let mut res: EvalResult = eval_starlark_config_file(
        logger,
        &config_path,
        &target_triple,
        release,
        verbose,
        None,
        resolve_targets,
    )?;

    for target in res.context.targets_to_resolve() {
        res.context.build_resolved_target(&target)?;
    }

    Ok(())
}

pub fn build_artifacts(
    logger: &slog::Logger,
    project_path: &Path,
    dest_path: &Path,
    release: bool,
    verbose: bool,
) -> Result<()> {
    let target = default_target()?;

    let config_path = match find_pyoxidizer_config_file_env(logger, project_path) {
        Some(p) => p,
        None => return Err(anyhow!("could not find PyOxidizer config file")),
    };

    build_pyoxidizer_artifacts(logger, &config_path, dest_path, &target, release, verbose)?;

    Ok(())
}

pub fn run(
    logger: &slog::Logger,
    project_path: &Path,
    target_triple: Option<&str>,
    release: bool,
    target: Option<&str>,
    _extra_args: &[&str],
    verbose: bool,
) -> Result<()> {
    let config_path = find_pyoxidizer_config_file_env(logger, project_path).ok_or_else(|| {
        anyhow!(
            "unable to find PyOxidizer config file at {}",
            project_path.display()
        )
    })?;
    let target_triple = resolve_target(target_triple)?;

    let resolve_targets = if let Some(target) = target {
        Some(vec![target.to_string()])
    } else {
        None
    };

    let mut res: EvalResult = eval_starlark_config_file(
        logger,
        &config_path,
        &target_triple,
        release,
        verbose,
        None,
        resolve_targets,
    )?;

    res.context.run_target(target)
}

/// Initialize a PyOxidizer configuration file in a given directory.
pub fn init_config_file(
    project_dir: &Path,
    code: Option<&str>,
    pip_install: &[&str],
) -> Result<()> {
    if project_dir.exists() && !project_dir.is_dir() {
        return Err(anyhow!(
            "existing path must be a directory: {}",
            project_dir.display()
        ));
    }

    if !project_dir.exists() {
        create_dir_all(project_dir)?;
    }

    let name = project_dir.iter().last().unwrap().to_str().unwrap();

    write_new_pyoxidizer_config_file(project_dir, name, code, pip_install)?;

    println!();
    println!("A new PyOxidizer configuration file has been created.");
    println!("This configuration file can be used by various `pyoxidizer`");
    println!("commands");
    println!();
    println!("For example, to build and run the default Python application:");
    println!();
    println!("  $ cd {}", project_dir.display());
    println!("  $ pyoxidizer run");
    println!();
    println!("The default configuration is to invoke a Python REPL. You can");
    println!("edit the configuration file to change behavior.");

    Ok(())
}

/// Initialize a new Rust project with PyOxidizer support.
pub fn init_rust_project(project_path: &Path) -> Result<()> {
    initialize_project(project_path, None, &[])?;
    println!();
    println!(
        "A new Rust binary application has been created in {}",
        project_path.display()
    );
    println!();
    println!("This application can be built by doing the following:");
    println!();
    println!("  $ cd {}", project_path.display());
    println!("  $ pyoxidizer build");
    println!("  $ pyoxidizer run");
    println!();
    println!("The default configuration is to invoke a Python REPL. You can");
    println!("edit the various pyoxidizer.*.bzl config files or the main.rs ");
    println!("file to change behavior. The application will need to be rebuilt ");
    println!("for configuration changes to take effect.");

    Ok(())
}

pub fn python_distribution_extract(dist_path: &str, dest_path: &str) -> Result<()> {
    let mut fh = std::fs::File::open(Path::new(dist_path))?;
    let mut data = Vec::new();
    fh.read_to_end(&mut data)?;
    let cursor = Cursor::new(data);
    let dctx = zstd::stream::Decoder::new(cursor)?;
    let mut tf = tar::Archive::new(dctx);

    println!("extracting archive to {}", dest_path);
    tf.unpack(dest_path)?;

    Ok(())
}

pub fn python_distribution_info(dist_path: &str) -> Result<()> {
    let mut fh = std::fs::File::open(Path::new(dist_path))?;
    let mut data = Vec::new();
    fh.read_to_end(&mut data)?;

    let temp_dir = tempdir::TempDir::new("python-distribution")?;
    let temp_dir_path = temp_dir.path();

    let cursor = Cursor::new(data);
    let dist = analyze_python_distribution_tar_zst(cursor, temp_dir_path)?;

    println!("High-Level Metadata");
    println!("===================");
    println!();
    println!("Flavor:       {}", dist.flavor);
    println!("Version:      {}", dist.version);
    println!("OS:           {}", dist.os);
    println!("Architecture: {}", dist.arch);
    println!();

    println!("Extension Modules");
    println!("=================");
    for (name, ems) in dist.extension_modules {
        println!("{}", name);
        println!("{}", "-".repeat(name.len()));
        println!();

        for em in ems {
            println!("{}", em.variant);
            println!("{}", "^".repeat(em.variant.len()));
            println!();
            println!("Required: {}", em.required);
            println!("Built-in Default: {}", em.builtin_default);
            if let Some(licenses) = em.licenses {
                println!("Licenses: {}", licenses.join(", "));
            }
            if !em.links.is_empty() {
                println!(
                    "Links: {}",
                    em.links
                        .iter()
                        .map(|l| l.name.clone())
                        .collect::<Vec<String>>()
                        .join(", ")
                );
            }

            println!();
        }
    }

    println!("Python Modules");
    println!("==============");
    println!();
    for name in dist.py_modules.keys() {
        println!("{}", name);
    }
    println!();

    println!("Python Resources");
    println!("================");
    println!();

    for (package, resources) in dist.resources {
        for name in resources.keys() {
            println!("[{}].{}", package, name);
        }
    }

    Ok(())
}

pub fn python_distribution_licenses(path: &str) -> Result<()> {
    let mut fh = std::fs::File::open(Path::new(path))?;
    let mut data = Vec::new();
    fh.read_to_end(&mut data)?;

    let temp_dir = tempdir::TempDir::new("python-distribution")?;
    let temp_dir_path = temp_dir.path();

    let cursor = Cursor::new(data);
    let dist = analyze_python_distribution_tar_zst(cursor, temp_dir_path)?;

    println!(
        "Python Distribution Licenses: {}",
        match dist.licenses {
            Some(licenses) => itertools::join(licenses, ", "),
            None => "NO LICENSE FOUND".to_string(),
        }
    );
    println!();
    println!("Extension Libraries and License Requirements");
    println!("============================================");
    println!();

    for (name, variants) in &dist.extension_modules {
        for variant in variants {
            if variant.links.is_empty() {
                continue;
            }

            let name = if variant.variant == "default" {
                name.clone()
            } else {
                format!("{} ({})", name, variant.variant)
            };

            println!("{}", name);
            println!("{}", "-".repeat(name.len()));
            println!();

            for link in &variant.links {
                println!("Dependency: {}", &link.name);
                println!(
                    "Link Type: {}",
                    if link.system {
                        "system"
                    } else if link.framework {
                        "framework"
                    } else {
                        "library"
                    }
                );

                println!();
            }

            if variant.license_public_domain.is_some() && variant.license_public_domain.unwrap() {
                println!("Licenses: Public Domain");
            } else if let Some(ref licenses) = variant.licenses {
                println!("Licenses: {}", itertools::join(licenses, ", "));
                for license in licenses {
                    println!("License Info: https://spdx.org/licenses/{}.html", license);
                }
            } else {
                println!("Licenses: UNKNOWN");
            }

            println!();
        }
    }

    Ok(())
}

/// Run a build from the context of a Rust build script.
pub fn run_build_script(
    logger: &slog::Logger,
    build_script: &str,
    resolve_target: Option<&str>,
) -> Result<()> {
    run_from_build(logger, build_script, resolve_target);

    Ok(())
}
