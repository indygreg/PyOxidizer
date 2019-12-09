// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Handle file layout of PyOxidizer projects.

use anyhow::{anyhow, Result};
use handlebars::Handlebars;
use lazy_static::lazy_static;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs::create_dir_all;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::environment::{
    canonicalize_path, PyOxidizerSource, BUILD_GIT_COMMIT, PYOXIDIZER_VERSION,
};
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

    let t = HANDLEBARS.render("new-pyoxidizer.bzl", &data)?;

    println!("writing {}", path.to_str().unwrap());
    let mut fh = std::fs::File::create(path)?;
    fh.write_all(t.as_bytes())?;

    Ok(())
}

/// Write a new build.rs file supporting PyOxidizer.
pub fn write_pyembed_build_rs(project_dir: &Path) -> Result<()> {
    let mut data: BTreeMap<String, String> = BTreeMap::new();
    data.insert(
        "pyoxidizer_exe".to_string(),
        canonicalize_path(&std::env::current_exe()?)?
            .display()
            .to_string(),
    );

    let t = HANDLEBARS.render("pyembed-build.rs", &data)?;

    let path = project_dir.to_path_buf().join("build.rs");

    println!("writing {}", path.to_str().unwrap());
    let mut fh = std::fs::File::create(path)?;
    fh.write_all(t.as_bytes())?;

    Ok(())
}

/// Write files for the pyembed crate into a destination directory.
pub fn write_pyembed_crate_files(dest_dir: &Path) -> Result<()> {
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

    let t = HANDLEBARS.render("pyembed-cargo.toml", &data)?;

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
pub fn add_pyoxidizer(project_dir: &Path, _suppress_help: bool) -> Result<()> {
    let existing_files = find_pyoxidizer_files(&project_dir);

    if !existing_files.is_empty() {
        return Err(anyhow!("existing PyOxidizer files found; cannot add"));
    }

    let cargo_toml = project_dir.to_path_buf().join("Cargo.toml");

    if !cargo_toml.exists() {
        return Err(anyhow!("Cargo.toml does not exist at destination"));
    }

    let pyembed_dir = project_dir.to_path_buf().join("pyembed");
    write_pyembed_crate_files(&pyembed_dir)?;

    let cargo_toml_data = std::fs::read(cargo_toml)?;
    let manifest = cargo_toml::Manifest::from_slice(&cargo_toml_data)?;

    let _package = match &manifest.package {
        Some(package) => package,
        None => panic!("no [package]; that's weird"),
    };

    // TODO look for pyembed dependency and print message about adding it.

    Ok(())
}
