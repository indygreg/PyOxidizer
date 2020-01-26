// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    anyhow::{anyhow, Context, Result},
    std::collections::btree_map::Iter,
    std::collections::{BTreeMap, BTreeSet},
    std::convert::TryFrom,
    std::io::Write,
    std::path::{Path, PathBuf},
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[cfg(unix)]
pub fn set_executable(file: &mut std::fs::File) -> Result<()> {
    let mut permissions = file.metadata()?.permissions();
    permissions.set_mode(0o770);
    file.set_permissions(permissions)?;
    Ok(())
}

#[cfg(windows)]
pub fn set_executable(_file: &mut std::fs::File) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
pub fn is_executable(metadata: &std::fs::Metadata) -> bool {
    let permissions = metadata.permissions();
    permissions.mode() & 0o111 != 0
}

#[cfg(windows)]
pub fn is_executable(_metadata: &std::fs::Metadata) -> bool {
    false
}

/// Represents file content, agnostic of storage location.
#[derive(Clone, Debug, PartialEq)]
pub struct FileContent {
    /// Raw data in the file.
    pub data: Vec<u8>,

    /// Whether the file is executable.
    pub executable: bool,
}

impl TryFrom<&Path> for FileContent {
    type Error = std::io::Error;

    fn try_from(value: &Path) -> Result<Self, Self::Error> {
        let data = std::fs::read(value)?;
        let metadata = std::fs::metadata(value)?;
        let executable = is_executable(&metadata);

        Ok(FileContent { data, executable })
    }
}

/// Represents a virtual tree of files.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FileManifest {
    files: BTreeMap<PathBuf, FileContent>,
}

impl FileManifest {
    /// Add a file to the manifest.
    pub fn add_file(&mut self, path: &Path, content: &FileContent) -> Result<()> {
        let path_s = path.display().to_string();

        if path_s.contains("..") {
            return Err(anyhow!("path cannot contain '..': {}", path.display()));
        }

        // is_absolute() on Windows doesn't check for leading /.
        if path_s.starts_with('/') || path.is_absolute() {
            return Err(anyhow!("path cannot be absolute: {}", path.display()));
        }

        self.files.insert(path.to_path_buf(), content.clone());

        Ok(())
    }

    pub fn add_manifest(&mut self, other: &FileManifest) -> Result<()> {
        for (key, value) in &other.files {
            self.add_file(key.as_path(), value)?;
        }

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

            for a in ans {
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

    /// Whether this manifest contains the specified file path.
    pub fn has_path(&self, path: &Path) -> bool {
        self.files.contains_key(path)
    }

    /// Write the contents of the install manifest to a filesystem path.
    pub fn write_to_path(&self, path: &Path) -> Result<()> {
        for (p, c) in &self.files {
            let dest_path = path.join(p);
            let parent = dest_path
                .parent()
                .ok_or_else(|| anyhow!("unable to resolve parent directory"))?;

            std::fs::create_dir_all(parent)
                .context("creating parent directory for FileManifest")?;

            let mut fh = std::fs::File::create(&dest_path)?;
            fh.write_all(&c.data)?;
            if c.executable {
                set_executable(&mut fh)?;
            }
        }

        Ok(())
    }

    /// Write the contents of the install manifest to a filesystem path,
    /// replacing any existing content at the specified path.
    pub fn replace_path(&self, path: &Path) -> Result<()> {
        if path.exists() {
            std::fs::remove_dir_all(path)?;
        }

        self.write_to_path(path)
    }
}

#[cfg(test)]
mod tests {
    use {super::*, itertools::Itertools};

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
