// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use itertools::Itertools;
use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{PathBuf, Path};

use crate::fsscan::{find_python_resources, PythonResourceType, walk_tree_files};

#[allow(unused)]
struct PkgConfig {
    version: String,
    stdlib_path: PathBuf,
}

/// Parse useful information out of Python's pkgconfig file.
fn parse_pkgconfig(dist_path: &Path) -> PkgConfig {
    let python_pc = dist_path.join("python/install/lib/pkgconfig/python3.pc");

    let buf = fs::read(python_pc).expect("failed to read pkgconfig");

    let reader = BufReader::new(&*buf);

    let mut version: String = String::new();

    for line in reader.lines() {
        let line = line.unwrap();

        if line.starts_with("Version: ") {
            version.insert_str(0, &line[9..])
        }
    }

    let stdlib_path = PathBuf::from(format!("lib/python{}", version));

    PkgConfig {
        version,
        stdlib_path,
    }
}

/// Represents contents of the config.c/config.c.in file.
#[derive(Debug)]
#[allow(unused)]
pub struct ConfigC {
    pub init_funcs: Vec<String>,
    pub init_mods: BTreeMap<String, String>,
}

/// Parse the content of a config.c/config.c.in file from CPython.
fn parse_config_c(data: &Vec<u8>) -> ConfigC {
    let reader = BufReader::new(&**data);

    let re_extern = regex::Regex::new(r"extern PyObject\* ([^\(]+)\(void\);").unwrap();
    let re_inittab_entry = regex::Regex::new(r##"\{"([^"]+)", ([^\}]+)\},"##).unwrap();

    let mut init_funcs: Vec<String> = Vec::new();
    let mut init_mods: BTreeMap<String, String> = BTreeMap::new();

    for line in reader.lines() {
        let line = line.unwrap();

        match re_extern.captures(&line) {
            Some(caps) => {
                init_funcs.push(caps.get(1).unwrap().as_str().to_string());
                ()
            },
            None => (),
        }

        match re_inittab_entry.captures(&line) {
            Some(caps) => {
                init_mods.insert(caps.get(1).unwrap().as_str().to_string(), caps.get(2).unwrap().as_str().to_string());
                ()
            },
            None => (),
        }
    }

    ConfigC {
        init_funcs,
        init_mods,
    }
}

#[derive(Clone, Debug)]
pub struct SetupEntry {
    pub module: String,
    pub object_filenames: Vec<String>,
    pub libraries: Vec<String>,
    pub frameworks: Vec<String>,
}

/// Parse a line in CPython's Setup.dist/Setup.local file.
fn parse_setup_line(modules: &mut BTreeMap<String, SetupEntry>, line: &str) {
    let line = match line.find("#") {
        Some(idx) => &line[0..idx],
        None => &line,
    };

    if line.len() < 1 {
        return;
    }

    // Lines have format: <module_name> <args>
    let words = line.split_whitespace().collect_vec();

    if words.len() < 2 {
        return;
    }

    let module = words[0];
    let mut object_filenames: Vec<String> = Vec::new();
    let mut libraries: Vec<String> = Vec::new();
    let mut frameworks: Vec<String> = Vec::new();

    for (idx, &word) in words.iter().enumerate() {
        // Object files are the basename of sources with the extension changed.
        if word.ends_with(".c") {
            let p = PathBuf::from(&word);
            let p = p.with_extension("o");
            let basename = p.file_name().unwrap().to_str().unwrap();
            object_filenames.push(basename.to_string());

        }
        else if word.starts_with("-l") {
            libraries.push(word[2..].to_string());
        }
        else if word == "-framework" {
            frameworks.push(String::from(words[idx + 1]));
        }
    }

    let entry = SetupEntry {
        module: module.to_string(),
        object_filenames,
        libraries,
        frameworks,
    };

    modules.insert(module.to_string(), entry);
}

/// Parse CPython's Setup.dist file.
fn parse_setup_dist(modules: &mut BTreeMap<String, SetupEntry>, data: &Vec<u8>) {
    let reader = BufReader::new(&**data);

    let mut found_start = false;

    for line in reader.lines() {
        let line = line.expect("could not obtain line");
        if !found_start {
            if line.starts_with("PYTHONPATH=") {
                found_start = true;
                continue;
            }
        }

        parse_setup_line(modules, &line);
    }
}

/// Parse CPython's Setup.local file.
fn parse_setup_local(modules: &mut BTreeMap<String, SetupEntry>, data: &Vec<u8>) {
    let reader = BufReader::new(&**data);

    for line in reader.lines() {
        let line = line.expect("could not obtain line");

        // Everything after the *disabled* line can be ignored.
        if line == "*disabled*" {
            break;
        }
        else if line == "*static*" {
            continue;
        }

        parse_setup_line(modules, &line);
    }
}

#[allow(unused)]
#[derive(Clone, Debug)]
pub struct PythonModuleData {
    pub py: PathBuf,
}

/// Represents a parsed Python distribution.
///
/// Distribution info is typically derived from a tarball containing a
/// Python install and its build artifacts.
#[allow(unused)]
#[derive(Debug)]
pub struct PythonDistributionInfo {
    /// Directory where distribution lives in the filesystem.
    pub temp_dir: tempdir::TempDir,

    pub config_c: ConfigC,
    pub config_c_in: ConfigC,
    pub extension_modules: BTreeMap<String, SetupEntry>,
    pub extension_modules_always: Vec<String>,
    pub frozen_c: Vec<u8>,

    /// Include files for Python.
    ///
    /// Keys are relative paths. Values are filesystem paths.
    pub includes: BTreeMap<String, PathBuf>,

    /// Static libraries available for linking.
    ///
    /// Keys are library names, without the "lib" prefix or file extension.
    /// Values are filesystem paths where library is located.
    pub libraries: BTreeMap<String, PathBuf>,

    /// Object files providing the core Python implementation.
    ///
    /// Keys are relative paths. Values are filesystem paths.
    pub objs_core: BTreeMap<PathBuf, PathBuf>,

    /// Object files providing extension modules.
    ///
    /// Keys are relative paths. Values are filesystem paths.
    pub objs_modules: BTreeMap<PathBuf, PathBuf>,

    pub py_modules: BTreeMap<String, PythonModuleData>,

    /// Non-module Python resource files.
    ///
    /// Keys are full module/resource names. Values are filesystem paths.
    pub resources: BTreeMap<String, PathBuf>,
}

/// Extract useful information from the files constituting a Python distribution.
///
/// Passing in a data structure with raw file data within is inefficient. But
/// it makes things easier to implement and allows us to do things like consume
/// tarballs without filesystem I/O.
pub fn analyze_python_distribution_data(temp_dir: tempdir::TempDir) -> Result<PythonDistributionInfo, &'static str> {
    let mut objs_core: BTreeMap<PathBuf, PathBuf> = BTreeMap::new();
    let mut objs_modules: BTreeMap<PathBuf, PathBuf> = BTreeMap::new();
    let mut config_c: Vec<u8> = Vec::new();
    let mut config_c_in: Vec<u8> = Vec::new();
    let mut extension_modules: BTreeMap<String, SetupEntry> = BTreeMap::new();
    let mut includes: BTreeMap<String, PathBuf> = BTreeMap::new();
    let mut libraries: BTreeMap<String, PathBuf> = BTreeMap::new();
    let mut frozen_c: Vec<u8> = Vec::new();
    let mut py_modules: BTreeMap<String, PythonModuleData> = BTreeMap::new();
    let mut resources: BTreeMap<String, PathBuf> = BTreeMap::new();

    for entry in fs::read_dir(temp_dir.path()).unwrap() {
        let entry = entry.expect("unable to get directory entry");

        match entry.file_name().to_str() {
            Some("python") => continue,
            Some(value) => panic!("unexpected entry in distribution root directory: {}", value),
            _ => panic!("error listing root directory of Python distribution"),
        };
    }

    let python_path = temp_dir.path().join("python");

    for entry in fs::read_dir(&python_path).unwrap() {
        let entry = entry.expect("unable to get directory entry");

        match entry.file_name().to_str() {
            Some("build") => continue,
            Some("install") => continue,
            Some("lib") => continue,
            Some("LICENSE.rst") => continue,
            Some(value) => panic!("unexpected entry in python/ directory: {}", value),
            _ => panic!("error listing python/ directory"),
        };
    }

    let pkgconfig = parse_pkgconfig(temp_dir.path());

    let build_path = python_path.join("build");

    for entry in walk_tree_files(&build_path) {
        let full_path = entry.path();
        let rel_path = full_path.strip_prefix(&build_path).expect("unable to strip path prefix");
        let rel_str = rel_path.to_str().expect("unable to convert path to str");

        let components = rel_path.iter().collect::<Vec<_>>();

        if components.len() < 1 {
            continue;
        }

        if rel_str.ends_with(".o") {
            match components[0].to_str().unwrap() {
                "Modules" => {
                    objs_modules.insert(rel_path.to_path_buf(), full_path.to_path_buf());
                    ()
                },
                "Objects" => {
                    objs_core.insert(rel_path.to_path_buf(), full_path.to_path_buf());
                    ()
                },
                "Parser" => {
                    objs_core.insert(rel_path.to_path_buf(), full_path.to_path_buf());
                    ()
                },
                "Programs" => {},
                "Python" => {
                    objs_core.insert(rel_path.to_path_buf(), full_path.to_path_buf());
                    ()
                },
                _ => panic!("unexpected object file: {}", rel_str)
            }
        } else if rel_str == "Modules/config.c" {
            config_c = fs::read(full_path).expect("could not read path");
        } else if rel_str == "Modules/config.c.in" {
            config_c_in = fs::read(full_path).expect("could not read path");
        } else if rel_str == "Modules/Setup.dist" {
            let data = fs::read(full_path).expect("could not read path");
            parse_setup_dist(&mut extension_modules, &data);
        } else if rel_str == "Modules/Setup.local" {
            let data = fs::read(full_path).expect("could not read path");
            parse_setup_local(&mut extension_modules, &data);
        } else if rel_str == "Python/frozen.c" {
            frozen_c = fs::read(full_path).expect("could not read path");
        }
        else {
            panic!("unhandled build/ file: {}", rel_str);
        }
    }

    let lib_path = python_path.join("lib");

    for entry in walk_tree_files(&lib_path) {
        let full_path = entry.path();
        let rel_path = full_path.strip_prefix(&lib_path).expect("unable to strip path");
        let rel_str = rel_path.to_str().expect("could not convert path to str");

        if rel_str.ends_with(".a") {
            if ! rel_str.starts_with("lib") {
                panic!(".a file does not begin with lib: {:?}", rel_path);
            }

            let name = &rel_str[3..rel_str.len() - 2];
            libraries.insert(name.to_string(), full_path.to_path_buf());
        }
    }

    let include_path = python_path.join("install/include");

    for entry in walk_tree_files(&include_path) {
        let full_path = entry.path();
        let rel_path = full_path.strip_prefix(&include_path).expect("unable to strip prefix");

        let components = rel_path.iter().map(|p| p.to_str().unwrap()).collect::<Vec<_>>();
        let rel = itertools::join(&components[1..components.len()], "/");

        includes.insert(rel, full_path.to_path_buf());
    }

    let stdlib_path = python_path.join("install").join(pkgconfig.stdlib_path);

    for entry in find_python_resources(&stdlib_path) {
        match entry.flavor {
            PythonResourceType::Resource => {
                resources.insert(entry.name.to_string(), entry.path);
                ()
            },
            _ => (),
        };
    }

    for entry in walk_tree_files(&stdlib_path) {
        let full_path = entry.path();

        let rel_path = full_path.strip_prefix(&stdlib_path).expect("unable to strip path");
        let rel_str = rel_path.to_str().expect("cannot convert path to str");

        let components = rel_path.iter().map(|p| p.to_str().unwrap()).collect::<Vec<_>>();
        let package_parts = &components[0..components.len() - 1];
        let module_name = rel_path.file_stem().unwrap().to_str().unwrap();

        let mut full_module_name: Vec<&str> = package_parts.to_vec();
        full_module_name.push(module_name);

        let mut full_module_name = itertools::join(full_module_name, ".");

        if full_module_name.ends_with(".__init__") {
            full_module_name = full_module_name[0..full_module_name.len() - 9].to_string();
        }

        if ! rel_str.ends_with(".py") {
            continue;
        }

        if py_modules.contains_key(&full_module_name) {
            panic!("duplicate python module: {}", full_module_name);
        }

        py_modules.insert(full_module_name, PythonModuleData {
            py: full_path.to_path_buf(),
        });
    }

    let config_c = parse_config_c(&config_c);
    let config_c_in = parse_config_c(&config_c_in);

    let extension_modules_always = vec![
        String::from("getbuildinfo.o"),
        String::from("getpath.o"),
        String::from("main.o"),
        String::from("gcmodule.o"),
    ];

    Ok(PythonDistributionInfo {
        temp_dir,
        config_c,
        config_c_in,
        extension_modules,
        extension_modules_always,
        frozen_c,
        includes,
        libraries,
        objs_core,
        objs_modules,
        py_modules,
        resources,
    })
}

/// Extract Python distribution data from a tar archive.
pub fn analyze_python_distribution_tar<R: Read>(source: R) -> Result<PythonDistributionInfo, &'static str> {
    let mut tf = tar::Archive::new(source);

    let temp_dir = tempdir::TempDir::new("python-distribution").expect("could not create temp directory");
    let temp_dir_path = temp_dir.path();

    tf.unpack(&temp_dir_path).expect("unable to extract tar archive");

    analyze_python_distribution_data(temp_dir)
}

/// Extract Python distribution data from a zstandard compressed tar archive.
pub fn analyze_python_distribution_tar_zst<R: Read>(source: R) -> Result<PythonDistributionInfo, &'static str> {
    let dctx = zstd::stream::Decoder::new(source).unwrap();

    analyze_python_distribution_tar(dctx)
}
