// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    convert::TryFrom,
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[cfg(unix)]
pub fn is_executable(metadata: &std::fs::Metadata) -> bool {
    let permissions = metadata.permissions();
    permissions.mode() & 0o111 != 0
}

#[cfg(windows)]
pub fn is_executable(_metadata: &std::fs::Metadata) -> bool {
    false
}

#[cfg(unix)]
pub fn set_executable(file: &mut std::fs::File) -> Result<(), std::io::Error> {
    let mut permissions = file.metadata()?.permissions();
    permissions.set_mode(0o770);
    file.set_permissions(permissions)?;
    Ok(())
}

#[cfg(windows)]
pub fn set_executable(_file: &mut std::fs::File) -> Result<(), std::io::Error> {
    Ok(())
}

/// Represents an abstract location for binary data.
///
/// Data can be backed by the filesystem or in memory.
#[derive(Clone, Debug, PartialEq)]
pub enum FileData<'a> {
    Path(PathBuf),
    Memory(Cow<'a, [u8]>),
}

impl<'a> FileData<'a> {
    /// Resolve the data for this instance.
    ///
    /// If backed by a file, the file will be read.
    pub fn resolve(&self) -> Result<Cow<'a, [u8]>, std::io::Error> {
        match self {
            Self::Path(p) => {
                let data = std::fs::read(p)?;

                Ok(Cow::Owned(data))
            }
            Self::Memory(data) => Ok(data.clone()),
        }
    }

    /// Convert this instance to a memory variant.
    ///
    /// This ensures any file-backed data is present in memory.
    pub fn to_memory(&self) -> Result<Self, std::io::Error> {
        Ok(Self::Memory(self.resolve()?))
    }
}

impl<'a> From<&Path> for FileData<'a> {
    fn from(path: &Path) -> Self {
        Self::Path(path.to_path_buf())
    }
}

impl<'a> From<Vec<u8>> for FileData<'a> {
    fn from(data: Vec<u8>) -> Self {
        Self::Memory(Cow::Owned(data))
    }
}

impl<'a> From<&'a [u8]> for FileData<'a> {
    fn from(data: &'a [u8]) -> Self {
        Self::Memory(Cow::Borrowed(data))
    }
}

/// Represents a virtual file, without an associated path.
#[derive(Clone, Debug, PartialEq)]
pub struct FileEntry<'a> {
    /// The content of the file.
    pub data: FileData<'a>,
    /// Whether the file is executable.
    pub executable: bool,
}

impl<'a> TryFrom<&Path> for FileEntry<'a> {
    type Error = std::io::Error;

    fn try_from(path: &Path) -> Result<Self, Self::Error> {
        let metadata = std::fs::metadata(path)?;
        let executable = is_executable(&metadata);

        Ok(Self {
            data: FileData::from(path),
            executable,
        })
    }
}

/// Represents a virtual file, with an associated path.
#[derive(Clone, Debug, PartialEq)]
pub struct File<'a> {
    pub path: PathBuf,
    pub entry: FileEntry<'a>,
}

impl<'a> TryFrom<&Path> for File<'a> {
    type Error = std::io::Error;

    fn try_from(path: &Path) -> Result<Self, Self::Error> {
        let entry = FileEntry::try_from(path)?;

        Ok(Self {
            path: path.to_path_buf(),
            entry,
        })
    }
}

impl<'a> From<File<'a>> for FileEntry<'a> {
    fn from(f: File<'a>) -> Self {
        f.entry
    }
}

impl<'a> File<'a> {
    /// Create a new instance from a path and `FileEntry`.
    pub fn new(path: impl AsRef<Path>, entry: FileEntry<'a>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            entry,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum FileManifestError {
    IllegalRelativePath(String),
    IllegalAbsolutePath(String),
}

impl std::fmt::Display for FileManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IllegalRelativePath(path) => {
                f.write_str(&format!("path cannot contain '..': {}", path))
            }
            Self::IllegalAbsolutePath(path) => {
                f.write_str(&format!("path cannot be absolute: {}", path))
            }
        }
    }
}

impl std::error::Error for FileManifestError {}

/// Represents a collection of files.
///
/// Files are keyed by their path. The file content is abstract and can be
/// backed by multiple sources.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FileManifest<'a> {
    files: BTreeMap<PathBuf, FileEntry<'a>>,
}

impl<'a> FileManifest<'a> {
    /// Add a `FileEntry` to this manifest under the given path.
    ///
    /// The path cannot contain relative paths and must not be absolute.
    pub fn add_file_entry(
        &mut self,
        path: impl AsRef<Path>,
        entry: impl Into<FileEntry<'a>>,
    ) -> Result<(), FileManifestError> {
        let path = path.as_ref();
        let path_s = path.display().to_string();

        if path_s.contains("..") {
            return Err(FileManifestError::IllegalRelativePath(path_s));
        }

        // is_absolute() on Windows doesn't check for leading /.
        if path_s.starts_with('/') || path.is_absolute() {
            return Err(FileManifestError::IllegalAbsolutePath(path_s));
        }

        self.files.insert(path.to_path_buf(), entry.into());

        Ok(())
    }

    /// Merge the content of another manifest into this one.
    ///
    /// All entries from the other manifest are overlayed into this manifest while
    /// preserving paths exactly. If this manifest already has an entry for a given
    /// path, it will be overwritten by an entry in the other manifest.
    pub fn add_manifest(&mut self, other: &Self) -> Result<(), FileManifestError> {
        for (key, value) in &other.files {
            self.add_file_entry(key, value.clone())?;
        }

        Ok(())
    }

    /// Obtain all relative directories contained within files in this manifest.
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

    /// Obtain an iterator over paths and file entries in this manifest.
    pub fn iter_entries(&self) -> std::collections::btree_map::Iter<PathBuf, FileEntry> {
        self.files.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_file_entry() -> Result<(), FileManifestError> {
        let mut m = FileManifest::default();
        let f = FileEntry {
            data: FileData::from(vec![42]),
            executable: false,
        };

        m.add_file_entry(Path::new("foo"), f.clone())?;

        let entries = m.iter_entries().collect::<Vec<_>>();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, &PathBuf::from("foo"));
        assert_eq!(entries[0].1, &f);

        Ok(())
    }

    #[test]
    fn test_add_bad_path() -> Result<(), FileManifestError> {
        let mut m = FileManifest::default();
        let f = FileEntry {
            data: FileData::from(vec![]),
            executable: false,
        };

        let res = m.add_file_entry(Path::new("../etc/passwd"), f.clone());
        assert_eq!(
            res.err(),
            Some(FileManifestError::IllegalRelativePath(
                "../etc/passwd".to_string()
            ))
        );

        let res = m.add_file_entry(Path::new("/foo"), f);
        assert_eq!(
            res.err(),
            Some(FileManifestError::IllegalAbsolutePath("/foo".to_string()))
        );

        Ok(())
    }

    #[test]
    fn test_relative_directories() -> Result<(), FileManifestError> {
        let mut m = FileManifest::default();
        let f = FileEntry {
            data: FileData::from(vec![]),
            executable: false,
        };

        m.add_file_entry(Path::new("foo"), f.clone())?;
        let dirs = m.relative_directories();
        assert_eq!(dirs.len(), 0);

        m.add_file_entry(Path::new("dir1/dir2/foo"), f)?;
        let dirs = m.relative_directories();
        assert_eq!(
            dirs,
            vec![PathBuf::from("dir1"), PathBuf::from("dir1/dir2")]
        );

        Ok(())
    }
}
