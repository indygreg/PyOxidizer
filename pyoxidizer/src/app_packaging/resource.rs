// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::collections::btree_map::Iter;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Represents file content, agnostic of storage location.
#[derive(Clone, Debug, PartialEq)]
pub struct FileContent {
    /// Raw data in the file.
    pub data: Vec<u8>,

    /// Whether the file is executable.
    pub executable: bool,
}

/// Represents a virtual tree of files.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FileManifest {
    files: BTreeMap<PathBuf, FileContent>,
}

impl FileManifest {
    /// Add a file to the manifest.
    pub fn add_file(&mut self, path: &Path, content: &FileContent) -> Result<(), String> {
        let path_s = path.display().to_string();

        if path_s.contains("..") {
            return Err(format!("path cannot contain '..': {}", path.display()));
        }

        // is_absolute() on Windows doesn't check for leading /.
        if path_s.starts_with('/') || path.is_absolute() {
            return Err(format!("path cannot be absolute: {}", path.display()));
        }

        self.files.insert(path.to_path_buf(), content.clone());

        Ok(())
    }

    /// All relative directories contained within files in this manifest.
    ///
    /// The root directory is not represented in the return value.
    pub fn relative_directories(&self) -> Vec<PathBuf> {
        let mut dirs = BTreeSet::new();

        for p in self.files.keys() {
            let mut ans = p.ancestors();
            ans.next();

            while let Some(a) = ans.next() {
                if a.display().to_string() != "" {
                    dirs.insert(a.to_path_buf());
                }
            }
        }

        dirs.iter().map(|x| x.to_path_buf()).collect()
    }

    /// Resolve all required directories relative to another directory.
    ///
    /// The root directory itself is included.
    pub fn resolve_directories(&self, relative_to: &Path) -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        dirs.push(relative_to.to_path_buf());

        for p in self.relative_directories() {
            dirs.push(relative_to.join(p));
        }

        dirs
    }

    /// Obtain an iterator over paths and file content in this manifest.
    pub fn entries(&self) -> Iter<PathBuf, FileContent> {
        self.files.iter()
    }
}

#[cfg(test)]
mod tests {
    use itertools::Itertools;

    use super::*;

    #[test]
    fn test_add() {
        let mut v = FileManifest::default();
        let f = FileContent {
            data: vec![],
            executable: false,
        };

        v.add_file(&PathBuf::from("foo"), &f).unwrap();

        let entries = v.entries().collect_vec();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, &PathBuf::from("foo"));
        assert_eq!(entries[0].1, &f);
    }

    #[test]
    fn test_add_bad_path() {
        let mut v = FileManifest::default();
        let f = FileContent {
            data: vec![],
            executable: false,
        };

        let res = v.add_file(&PathBuf::from("../etc/passwd"), &f);
        assert!(res.is_err());

        let res = v.add_file(&PathBuf::from("/foo"), &f);
        assert!(res.is_err());
    }

    #[test]
    fn test_relative_directories() {
        let mut v = FileManifest::default();
        let f = FileContent {
            data: vec![],
            executable: false,
        };

        v.add_file(&PathBuf::from("foo"), &f).unwrap();
        let dirs = v.relative_directories();
        assert_eq!(dirs.len(), 0);

        v.add_file(&PathBuf::from("dir1/dir2/foo"), &f).unwrap();
        let dirs = v.relative_directories();
        assert_eq!(
            dirs,
            vec![PathBuf::from("dir1"), PathBuf::from("dir1/dir2")]
        );
    }

    #[test]
    fn test_resolve_directories() {
        let mut v = FileManifest::default();
        let f = FileContent {
            data: vec![],
            executable: false,
        };

        v.add_file(&PathBuf::from("foo"), &f).unwrap();
        v.add_file(&PathBuf::from("dir1/dir2/foo"), &f).unwrap();

        let dirs = v.resolve_directories(&PathBuf::from("/tmp"));
        assert_eq!(
            dirs,
            vec![
                PathBuf::from("/tmp"),
                PathBuf::from("/tmp/dir1"),
                PathBuf::from("/tmp/dir1/dir2")
            ]
        )
    }
}
