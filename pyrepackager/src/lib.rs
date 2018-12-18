// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

extern crate byteorder;
extern crate hex;
extern crate itertools;
extern crate regex;
extern crate reqwest;
#[macro_use]
extern crate serde_derive;
extern crate sha2;
extern crate tar;
extern crate toml;
extern crate url;
extern crate walkdir;
extern crate zstd;

pub mod config;

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{PathBuf, Path};

use byteorder::{LittleEndian, WriteBytesExt};

#[allow(unused)]
const STDLIB_TEST_DIRS: &[&str] = &[
    "bsddb/test",
    "ctypes/test",
    "distutils/tests",
    "email/test",
    "idlelib/idle_test",
    "json/tests",
    "lib-tk/test",
    "lib2to3/tests",
    "sqlite3/test",
    "test",
    "tkinter/test",
    "unittest/test",
];

#[allow(unused)]
const STDLIB_NONTEST_IGNORE_DIRS: &[&str] = &[
    // The config directory describes how Python was built. It isn't relevant.
    "config",
    // ensurepip is useful for Python installs, which we're not. Ignore it.
    "ensurepip",
    // We don't care about the IDLE IDE.
    "idlelib",
    // lib2to3 is used for python Python 2 to Python 3. While there may be some
    // useful generic functions in there for rewriting Python source, it is
    // quite large. So let's not include it.
    "lib2to3",
    // site-packages is where additional packages go. We don't use it.
    "site-packages",
];

#[allow(unused)]
const STDLIB_IGNORE_FILES: &[&str] = &[
    // These scripts are used for building macholib. They don't need to be in
    // the standard library.
    "ctypes/macholib/fetch_macholib",
    "ctypes/macholib/etch_macholib.bat",
    "ctypes/macholib/README.ctypes",
    "distutils/README",
    "wsgiref.egg-info",
];

#[allow(unused)]
pub const PYTHON_IMPORTER: &'static [u8] = include_bytes!("memoryimporter.py");

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

pub fn find_python_modules(root_path: &Path) -> Result<BTreeMap<String, Vec<u8>>, &'static str> {
    let mut mods = BTreeMap::new();

    for entry in walkdir::WalkDir::new(&root_path).into_iter() {
        let entry = entry.unwrap();

        let path = entry.into_path();
        let path_str = path.to_str().unwrap();

        if !path_str.ends_with(".py") {
            continue;
        }

        let rel_path = path.strip_prefix(&root_path).unwrap();

        let components = rel_path.iter().map(|p| p.to_str().unwrap()).collect::<Vec<_>>();

        let package_parts = &components[0..components.len() - 1];
        let module_name = rel_path.file_stem().unwrap().to_str().unwrap();

        let mut full_module_name: Vec<&str> = package_parts.to_vec();

        if module_name != "__init__" {
            full_module_name.push(module_name);
        }

        let full_module_name = itertools::join(full_module_name, ".");

        let mut fh = File::open(&path).unwrap();
        let mut data = Vec::new();
        fh.read_to_end(&mut data).unwrap();

        mods.insert(full_module_name, data);
    }

    Ok(mods)
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

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let source = std::fs::File::open("/home/gps/src/python-build-standalone/build/cpython-linux64.tar.zst").unwrap();
        super::analyze_python_distribution_tar_zst(source).unwrap();
    }
}
