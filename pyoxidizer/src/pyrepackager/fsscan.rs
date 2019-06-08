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

        if path.is_dir() {
            None
        } else {
            Some(entry)
        }
    });

    Box::new(filtered)
}

/// Represents the type of a Python resource.
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
    /// Python package name of this resource.
    ///
    /// For resources in the root, this will likely be the resource name.
    ///
    /// For modules that are packages (e.g. `__init__.py` or `__init__.pyc`
    /// files, this will be the same as `full_name`.
    ///
    /// For regular modules, this will be all but the final component in
    /// `full_name`.
    pub package: String,

    /// Final "stem" name of this resource.
    ///
    /// This is derived from the file name's basename.
    ///
    /// For resources that define packages, this is an empty string.
    pub stem: String,

    /// Full resource name of this resource.
    ///
    /// This is typically how `importlib` refers to the resource.
    ///
    /// e.g. `foo.bar`.
    ///
    /// For resources that are packages, this is equivalent to `package`.
    /// For non-package resources, this is `package.stem`.
    pub full_name: String,

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

            if path.is_dir() {
                None
            } else {
                Some(entry)
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

        res.as_ref()?;

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

        let resource = match rel_path.extension().and_then(OsStr::to_str) {
            Some("py") => {
                let package_parts = &components[0..components.len() - 1];
                let mut package = itertools::join(package_parts, ".");

                let module_name = rel_path
                    .file_stem()
                    .expect("unable to get file stem")
                    .to_str()
                    .expect("unable to convert path to str");

                let mut full_module_name: Vec<&str> = package_parts.to_vec();

                let stem = if module_name == "__init__" {
                    "".to_string()
                } else {
                    full_module_name.push(module_name);
                    module_name.to_string()
                };

                let full_module_name = itertools::join(full_module_name, ".");

                if package.is_empty() {
                    package = full_module_name.clone();
                }

                PythonResource {
                    package,
                    stem,
                    full_name: full_module_name,
                    path: path.to_path_buf(),
                    flavor: PythonResourceType::Source,
                }
            }
            Some("pyc") => {
                // .pyc files should be in a __pycache__ directory.
                if components.len() < 2 {
                    panic!("encountered .pyc file with invalid path: {}", rel_str);
                }

                // Possibly from Python 2?
                if components[components.len() - 2] != "__pycache__" {
                    let package_parts = &components[0..components.len() - 1];
                    let package = itertools::join(package_parts, ".");
                    let full_name = itertools::join(&components, ".");
                    let stem = components[components.len() - 1].to_string();

                    return Some(PythonResource {
                        package,
                        stem,
                        full_name,
                        path: path.to_path_buf(),
                        flavor: PythonResourceType::Other,
                    });
                }

                let package_parts = &components[0..components.len() - 2];
                let mut package = itertools::join(package_parts, ".");

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

                let stem = if module_name == "__init__" {
                    "".to_string()
                } else {
                    full_module_name.push(&module_name);
                    module_name.clone()
                };

                let full_module_name = itertools::join(full_module_name, ".");

                if package.is_empty() {
                    package = full_module_name.clone();
                }

                let flavor;

                if rel_str.ends_with(".opt-1.pyc") {
                    flavor = PythonResourceType::BytecodeOpt1;
                } else if rel_str.ends_with(".opt-2.pyc") {
                    flavor = PythonResourceType::BytecodeOpt2;
                } else {
                    flavor = PythonResourceType::Bytecode;
                }

                PythonResource {
                    package,
                    stem,
                    full_name: full_module_name,
                    path: path.to_path_buf(),
                    flavor,
                }
            }
            _ => {
                // If it isn't a .py or a .pyc file, it is a resource file.
                let package_parts = &components[0..components.len() - 1];
                let mut package = itertools::join(package_parts, ".");

                let name = itertools::join(&components, ".");
                let stem = components[components.len() - 1].to_string();

                if package.is_empty() {
                    package = name.clone();
                }

                PythonResource {
                    package,
                    stem,
                    full_name: name,
                    path: path.to_path_buf(),
                    flavor: PythonResourceType::Resource,
                }
            }
        };

        Some(resource)
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

        mods.insert(resource.full_name, data);
    }

    Ok(mods)
}
