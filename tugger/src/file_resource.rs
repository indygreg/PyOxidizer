// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    anyhow::{anyhow, Context, Result},
    std::{
        collections::{btree_map::Iter, BTreeMap, BTreeSet},
        convert::TryFrom,
        ffi::OsStr,
        io::Write,
        path::{Path, PathBuf},
    },
    virtual_file_manifest::{is_executable, set_executable},
};

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
    pub fn add_file<P: AsRef<Path>>(&mut self, path: P, content: &FileContent) -> Result<()> {
        let path = path.as_ref();
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

    /// Obtain entries in this manifest grouped by directory.
    ///
    /// The returned map has keys corresponding to the relative directory and
    /// values of files in that directory.
    ///
    /// The root directory is modeled by the `None` key.
    pub fn entries_by_directory(&self) -> BTreeMap<Option<&Path>, BTreeMap<&OsStr, &FileContent>> {
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

impl<'a> TryFrom<virtual_file_manifest::FileManifest<'a>> for FileManifest {
    type Error = anyhow::Error;

    fn try_from(other: virtual_file_manifest::FileManifest) -> Result<Self, Self::Error> {
        let mut m = FileManifest::default();

        for (k, v) in other.iter_entries() {
            let content = FileContent {
                data: v.data.resolve()?.to_vec(),
                executable: v.executable,
            };

            m.add_file(k, &content)?;
        }

        Ok(m)
    }
}

#[cfg(test)]
mod tests {
    use {super::*, std::iter::FromIterator};

    #[test]
    fn test_add() {
        let mut v = FileManifest::default();
        let f = FileContent {
            data: vec![],
            executable: false,
        };

        v.add_file(&PathBuf::from("foo"), &f).unwrap();

        let entries = v.entries().collect::<Vec<_>>();

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

    #[test]
    fn test_entries_by_directory() -> Result<()> {
        let c = FileContent {
            data: vec![42],
            executable: false,
        };

        let mut m = FileManifest::default();
        m.add_file(Path::new("root.txt"), &c)?;
        m.add_file(Path::new("dir0/dir0_file0.txt"), &c)?;
        m.add_file(Path::new("dir0/child0/dir0_child0_file0.txt"), &c)?;
        m.add_file(Path::new("dir0/child0/dir0_child0_file1.txt"), &c)?;
        m.add_file(Path::new("dir0/child1/dir0_child1_file0.txt"), &c)?;
        m.add_file(Path::new("dir1/child0/dir1_child0_file0.txt"), &c)?;

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
