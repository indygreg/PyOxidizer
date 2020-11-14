// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    convert::TryFrom,
    ffi::OsStr,
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

    /// Add an iterable of `File` to this manifest.
    pub fn add_files(
        &mut self,
        files: impl Iterator<Item = File<'a>>,
    ) -> Result<(), FileManifestError> {
        for file in files {
            self.add_file_entry(file.path, file.entry)?;
        }

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

    /// Resolve all required directories relative to another directory.
    ///
    /// The root directory itself is included.
    pub fn resolve_directories(&self, relative_to: impl AsRef<Path>) -> Vec<PathBuf> {
        let relative_to = relative_to.as_ref();

        let mut dirs = Vec::new();
        dirs.push(relative_to.to_path_buf());

        for p in self.relative_directories() {
            dirs.push(relative_to.join(p));
        }

        dirs
    }

    /// Obtain an iterator over paths and file entries in this manifest.
    pub fn iter_entries(&self) -> std::collections::btree_map::Iter<PathBuf, FileEntry> {
        self.files.iter()
    }

    /// Obtain an iterator of entries as `File` instances.
    pub fn iter_files(&self) -> impl std::iter::Iterator<Item = File> {
        self.files.iter().map(|(k, v)| File::new(k, v.clone()))
    }

    /// Obtain entries in this manifest grouped by directory.
    ///
    /// The returned map has keys corresponding to the relative directory and
    /// values of files in that directory.
    ///
    /// The root directory is modeled by the `None` key.
    pub fn entries_by_directory(&self) -> BTreeMap<Option<&Path>, BTreeMap<&OsStr, &FileEntry>> {
        let mut res = BTreeMap::new();

        for (path, content) in &self.files {
            let parent = match path.parent() {
                Some(p) => {
                    if p == Path::new("") {
                        None
                    } else {
                        Some(p)
                    }
                }
                None => None,
            };
            let filename = path.file_name().unwrap();

            let entry = res.entry(parent).or_insert_with(BTreeMap::new);
            entry.insert(filename, content);

            // Ensure there are keys for all parents.
            if let Some(parent) = parent {
                let mut parent = parent.parent();
                while parent.is_some() && parent != Some(Path::new("")) {
                    res.entry(parent).or_insert_with(BTreeMap::new);
                    parent = parent.unwrap().parent();
                }
            }
        }

        res.entry(None).or_insert_with(BTreeMap::new);

        res
    }
}

#[cfg(test)]
mod tests {
    use {super::*, std::iter::FromIterator};

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
    fn test_add_files() -> Result<(), FileManifestError> {
        let mut m = FileManifest::default();

        let files = vec![
            File {
                path: Path::new("foo").into(),
                entry: FileEntry {
                    data: FileData::from(vec![42]),
                    executable: false,
                },
            },
            File {
                path: Path::new("dir0/file0").into(),
                entry: FileEntry {
                    data: FileData::from(vec![42]),
                    executable: false,
                },
            },
        ];

        m.add_files(files.into_iter())?;

        assert_eq!(m.files.len(), 2);

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

    #[test]
    fn test_resolve_directories() -> Result<(), FileManifestError> {
        let mut m = FileManifest::default();
        let f = FileEntry {
            data: vec![].into(),
            executable: false,
        };

        m.add_file_entry(Path::new("foo"), f.clone())?;
        m.add_file_entry(Path::new("dir1/dir2/foo"), f)?;

        let dirs = m.resolve_directories(Path::new("/tmp"));
        assert_eq!(
            dirs,
            vec![
                PathBuf::from("/tmp"),
                PathBuf::from("/tmp/dir1"),
                PathBuf::from("/tmp/dir1/dir2")
            ]
        );

        Ok(())
    }

    #[test]
    fn test_entries_by_directory() -> Result<(), FileManifestError> {
        let c = FileEntry {
            data: vec![42].into(),
            executable: false,
        };

        let mut m = FileManifest::default();
        m.add_file_entry(Path::new("root.txt"), c.clone())?;
        m.add_file_entry(Path::new("dir0/dir0_file0.txt"), c.clone())?;
        m.add_file_entry(Path::new("dir0/child0/dir0_child0_file0.txt"), c.clone())?;
        m.add_file_entry(Path::new("dir0/child0/dir0_child0_file1.txt"), c.clone())?;
        m.add_file_entry(Path::new("dir0/child1/dir0_child1_file0.txt"), c.clone())?;
        m.add_file_entry(Path::new("dir1/child0/dir1_child0_file0.txt"), c.clone())?;

        let entries = m.entries_by_directory();

        assert_eq!(entries.keys().count(), 6);
        assert_eq!(
            entries.keys().collect::<Vec<_>>(),
            vec![
                &None,
                &Some(Path::new("dir0")),
                &Some(Path::new("dir0/child0")),
                &Some(Path::new("dir0/child1")),
                &Some(Path::new("dir1")),
                &Some(Path::new("dir1/child0")),
            ]
        );

        assert_eq!(
            entries.get(&None).unwrap(),
            &BTreeMap::from_iter([(OsStr::new("root.txt"), &c),].iter().cloned())
        );
        assert_eq!(
            entries.get(&Some(Path::new("dir0"))).unwrap(),
            &BTreeMap::from_iter([(OsStr::new("dir0_file0.txt"), &c)].iter().cloned())
        );
        assert_eq!(
            entries.get(&Some(Path::new("dir0/child0"))).unwrap(),
            &BTreeMap::from_iter(
                [
                    (OsStr::new("dir0_child0_file0.txt"), &c),
                    (OsStr::new("dir0_child0_file1.txt"), &c)
                ]
                .iter()
                .cloned()
            )
        );
        assert_eq!(
            entries.get(&Some(Path::new("dir0/child1"))).unwrap(),
            &BTreeMap::from_iter([(OsStr::new("dir0_child1_file0.txt"), &c)].iter().cloned())
        );
        assert_eq!(
            entries.get(&Some(Path::new("dir1/child0"))).unwrap(),
            &BTreeMap::from_iter([(OsStr::new("dir1_child0_file0.txt"), &c)].iter().cloned())
        );

        Ok(())
    }
}
