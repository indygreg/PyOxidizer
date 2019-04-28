// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

extern crate pyrepackager;

use std::env;
use std::fs::File;
use std::io::{Cursor, Read, Write};
use std::path::Path;

use pyrepackager::config::{parse_config, resolve_python_distribution_archive};
use pyrepackager::dist::analyze_python_distribution_tar_zst;
use pyrepackager::repackage::{
    derive_importlib, link_libpython, resolve_python_resources, write_data_rs,
};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=PYOXIDIZER_CONFIG");

    let config_env =
        env::var("PYOXIDIZER_CONFIG").expect("PYOXIDIZER_CONFIG environment variable not set");
    let config_path = Path::new(&config_env);

    if !config_path.exists() {
        panic!(
            "config file {} defined by PYOXIDIZER_CONFIG does not exist",
            config_env
        );
    }

    println!("cargo:rerun-if-changed={}", config_env);

    let out_dir = env::var("OUT_DIR").unwrap();
    let out_dir_path = Path::new(&out_dir);

    let mut fh = File::open(config_path).unwrap();

    let mut config_data = Vec::new();
    fh.read_to_end(&mut config_data).unwrap();

    let config = parse_config(&config_data);

    if config.python_distribution_path.is_some() {
        println!(
            "cargo:rerun-if-changed={}",
            config.python_distribution_path.as_ref().unwrap()
        );
    }

    // Obtain the configured Python distribution and parse it to a data structure.
    let python_distribution_path = resolve_python_distribution_archive(&config, &out_dir_path);
    let mut fh = File::open(python_distribution_path).unwrap();
    let mut python_distribution_data = Vec::new();
    fh.read_to_end(&mut python_distribution_data).unwrap();
    let dist_cursor = Cursor::new(python_distribution_data);
    let dist = analyze_python_distribution_tar_zst(dist_cursor).unwrap();

    // Produce a static library containing the Python bits we need.
    // As a side-effect, this will emit the cargo: lines needed to link this
    // library.
    link_libpython(&dist);

    // Produce the frozen importlib modules.
    let importlib = derive_importlib(&dist);

    let importlib_bootstrap_path = Path::new(&out_dir).join("importlib_bootstrap.pyc");
    let mut fh = File::create(&importlib_bootstrap_path).unwrap();
    fh.write(&importlib.bootstrap_bytecode).unwrap();

    let importlib_bootstrap_external_path =
        Path::new(&out_dir).join("importlib_bootstrap_external.pyc");
    let mut fh = File::create(&importlib_bootstrap_external_path).unwrap();
    fh.write(&importlib.bootstrap_external_bytecode).unwrap();

    let resources = resolve_python_resources(&config.python_packaging, &dist);

    // Produce the packed data structures containing Python modules.
    // TODO there is tons of room to customize this behavior, including
    // reordering modules so the memory order matches import order.

    let module_names_path = Path::new(&out_dir).join("py-module-names");
    let py_modules_path = Path::new(&out_dir).join("py-modules");
    let pyc_modules_path = Path::new(&out_dir).join("pyc-modules");

    resources.write_blobs(&module_names_path, &py_modules_path, &pyc_modules_path);

    let dest_path = Path::new(&out_dir).join("data.rs");
    write_data_rs(&dest_path, &config, &importlib_bootstrap_path, &importlib_bootstrap_external_path, &py_modules_path, &pyc_modules_path);
}
