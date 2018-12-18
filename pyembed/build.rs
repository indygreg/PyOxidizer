// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

extern crate cpython;
extern crate libc;
extern crate pyrepackager;
extern crate python3_sys as pyffi;

use cpython::{Python, PyBytes, PyErr, PyObject};
use libc::c_char;
use pyrepackager::{analyze_python_distribution_tar_zst, BlobEntries, BlobEntry, find_python_modules, PYTHON_IMPORTER, PythonModuleData, write_blob_entries};
use pyrepackager::config::{parse_config, resolve_python_distribution_archive};
use pyffi::{Py_CompileStringExFlags, Py_file_input, Py_MARSHAL_VERSION, PyMarshal_WriteObjectToString};

use std::collections::BTreeMap;
use std::env;
use std::ffi::CString;
use std::fs::File;
use std::io::{Cursor, Read, Write};
use std::path::Path;

/// Compile Python source to bytecode in-process.
///
/// This can be used to produce data for a frozen module.
fn compile_bytecode(source: &Vec<u8>, filename: &str) -> Vec<u8> {
    // Need to convert to CString to ensure trailing NULL is present.
    let source = CString::new(source.clone()).unwrap();
    let filename = CString::new(filename).unwrap();

    // TODO this shouldn't be needed. Is no-auto-initialize being inherited?
    cpython::prepare_freethreaded_python();

    let gil = Python::acquire_gil();
    let py = gil.python();

    // We can pick up a different Python version from what the distribution is
    // running. This will result in "bad" bytecode being generated. Check for
    // that.
    // TODO we should validate against the parsed distribution instead of
    // hard-coding the version number.
    if pyffi::Py_MARSHAL_VERSION != 4 {
        panic!("unrecognized marshal version {}; did build.rs link against Python 3.7?", pyffi::Py_MARSHAL_VERSION);
    }

    let mut flags = pyffi::PyCompilerFlags {
        cf_flags: 0,
    };

    let code = unsafe {
        let flags_ptr = &mut flags;
        Py_CompileStringExFlags(source.as_ptr() as *const c_char, filename.as_ptr() as *const c_char, Py_file_input, flags_ptr, 0)
    };

    if PyErr::occurred(py) {
        let err = PyErr::fetch(py);
        err.print(py);
        panic!("Python error when compiling {}", filename.to_str().unwrap());
    }

    if code.is_null() {
        panic!("code is null without Python error. Huh?");
    }

    let marshalled = unsafe {
        PyMarshal_WriteObjectToString(code, Py_MARSHAL_VERSION)
    };

    let marshalled = unsafe {
        PyObject::from_owned_ptr(py, marshalled)
    };

    let data = marshalled.cast_as::<PyBytes>(py).unwrap().data(py);

    return data.to_vec();
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

    unsafe {
        pyffi::Py_OptimizeFlag = config.package_optimize_level as i32;
        pyffi::Py_UnbufferedStdioFlag = 1;
    }

    // Obtain the configured Python distribution and parse it to a data structure.
    let python_distribution_path = resolve_python_distribution_archive(&config, &out_dir_path);
    let mut fh = File::open(python_distribution_path).unwrap();
    let mut python_distribution_data = Vec::new();
    fh.read_to_end(&mut python_distribution_data).unwrap();
    let dist_cursor = Cursor::new(python_distribution_data);
    let dist = analyze_python_distribution_tar_zst(dist_cursor).unwrap();

    // Produce the frozen importlib modules.
    //
    // importlib._bootstrap isn't modified.
    //
    // importlib._bootstrap_external is modified. We take the original Python
    // source and concatenate with code that provides the memory importer. We
    // then generate bytecode for that.
    let mod_importlib_bootstrap = dist.py_modules.get("importlib._bootstrap").unwrap();
    let mod_importlib_bootstrap_external = dist.py_modules.get("importlib._bootstrap_external").unwrap();

    let importlib_bootstrap_source = &mod_importlib_bootstrap.py;
    let module_name = "<frozen importlib._bootstrap>";
    let importlib_bootstrap_bytecode = compile_bytecode(importlib_bootstrap_source, module_name);

    let mut importlib_bootstrap_external_source = mod_importlib_bootstrap_external.py.clone();
    importlib_bootstrap_external_source.extend("\n# END OF importlib/_bootstrap_external.py\n\n".bytes());
    importlib_bootstrap_external_source.extend(PYTHON_IMPORTER);
    let module_name = "<frozen importlib._bootstrap_external>";
    let importlib_bootstrap_external_bytecode = compile_bytecode(&importlib_bootstrap_external_source, module_name);

    let importlib_bootstrap_path = Path::new(&out_dir).join("importlib_bootstrap.pyc");
    let mut fh = File::create(&importlib_bootstrap_path).unwrap();
    fh.write(&importlib_bootstrap_bytecode).unwrap();

    let importlib_bootstrap_external_path = Path::new(&out_dir).join("importlib_bootstrap_external.pyc");
    let mut fh = File::create(&importlib_bootstrap_external_path).unwrap();
    fh.write(&importlib_bootstrap_external_bytecode).unwrap();

    let mut all_py_modules: BTreeMap<String, PythonModuleData> = BTreeMap::new();
    for (name, entry) in dist.py_modules {
        all_py_modules.insert(name.clone(), entry.clone());
    }

    // Collect additional Python modules and resources to embed in the interpreter.
    // Reverse iteration order so first entry in config is used (last write wins).
    for path in config.package_module_paths.iter().rev() {
        for (name, source) in find_python_modules(&path).unwrap() {
            let bytecode = compile_bytecode(&source, &name);

            let (pyc, pyc_opt1, pyc_opt2) = match config.package_optimize_level {
                0 => (Some(bytecode), None, None),
                1 => (None, Some(bytecode), None),
                2 => (None, None, Some(bytecode)),
                _ => panic!("unsupported optimization level"),
            };

            all_py_modules.insert(name, PythonModuleData {
                py: source,
                pyc,
                pyc_opt1,
                pyc_opt2,
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
            data: module.py.clone(),
        });

        let pyc_data = match config.package_optimize_level {
            0 => match &module.pyc {
                Some(data) => Some(data.clone()),
                None => None,
            },
            1 => match &module.pyc_opt1 {
                Some(data) => Some(data.clone()),
                None => None,
            },
            2 => match &module.pyc_opt2 {
                Some(data) => Some(data.clone()),
                None => None,
            },
            _ => panic!("unsupported Python optimization level"),
        };

        if pyc_data.is_some() {
            pyc_modules.push(BlobEntry {
                name: name.clone(),
                data: pyc_data.unwrap(),
            });
        }
    }

    let py_modules_path = Path::new(&out_dir).join("py-modules");
    let pyc_modules_path = Path::new(&out_dir).join("pyc-modules");

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

    f.write_fmt(format_args!("pub const FROZEN_IMPORTLIB_DATA: &'static [u8] = include_bytes!(\"{}\");\n",
        importlib_bootstrap_path.to_str().unwrap())).unwrap();
    f.write_fmt(format_args!("pub const FROZEN_IMPORTLIB_EXTERNAL_DATA: &'static [u8] = include_bytes!(\"{}\");\n",
        importlib_bootstrap_external_path.to_str().unwrap())).unwrap();
    f.write_fmt(format_args!("pub const PY_MODULES_DATA: &'static [u8] = include_bytes!(\"{}\");\n",
        py_modules_path.to_str().unwrap())).unwrap();
    f.write_fmt(format_args!("pub const PYC_MODULES_DATA: &'static [u8] = include_bytes!(\"{}\");\n",
        pyc_modules_path.to_str().unwrap())).unwrap();
}
