// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use handlebars::Handlebars;
use lazy_static::lazy_static;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process;

use super::pyrepackager::fsscan::walk_tree_files;
use super::python_distributions::CPYTHON_BY_TRIPLE;

lazy_static! {
    static ref PYEMBED_RS_FILES: BTreeMap<&'static str, &'static [u8]> = {
        let mut res: BTreeMap<&'static str, &'static [u8]> = BTreeMap::new();

        res.insert("lib.rs", include_bytes!("pyembed/lib.rs"));
        res.insert("data.rs", include_bytes!("pyembed/data.rs"));
        res.insert("pyalloc.rs", include_bytes!("pyembed/pyalloc.rs"));
        res.insert("pyinterp.rs", include_bytes!("pyembed/pyinterp.rs"));
        res.insert("pymodules_module.rs", include_bytes!("pyembed/pymodules_module.rs"));
        res.insert("pystr.rs", include_bytes!("pyembed/pystr.rs"));

        res
    };

    static ref HANDLEBARS: Handlebars = {
        let mut handlebars = Handlebars::new();

        handlebars.register_template_string("new-main.rs", include_str!("templates/new-main.rs")).unwrap();
        handlebars.register_template_string("new-pyoxidizer.toml", include_str!("templates/new-pyoxidizer.toml")).unwrap();
        handlebars.register_template_string("pyembed-build.rs", include_str!("templates/pyembed-build.rs")).unwrap();
        handlebars.register_template_string("pyembed-cargo.toml", include_str!("templates/pyembed-cargo.toml")).unwrap();

        handlebars
    };
}

/// Find existing PyOxidizer files in a project directory.
pub fn find_pyoxidizer_files(root: &Path) -> Vec<PathBuf> {
    let mut res: Vec<PathBuf> = Vec::new();

    for f in walk_tree_files(&root) {
        let path = f.path().strip_prefix(root).expect("unable to strip prefix");
        let path_s = path.to_str().expect("unable to convert path to str");

        if path_s.contains("pyoxidizer") {
            res.push(path.to_path_buf());
        }
        else if path_s.contains("pyembed") {
            res.push(path.to_path_buf());
        }
    }

    res
}

fn populate_template_data(data: &mut BTreeMap<String, String>) {
    let env = super::environment::resolve_environment();

    if let Some(repo_path) = env.pyoxidizer_repo_path {
        data.insert(String::from("pyoxidizer_local_repo_path"), String::from(repo_path.to_str().unwrap()));
    }
}

pub fn update_new_cargo_toml(path: &Path) -> Result<(), std::io::Error> {
    let mut fh = std::fs::OpenOptions::new().append(true).open(path)?;
    fh.write(b"pyembed = { path = \"pyembed\" }\n")?;

    Ok(())
}

/// Write a new build.rs file supporting PyOxidizer.
pub fn write_pyembed_build_rs(project_dir: &Path) -> Result<(), std::io::Error> {
    let data: BTreeMap<String, String> = BTreeMap::new();
    let t = HANDLEBARS.render("pyembed-build.rs", &data).expect("unable to render pyembed-build.rs");

    let path = project_dir.to_path_buf().join("build.rs");

    println!("writing {}", path.to_str().unwrap());
    let mut fh = std::fs::File::create(path)?;
    fh.write_all(t.as_bytes())?;

    Ok(())
}

/// Write a new main.rs file that runs the embedded Python interpreter.
pub fn write_new_main_rs(path: &Path) -> Result<(), std::io::Error> {
    let data: BTreeMap<String, String> = BTreeMap::new();
    let t = HANDLEBARS.render("new-main.rs", &data).expect("unable to render template");

    println!("writing {}", path.to_str().unwrap());
    let mut fh = std::fs::File::create(path)?;
    fh.write_all(t.as_bytes())?;

    Ok(())
}

/// Writes default PyOxidizer config files into a project directory.
pub fn write_new_pyoxidizer_config_files(project_dir: &Path, name: &str) -> Result<(), std::io::Error> {
    for (triple, dist) in CPYTHON_BY_TRIPLE.iter() {
        let basename = format!("pyoxidizer.{}.toml", triple);
        let path = project_dir.to_path_buf().join(basename);

        let mut data = BTreeMap::new();
        data.insert("python_distribution_url", dist.url.clone());
        data.insert("python_distribution_sha256", dist.sha256.clone());
        data.insert("program_name", name.to_string());

        let t = HANDLEBARS.render("new-pyoxidizer.toml", &data).expect("unable to render template");

        println!("writing {}", path.to_str().unwrap());
        let mut fh = std::fs::File::create(path)?;
        fh.write_all(t.as_bytes())?;
    }

    Ok(())
}

/// Write files for the pyembed crate into a destination directory.
pub fn write_pyembed_crate_files(dest_dir: &Path) -> Result<(), std::io::Error> {
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

    let t = HANDLEBARS.render("pyembed-cargo.toml", &data).expect("unable to render pyembed-cargo.toml");

    let path = dest_dir.to_path_buf().join("Cargo.toml");
    println!("writing {}", path.to_str().unwrap());
    let mut fh = std::fs::OpenOptions::new().write(true).create_new(true).open(path)?;
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

    if existing_files.len() > 0 {
        return Err("existing PyOxidizer files found; cannot add".to_string());
    }

    let cargo_toml = project_dir.to_path_buf().join("Cargo.toml");

    if !cargo_toml.exists() {
        return Err("Cargo.toml does not exist at destination".to_string());
    }

    let pyembed_dir = project_dir.to_path_buf().join("pyembed");
    write_pyembed_crate_files(&pyembed_dir).or(Err("error writing pyembed crate files"))?;

    let cargo_toml_data = std::fs::read(cargo_toml).or(Err("error reading Cargo.toml"))?;
    let manifest = cargo_toml::Manifest::from_slice(&cargo_toml_data).expect("unable to parse Cargo.toml");

    let _package = match &manifest.package {
        Some(package) => package,
        None => panic!("no [package]; that's weird")
    };

    // TODO look for pyembed dependency and print message about adding it.

    Ok(())
}

/// Initialize a new Rust project with PyOxidizer support.
pub fn init(project_path: &str) -> Result<(), String> {
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
        Err(e) => return Err(e.to_string())
    }

    let path = PathBuf::from(project_path);
    let name = path.iter().last().unwrap().to_str().unwrap();
    add_pyoxidizer(&path, true)?;
    update_new_cargo_toml(&path.join("Cargo.toml")).or(Err("unable to update Cargo.toml"))?;
    write_new_main_rs(&path.join("src").join("main.rs")).or(Err("unable to write main.rs"))?;
    write_new_pyoxidizer_config_files(&path, &name).or(Err("unable to write PyOxidizer config files"))?;

    Ok(())
}
