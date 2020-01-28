// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Handle file layout of PyOxidizer projects.

use anyhow::{anyhow, Result};
use handlebars::Handlebars;
use lazy_static::lazy_static;
use serde::Serialize;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::environment::{PyOxidizerSource, BUILD_GIT_COMMIT, PYOXIDIZER_VERSION};
use crate::py_packaging::fsscan::walk_tree_files;

lazy_static! {
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
    };
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

/// Write a new main.rs file that runs the embedded Python interpreter.
pub fn write_new_main_rs(path: &Path) -> Result<()> {
    let data: BTreeMap<String, String> = BTreeMap::new();
    let t = HANDLEBARS.render("new-main.rs", &data)?;

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
) -> Result<()> {
    let path = project_dir.join("pyoxidizer.bzl");

    let mut data = TemplateData::new();
    populate_template_data(&mut data);
    data.program_name = Some(name.to_string());

    if let Some(code) = code {
        // Replace " with \" to work around
        // https://github.com/google/starlark-rust/issues/230.
        data.code = Some(code.replace("\"", "\\\""));
    }

    data.pip_install_simple = pip_install.iter().map(|v| (*v).to_string()).collect();

    let t = HANDLEBARS.render("new-pyoxidizer.bzl", &data)?;

    println!("writing {}", path.to_str().unwrap());
    let mut fh = std::fs::File::create(path)?;
    fh.write_all(t.as_bytes())?;

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
/// a sub-directory defined by ``module_name``.
pub fn add_pyoxidizer(project_dir: &Path, _suppress_help: bool) -> Result<()> {
    let existing_files = find_pyoxidizer_files(&project_dir);

    if !existing_files.is_empty() {
        return Err(anyhow!("existing PyOxidizer files found; cannot add"));
    }

    let cargo_toml = project_dir.to_path_buf().join("Cargo.toml");

    if !cargo_toml.exists() {
        return Err(anyhow!("Cargo.toml does not exist at destination"));
    }

    let cargo_toml_data = std::fs::read(cargo_toml)?;
    let manifest = cargo_toml::Manifest::from_slice(&cargo_toml_data)?;

    let _package = match &manifest.package {
        Some(package) => package,
        None => panic!("no [package]; that's weird"),
    };

    // TODO look for pyembed dependency and print message about adding it.

    Ok(())
}

/// How to define the ``pyembed`` crate dependency.
///
/// TODO support a Git repository location.
pub enum PyembedLocation {
    /// Use a specific version, installed from the crate registry.
    ///
    /// (This is how most Rust dependencies are defined.)
    Version(String),

    /// Use a local filesystem path.
    Path(PathBuf),
}

/// Update the Cargo.toml of a new Rust project to use pyembed.
pub fn update_new_cargo_toml(path: &Path, pyembed_location: &PyembedLocation) -> Result<()> {
    let mut fh = std::fs::OpenOptions::new().append(true).open(path)?;

    fh.write_all(b"jemallocator-global = { version = \"0.3\", optional = true }\n")?;

    match pyembed_location {
        PyembedLocation::Version(version) => {
            fh.write_all(format!("pyembed = \"{}\"\n", version).as_bytes())?;
        }
        PyembedLocation::Path(path) => {
            fh.write_all(format!("pyembed = {{ path = \"{}\" }}\n", path.display()).as_bytes())?;
        }
    }

    fh.write_all(b"\n")?;
    fh.write_all(b"[features]\n")?;
    fh.write_all(b"default = []\n")?;
    fh.write_all(b"jemalloc = [\"jemallocator-global\", \"pyembed/jemalloc\"]\n")?;

    Ok(())
}

/// Initialize a new Rust project using PyOxidizer.
///
/// The created binary application will have the name of the final
/// path component.
pub fn initialize_project(
    project_path: &Path,
    pyembed_location: &PyembedLocation,
    code: Option<&str>,
    pip_install: &[&str],
) -> Result<()> {
    let status = std::process::Command::new("cargo")
        .arg("init")
        .arg("--bin")
        .arg(project_path)
        .status()?;

    if !status.success() {
        return Err(anyhow!("cargo init failed"));
    }

    let path = PathBuf::from(project_path);
    let name = path.iter().last().unwrap().to_str().unwrap();
    add_pyoxidizer(&path, true)?;
    update_new_cargo_toml(&path.join("Cargo.toml"), pyembed_location)?;
    write_new_main_rs(&path.join("src").join("main.rs"))?;
    write_new_pyoxidizer_config_file(&path, &name, code, pip_install)?;

    Ok(())
}
