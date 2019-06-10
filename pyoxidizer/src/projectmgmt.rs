// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Manage PyOxidizer projects.

use handlebars::Handlebars;
use itertools::Itertools;
use lazy_static::lazy_static;
use std::collections::BTreeMap;
use std::error::Error;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::process;

use super::environment::PyOxidizerSource;
use super::pyrepackager::dist::analyze_python_distribution_tar_zst;
use super::pyrepackager::fsscan::walk_tree_files;
use super::pyrepackager::repackage::run_from_build;
use super::python_distributions::CPYTHON_BY_TRIPLE;

lazy_static! {
    static ref PYEMBED_RS_FILES: BTreeMap<&'static str, &'static [u8]> = {
        let mut res: BTreeMap<&'static str, &'static [u8]> = BTreeMap::new();

        res.insert("config.rs", include_bytes!("pyembed/config.rs"));
        res.insert("lib.rs", include_bytes!("pyembed/lib.rs"));
        res.insert("data.rs", include_bytes!("pyembed/data.rs"));
        res.insert("importer.rs", include_bytes!("pyembed/importer.rs"));
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
                "new-pyoxidizer.toml",
                include_str!("templates/new-pyoxidizer.toml"),
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

fn populate_template_data(data: &mut BTreeMap<String, String>) {
    let env = super::environment::resolve_environment().unwrap();

    match env.pyoxidizer_source {
        PyOxidizerSource::LocalPath { path } => {
            data.insert(
                String::from("pyoxidizer_local_repo_path"),
                path.display().to_string(),
            );
        }
        PyOxidizerSource::GitUrl { url, commit } => {
            data.insert(String::from("pyoxidizer_git_url"), url);
            data.insert(String::from("pyoxidizer_git_commit"), commit);
        }
    }
}

pub fn update_new_cargo_toml(path: &Path, jemalloc: bool) -> Result<(), std::io::Error> {
    let mut fh = std::fs::OpenOptions::new().append(true).open(path)?;

    if jemalloc {
        fh.write_all(b"jemallocator-global = \"0.3\"\n")?;
    }

    fh.write_all(b"pyembed = { path = \"pyembed\" }\n")?;

    Ok(())
}

/// Write a new build.rs file supporting PyOxidizer.
pub fn write_pyembed_build_rs(project_dir: &Path) -> Result<(), std::io::Error> {
    let mut data: BTreeMap<String, String> = BTreeMap::new();
    data.insert(
        "pyoxidizer_exe".to_string(),
        std::env::current_exe()?
            .canonicalize()?
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
    enable_jemalloc: bool,
) -> Result<(), std::io::Error> {
    let path = project_dir.to_path_buf().join("pyoxidizer.toml");

    let distributions = CPYTHON_BY_TRIPLE
        .iter()
        .map(|(triple, dist)| {
            format!(
                "[[python_distribution]]\ntarget = \"{}\"\nurl = \"{}\"\nsha256 = \"{}\"\n",
                triple.clone(),
                dist.url.clone(),
                dist.sha256.clone()
            )
            .to_string()
        })
        .collect_vec();

    let mut data = BTreeMap::new();

    data.insert("python_distributions", distributions.join("\n"));
    data.insert("program_name", name.to_string());

    if enable_jemalloc {
        data.insert("jemalloc", "1".to_string());
    }

    let t = HANDLEBARS
        .render("new-pyoxidizer.toml", &data)
        .expect("unable to render template");

    println!("writing {}", path.to_str().unwrap());
    let mut fh = std::fs::File::create(path)?;
    fh.write_all(t.as_bytes())?;

    Ok(())
}

/// Write files for the pyembed crate into a destination directory.
pub fn write_pyembed_crate_files(dest_dir: &Path, jemalloc: bool) -> Result<(), std::io::Error> {
    println!("creating {}", dest_dir.to_str().unwrap());
    std::fs::create_dir_all(dest_dir)?;

    let src_dir = dest_dir.to_path_buf().join("src");
    println!("creating {}", src_dir.to_str().unwrap());
    std::fs::create_dir_all(&src_dir)?;

    for (rs, data) in PYEMBED_RS_FILES.iter() {
        let path = src_dir.join(rs);
        println!("writing {}", path.to_str().unwrap());
        let mut fh = std::fs::File::create(path)?;
        fh.write_all(&data)?;
    }

    let mut data = BTreeMap::new();
    populate_template_data(&mut data);

    if jemalloc {
        data.insert("jemalloc".to_string(), "1".to_string());
    }

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
pub fn add_pyoxidizer(
    project_dir: &Path,
    _suppress_help: bool,
    jemalloc: bool,
) -> Result<(), String> {
    let existing_files = find_pyoxidizer_files(&project_dir);

    if !existing_files.is_empty() {
        return Err("existing PyOxidizer files found; cannot add".to_string());
    }

    let cargo_toml = project_dir.to_path_buf().join("Cargo.toml");

    if !cargo_toml.exists() {
        return Err("Cargo.toml does not exist at destination".to_string());
    }

    let pyembed_dir = project_dir.to_path_buf().join("pyembed");
    write_pyembed_crate_files(&pyembed_dir, jemalloc)
        .or(Err("error writing pyembed crate files"))?;

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

fn build_project(project_path: &Path, release: bool) -> Result<(), String> {
    let mut args = Vec::new();
    args.push("build");
    if release {
        args.push("--release");
    }

    let current_exe = std::env::current_exe()
        .or_else(|e| Err(e.to_string()))?
        .canonicalize()
        .or_else(|e| Err(e.to_string()))?
        .display()
        .to_string();

    match process::Command::new("cargo")
        .args(args)
        .current_dir(&project_path)
        .env("PYOXIDIZER_EXE", current_exe)
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

fn run_project(project_path: &Path, release: bool) -> Result<(), String> {
    let mut args = Vec::new();
    args.push("run");
    if release {
        args.push("--release");
    }

    let current_exe = std::env::current_exe()
        .or_else(|e| Err(e.to_string()))?
        .canonicalize()
        .or_else(|e| Err(e.to_string()))?
        .display()
        .to_string();

    match process::Command::new("cargo")
        .args(args)
        .current_dir(&project_path)
        .env("PYOXIDIZER_EXE", current_exe)
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
pub fn build(project_path: &str, debug: bool, release: bool) -> Result<(), String> {
    let path = PathBuf::from(project_path)
        .canonicalize()
        .or_else(|e| Err(e.description().to_owned()))?;

    if find_pyoxidizer_files(&path).is_empty() {
        return Err("no PyOxidizer files in specified path".to_string());
    }

    if debug {
        build_project(&path, false)?;
    }

    if release {
        build_project(&path, true)?;
    }

    Ok(())
}

pub fn run(project_path: &str, release: bool) -> Result<(), String> {
    let path = PathBuf::from(project_path)
        .canonicalize()
        .or_else(|e| Err(e.to_string()))?;

    if find_pyoxidizer_files(&path).is_empty() {
        return Err("no PyOxidizer files in specified path".to_string());
    }

    run_project(&path, release)
}

/// Initialize a new Rust project with PyOxidizer support.
pub fn init(project_path: &str, jemalloc: bool) -> Result<(), String> {
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
    add_pyoxidizer(&path, true, jemalloc)?;
    update_new_cargo_toml(&path.join("Cargo.toml"), jemalloc)
        .or(Err("unable to update Cargo.toml"))?;
    write_new_main_rs(&path.join("src").join("main.rs")).or(Err("unable to write main.rs"))?;
    write_new_pyoxidizer_config_file(&path, &name, jemalloc)
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
    println!("  $ cargo build");
    println!("  $ cargo run");
    println!();
    println!("The default configuration is to invoke a Python REPL. You can");
    println!("edit the various pyoxidizer.*.toml config files or the main.rs ");
    println!("file to change behavior. The application will need to be rebuilt ");
    println!("for configuration changes to take effect.");

    Ok(())
}

pub fn python_distribution_licenses(path: &str) -> Result<(), String> {
    let mut fh = std::fs::File::open(Path::new(path)).or_else(|e| Err(e.to_string()))?;
    let mut data = Vec::new();
    fh.read_to_end(&mut data).or_else(|e| Err(e.to_string()))?;

    let cursor = Cursor::new(data);
    let dist = analyze_python_distribution_tar_zst(cursor)?;

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

                if link.license_public_domain.is_some() && link.license_public_domain.unwrap() {
                    println!("Licenses: Public Domain");
                } else if let Some(ref licenses) = link.licenses {
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
    }

    Ok(())
}

pub fn run_build_script(logger: &slog::Logger, build_script: &str) -> Result<(), String> {
    run_from_build(logger, build_script);

    Ok(())
}
