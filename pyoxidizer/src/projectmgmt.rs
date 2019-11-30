// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Manage PyOxidizer projects.

use handlebars::Handlebars;
use lazy_static::lazy_static;
use serde::Serialize;
use slog::warn;
use std::collections::BTreeMap;
use std::error::Error;
use std::fs::create_dir_all;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::process;

use super::distribution::produce_distributions;
use super::environment::{
    canonicalize_path, PyOxidizerSource, BUILD_GIT_COMMIT, MINIMUM_RUST_VERSION, PYOXIDIZER_VERSION,
};
use crate::app_packaging::config::find_pyoxidizer_config_file_env;
use crate::app_packaging::repackage::{package_project, process_config, run_from_build};
use crate::app_packaging::state::BuildContext;
use crate::py_packaging::config::RawAllocator;
use crate::py_packaging::distribution::{analyze_python_distribution_tar_zst, python_exe_path};
use crate::py_packaging::fsscan::walk_tree_files;

lazy_static! {
    static ref PYEMBED_RS_FILES: BTreeMap<&'static str, &'static [u8]> = {
        let mut res: BTreeMap<&'static str, &'static [u8]> = BTreeMap::new();

        res.insert("config.rs", include_bytes!("pyembed/config.rs"));
        res.insert("lib.rs", include_bytes!("pyembed/lib.rs"));
        res.insert("data.rs", include_bytes!("pyembed/data.rs"));
        res.insert("importer.rs", include_bytes!("pyembed/importer.rs"));
        res.insert("osutils.rs", include_bytes!("pyembed/osutils.rs"));
        res.insert("pyalloc.rs", include_bytes!("pyembed/pyalloc.rs"));
        res.insert("pyinterp.rs", include_bytes!("pyembed/pyinterp.rs"));
        res.insert("pystr.rs", include_bytes!("pyembed/pystr.rs"));

        res
    };
    static ref HANDLEBARS: Handlebars = {
        let mut handlebars = Handlebars::new();

        handlebars
            .register_template_string("new-main.rs", include_str!("templates/new-main.rs"))
            .unwrap();
        handlebars
            .register_template_string(
                "new-pyoxidizer.bzl",
                include_str!("templates/new-pyoxidizer.bzl"),
            )
            .unwrap();
        handlebars
            .register_template_string(
                "pyembed-build.rs",
                include_str!("templates/pyembed-build.rs"),
            )
            .unwrap();
        handlebars
            .register_template_string(
                "pyembed-cargo.toml",
                include_str!("templates/pyembed-cargo.toml"),
            )
            .unwrap();

        handlebars
    };
}

/// Attempt to resolve the default Rust target for a build.
pub fn default_target() -> Result<String, String> {
    // TODO derive these more intelligently.
    if cfg!(target_os = "linux") {
        Ok("x86_64-unknown-linux-gnu".to_string())
    } else if cfg!(target_os = "windows") {
        Ok("x86_64-pc-windows-msvc".to_string())
    } else if cfg!(target_os = "macos") {
        Ok("x86_64-apple-darwin".to_string())
    } else {
        Err("unable to resolve target".to_string())
    }
}

/// Find existing PyOxidizer files in a project directory.
pub fn find_pyoxidizer_files(root: &Path) -> Vec<PathBuf> {
    let mut res: Vec<PathBuf> = Vec::new();

    for f in walk_tree_files(&root) {
        let path = f.path().strip_prefix(root).expect("unable to strip prefix");
        let path_s = path.to_str().expect("unable to convert path to str");

        if path_s.contains("pyoxidizer") || path_s.contains("pyembed") {
            res.push(path.to_path_buf());
        }
    }

    res
}

#[derive(Serialize)]
struct PythonDistribution {
    build_target: String,
    url: String,
    sha256: String,
}

#[derive(Serialize)]
struct TemplateData {
    pyoxidizer_version: Option<String>,
    pyoxidizer_commit: Option<String>,
    pyoxidizer_local_repo_path: Option<String>,
    pyoxidizer_git_url: Option<String>,
    pyoxidizer_git_commit: Option<String>,
    pyoxidizer_git_tag: Option<String>,

    python_distributions: Vec<PythonDistribution>,
    program_name: Option<String>,
    code: Option<String>,
    pip_install_simple: Vec<String>,
}

impl TemplateData {
    fn new() -> TemplateData {
        TemplateData {
            pyoxidizer_version: None,
            pyoxidizer_commit: None,
            pyoxidizer_local_repo_path: None,
            pyoxidizer_git_url: None,
            pyoxidizer_git_commit: None,
            pyoxidizer_git_tag: None,
            python_distributions: Vec::new(),
            program_name: None,
            code: None,
            pip_install_simple: Vec::new(),
        }
    }
}

fn populate_template_data(data: &mut TemplateData) {
    let env = super::environment::resolve_environment().unwrap();

    data.pyoxidizer_version = Some(PYOXIDIZER_VERSION.to_string());
    data.pyoxidizer_commit = Some(BUILD_GIT_COMMIT.to_string());

    match env.pyoxidizer_source {
        PyOxidizerSource::LocalPath { path } => {
            data.pyoxidizer_local_repo_path = Some(path.display().to_string());
        }
        PyOxidizerSource::GitUrl { url, commit, tag } => {
            data.pyoxidizer_git_url = Some(url);

            if let Some(commit) = commit {
                data.pyoxidizer_git_commit = Some(commit);
            }
            if let Some(tag) = tag {
                data.pyoxidizer_git_tag = Some(tag);
            }
        }
    }
}

pub fn update_new_cargo_toml(path: &Path) -> Result<(), std::io::Error> {
    let mut fh = std::fs::OpenOptions::new().append(true).open(path)?;

    fh.write_all(b"jemallocator-global = { version = \"0.3\", optional = true }\n")?;
    fh.write_all(b"pyembed = { path = \"pyembed\" }\n")?;
    fh.write_all(b"\n")?;
    fh.write_all(b"[features]\n")?;
    fh.write_all(b"default = []\n")?;
    fh.write_all(b"jemalloc = [\"jemallocator-global\", \"pyembed/jemalloc\"]\n")?;

    Ok(())
}

/// Write a new build.rs file supporting PyOxidizer.
pub fn write_pyembed_build_rs(project_dir: &Path) -> Result<(), std::io::Error> {
    let mut data: BTreeMap<String, String> = BTreeMap::new();
    data.insert(
        "pyoxidizer_exe".to_string(),
        canonicalize_path(&std::env::current_exe()?)?
            .display()
            .to_string(),
    );

    let t = HANDLEBARS
        .render("pyembed-build.rs", &data)
        .expect("unable to render pyembed-build.rs");

    let path = project_dir.to_path_buf().join("build.rs");

    println!("writing {}", path.to_str().unwrap());
    let mut fh = std::fs::File::create(path)?;
    fh.write_all(t.as_bytes())?;

    Ok(())
}

/// Write a new main.rs file that runs the embedded Python interpreter.
pub fn write_new_main_rs(path: &Path) -> Result<(), std::io::Error> {
    let data: BTreeMap<String, String> = BTreeMap::new();
    let t = HANDLEBARS
        .render("new-main.rs", &data)
        .expect("unable to render template");

    println!("writing {}", path.to_str().unwrap());
    let mut fh = std::fs::File::create(path)?;
    fh.write_all(t.as_bytes())?;

    Ok(())
}

/// Writes default PyOxidizer config files into a project directory.
pub fn write_new_pyoxidizer_config_file(
    project_dir: &Path,
    name: &str,
    code: Option<&str>,
    pip_install: &[&str],
) -> Result<(), std::io::Error> {
    let path = project_dir.to_path_buf().join("pyoxidizer.bzl");

    let mut data = TemplateData::new();
    populate_template_data(&mut data);
    data.program_name = Some(name.to_string());

    if let Some(code) = code {
        // Replace " with \" to work around
        // https://github.com/google/starlark-rust/issues/230.
        data.code = Some(code.replace("\"", "\\\""));
    }

    data.pip_install_simple = pip_install.iter().map(|v| v.to_string()).collect();

    let t = HANDLEBARS
        .render("new-pyoxidizer.bzl", &data)
        .expect("unable to render template");

    println!("writing {}", path.to_str().unwrap());
    let mut fh = std::fs::File::create(path)?;
    fh.write_all(t.as_bytes())?;

    Ok(())
}

/// Write files for the pyembed crate into a destination directory.
pub fn write_pyembed_crate_files(dest_dir: &Path) -> Result<(), std::io::Error> {
    println!("creating {}", dest_dir.to_str().unwrap());
    create_dir_all(dest_dir)?;

    let src_dir = dest_dir.to_path_buf().join("src");
    println!("creating {}", src_dir.to_str().unwrap());
    create_dir_all(&src_dir)?;

    for (rs, data) in PYEMBED_RS_FILES.iter() {
        let path = src_dir.join(rs);
        println!("writing {}", path.to_str().unwrap());
        let mut fh = std::fs::File::create(path)?;
        fh.write_all(&data)?;
    }

    let mut data = TemplateData::new();
    populate_template_data(&mut data);

    let t = HANDLEBARS
        .render("pyembed-cargo.toml", &data)
        .expect("unable to render pyembed-cargo.toml");

    let path = dest_dir.to_path_buf().join("Cargo.toml");
    println!("writing {}", path.to_str().unwrap());
    let mut fh = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    fh.write_all(t.as_bytes())?;

    write_pyembed_build_rs(&dest_dir)?;

    Ok(())
}

/// Add PyOxidizer to an existing Rust project on the filesystem.
///
/// The target directory must not already have PyOxidizer files. This
/// will be verified during execution.
///
/// When called, various Rust source files required to embed Python
/// are created at the target directory. Instructions for finalizing the
/// configuration are also printed to stdout.
///
/// The Rust source files added to the target project are installed into
/// a sub-directory defined by ``module_name``. This is typically ``pyembed``.
pub fn add_pyoxidizer(project_dir: &Path, _suppress_help: bool) -> Result<(), String> {
    let existing_files = find_pyoxidizer_files(&project_dir);

    if !existing_files.is_empty() {
        return Err("existing PyOxidizer files found; cannot add".to_string());
    }

    let cargo_toml = project_dir.to_path_buf().join("Cargo.toml");

    if !cargo_toml.exists() {
        return Err("Cargo.toml does not exist at destination".to_string());
    }

    let pyembed_dir = project_dir.to_path_buf().join("pyembed");
    write_pyembed_crate_files(&pyembed_dir).or(Err("error writing pyembed crate files"))?;

    let cargo_toml_data = std::fs::read(cargo_toml).or(Err("error reading Cargo.toml"))?;
    let manifest =
        cargo_toml::Manifest::from_slice(&cargo_toml_data).expect("unable to parse Cargo.toml");

    let _package = match &manifest.package {
        Some(package) => package,
        None => panic!("no [package]; that's weird"),
    };

    // TODO look for pyembed dependency and print message about adding it.

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

/// Build PyOxidizer artifacts for a project.
fn build_pyoxidizer_artifacts(
    logger: &slog::Logger,
    context: &mut BuildContext,
) -> Result<(), String> {
    let pyoxidizer_artifacts_path = &context.pyoxidizer_artifacts_path;

    create_dir_all(&pyoxidizer_artifacts_path).or_else(|e| Err(e.to_string()))?;

    let pyoxidizer_artifacts_path = canonicalize_path(pyoxidizer_artifacts_path)
        .expect("unable to canonicalize artifacts directory");

    if !artifacts_current(logger, &context.config_path, &pyoxidizer_artifacts_path) {
        process_config(logger, context, "0");
    }

    Ok(())
}

/// Build an oxidized Rust application at the specified project path.
pub fn build_project(logger: &slog::Logger, context: &mut BuildContext) -> Result<(), String> {
    if let Ok(rust_version) = rustc_version::version() {
        if rust_version.lt(&MINIMUM_RUST_VERSION) {
            return Err(format!(
                "PyOxidizer requires Rust {}; version {} found",
                *MINIMUM_RUST_VERSION, rust_version,
            ));
        }
    } else {
        return Err("unable to determine Rust version; is Rust installed?".to_string());
    }

    // Our build process is to first generate artifacts from the PyOxidizer
    // configuration within this process then call out to `cargo build`. We do
    // this because it is easier to emit output from this process than to have
    // it proxied via cargo.
    build_pyoxidizer_artifacts(logger, context)?;

    let mut args = Vec::new();
    args.push("build");

    args.push("--target");
    args.push(&context.target_triple);

    // We use an explicit target directory so we can be sure we write our
    // artifacts to the same directory that cargo is using (unless the config
    // file overwrites the artifacts directory, of course).
    let target_dir = context.target_base_path.display().to_string();
    args.push("--target-dir");
    args.push(&target_dir);

    args.push("--bin");
    args.push(&context.config.build_config.application_name);

    if context.release {
        args.push("--release");
    }

    if context.config.embedded_python_config.raw_allocator == RawAllocator::Jemalloc {
        args.push("--features");
        args.push("jemalloc");
    }

    let mut envs = Vec::new();
    envs.push((
        "PYOXIDIZER_ARTIFACT_DIR",
        context.pyoxidizer_artifacts_path.display().to_string(),
    ));
    envs.push(("PYOXIDIZER_REUSE_ARTIFACTS", "1".to_string()));

    // Set PYTHON_SYS_EXECUTABLE so python3-sys uses our distribution's Python to
    // configure itself.
    let python_exe_path = python_exe_path(&context.python_distribution_path);
    envs.push((
        "PYTHON_SYS_EXECUTABLE",
        python_exe_path.display().to_string(),
    ));

    // static-nobundle link kind requires nightly Rust compiler until
    // https://github.com/rust-lang/rust/issues/37403 is resolved.
    if cfg!(windows) {
        envs.push(("RUSTC_BOOTSTRAP", "1".to_string()));
        // Allow multiple definitions when using link.exe
        if context.target_triple.contains("msvc") {
            envs.push(("RUSTFLAGS", "-C link-args=/FORCE:MULTIPLE".to_string()));
        }
    }

    match process::Command::new("cargo")
        .args(args)
        .current_dir(&context.project_path)
        .envs(envs)
        .status()
    {
        Ok(status) => {
            if status.success() {
                Ok(())
            } else {
                Err("cargo build failed".to_string())
            }
        }
        Err(e) => Err(e.to_string()),
    }
}

pub fn resolve_build_context(
    logger: &slog::Logger,
    project_path: &str,
    config_path: Option<&str>,
    target: Option<&str>,
    release: bool,
    force_artifacts_path: Option<&Path>,
    verbose: bool,
) -> Result<BuildContext, String> {
    let path = canonicalize_path(&PathBuf::from(project_path))
        .or_else(|e| Err(e.description().to_owned()))?;

    if find_pyoxidizer_files(&path).is_empty() {
        return Err("no PyOxidizer files in specified path".to_string());
    }

    let target = match target {
        Some(v) => v.to_string(),
        None => default_target()?,
    };

    let config_path = match config_path {
        Some(p) => PathBuf::from(p),
        None => match find_pyoxidizer_config_file_env(logger, &path) {
            Some(p) => p,
            None => return Err("unable to find PyOxidizer config file".to_string()),
        },
    };

    BuildContext::new(
        logger,
        &path,
        &config_path,
        None,
        &target,
        release,
        force_artifacts_path,
        verbose,
    )
}

fn run_project(
    logger: &slog::Logger,
    context: &mut BuildContext,
    extra_args: &[&str],
) -> Result<(), String> {
    // We call our build wrapper and invoke the binary directly. This allows
    // build output to be printed.
    build_project(logger, context)?;

    package_project(logger, context)?;

    match process::Command::new(&context.app_exe_path)
        .current_dir(&context.project_path)
        .args(extra_args)
        .status()
    {
        Ok(status) => {
            if status.success() {
                Ok(())
            } else {
                Err("cargo run failed".to_string())
            }
        }
        Err(e) => Err(e.to_string()),
    }
}

/// Build a PyOxidizer enabled project.
///
/// This is a glorified wrapper around `cargo build`. Our goal is to get the
/// output from repackaging to give the user something for debugging.
pub fn build(
    logger: &slog::Logger,
    project_path: &str,
    target: Option<&str>,
    release: bool,
    verbose: bool,
) -> Result<(), String> {
    let mut context =
        resolve_build_context(logger, project_path, None, target, release, None, verbose)?;
    build_project(logger, &mut context)?;
    package_project(logger, &mut context)?;

    warn!(
        logger,
        "executable path: {}",
        context.app_exe_path.display()
    );

    Ok(())
}

pub fn build_artifacts(
    logger: &slog::Logger,
    project_path: &Path,
    dest_path: &Path,
    target: Option<&str>,
    release: bool,
    verbose: bool,
) -> Result<(), String> {
    let mut context = resolve_build_context(
        logger,
        project_path.to_str().unwrap(),
        None,
        target,
        release,
        Some(dest_path),
        verbose,
    )?;

    build_pyoxidizer_artifacts(logger, &mut context)?;

    Ok(())
}

pub fn run(
    logger: &slog::Logger,
    project_path: &str,
    target: Option<&str>,
    release: bool,
    extra_args: &[&str],
    verbose: bool,
) -> Result<(), String> {
    let mut context =
        resolve_build_context(logger, project_path, None, target, release, None, verbose)?;

    run_project(logger, &mut context, extra_args)
}

/// Initialize a new Rust project with PyOxidizer support.
///
/// `code` can specify custom Python code to run by default in the new
/// application.
///
/// `pip_install` can specify Python packages to `pip install` for the application.
pub fn init(project_path: &str, code: Option<&str>, pip_install: &[&str]) -> Result<(), String> {
    let res = process::Command::new("cargo")
        .arg("init")
        .arg("--bin")
        .arg(project_path)
        .status();

    match res {
        Ok(status) => {
            if !status.success() {
                return Err("cargo init failed".to_string());
            }
        }
        Err(e) => return Err(e.to_string()),
    }

    let path = PathBuf::from(project_path);
    let name = path.iter().last().unwrap().to_str().unwrap();
    add_pyoxidizer(&path, true)?;
    update_new_cargo_toml(&path.join("Cargo.toml")).or(Err("unable to update Cargo.toml"))?;
    write_new_main_rs(&path.join("src").join("main.rs")).or(Err("unable to write main.rs"))?;
    write_new_pyoxidizer_config_file(&path, &name, code, pip_install)
        .or(Err("unable to write PyOxidizer config files"))?;

    println!();
    println!(
        "A new Rust binary application has been created in {}",
        path.display()
    );
    println!();
    println!("This application can be built by doing the following:");
    println!();
    println!("  $ cd {}", path.display());
    println!("  $ pyoxidizer build");
    println!("  $ pyoxidizer run");
    println!();
    println!("The default configuration is to invoke a Python REPL. You can");
    println!("edit the various pyoxidizer.*.bzl config files or the main.rs ");
    println!("file to change behavior. The application will need to be rebuilt ");
    println!("for configuration changes to take effect.");

    Ok(())
}

/// Produce distributions for an application.
pub fn distributions(
    logger: &slog::Logger,
    project_path: &str,
    target: Option<&str>,
    types: &[&str],
) -> Result<(), String> {
    let mut context = resolve_build_context(logger, project_path, None, target, true, None, false)?;

    build_project(logger, &mut context)?;
    package_project(logger, &mut context)?;
    produce_distributions(logger, &context, types)?;

    Ok(())
}

pub fn python_distribution_extract(dist_path: &str, dest_path: &str) -> Result<(), String> {
    let mut fh = std::fs::File::open(Path::new(dist_path)).or_else(|e| Err(e.to_string()))?;
    let mut data = Vec::new();
    fh.read_to_end(&mut data).or_else(|e| Err(e.to_string()))?;
    let cursor = Cursor::new(data);
    let dctx = zstd::stream::Decoder::new(cursor).or_else(|e| Err(e.to_string()))?;
    let mut tf = tar::Archive::new(dctx);

    println!("extracting archive to {}", dest_path);
    tf.unpack(dest_path).or_else(|e| Err(e.to_string()))?;

    Ok(())
}

pub fn python_distribution_info(dist_path: &str) -> Result<(), String> {
    let mut fh = std::fs::File::open(Path::new(dist_path)).or_else(|e| Err(e.to_string()))?;
    let mut data = Vec::new();
    fh.read_to_end(&mut data).or_else(|e| Err(e.to_string()))?;

    let temp_dir = tempdir::TempDir::new("python-distribution").or_else(|e| Err(e.to_string()))?;
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

pub fn python_distribution_licenses(path: &str) -> Result<(), String> {
    let mut fh = std::fs::File::open(Path::new(path)).or_else(|e| Err(e.to_string()))?;
    let mut data = Vec::new();
    fh.read_to_end(&mut data).or_else(|e| Err(e.to_string()))?;

    let temp_dir = tempdir::TempDir::new("python-distribution").or_else(|e| Err(e.to_string()))?;
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

pub fn run_build_script(logger: &slog::Logger, build_script: &str) -> Result<(), String> {
    run_from_build(logger, build_script);

    Ok(())
}
