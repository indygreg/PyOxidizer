// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use itertools::Itertools;
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

pub fn walk_tree_files(path: &Path) -> Box<Iterator<Item = walkdir::DirEntry>> {
    let res = walkdir::WalkDir::new(path);

    let filtered = res.into_iter().filter_map(|entry| {
        let entry = entry.expect("unable to get directory entry");

        let path = entry.path();

        match path.is_dir() {
            true => None,
            false => Some(entry),
        }
    });

    Box::new(filtered)
}

/// Represents the type of Python resource.
#[derive(Debug, PartialEq)]
pub enum PythonResourceType {
    Source,
    Bytecode,
    BytecodeOpt1,
    BytecodeOpt2,
    Resource,
    Other,
}

/// Represents a resource in a Python directory.
///
/// A resource can be a Python source file, a bytecode file, or a resource
/// file.
///
/// TODO track the package name
#[derive(Debug)]
pub struct PythonResource {
    /// Name of this resource.
    ///
    /// This is the full resource name as referenced by ``importlib``.
    ///
    /// e.g. foo.bar
    pub name: String,

    /// Filesystem path to this resource.
    pub path: PathBuf,

    /// The type of resource this is.
    pub flavor: PythonResourceType,
}

pub struct PythonResourceIterator {
    root_path: PathBuf,
    walkdir_result: Box<Iterator<Item = walkdir::DirEntry>>,
}

impl PythonResourceIterator {
    fn new(path: &Path) -> PythonResourceIterator {
        let res = walkdir::WalkDir::new(path);

        let filtered = res.into_iter().filter_map(|entry| {
            let entry = entry.expect("unable to get directory entry");

            let path = entry.path();

            match path.is_dir() {
                true => None,
                false => Some(entry),
            }
        });

        PythonResourceIterator {
            root_path: path.to_path_buf(),
            walkdir_result: Box::new(filtered),
        }
    }
}

impl Iterator for PythonResourceIterator {
    type Item = PythonResource;

    fn next(&mut self) -> Option<PythonResource> {
        let res = self.walkdir_result.next();

        if res.is_none() {
            return None;
        }

        let entry = res.unwrap();

        let path = entry.path();

        let rel_path = path
            .strip_prefix(&self.root_path)
            .expect("unable to strip path prefix");
        let rel_str = rel_path.to_str().expect("could not convert path to str");
        let components = rel_path
            .iter()
            .map(|p| p.to_str().expect("unable to get path as str"))
            .collect::<Vec<_>>();

        let (module_name, flavor) = match rel_path.extension().and_then(OsStr::to_str) {
            Some("py") => {
                let package_parts = &components[0..components.len() - 1];
                let module_name = rel_path
                    .file_stem()
                    .expect("unable to get file stemp")
                    .to_str()
                    .expect("unable to convert path to str");

                let mut full_module_name: Vec<&str> = package_parts.to_vec();

                if module_name != "__init__" {
                    full_module_name.push(module_name);
                }

                let full_module_name = itertools::join(full_module_name, ".");

                (full_module_name, PythonResourceType::Source)
            }
            Some("pyc") => {
                // .pyc files should be in a __pycache__ directory.
                if components.len() < 2 {
                    panic!("encountered .pyc file with invalid path: {}", rel_str);
                }

                if components[components.len() - 2] != "__pycache__" {
                    // Possibly from Python 2?
                    let name = itertools::join(components, ".");

                    return Some(PythonResource {
                        name,
                        path: path.to_path_buf(),
                        flavor: PythonResourceType::Other,
                    });
                }

                let package_parts = &components[0..components.len() - 2];

                // Files have format <package>/__pycache__/<module>.cpython-37.opt-1.pyc
                let module_name = rel_path
                    .file_stem()
                    .expect("unable to get file stem")
                    .to_str()
                    .expect("unable to convert file stem to str");
                let module_name_parts = module_name.split('.').collect_vec();
                let module_name =
                    itertools::join(&module_name_parts[0..module_name_parts.len() - 1], ".");

                let mut full_module_name: Vec<&str> = package_parts.to_vec();

                if module_name != "__init__" {
                    full_module_name.push(&module_name);
                }

                let full_module_name = itertools::join(full_module_name, ".");

                let flavor;

                if rel_str.ends_with(".opt-1.pyc") {
                    flavor = PythonResourceType::BytecodeOpt1;
                } else if rel_str.ends_with(".opt-2.pyc") {
                    flavor = PythonResourceType::BytecodeOpt2;
                } else {
                    flavor = PythonResourceType::Bytecode;
                }

                (full_module_name, flavor)
            }
            _ => {
                // If it isn't a .py or a .pyc file, it is a resource file.
                let name = itertools::join(components, ".");

                (name, PythonResourceType::Resource)
            }
        };

        Some(PythonResource {
            name: module_name.clone(),
            path: path.to_path_buf(),
            flavor,
        })
    }
}

/// Find Python resources in a directory.
///
/// Given a root directory path, walk the directory and find all Python
/// resources in it.
///
/// A resource is a Python source file, bytecode file, or resource file which
/// can be addressed via the ``A.B.C`` naming convention.
///
/// Returns an iterator of ``PythonResource`` instances.
pub fn find_python_resources(root_path: &Path) -> PythonResourceIterator {
    PythonResourceIterator::new(root_path)
}

pub fn find_python_modules(root_path: &Path) -> Result<BTreeMap<String, Vec<u8>>, &'static str> {
    let mut mods = BTreeMap::new();

    for resource in find_python_resources(root_path) {
        if resource.flavor != PythonResourceType::Source {
            continue;
        }

        let data = fs::read(resource.path).expect("unable to read file");

        mods.insert(resource.name, data);
    }

    Ok(mods)
}
