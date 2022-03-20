// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Bundles backed by a directory.

use {
    crate::BundlePackageType,
    anyhow::{anyhow, Context, Result},
    std::path::{Path, PathBuf},
    tugger_file_manifest::{is_executable, FileEntry, FileManifest},
};

/// An Apple bundle backed by a filesystem/directory.
///
/// Instances represent a type-agnostic bundle (macOS application bundle, iOS
/// application bundle, framework bundles, etc).
pub struct DirectoryBundle {
    /// Root directory of this bundle.
    root: PathBuf,

    /// Name of the root directory.
    root_name: String,

    /// Whether the bundle is shallow.
    ///
    /// If false, content is in a `Contents/` sub-directory.
    shallow: bool,

    /// The type of this bundle.
    package_type: BundlePackageType,

    /// Parsed `Info.plist` file.
    info_plist: plist::Dictionary,
}

impl DirectoryBundle {
    /// Open an existing bundle from a filesystem path.
    ///
    /// The specified path should be the root directory of the bundle.
    ///
    /// This will validate that the directory is a bundle and error if not.
    /// Validation is limited to locating an `Info.plist` file, which is
    /// required for all bundle types.
    pub fn new_from_path(directory: &Path) -> Result<Self> {
        if !directory.is_dir() {
            return Err(anyhow!("{} is not a directory", directory.display()));
        }

        let root_name = directory
            .file_name()
            .ok_or_else(|| anyhow!("unable to resolve root directory name"))?
            .to_string_lossy()
            .to_string();

        let contents = directory.join("Contents");
        let shallow = !contents.is_dir();

        let app_plist = if shallow {
            directory.join("Info.plist")
        } else {
            contents.join("Info.plist")
        };

        let framework_plist = directory.join("Resources").join("Info.plist");

        let (package_type, info_plist_path) = if app_plist.is_file() {
            if root_name.ends_with(".app") {
                (BundlePackageType::App, app_plist)
            } else {
                (BundlePackageType::Bundle, app_plist)
            }
        } else if framework_plist.is_file() {
            if root_name.ends_with(".framework") {
                (BundlePackageType::Framework, framework_plist)
            } else {
                (BundlePackageType::Bundle, framework_plist)
            }
        } else {
            return Err(anyhow!("Info.plist not found; not a valid bundle"));
        };

        let info_plist_data = std::fs::read(&info_plist_path)?;
        let cursor = std::io::Cursor::new(info_plist_data);
        let value = plist::Value::from_reader_xml(cursor).context("parsing Info.plist XML")?;
        let info_plist = value
            .into_dictionary()
            .ok_or_else(|| anyhow!("{} is not a dictionary", info_plist_path.display()))?;

        Ok(Self {
            root: directory.to_path_buf(),
            root_name,
            shallow,
            package_type,
            info_plist,
        })
    }

    /// Resolve the absolute path to a file in the bundle.
    pub fn resolve_path(&self, path: impl AsRef<Path>) -> PathBuf {
        if self.shallow {
            self.root.join(path.as_ref())
        } else {
            self.root.join("Contents").join(path.as_ref())
        }
    }

    /// The root directory of this bundle.
    pub fn root_dir(&self) -> &Path {
        &self.root
    }

    /// The on-disk name of this bundle.
    ///
    /// This is effectively the directory name of the bundle. Contains the `.app`,
    /// `.framework`, etc suffix.
    pub fn name(&self) -> &str {
        &self.root_name
    }

    /// Whether this is a shallow bundle.
    ///
    /// If false, content is likely in a `Contents` directory.
    pub fn shallow(&self) -> bool {
        self.shallow
    }

    /// Obtain the path to the `Info.plist` file.
    pub fn info_plist_path(&self) -> PathBuf {
        match self.package_type {
            BundlePackageType::App | BundlePackageType::Bundle => self.resolve_path("Info.plist"),
            BundlePackageType::Framework => self.root.join("Resources").join("Info.plist"),
        }
    }

    /// Obtain the parsed `Info.plist` file.
    pub fn info_plist(&self) -> &plist::Dictionary {
        &self.info_plist
    }

    /// Obtain an `Info.plist` key as a `String`.
    ///
    /// Will return `None` if the specified key doesn't exist. Errors if the key value
    /// is not a string.
    pub fn info_plist_key_string(&self, key: &str) -> Result<Option<String>> {
        if let Some(value) = self.info_plist.get(key) {
            Ok(Some(
                value
                    .as_string()
                    .ok_or_else(|| anyhow!("key {} is not a string", key))?
                    .to_string(),
            ))
        } else {
            Ok(None)
        }
    }

    /// Obtain the type of bundle.
    pub fn package_type(&self) -> BundlePackageType {
        self.package_type
    }

    /// Obtain the bundle display name.
    ///
    /// This retrieves the value of `CFBundleDisplayName` from the `Info.plist`.
    pub fn display_name(&self) -> Result<Option<String>> {
        self.info_plist_key_string("CFBundleDisplayName")
    }

    /// Obtain the bundle identifier.
    ///
    /// This retrieves `CFBundleIdentifier` from the `Info.plist`.
    pub fn identifier(&self) -> Result<Option<String>> {
        self.info_plist_key_string("CFBundleIdentifier")
    }

    /// Obtain the bundle version string.
    ///
    /// This retrieves `CFBundleVersion` from the `Info.plist`.
    pub fn version(&self) -> Result<Option<String>> {
        self.info_plist_key_string("CFBundleVersion")
    }

    /// Obtain the name of the bundle's main executable file.
    ///
    /// This retrieves `CFBundleExecutable` from the `Info.plist`.
    pub fn main_executable(&self) -> Result<Option<String>> {
        self.info_plist_key_string("CFBundleExecutable")
    }

    /// Obtain filenames of bundle icon files.
    ///
    /// This retrieves `CFBundleIconFiles` from the `Info.plist`.
    pub fn icon_files(&self) -> Result<Option<Vec<String>>> {
        if let Some(value) = self.info_plist.get("CFBundleIconFiles") {
            let values = value
                .as_array()
                .ok_or_else(|| anyhow!("CFBundleIconFiles not an array"))?;

            Ok(Some(
                values
                    .iter()
                    .map(|x| {
                        Ok(x.as_string()
                            .ok_or_else(|| anyhow!("CFBundleIconFiles value not a string"))?
                            .to_string())
                    })
                    .collect::<Result<Vec<_>>>()?,
            ))
        } else {
            Ok(None)
        }
    }

    /// Obtain all files within this bundle.
    ///
    /// The iteration order is deterministic.
    ///
    /// `traverse_nested` defines whether to traverse into nested bundles.
    pub fn files(&self, traverse_nested: bool) -> Result<Vec<DirectoryBundleFile<'_>>> {
        let nested_dirs = self
            .nested_bundles()?
            .into_iter()
            .map(|(_, bundle)| bundle.root_dir().to_path_buf())
            .collect::<Vec<_>>();

        Ok(walkdir::WalkDir::new(&self.root)
            .sort_by(|a, b| a.file_name().cmp(b.file_name()))
            .into_iter()
            .map(|entry| {
                let entry = entry?;

                Ok(entry.path().to_path_buf())
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .filter_map(|path| {
                if path.is_dir()
                    || (!traverse_nested
                        && nested_dirs
                            .iter()
                            .any(|prefix| path.strip_prefix(prefix).is_ok()))
                {
                    None
                } else {
                    Some(DirectoryBundleFile::new(self, path))
                }
            })
            .collect::<Vec<_>>())
    }

    /// Obtain all files in this bundle as a [FileManifest].
    pub fn files_manifest(&self, traverse_nested: bool) -> Result<FileManifest> {
        let mut m = FileManifest::default();

        for f in self.files(traverse_nested)? {
            m.add_file_entry(f.relative_path(), f.as_file_entry()?)?;
        }

        Ok(m)
    }

    /// Obtain all nested bundles within this one.
    ///
    /// This walks the directory tree for directories that can be parsed
    /// as bundles.
    ///
    /// This will descend infinitely into nested bundles. i.e. we don't stop
    /// traversing directories when we encounter a bundle.
    pub fn nested_bundles(&self) -> Result<Vec<(String, Self)>> {
        Ok(walkdir::WalkDir::new(&self.root)
            .sort_by(|a, b| a.file_name().cmp(b.file_name()))
            .into_iter()
            .map(|entry| {
                let entry = entry?;

                Ok(entry.path().to_path_buf())
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .filter_map(|p| {
                let file_name = p.file_name().map(|x| x.to_string_lossy());

                if p.is_dir() && file_name != Some("Contents".into()) && p != self.root {
                    if let Ok(bundle) = Self::new_from_path(&p) {
                        let rel = bundle
                            .root
                            .strip_prefix(&self.root)
                            .expect("nested bundle should be in sub-directory of main");

                        Some((rel.to_string_lossy().to_string(), bundle))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect::<Vec<_>>())
    }
}

/// Represents a file in a [DirectoryBundle].
pub struct DirectoryBundleFile<'a> {
    bundle: &'a DirectoryBundle,
    absolute_path: PathBuf,
    relative_path: PathBuf,
}

impl<'a> DirectoryBundleFile<'a> {
    fn new(bundle: &'a DirectoryBundle, absolute_path: PathBuf) -> Self {
        let relative_path = absolute_path
            .strip_prefix(&bundle.root)
            .expect("path prefix strip should have worked")
            .to_path_buf();

        Self {
            bundle,
            absolute_path,
            relative_path,
        }
    }

    /// Absolute path to this file.
    pub fn absolute_path(&self) -> &Path {
        &self.absolute_path
    }

    /// Relative path within the bundle to this file.
    pub fn relative_path(&self) -> &Path {
        &self.relative_path
    }

    /// Whether this is the `Info.plist` file.
    pub fn is_info_plist(&self) -> bool {
        self.absolute_path == self.bundle.info_plist_path()
    }

    /// Whether this is the main executable for the bundle.
    pub fn is_main_executable(&self) -> Result<bool> {
        if let Some(main) = self.bundle.main_executable()? {
            if self.bundle.shallow() {
                Ok(self.absolute_path == self.bundle.resolve_path(main))
            } else {
                Ok(self.absolute_path == self.bundle.resolve_path(format!("MacOS/{}", main)))
            }
        } else {
            Ok(false)
        }
    }

    /// Whether this file is in the code signature directory.
    pub fn is_in_code_signature_directory(&self) -> bool {
        let prefix = self.bundle.resolve_path("_CodeSignature");

        self.absolute_path.starts_with(&prefix)
    }

    /// Obtain the symlink target for this file.
    ///
    /// If `None`, the file is not a symlink.
    pub fn symlink_target(&self) -> Result<Option<PathBuf>> {
        let metadata = self.metadata()?;

        if metadata.file_type().is_symlink() {
            Ok(Some(std::fs::read_link(&self.absolute_path)?))
        } else {
            Ok(None)
        }
    }

    /// Obtain metadata for this file.
    pub fn metadata(&self) -> Result<std::fs::Metadata> {
        Ok(self.absolute_path.metadata()?)
    }

    /// Convert this instance to a [FileEntry].
    pub fn as_file_entry(&self) -> Result<FileEntry> {
        let metadata = self.metadata()?;

        let mut entry = FileEntry::new_from_path(self.absolute_path(), is_executable(&metadata));

        if let Some(target) = self.symlink_target()? {
            entry.set_link_target(target);
        }

        Ok(entry)
    }
}
