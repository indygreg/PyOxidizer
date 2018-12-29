// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

extern crate byteorder;
extern crate cc;
extern crate cpython_copy as cpython;
extern crate hex;
extern crate itertools;
#[macro_use]
extern crate lazy_static;
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
pub mod fsscan;
pub mod repackage;

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

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let source = std::fs::File::open("/home/gps/src/python-build-standalone/build/cpython-linux64.tar.zst").unwrap();
        super::analyze_python_distribution_tar_zst(source).unwrap();
    }
}
