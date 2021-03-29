// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Bundles backed by a directory.

use {
    crate::BundlePackageType,
    anyhow::{anyhow, Context, Result},
    std::path::{Path, PathBuf},
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
}

impl DirectoryBundle {
    /// Open an existing bundle from a filesystem path.
    ///
    /// The specified path should be the root directory of the bundle.
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

        Ok(Self {
            root: directory.to_path_buf(),
            root_name,
            shallow,
        })
    }

    fn resolve_path(&self, path: impl AsRef<Path>) -> PathBuf {
        if self.shallow {
            self.root.join(path.as_ref())
        } else {
            self.root.join("Contents").join(path.as_ref())
        }
    }

    /// Obtain the parsed `Info.plist` file.
    pub fn info_plist(&self) -> Result<Option<plist::Dictionary>> {
        let path = self.resolve_path("Info.plist");

        match std::fs::read(&path) {
            Ok(data) => {
                let cursor = std::io::Cursor::new(data);

                let value =
                    plist::Value::from_reader_xml(cursor).context("parsing Info.plist XML")?;

                if let Some(dict) = value.into_dictionary() {
                    Ok(Some(dict))
                } else {
                    Err(anyhow!("{} is not a dictionary", path.display()))
                }
            }
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    Ok(None)
                } else {
                    Err(e.into())
                }
            }
        }
    }

    /// Obtain an `Info.plist` key as a `String`.
    ///
    /// Will return `None` if there is no `Info.plist` file or the specified
    /// key doesn't exist. Errors on `Info.plist` parse error or if the key value
    /// is not a string.
    pub fn info_plist_key_string(&self, key: &str) -> Result<Option<String>> {
        if let Some(plist) = self.info_plist()? {
            if let Some(value) = plist.get(key) {
                Ok(Some(
                    value
                        .as_string()
                        .ok_or_else(|| anyhow!("key {} is not a string", key))?
                        .to_string(),
                ))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    /// Obtain the type of bundle.
    ///
    /// This sniffs the extension of the root directory for well-defined suffixes.
    pub fn package_type(&self) -> BundlePackageType {
        if self.root_name.ends_with(".app") {
            BundlePackageType::App
        } else if self.root_name.ends_with(".framework") {
            BundlePackageType::Framework
        } else {
            BundlePackageType::Bundle
        }
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
        if let Some(plist) = self.info_plist()? {
            if let Some(value) = plist.get("CFBundleIconFiles") {
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
        } else {
            Ok(None)
        }
    }

    /// Obtain all files within this bundle.
    ///
    /// The iteration order is deterministic.
    pub fn files(&self) -> Result<Vec<DirectoryBundleFile<'_>>> {
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
                if path.is_dir() {
                    None
                } else {
                    Some(DirectoryBundleFile::new(self, path))
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
        self.absolute_path == self.bundle.resolve_path("Info.plist")
    }

    /// Whether this is the main executable for the bundle.
    pub fn is_main_executable(&self) -> Result<bool> {
        if let Some(main) = self.bundle.main_executable()? {
            Ok(self.absolute_path == self.bundle.resolve_path(main))
        } else {
            Ok(false)
        }
    }

    /// Whether this file is in the code signature directory.
    pub fn is_in_code_signature_directory(&self) -> bool {
        let prefix = self.bundle.resolve_path("_CodeSignature");

        self.absolute_path.starts_with(&prefix)
    }
}
