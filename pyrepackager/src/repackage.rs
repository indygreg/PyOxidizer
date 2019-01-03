// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use byteorder::{LittleEndian, WriteBytesExt};
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::fs::create_dir_all;
use std::io::Write;
use std::iter::FromIterator;
use std::path::PathBuf;

use super::bytecode::compile_bytecode;
use super::dist::PythonDistributionInfo;

pub const PYTHON_IMPORTER: &'static [u8] = include_bytes!("memoryimporter.py");

const STDLIB_TEST_PACKAGES: &[&str] = &[
    "bsddb.test",
    "ctypes.test",
    "distutils.tests",
    "email.test",
    "idlelib.idle_test",
    "json.tests",
    "lib-tk.test",
    "lib2to3.tests",
    "sqlite3.test",
    "test",
    "tkinter.test",
    "unittest.test",
];

/// Libraries provided by the host that we can ignore in Python module library dependencies.
///
/// Libraries in this data structure are not provided by the Python distribution.
/// A library should only be in this data structure if it is universally distributed
/// by the OS. It is assumed that all binaries produced for the target will link
/// against these libraries by default.
lazy_static! {
    static ref OS_IGNORE_LIBRARIES: Vec<&'static str> = {
        let mut v = Vec::new();

        if cfg!(target_os = "linux") {
            v.push("dl");
            v.push("m");
        }
        else if cfg!(target_os = "macos") {
            v.push("dl");
            v.push("m");
        }

        v
    };
}

/// Python extension modules that should never be included.
///
/// Ideally this data structure doesn't exist. But there are some problems
/// with various extensions on various targets.
lazy_static! {
    static ref OS_IGNORE_EXTENSIONS: Vec<&'static str> = {
        let mut v = Vec::new();

        if cfg!(target_os = "linux") {
            // Linking issues.
            v.push("_crypt");

            // Linking issues.
            v.push("nis");
        }

        else if cfg!(target_os = "macos") {
            // curses and readline have linking issues.
            v.push("_curses");
            v.push("_curses_panel");
            v.push("readline");
        }

        v
    };
}

pub struct ImportlibData {
    pub bootstrap_source: Vec<u8>,
    pub bootstrap_bytecode: Vec<u8>,
    pub bootstrap_external_source: Vec<u8>,
    pub bootstrap_external_bytecode: Vec<u8>,
}

/// Produce frozen importlib bytecode data.
///
/// importlib._bootstrap isn't modified.
///
/// importlib._bootstrap_external is modified. We take the original Python
/// source and concatenate with code that provides the memory importer.
/// Bytecode is then derived from it.
pub fn derive_importlib(dist: &PythonDistributionInfo) -> ImportlibData {
    let mod_bootstrap_path = dist.py_modules.get("importlib._bootstrap").unwrap();
    let mod_bootstrap_external_path = dist.py_modules.get("importlib._bootstrap_external").unwrap();

    let bootstrap_source = fs::read(&mod_bootstrap_path).expect("unable to read bootstrap source");
    let module_name = "<frozen importlib._bootstrap>";
    let bootstrap_bytecode = compile_bytecode(&bootstrap_source, module_name, 0).expect("error compiling bytecode");

    let mut bootstrap_external_source = fs::read(&mod_bootstrap_external_path).expect("unable to read bootstrap_external source");
    bootstrap_external_source.extend("\n# END OF importlib/_bootstrap_external.py\n\n".bytes());
    bootstrap_external_source.extend(PYTHON_IMPORTER);
    let module_name = "<frozen importlib._bootstrap_external>";
    let bootstrap_external_bytecode = compile_bytecode(&bootstrap_external_source, module_name, 0).expect("error compiling bytecode");

    ImportlibData {
        bootstrap_source: bootstrap_source,
        bootstrap_bytecode,
        bootstrap_external_source: bootstrap_external_source,
        bootstrap_external_bytecode,
    }
}

/// Represents a resource entry. Simple a name-value pair.
pub struct BlobEntry {
    pub name: String,
    pub data: Vec<u8>,
}

/// Represents an ordered collection of resource entries.
pub type BlobEntries = Vec<BlobEntry>;

/// Serialize a BlobEntries to a writer.
///
/// Format:
///    Little endian u32 total number of entries.
///    Array of 2-tuples of
///        Little endian u32 length of entity name
///        Little endian u32 length of entity value
///    Vector of entity names, with no padding
///    Vector of entity values, with no padding
///
/// The "index" data is self-contained in the beginning of the data structure
/// to allow a linear read of a contiguous memory region in order to load
/// the index.
pub fn write_blob_entries<W: Write>(mut dest: W, entries: &BlobEntries) -> std::io::Result<()> {
    dest.write_u32::<LittleEndian>(entries.len() as u32)?;

    for entry in entries.iter() {
        let name_bytes = entry.name.as_bytes();
        dest.write_u32::<LittleEndian>(name_bytes.len() as u32)?;
        dest.write_u32::<LittleEndian>(entry.data.len() as u32)?;
    }

    for entry in entries.iter() {
        let name_bytes = entry.name.as_bytes();
        dest.write(name_bytes)?;
    }

    for entry in entries.iter() {
        dest.write(entry.data.as_slice())?;
    }

    Ok(())
}

/// Produce the content of the config.c file containing built-in extensions.
fn make_config_c(dist: &PythonDistributionInfo, extensions: &BTreeSet<&String>) -> String {
    // It is easier to construct the file from scratch than parse the template
    // and insert things in the right places.
    let mut lines: Vec<String> = Vec::new();

    lines.push(String::from("#include \"Python.h\""));

    // Entries from config_c.in are special and always present.
    for (_, init_fn) in &dist.config_c_in.init_mods {
        if init_fn == "NULL" {
            continue;
        }

        lines.push(String::from(format!("extern PyObject* {}(void);", init_fn)));
    }

    for (module, init_fn) in &dist.config_c.init_mods {
        if dist.config_c_in.init_mods.contains_key(module) {
            continue;
        }

        if !extensions.contains(module) {
            continue;
        }

        lines.push(String::from(format!("extern PyObject* {}(void);", init_fn)));
    }

    lines.push(String::from("struct _inittab _PyImport_Inittab[] = {"));

    for (module, init_fn) in &dist.config_c_in.init_mods {
        lines.push(String::from(format!("{{\"{}\", {}}},", module, init_fn)));
    }

    for (module, init_fn) in &dist.config_c.init_mods {
        if dist.config_c_in.init_mods.contains_key(module) {
            continue;
        }

        if !extensions.contains(module) {
            continue;
        }

        lines.push(String::from(format!("{{\"{}\", {}}},", module, init_fn)));
    }

    lines.push(String::from("{0, 0}"));
    lines.push(String::from("};"));

    lines.join("\n")
}

/// Create a static libpython from a Python distribution.
pub fn link_libpython(dist: &PythonDistributionInfo) {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    let temp_dir = tempdir::TempDir::new("libpython").unwrap();
    let temp_dir_path = temp_dir.path();

    let mut build = cc::Build::new();

    for (rel_path, fs_path) in &dist.objs_core {
        let parent = temp_dir_path.join(rel_path.parent().unwrap());
        create_dir_all(parent).unwrap();

        let full = temp_dir_path.join(rel_path);
        fs::copy(fs_path, &full).expect("unable to copy object file");

        build.object(&full);
    }

    // Always include some Modules/ object files that are part of the "core"
    // modules functionality.
    let modules_path = temp_dir_path.join("Modules");
    create_dir_all(modules_path).unwrap();

    for object_filename in &dist.extension_modules_always {
        let module_path = PathBuf::from(format!("Modules/{}", object_filename));
        let fs_path = dist.objs_modules.get(&module_path).expect(&format!("object file not found: {}", module_path.to_str().unwrap()));

        let full = temp_dir_path.join(module_path);
        fs::copy(fs_path, &full).expect("unable to copy object file");

        build.object(&full);
    }

    // Relevant extension modules are the intersection of modules that are
    // built/available and what's requested from the current config.
    let mut extension_modules: BTreeSet<&String> = BTreeSet::from_iter(dist.extension_modules.keys());

    for e in OS_IGNORE_EXTENSIONS.as_slice() {
        extension_modules.remove(&String::from(*e));
    }

    // TODO accept an argument that specifies which extension modules are
    // relevant.

    // We derive a custom Modules/config.c from the set of extension modules.
    // We need to do this because config.c defines the built-in extensions and
    // their initialization functions and the file generated by the source
    // distribution may not align with what we want.
    let config_c_source = make_config_c(&dist, &extension_modules);
    let config_c_path = out_dir.join("config.c");

    fs::write(&config_c_path, config_c_source.as_bytes()).expect("unable to write config.c");

    // We need to make all .h includes accessible.
    for (name, fs_path) in &dist.includes {
        let full = temp_dir_path.join(name);

        create_dir_all(full.parent().expect("parent directory")).expect("create include directory");

        fs::copy(fs_path, full).expect("unable to copy include file");
    }

    // TODO flags should come from parsed distribution config.
    cc::Build::new()
        .file(config_c_path)
        .include(temp_dir_path)
        .define("NDEBUG", None)
        .define("Py_BUILD_CORE", None)
        .flag("-std=c99")
        .compile("pyembeddedconfig");

    // For each extension module, extract and use its object file. We also
    // use this pass to collect the set of libraries that we need to link
    // against.
    let mut needed_libraries: BTreeSet<&str> = BTreeSet::new();
    let mut needed_frameworks: BTreeSet<&str> = BTreeSet::new();

    for name in extension_modules {
        let entry = dist.extension_modules.get(name).unwrap();

        for path in &entry.object_paths {
            build.object(path);
        }

        for entry in &entry.links {
            if entry.framework {
                needed_frameworks.insert(&entry.name);
                println!("framework {} required by {}", entry.name, name);
            }

            else if let Some(_lib) = &entry.static_path {
                needed_libraries.insert(&entry.name);
                println!("static library {} required by {}", entry.name, name);
            }
            else if let Some(_lib) = &entry.dynamic_path {
                needed_libraries.insert(&entry.name);
                println!("dynamic library {} required by {}", entry.name, name);
            }
        }
    }

    for library in needed_libraries {
        if OS_IGNORE_LIBRARIES.contains(&library) {
            continue;
        }

        // Otherwise find the library in the distribution. Extract it. And statically link against it.
        let fs_path = dist.libraries.get(library).expect(&format!("unable to find library {}", library));

        let library_path = out_dir.join(format!("lib{}.a", library));
        fs::copy(fs_path, library_path).expect("unable to copy library file");

        println!("cargo:rustc-link-lib=static={}", library);
    }

    for framework in needed_frameworks {
        println!("cargo:rustc-link-lib=framework={}", framework);
    }

    build.compile("pyembedded");
}

pub fn is_stdlib_test_package(name: &str) -> bool {
    for package in STDLIB_TEST_PACKAGES {
        let prefix = format!("{}.", package);

        if name.starts_with(&prefix) {
            return true;
        }
    }

    false
}