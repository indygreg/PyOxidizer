// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Utility functions related to Python modules. */

use {std::collections::BTreeSet, std::path::Path, std::path::PathBuf};

/// Represents file name suffixes for Python modules.
#[derive(Clone, Debug, PartialEq)]
pub struct PythonModuleSuffixes {
    /// Suffixes for Python source modules.
    pub source: Vec<String>,

    /// Suffixes for Python bytecode modules.
    pub bytecode: Vec<String>,

    /// Suffixes for Python debug bytecode modules.
    pub debug_bytecode: Vec<String>,

    /// Suffixes for Python optimized bytecode modules.
    pub optimized_bytecode: Vec<String>,

    /// Suffixes for Python extension modules.
    pub extension: Vec<String>,
}

/// Resolve the set of packages present in a fully qualified module name.
pub fn packages_from_module_name(module: &str) -> BTreeSet<String> {
    let mut package_names = BTreeSet::new();

    let mut search: &str = &module;

    while let Some(idx) = search.rfind('.') {
        package_names.insert(search[0..idx].to_string());
        search = &search[0..idx];
    }

    package_names
}

/// Resolve the set of packages present in a series of fully qualified module names.
pub fn packages_from_module_names<I>(names: I) -> BTreeSet<String>
where
    I: Iterator<Item = String>,
{
    let mut package_names = BTreeSet::new();

    for name in names {
        let mut search: &str = &name;

        while let Some(idx) = search.rfind('.') {
            package_names.insert(search[0..idx].to_string());
            search = &search[0..idx];
        }
    }

    package_names
}

/// Resolve the filesystem path for a module.
///
/// Takes a path prefix, fully-qualified module name, whether the module is a package,
/// and an optional bytecode tag to apply.
pub fn resolve_path_for_module(
    root: &str,
    name: &str,
    is_package: bool,
    bytecode_tag: Option<&str>,
) -> PathBuf {
    let mut module_path = PathBuf::from(root);

    let parts = name.split('.').collect::<Vec<&str>>();

    // All module parts up to the final one are packages/directories.
    for part in &parts[0..parts.len() - 1] {
        module_path.push(*part);
    }

    // A package always exists in its own directory.
    if is_package {
        module_path.push(parts[parts.len() - 1]);
    }

    // If this is a bytecode module, files go in a __pycache__ directories.
    if bytecode_tag.is_some() {
        module_path.push("__pycache__");
    }

    // Packages get normalized to /__init__.py.
    let basename = if is_package {
        "__init__"
    } else {
        parts[parts.len() - 1]
    };

    let suffix = if let Some(tag) = bytecode_tag {
        format!(".{}.pyc", tag)
    } else {
        ".py".to_string()
    };

    module_path.push(format!("{}{}", basename, suffix));

    module_path
}

pub fn is_package_from_path(path: &Path) -> bool {
    let file_name = path.file_name().unwrap().to_str().unwrap();
    file_name.starts_with("__init__.")
}

#[cfg(test)]
mod tests {
    use {super::*, std::iter::FromIterator};

    #[test]
    fn test_packages_from_module_name() {
        assert_eq!(
            packages_from_module_name("foo.bar"),
            BTreeSet::from_iter(vec!["foo".to_string()])
        );
        assert_eq!(
            packages_from_module_name("foo.bar.baz"),
            BTreeSet::from_iter(vec!["foo".to_string(), "foo.bar".to_string()])
        );
    }

    #[test]
    fn test_resolve_path_for_module() {
        assert_eq!(
            resolve_path_for_module(".", "foo", false, None),
            PathBuf::from("./foo.py")
        );
        assert_eq!(
            resolve_path_for_module(".", "foo", false, Some("cpython-37")),
            PathBuf::from("./__pycache__/foo.cpython-37.pyc")
        );
        assert_eq!(
            resolve_path_for_module(".", "foo", true, None),
            PathBuf::from("./foo/__init__.py")
        );
        assert_eq!(
            resolve_path_for_module(".", "foo", true, Some("cpython-37")),
            PathBuf::from("./foo/__pycache__/__init__.cpython-37.pyc")
        );
        assert_eq!(
            resolve_path_for_module(".", "foo.bar", false, None),
            PathBuf::from("./foo/bar.py")
        );
        assert_eq!(
            resolve_path_for_module(".", "foo.bar", false, Some("cpython-37")),
            PathBuf::from("./foo/__pycache__/bar.cpython-37.pyc")
        );
        assert_eq!(
            resolve_path_for_module(".", "foo.bar", true, None),
            PathBuf::from("./foo/bar/__init__.py")
        );
        assert_eq!(
            resolve_path_for_module(".", "foo.bar", true, Some("cpython-37")),
            PathBuf::from("./foo/bar/__pycache__/__init__.cpython-37.pyc")
        );
        assert_eq!(
            resolve_path_for_module(".", "foo.bar.baz", false, None),
            PathBuf::from("./foo/bar/baz.py")
        );
        assert_eq!(
            resolve_path_for_module(".", "foo.bar.baz", false, Some("cpython-37")),
            PathBuf::from("./foo/bar/__pycache__/baz.cpython-37.pyc")
        );
        assert_eq!(
            resolve_path_for_module(".", "foo.bar.baz", true, None),
            PathBuf::from("./foo/bar/baz/__init__.py")
        );
        assert_eq!(
            resolve_path_for_module(".", "foo.bar.baz", true, Some("cpython-37")),
            PathBuf::from("./foo/bar/baz/__pycache__/__init__.cpython-37.pyc")
        );
    }
}
