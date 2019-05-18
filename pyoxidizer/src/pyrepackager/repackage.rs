// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use byteorder::{LittleEndian, WriteBytesExt};
use lazy_static::lazy_static;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::fs::create_dir_all;
use std::io::{BufRead, BufReader, Cursor, Error as IOError, Read, Write};
use std::path::{Path, PathBuf};

use super::bytecode::BytecodeCompiler;
use super::config::{parse_config, Config, PythonExtensions, PythonPackaging, RunMode};
use super::dist::{
    analyze_python_distribution_tar_zst, resolve_python_distribution_archive,
    PythonDistributionInfo,
};
use super::fsscan::{find_python_resources, PythonResourceType};

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
        } else if cfg!(target_os = "macos") {
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

pub fn is_stdlib_test_package(name: &str) -> bool {
    for package in STDLIB_TEST_PACKAGES {
        let prefix = format!("{}.", package);

        if name.starts_with(&prefix) {
            return true;
        }
    }

    false
}

/// Represents a resource entry. Simply a name-value pair.
pub struct BlobEntry {
    pub name: String,
    pub data: Vec<u8>,
}

/// Represents an ordered collection of resource entries.
pub type BlobEntries = Vec<BlobEntry>;

/// Represents a resource to make available to the Python interpreter.
#[derive(Debug)]
pub enum PythonResource {
    ModuleSource { name: String, source: Vec<u8> },
    ModuleBytecode { name: String, bytecode: Vec<u8> },
    Resource { name: String, data: Vec<u8> },
}

#[derive(Debug)]
pub enum ResourceAction {
    Add,
    Remove,
}

#[derive(Debug)]
pub struct PythonResourceEntry {
    action: ResourceAction,
    resource: PythonResource,
}

/// Represents Python resources to embed in a binary.
pub struct PythonResources {
    pub module_sources: BTreeMap<String, Vec<u8>>,
    pub module_bytecodes: BTreeMap<String, Vec<u8>>,
    pub all_modules: BTreeSet<String>,
    pub resources: BTreeMap<String, Vec<u8>>,
    pub read_files: Vec<PathBuf>,
}

impl PythonResources {
    pub fn sources_blob(&self) -> BlobEntries {
        let mut sources = BlobEntries::new();

        for (name, source) in &self.module_sources {
            sources.push(BlobEntry {
                name: name.clone(),
                data: source.clone(),
            });
        }

        sources
    }

    pub fn bytecodes_blob(&self) -> BlobEntries {
        let mut bytecodes = BlobEntries::new();

        for (name, bytecode) in &self.module_bytecodes {
            bytecodes.push(BlobEntry {
                name: name.clone(),
                data: bytecode.clone(),
            });
        }

        bytecodes
    }

    pub fn write_blobs(
        &self,
        module_names_path: &PathBuf,
        modules_path: &PathBuf,
        bytecodes_path: &PathBuf,
    ) {
        let mut fh = fs::File::create(module_names_path).expect("error creating file");
        for name in &self.all_modules {
            fh.write_all(name.as_bytes()).expect("failed to write");
            fh.write_all(b"\n").expect("failed to write");
        }

        let fh = fs::File::create(modules_path).unwrap();
        write_blob_entries(&fh, &self.sources_blob()).unwrap();

        let fh = fs::File::create(bytecodes_path).unwrap();
        write_blob_entries(&fh, &self.bytecodes_blob()).unwrap();
    }
}

fn read_resource_names_file(path: &Path) -> Result<BTreeSet<String>, IOError> {
    let fh = fs::File::open(path)?;

    let mut res: BTreeSet<String> = BTreeSet::new();

    for line in BufReader::new(fh).lines() {
        let line = line?;

        if line.starts_with('#') || line.is_empty() {
            continue;
        }

        res.insert(line);
    }

    Ok(res)
}

fn bytecode_compiler(dist: &PythonDistributionInfo) -> BytecodeCompiler {
    BytecodeCompiler::new(&dist.python_exe)
}

fn filter_btreemap<K: std::clone::Clone + std::cmp::Ord, V>(
    m: &mut BTreeMap<K, V>,
    f: &BTreeSet<K>,
) {
    let keys: Vec<K> = m.keys().cloned().collect();

    for key in keys {
        if !f.contains(&key) {
            m.remove(&key);
        }
    }
}

/// Resolves a Python packaging rule to resources to package.
fn resolve_python_packaging(
    package: &PythonPackaging,
    dist: &PythonDistributionInfo,
) -> Vec<PythonResourceEntry> {
    let mut res = Vec::new();

    match package {
        PythonPackaging::Stdlib {
            optimize_level,
            exclude_test_modules,
            include_source,
        } => {
            let mut compiler = bytecode_compiler(&dist);

            for (name, fs_path) in &dist.py_modules {
                if is_stdlib_test_package(&name) && *exclude_test_modules {
                    println!("skipping test stdlib module: {}", name);
                    continue;
                }

                let source = fs::read(fs_path).expect("error reading source file");

                let bytecode = match compiler.compile(&source, &name, *optimize_level as i32) {
                    Ok(res) => res,
                    Err(msg) => panic!("error compiling bytecode: {}", msg),
                };

                if *include_source {
                    res.push(PythonResourceEntry {
                        action: ResourceAction::Add,
                        resource: PythonResource::ModuleSource {
                            name: name.clone(),
                            source,
                        },
                    });
                }

                res.push(PythonResourceEntry {
                    action: ResourceAction::Add,
                    resource: PythonResource::ModuleBytecode {
                        name: name.clone(),
                        bytecode,
                    },
                });
            }
        }
        PythonPackaging::Virtualenv {
            path,
            optimize_level,
            excludes,
            include_source,
        } => {
            let mut packages_path = PathBuf::from(path);

            if dist.os == "windows" {
                packages_path.push("Lib");
            } else {
                packages_path.push("lib");
            }

            packages_path.push("python".to_owned() + &dist.version[0..3]);
            packages_path.push("site-packages");

            let mut compiler = bytecode_compiler(&dist);

            for resource in find_python_resources(&packages_path) {
                match resource.flavor {
                    PythonResourceType::Source => {
                        let mut relevant = true;

                        for exclude in excludes {
                            let prefix = exclude.clone() + ".";

                            if &resource.name == exclude || resource.name.starts_with(&prefix) {
                                relevant = false;
                            }
                        }

                        if !relevant {
                            continue;
                        }

                        let source = fs::read(resource.path).expect("error reading source file");
                        let bytecode =
                            match compiler.compile(&source, &resource.name, *optimize_level as i32)
                            {
                                Ok(res) => res,
                                Err(msg) => panic!("error compiling bytecode: {}", msg),
                            };

                        if *include_source {
                            res.push(PythonResourceEntry {
                                action: ResourceAction::Add,
                                resource: PythonResource::ModuleSource {
                                    name: resource.name.clone(),
                                    source,
                                },
                            });
                        }

                        res.push(PythonResourceEntry {
                            action: ResourceAction::Add,
                            resource: PythonResource::ModuleBytecode {
                                name: resource.name.clone(),
                                bytecode,
                            },
                        });
                    }
                    _ => {}
                }
            }
        }
        PythonPackaging::PackageRoot {
            path,
            packages,
            optimize_level,
            excludes,
            include_source,
        } => {
            let path = PathBuf::from(path);

            let mut compiler = bytecode_compiler(&dist);

            for resource in find_python_resources(&path) {
                match resource.flavor {
                    PythonResourceType::Source => {
                        let mut relevant = false;

                        for package in packages {
                            let prefix = package.clone() + ".";

                            if &resource.name == package || resource.name.starts_with(&prefix) {
                                relevant = true;
                            }
                        }

                        for exclude in excludes {
                            let prefix = exclude.clone() + ".";

                            if &resource.name == exclude || resource.name.starts_with(&prefix) {
                                relevant = false;
                            }
                        }

                        if !relevant {
                            continue;
                        }

                        let source = fs::read(resource.path).expect("error reading source file");
                        let bytecode =
                            match compiler.compile(&source, &resource.name, *optimize_level as i32)
                            {
                                Ok(res) => res,
                                Err(msg) => panic!("error compiling bytecode: {}", msg),
                            };

                        if *include_source {
                            res.push(PythonResourceEntry {
                                action: ResourceAction::Add,
                                resource: PythonResource::ModuleSource {
                                    name: resource.name.clone(),
                                    source,
                                },
                            });
                        }

                        res.push(PythonResourceEntry {
                            action: ResourceAction::Add,
                            resource: PythonResource::ModuleBytecode {
                                name: resource.name.clone(),
                                bytecode,
                            },
                        });
                    }
                    _ => {}
                }
            }
        }
        // This is a no-op because it can only be handled at a higher level.
        PythonPackaging::FilterFileInclude { .. } => {}
    }

    res
}

/// Resolves a series of packaging rules to a final set of resources to package.
pub fn resolve_python_resources(
    packages: &Vec<PythonPackaging>,
    dist: &PythonDistributionInfo,
) -> PythonResources {
    let mut sources: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut bytecodes: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut resources: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut read_files: Vec<PathBuf> = Vec::new();

    for packaging in packages {
        for entry in resolve_python_packaging(packaging, dist) {
            match (entry.action, entry.resource) {
                (ResourceAction::Add, PythonResource::ModuleSource { name, source }) => {
                    sources.insert(name.clone(), source);
                }
                (ResourceAction::Remove, PythonResource::ModuleSource { name, .. }) => {
                    sources.remove(&name);
                }
                (ResourceAction::Add, PythonResource::ModuleBytecode { name, bytecode }) => {
                    bytecodes.insert(name.clone(), bytecode);
                }
                (ResourceAction::Remove, PythonResource::ModuleBytecode { name, .. }) => {
                    bytecodes.remove(&name);
                }
                (ResourceAction::Add, PythonResource::Resource { name, data }) => {
                    resources.insert(name, data);
                }
                (ResourceAction::Remove, PythonResource::Resource { name, .. }) => {
                    resources.remove(&name);
                }
            }
        }

        if let PythonPackaging::FilterFileInclude { path } = packaging {
            let path = Path::new(path);
            let include_names =
                read_resource_names_file(path).expect("failed to read resource names file");

            filter_btreemap(&mut sources, &include_names);
            filter_btreemap(&mut bytecodes, &include_names);
            filter_btreemap(&mut resources, &include_names);

            read_files.push(PathBuf::from(path));
        }
    }

    let mut all_modules: BTreeSet<String> = BTreeSet::new();
    for name in sources.keys() {
        all_modules.insert(name.to_string());
    }
    for name in bytecodes.keys() {
        all_modules.insert(name.to_string());
    }

    PythonResources {
        module_sources: sources,
        module_bytecodes: bytecodes,
        all_modules,
        resources,
        read_files,
    }
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
    let mut compiler = bytecode_compiler(&dist);

    let mod_bootstrap_path = &dist.py_modules["importlib._bootstrap"];
    let mod_bootstrap_external_path = &dist.py_modules["importlib._bootstrap_external"];

    let bootstrap_source = fs::read(&mod_bootstrap_path).expect("unable to read bootstrap source");
    let module_name = "<frozen importlib._bootstrap>";
    let bootstrap_bytecode = compiler
        .compile(&bootstrap_source, module_name, 0)
        .expect("error compiling bytecode");

    let mut bootstrap_external_source =
        fs::read(&mod_bootstrap_external_path).expect("unable to read bootstrap_external source");
    bootstrap_external_source.extend("\n# END OF importlib/_bootstrap_external.py\n\n".bytes());
    bootstrap_external_source.extend(PYTHON_IMPORTER);
    let module_name = "<frozen importlib._bootstrap_external>";
    let bootstrap_external_bytecode = compiler
        .compile(&bootstrap_external_source, module_name, 0)
        .expect("error compiling bytecode");

    ImportlibData {
        bootstrap_source,
        bootstrap_bytecode,
        bootstrap_external_source,
        bootstrap_external_bytecode,
    }
}

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
        dest.write_all(name_bytes)?;
    }

    for entry in entries.iter() {
        dest.write_all(entry.data.as_slice())?;
    }

    Ok(())
}

/// Produce the content of the config.c file containing built-in extensions.
fn make_config_c(dist: &PythonDistributionInfo, extensions: &BTreeSet<&String>) -> String {
    // It is easier to construct the file from scratch than parse the template
    // and insert things in the right places.
    let mut lines: Vec<String> = Vec::new();

    lines.push(String::from("#include \"Python.h\""));

    // Declare the initialization functions.
    for variants in dist.extension_modules.values() {
        // TODO support choosing variant.
        let entry = &variants[0];

        if !entry.builtin_default && !extensions.contains(&entry.module) {
            continue;
        }

        if let Some(init_fn) = &entry.init_fn {
            if init_fn == "NULL" {
                continue;
            }

            lines.push(format!("extern PyObject* {}(void);", init_fn));
        }
    }

    lines.push(String::from("struct _inittab _PyImport_Inittab[] = {"));

    for variants in dist.extension_modules.values() {
        // TODO support choosing variant.
        let entry = &variants[0];

        if !entry.builtin_default && !extensions.contains(&entry.module) {
            continue;
        }

        if let Some(init_fn) = &entry.init_fn {
            if init_fn == "NULL" {
                continue;
            }

            lines.push(format!("{{\"{}\", {}}},", entry.module, init_fn));
        }
    }

    lines.push(String::from("{0, 0}"));
    lines.push(String::from("};"));

    lines.join("\n")
}

/// Create a static libpython from a Python distribution.
///
/// This should only be called from the context of a build script, as it
/// emits cargo: lines to stdout.
pub fn link_libpython(config: &Config, dist: &PythonDistributionInfo) {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    let temp_dir = tempdir::TempDir::new("libpython").unwrap();
    let temp_dir_path = temp_dir.path();

    let mut build = cc::Build::new();

    for (rel_path, fs_path) in &dist.objs_core {
        let parent = temp_dir_path.join(rel_path.parent().unwrap());
        create_dir_all(parent).unwrap();

        let full = temp_dir_path.join(rel_path);
        fs::copy(fs_path, &full).expect("unable to copy object file");

        println!("adding {:?} to embedded Python", full);
        build.object(&full);
    }

    // Relevant extension modules are the intersection of modules that are
    // built/available and what's requested from the current config.
    let mut extension_modules: BTreeSet<&String> = BTreeSet::new();

    for (name, em) in &dist.extension_modules {
        // Always include builtin extensions that must be present.
        for extension in em {
            if extension.builtin_default {
                extension_modules.insert(name);
            }
            if extension.required {
                extension_modules.insert(name);
            }
        }

        match &config.python_extensions {
            PythonExtensions::All {} => {
                extension_modules.insert(name);
            }
            PythonExtensions::None {} => {}
            PythonExtensions::NoLibraries {} => {
                let mut have_library = false;

                for extension in em {
                    if !extension.links.is_empty() {
                        have_library = true;
                    }
                }

                if !have_library {
                    extension_modules.insert(name);
                }
            }
            PythonExtensions::ExplicitIncludes { includes } => {
                if includes.contains(&name.to_owned()) {
                    extension_modules.insert(name);
                }
            }
            PythonExtensions::ExplicitExcludes { excludes } => {
                if !excludes.contains(&name.to_owned()) {
                    extension_modules.insert(name);
                }
            }
        }
    }

    for e in OS_IGNORE_EXTENSIONS.as_slice() {
        extension_modules.remove(&String::from(*e));
    }

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
    let mut needed_system_libraries: BTreeSet<&str> = BTreeSet::new();

    for entry in &dist.links_core {
        if entry.framework {
            println!("framework {} required by core", entry.name);
            needed_frameworks.insert(&entry.name);
        } else if entry.system {
            println!("system library {} required by core", entry.name);
            needed_system_libraries.insert(&entry.name);
        }
        // TODO handle static/dynamic libraries.
    }

    for name in extension_modules {
        println!("adding extension {}", name);
        let variants = &dist.extension_modules[name];

        // TODO support choosing which variant is used.
        let entry = &variants[0];

        if entry.builtin_default {
            println!(
                "{} is built-in and doesn't need special build actions",
                name
            );
            continue;
        }

        for path in &entry.object_paths {
            println!("adding object file {:?} for extension {}", path, name);
            build.object(path);
        }

        for entry in &entry.links {
            if entry.framework {
                needed_frameworks.insert(&entry.name);
                println!("framework {} required by {}", entry.name, name);
            } else if entry.system {
                println!("system library {} required by {}", entry.name, name);
                needed_system_libraries.insert(&entry.name);
            } else if let Some(_lib) = &entry.static_path {
                needed_libraries.insert(&entry.name);
                println!("static library {} required by {}", entry.name, name);
            } else if let Some(_lib) = &entry.dynamic_path {
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
        let fs_path = dist
            .libraries
            .get(library)
            .expect(&format!("unable to find library {}", library));
        println!("{:?}", fs_path);

        let library_path = out_dir.join(format!("lib{}.a", library));
        fs::copy(fs_path, library_path).expect("unable to copy library file");

        println!("cargo:rustc-link-lib=static={}", library);
    }

    for framework in needed_frameworks {
        println!("cargo:rustc-link-lib=framework={}", framework);
    }

    for lib in needed_system_libraries {
        println!("cargo:rustc-link-lib={}", lib);
    }

    // python3-sys uses #[link(name="pythonXY")] attributes heavily on Windows. Its
    // build.rs then remaps ``pythonXY`` to e.g. ``python37``. This causes Cargo to
    // link against ``python37.lib`` (or ``pythonXY.lib`` if the
    // ``rustc-link-lib=pythonXY:python{}{}`` line is missing, which is the case
    // in our invocation).
    //
    // We don't want the "real" libpython being linked. And this is a very real
    // possibility since the path to it could be in an environment variable
    // outside of our control!
    //
    // In addition, we can't naively remap ``pythonXY`` ourselves without adding
    // a ``#[link]`` to the crate.
    //
    // Our current workaround is to produce a ``pythonXY.lib`` file. This satisfies
    // the requirement of ``python3-sys`` that a ``pythonXY.lib`` file exists.

    build.compile("pythonXY");
}

pub fn write_data_rs(
    path: &PathBuf,
    config: &Config,
    importlib_bootstrap_path: &PathBuf,
    importlib_bootstrap_external_path: &PathBuf,
    py_modules_path: &PathBuf,
    pyc_modules_path: &PathBuf,
) {
    let mut f = fs::File::create(&path).unwrap();

    f.write_fmt(format_args!(
        "pub const STANDARD_IO_ENCODING: Option<String> = {};\n",
        match &config.stdio_encoding_name {
            // TODO print out value.
            Some(_value) => "Some(\"\")",
            None => "None",
        }
    ))
    .unwrap();
    f.write_fmt(format_args!(
        "pub const STANDARD_IO_ERRORS: Option<String> = {};\n",
        match &config.stdio_encoding_errors {
            Some(_value) => "Some(\"\")",
            None => "None",
        }
    ))
    .unwrap();

    f.write_fmt(format_args!(
        "pub const DONT_WRITE_BYTECODE: bool = {};\n",
        config.dont_write_bytecode
    ))
    .unwrap();
    f.write_fmt(format_args!(
        "pub const IGNORE_ENVIRONMENT: bool = {};\n",
        config.ignore_environment
    ))
    .unwrap();
    f.write_fmt(format_args!(
        "pub const OPT_LEVEL: i32 = {};\n",
        config.optimize_level
    ))
    .unwrap();
    f.write_fmt(format_args!(
        "pub const NO_SITE: bool = {};\n",
        config.no_site
    ))
    .unwrap();
    f.write_fmt(format_args!(
        "pub const NO_USER_SITE_DIRECTORY: bool = {};\n",
        config.no_user_site_directory
    ))
    .unwrap();
    f.write_fmt(format_args!(
        "pub const PROGRAM_NAME: &str = \"{}\";\n",
        config.program_name
    ))
    .unwrap();
    f.write_fmt(format_args!(
        "pub const UNBUFFERED_STDIO: bool = {};\n",
        config.unbuffered_stdio
    ))
    .unwrap();

    f.write_fmt(format_args!(
        "pub const FROZEN_IMPORTLIB_DATA: &'static [u8] = include_bytes!(r\"{}\");\n",
        importlib_bootstrap_path.to_str().unwrap()
    ))
    .unwrap();
    f.write_fmt(format_args!(
        "pub const FROZEN_IMPORTLIB_EXTERNAL_DATA: &'static [u8] = include_bytes!(r\"{}\");\n",
        importlib_bootstrap_external_path.to_str().unwrap()
    ))
    .unwrap();
    f.write_fmt(format_args!(
        "pub const PY_MODULES_DATA: &'static [u8] = include_bytes!(r\"{}\");\n",
        py_modules_path.to_str().unwrap()
    ))
    .unwrap();
    f.write_fmt(format_args!(
        "pub const PYC_MODULES_DATA: &'static [u8] = include_bytes!(r\"{}\");\n",
        pyc_modules_path.to_str().unwrap()
    ))
    .unwrap();

    f.write_fmt(format_args!(
        "pub const RUN_MODE: i32 = {};\n",
        match config.run {
            RunMode::Repl {} => 0,
            RunMode::Module { .. } => 1,
            RunMode::Eval { .. } => 2,
        }
    ))
    .unwrap();

    f.write_fmt(format_args!(
        "pub const RUN_MODULE_NAME: Option<&'static str> = {};\n",
        match &config.run {
            RunMode::Module { module } => "Some(\"".to_owned() + &module + "\")",
            _ => "None".to_owned(),
        }
    ))
    .unwrap();

    f.write_fmt(format_args!(
        "pub const RUN_CODE: Option<&'static str> = {};\n",
        match &config.run {
            RunMode::Eval { code } => "Some(\"".to_owned() + &code + "\")",
            _ => "None".to_owned(),
        }
    ))
    .unwrap();

    f.write_fmt(format_args!(
        "pub const WRITE_MODULES_DIRECTORY_ENV: Option<&'static str> = {};\n",
        match &config.write_modules_directory_env {
            Some(path) => "Some(\"".to_owned() + &path + "\")",
            _ => "None".to_owned(),
        }
    ))
    .unwrap();
}

pub fn process_config(config_path: &Path, out_dir: &Path) {
    let mut fh = fs::File::open(config_path).unwrap();

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
    let python_distribution_path = resolve_python_distribution_archive(&config, &out_dir);
    let mut fh = fs::File::open(python_distribution_path).unwrap();
    let mut python_distribution_data = Vec::new();
    fh.read_to_end(&mut python_distribution_data).unwrap();
    let dist_cursor = Cursor::new(python_distribution_data);
    let dist = analyze_python_distribution_tar_zst(dist_cursor).unwrap();

    // Produce a static library containing the Python bits we need.
    // As a side-effect, this will emit the cargo: lines needed to link this
    // library.
    link_libpython(&config, &dist);

    // Produce the frozen importlib modules.
    let importlib = derive_importlib(&dist);

    let importlib_bootstrap_path = Path::new(&out_dir).join("importlib_bootstrap.pyc");
    let mut fh = fs::File::create(&importlib_bootstrap_path).unwrap();
    fh.write_all(&importlib.bootstrap_bytecode).unwrap();

    let importlib_bootstrap_external_path =
        Path::new(&out_dir).join("importlib_bootstrap_external.pyc");
    let mut fh = fs::File::create(&importlib_bootstrap_external_path).unwrap();
    fh.write_all(&importlib.bootstrap_external_bytecode)
        .unwrap();

    let resources = resolve_python_resources(&config.python_packaging, &dist);

    for p in &resources.read_files {
        println!("cargo:rerun-if-changed={}", p.to_str().unwrap());
    }

    // Produce the packed data structures containing Python modules.
    // TODO there is tons of room to customize this behavior, including
    // reordering modules so the memory order matches import order.

    let module_names_path = Path::new(&out_dir).join("py-module-names");
    let py_modules_path = Path::new(&out_dir).join("py-modules");
    let pyc_modules_path = Path::new(&out_dir).join("pyc-modules");

    resources.write_blobs(&module_names_path, &py_modules_path, &pyc_modules_path);

    let dest_path = Path::new(&out_dir).join("data.rs");
    write_data_rs(
        &dest_path,
        &config,
        &importlib_bootstrap_path,
        &importlib_bootstrap_external_path,
        &py_modules_path,
        &pyc_modules_path,
    );
}

pub fn find_pyoxidizer_config_file(start_dir: &Path, target: &str) -> Option<PathBuf> {
    let basename = format!("pyoxidizer.{}.toml", target);

    for test_dir in start_dir.ancestors() {
        let candidate = test_dir.to_path_buf().join(&basename);

        if candidate.exists() {
            return Some(candidate);
        }
    }

    None
}

/// Runs packaging/embedding from the context of a build script.
///
/// This function should be called by the build script for the package
/// that wishes to embed a Python interpreter/application. When called,
/// a PyOxidizer configuration file is found and read. The configuration
/// is then applied to the current build. This involves obtaining a
/// Python distribution to embed (possibly by downloading it from the Internet),
/// analyzing the contents of that distribution, extracting relevant files
/// from the distribution, compiling Python bytecode, and generating
/// resources required to build the ``pyembed`` crate/modules.
///
/// If everything works as planned, this whole process should be largely
/// invisible and the calling application will have an embedded Python
/// interpreter when it is built.
pub fn run_from_build(build_script: &str) {
    // Adding our our rerun-if-changed lines will overwrite the default, so
    // we need to emit the build script name explicitly.
    println!("cargo:rerun-if-changed={}", build_script);

    println!("cargo:rerun-if-env-changed=PYOXIDIZER_CONFIG");

    let config_path = match env::var("PYOXIDIZER_CONFIG") {
        Ok(config_env) => {
            println!(
                "using PyOxidizer config file from PYOXIDIZER_CONFIG: {}",
                config_env
            );
            PathBuf::from(config_env)
        }
        Err(_) => {
            let target = env::var("TARGET").expect("TARGET not found");
            let manifest_dir =
                env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not found");

            let path = find_pyoxidizer_config_file(&PathBuf::from(manifest_dir), &target);

            if path.is_none() {
                panic!("Could not find PyOxidizer config file");
            }

            path.unwrap()
        }
    };

    if !config_path.exists() {
        panic!("PyOxidizer config file does not exist");
    }

    println!(
        "cargo:rerun-if-changed={}",
        config_path.to_str().expect("could not convert path to str")
    );

    let out_dir = env::var("OUT_DIR").unwrap();
    let out_dir_path = Path::new(&out_dir);

    process_config(&config_path, out_dir_path);
}
