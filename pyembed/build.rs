// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

extern crate pyrepackager;

use pyrepackager::bytecode::compile_bytecode;
use pyrepackager::config::{parse_config, resolve_python_distribution_archive};
use pyrepackager::dist::{analyze_python_distribution_tar_zst};
use pyrepackager::fsscan::find_python_modules;
use pyrepackager::repackage::{BlobEntries, BlobEntry, derive_importlib, link_libpython, is_stdlib_test_package, write_blob_entries};

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::fs::File;
use std::io::{Cursor, Read, Write};
use std::path::Path;

struct PythonModuleData {
    source: Vec<u8>,
    bytecode: Option<Vec<u8>>,
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=PYOXIDIZER_CONFIG");

    let config_env = env::var("PYOXIDIZER_CONFIG").expect("PYOXIDIZER_CONFIG environment variable not set");
    let config_path = Path::new(&config_env);

    if !config_path.exists() {
        panic!("config file {} defined by PYOXIDIZER_CONFIG does not exist", config_env);
    }

    println!("cargo:rerun-if-changed={}", config_env);

    let out_dir = env::var("OUT_DIR").unwrap();
    let out_dir_path = Path::new(&out_dir);

    let mut fh = File::open(config_path).unwrap();

    let mut config_data = Vec::new();
    fh.read_to_end(&mut config_data).unwrap();

    let config = parse_config(&config_data);

    if config.python_distribution_path.is_some() {
        println!("cargo:rerun-if-changed={}", config.python_distribution_path.as_ref().unwrap());
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

    let importlib_bootstrap_external_path = Path::new(&out_dir).join("importlib_bootstrap_external.pyc");
    let mut fh = File::create(&importlib_bootstrap_external_path).unwrap();
    fh.write(&importlib.bootstrap_external_bytecode).unwrap();

    let mut all_py_modules: BTreeMap<String, PythonModuleData> = BTreeMap::new();

    for (name, fs_path) in dist.py_modules {
        if is_stdlib_test_package(&name) {
            println!("skipping test stdlib module: {}", name);
            continue;
        }

        let source = fs::read(fs_path).expect("error reading source file");

        let bytecode = match compile_bytecode(&source, &name, config.package_optimize_level as i32) {
            Ok(res) => Some(res),
            Err(msg) => panic!("error compiling bytecode: {}", msg),
        };

        all_py_modules.insert(name.clone(), PythonModuleData {
            source,
            bytecode,
        });
    }

    // Collect additional Python modules and resources to embed in the interpreter.
    // Reverse iteration order so first entry in config is used (last write wins).
    for path in config.package_module_paths.iter().rev() {
        for (name, source) in find_python_modules(&path).unwrap() {
            let bytecode = compile_bytecode(&source, &name, config.package_optimize_level as i32).expect("error compiling bytecode");

            all_py_modules.insert(name.clone(), PythonModuleData {
                source,
                bytecode: Some(bytecode),
            });
        }
    }

    // Produce the packed data structures containing Python modules.
    // TODO there is tons of room to customize this behavior, including
    // reordering modules so the memory order matches import order.

    let mut py_modules = BlobEntries::new();
    let mut pyc_modules = BlobEntries::new();

    for (name, module) in &all_py_modules {
        py_modules.push(BlobEntry {
            name: name.clone(),
            data: module.source.clone(),
        });

        let pyc_data = &module.bytecode;

        if pyc_data.is_some() {
            pyc_modules.push(BlobEntry {
                name: name.clone(),
                data: pyc_data.clone().unwrap(),
            });
        }
    }

    let module_names_path = Path::new(&out_dir).join("py-module-names");
    let py_modules_path = Path::new(&out_dir).join("py-modules");
    let pyc_modules_path = Path::new(&out_dir).join("pyc-modules");

    let mut fh = File::create(&module_names_path).expect("error creating file");
    for name in all_py_modules.keys() {
        fh.write(name.as_bytes()).expect("failed to write");
        fh.write(b"\n").expect("failed to write");
    }

    let fh = File::create(&py_modules_path).unwrap();
    write_blob_entries(&fh, &py_modules).unwrap();

    let fh = File::create(&pyc_modules_path).unwrap();
    write_blob_entries(&fh, &pyc_modules).unwrap();

    let dest_path = Path::new(&out_dir).join("data.rs");

    let mut f = File::create(&dest_path).unwrap();

    f.write_fmt(format_args!("pub const STANDARD_IO_ENCODING: Option<String> = {};\n", match config.stdio_encoding_name {
        // TODO print out value.
        Some(_value) => "Some(\"\")",
        None => "None",
    })).unwrap();
    f.write_fmt(format_args!("pub const STANDARD_IO_ERRORS: Option<String> = {};\n", match config.stdio_encoding_errors {
        Some(_value) => "Some(\"\")",
        None => "None",
    })).unwrap();

    f.write_fmt(format_args!("pub const DONT_WRITE_BYTECODE: bool = {};\n", config.dont_write_bytecode)).unwrap();
    f.write_fmt(format_args!("pub const IGNORE_ENVIRONMENT: bool = {};\n", config.ignore_environment)).unwrap();
    f.write_fmt(format_args!("pub const OPT_LEVEL: i32 = {};\n", config.optimize_level)).unwrap();
    f.write_fmt(format_args!("pub const NO_SITE: bool = {};\n", config.no_site)).unwrap();
    f.write_fmt(format_args!("pub const NO_USER_SITE_DIRECTORY: bool = {};\n", config.no_user_site_directory)).unwrap();
    f.write_fmt(format_args!("pub const PROGRAM_NAME: &str = \"{}\";\n", config.program_name)).unwrap();
    f.write_fmt(format_args!("pub const UNBUFFERED_STDIO: bool = {};\n", config.unbuffered_stdio)).unwrap();

    f.write_fmt(format_args!("pub const FROZEN_IMPORTLIB_DATA: &'static [u8] = include_bytes!(r\"{}\");\n",
        importlib_bootstrap_path.to_str().unwrap())).unwrap();
    f.write_fmt(format_args!("pub const FROZEN_IMPORTLIB_EXTERNAL_DATA: &'static [u8] = include_bytes!(r\"{}\");\n",
        importlib_bootstrap_external_path.to_str().unwrap())).unwrap();
    f.write_fmt(format_args!("pub const PY_MODULES_DATA: &'static [u8] = include_bytes!(r\"{}\");\n",
        py_modules_path.to_str().unwrap())).unwrap();
    f.write_fmt(format_args!("pub const PYC_MODULES_DATA: &'static [u8] = include_bytes!(r\"{}\");\n",
        pyc_modules_path.to_str().unwrap())).unwrap();
}
