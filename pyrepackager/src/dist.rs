// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;

#[allow(unused)]
struct PkgConfig {
    version: String,
    stdlib_path: PathBuf,
}

fn parse_pkgconfig(files: &BTreeMap<PathBuf, Vec<u8>>) -> PkgConfig {
    let python_pc = PathBuf::from("python/install/lib/pkgconfig/python3.pc");

    if !files.contains_key(&python_pc) {
        panic!("{} not found", python_pc.to_str().unwrap());
    }

    let buf = files.get(&python_pc).unwrap().to_vec();
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

#[allow(unused)]
#[derive(Clone, Debug)]
pub struct PythonModuleData {
    pub py: Vec<u8>,
    pub pyc: Option<Vec<u8>>,
    pub pyc_opt1: Option<Vec<u8>>,
    pub pyc_opt2: Option<Vec<u8>>,
}

#[allow(unused)]
#[derive(Debug)]
pub struct PythonDistributionInfo {
    pub config_c: ConfigC,
    pub config_c_in: ConfigC,
    pub frozen_c: Vec<u8>,
    pub objs_core: BTreeMap<PathBuf, Vec<u8>>,
    pub objs_modules: BTreeMap<PathBuf, Vec<u8>>,
    pub py_modules: BTreeMap<String, PythonModuleData>,
}

/// Extract useful information from the files constituting a Python distribution.
///
/// Passing in a data structure with raw file data within is inefficient. But
/// it makes things easier to implement and allows us to do things like consume
/// tarballs without filesystem I/O.
pub fn analyze_python_distribution_data(files: &BTreeMap<PathBuf, Vec<u8>>) -> Result<PythonDistributionInfo, &'static str> {
    let mut objs_core: BTreeMap<PathBuf, Vec<u8>> = BTreeMap::new();
    let mut objs_modules: BTreeMap<PathBuf, Vec<u8>> = BTreeMap::new();
    let mut config_c: Vec<u8> = Vec::new();
    let mut config_c_in: Vec<u8> = Vec::new();
    let mut frozen_c: Vec<u8> = Vec::new();
    let mut py_modules: BTreeMap<String, PythonModuleData> = BTreeMap::new();

    let pkgconfig = parse_pkgconfig(files);

    for (full_path, data) in files.iter() {
        let path = full_path.strip_prefix("python/").unwrap();

        if path.starts_with("build/") {
            let rel_path = path.strip_prefix("build/").unwrap();
            let rel_str = rel_path.to_str().unwrap();

            let components = rel_path.iter().collect::<Vec<_>>();

            if components.len() < 1 {
                continue;
            }

            if rel_str.ends_with(".o") {
                match components[0].to_str().unwrap() {
                    "Modules" => {
                        objs_modules.insert(rel_path.to_path_buf(), data.clone());
                        ()
                    },
                    "Objects" => {
                        objs_core.insert(rel_path.to_path_buf(), data.clone());
                        ()
                    },
                    "Parser" => {
                        objs_core.insert(rel_path.to_path_buf(), data.clone());
                        ()
                    },
                    "Programs" => {},
                    "Python" => {
                        objs_core.insert(rel_path.to_path_buf(), data.clone());
                        ()
                    },
                    _ => panic!("unexpected object file: {}", rel_str)
                }
            } else if rel_str == "Modules/config.c" {
                config_c = data.clone();
            } else if rel_str == "Modules/config.c.in" {
                config_c_in = data.clone();
            } else if rel_str == "Python/frozen.c" {
                frozen_c = data.clone();
            }
            else {
                panic!("unhandled build/ file: {}", rel_str);
            }
        }
        else if path.starts_with("install/") {
            let rel_path = path.strip_prefix("install/").unwrap();
            let rel_str = rel_path.to_str().unwrap();
            let components = rel_path.iter().map(|p| p.to_str().unwrap()).collect::<Vec<_>>();

            if rel_str.starts_with(pkgconfig.stdlib_path.to_str().unwrap()) {
                if components.len() < 3 {
                    continue;
                }

                let package_parts = &components[2..components.len() - 1];
                let module_name = rel_path.file_stem().unwrap().to_str().unwrap();

                let mut full_module_name: Vec<&str> = package_parts.to_vec();
                full_module_name.push(module_name);

                let mut full_module_name = itertools::join(full_module_name, ".");

                if full_module_name.ends_with(".__init__") {
                    full_module_name = full_module_name[0..full_module_name.len() - 9].to_string();
                }

                if rel_str.ends_with(".py") {
                    if py_modules.contains_key(&full_module_name) {
                        panic!("duplicate python module: {}", full_module_name);
                    }

                    // The .pyc paths are in a __pycache__ sibling directory.
                    let pycache_path = full_path.parent().unwrap().join("__pycache__");

                    // TODO should derive base name from build config.
                    let base = "cpython-37";

                    let pyc_path = pycache_path.join(format!("{}.{}.pyc", module_name, base));
                    let pyc_opt1_path = pycache_path.join(format!("{}.{}.opt-1.pyc", module_name, base));
                    let pyc_opt2_path = pycache_path.join(format!("{}.{}.opt-2.pyc", module_name, base));

                    // First 16 bytes of pyc files are used for validation. We don't need this
                    // data so we strip it.
                    let pyc_data = match files.get(&pyc_path) {
                        Some(v) => Some(v[16..].to_vec()),
                        None => None,
                    };

                    let pyc_opt1_data = match files.get(&pyc_opt1_path) {
                        Some(v) => Some(v[16..].to_vec()),
                        None => None,
                    };

                    let pyc_opt2_data = match files.get(&pyc_opt2_path) {
                        Some(v) => Some(v[16..].to_vec()),
                        None => None,
                    };

                    py_modules.insert(full_module_name, PythonModuleData {
                        py: data.clone(),
                        pyc: pyc_data,
                        pyc_opt1: pyc_opt1_data,
                        pyc_opt2: pyc_opt2_data,
                    });

                }
                else if rel_str.ends_with(".pyc") {
                    // Should be handled by .py branch.
                    continue;
                }
                // TODO do we care about non-py files?
            }
            // TODO do we care about non-stdlib files?
        }
        else if path.to_str().unwrap() == "LICENSE.rst" {
            continue;
        }
        else {
            panic!("unexpected path in archive: {}", path.to_str().unwrap());
        }
    }

    let config_c = parse_config_c(&config_c);
    let config_c_in = parse_config_c(&config_c_in);

    Ok(PythonDistributionInfo {
        config_c,
        config_c_in,
        frozen_c,
        objs_core,
        objs_modules,
        py_modules,
    })
}

pub fn analyze_python_distribution_tar<R: Read>(source: R) -> Result<PythonDistributionInfo, &'static str> {
    let mut tf = tar::Archive::new(source);

    // Buffering everything to memory isn't very efficient. But it makes things
    // easier to implement. This is part of the build system, so resource
    // constraints hopefully aren't a problem.
    let mut files: BTreeMap<PathBuf, Vec<u8>> = BTreeMap::new();

    // For collecting symlinks so we can resolve content after first pass.
    let mut links: BTreeMap<PathBuf, PathBuf> = BTreeMap::new();

    for entry in tf.entries().unwrap() {
        let mut entry = entry.unwrap();

        let et = entry.header().entry_type();

        if et.is_dir() {
            continue;
        }
        else if et.is_symlink() {
            let target = entry.path().unwrap().parent().unwrap().join(entry.link_name().unwrap().unwrap());

            links.insert(entry.path().unwrap().to_path_buf(), target);
        }

        let mut buf: Vec<u8> = Vec::new();
        entry.read_to_end(&mut buf).unwrap();

        files.insert(entry.path().unwrap().to_path_buf(), buf);
    }

    // Replace content of symlinks with data of the target.
    for (source, dest) in links.iter() {
        files.insert(source.clone(), files.get(dest).unwrap().clone());
    }

    analyze_python_distribution_data(&files)
}

pub fn analyze_python_distribution_tar_zst<R: Read>(source: R) -> Result<PythonDistributionInfo, &'static str> {
    let dctx = zstd::stream::Decoder::new(source).unwrap();

    analyze_python_distribution_tar(dctx)
}
