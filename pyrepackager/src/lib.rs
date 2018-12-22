// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

extern crate byteorder;
extern crate cc;
extern crate cpython_copy as cpython;
extern crate hex;
extern crate itertools;
extern crate libc;
extern crate python3_copy_sys as pyffi;
extern crate regex;
extern crate reqwest;
#[macro_use]
extern crate serde_derive;
extern crate sha2;
extern crate tar;
extern crate tempdir;
extern crate toml;
extern crate url;
extern crate walkdir;
extern crate zstd;

pub mod bytecode;
pub mod config;
pub mod dist;
pub mod repackage;

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

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

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let source = std::fs::File::open("/home/gps/src/python-build-standalone/build/cpython-linux64.tar.zst").unwrap();
        super::analyze_python_distribution_tar_zst(source).unwrap();
    }
}
