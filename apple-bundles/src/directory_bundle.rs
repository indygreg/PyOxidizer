// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Bundles backed by a directory.

use {
    crate::BundlePackageType,
    anyhow::{anyhow, Context, Result},
    std::{
        collections::HashSet,
        path::{Path, PathBuf},
    },
    tugger_file_manifest::{is_executable, FileEntry, FileManifest},
};

/// An Apple bundle backed by a filesystem/directory.
///
/// Instances represent a type-agnostic bundle (macOS application bundle, iOS
/// application bundle, framework bundles, etc).
#[derive(Clone, Debug)]
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

        // Shallow bundles make it very easy to mis-identify a directory as a bundle.
        // The the following iOS app bundle directory structure:
        //
        //   MyApp.app
        //     MyApp
        //     Info.plist
        //
        // And take this framework directory structure:
        //
        //   MyFramework.framework
        //     MyFramework -> Versions/Current/MyFramework
        //     Resources/
        //       Info.plist
        //
        // Depending on how we probe the directories, `MyFramework.framework/Resources`
        // looks like a shallow app bundle!
        //
        // Frameworks are also an interesting use case. Frameworks often have a `Versions/`
        // where there may exist multiple versions of the framework. Each directory under
        // `Versions` is itself a valid framework!
        //
        //   MyFramework.framework
        //     MyFramework -> Versions/Current/MyFramework
        //     Resources -> Versions/Current/Resources
        //     Versions/
        //       A/
        //         MyFramework
        //         Resources/
        //           Info.plist
        //       Current -> A

        // Frameworks must have a `Resources/Info.plist`. It is tempting to look for the
        // `.framework` extension as well. However
        let (package_type, info_plist_path) = if framework_plist.is_file() {
            (BundlePackageType::Framework, framework_plist)
        } else if app_plist.is_file() {
            if root_name.ends_with(".app") {
                (BundlePackageType::App, app_plist)
            } else {
                // This can definitely lead to false positives.
                (BundlePackageType::Bundle, app_plist)
            }
        } else {
            return Err(anyhow!("Info.plist not found; not a valid bundle"));
        };

        let info_plist_data = std::fs::read(&info_plist_path)?;
        let cursor = std::io::Cursor::new(info_plist_data);
        let value = plist::Value::from_reader(cursor).context("parsing Info.plist")?;
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
            .nested_bundles(true)?
            .into_iter()
            .map(|(_, bundle)| bundle.root_dir().to_path_buf())
            .collect::<Vec<_>>();

        Ok(walkdir::WalkDir::new(&self.root)
            .sort_by_file_name()
            .into_iter()
            .map(|entry| {
                let entry = entry?;

                Ok(entry.path().to_path_buf())
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .filter_map(|path| {
                // This path is part of a known nested bundle and we're not in traversal mode.
                // Stop immediately.
                if !traverse_nested
                    && nested_dirs
                        .iter()
                        .any(|prefix| path.strip_prefix(prefix).is_ok())
                {
                    None
                // Symlinks are emitted as files, even if they point to a directory. It is
                // up to callers to handle symlinks correctly.
                } else if path.is_symlink() || !path.is_dir() {
                    Some(DirectoryBundleFile::new(self, path))
                } else {
                    None
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
    /// If `descend` is true, we will descend into nested bundles and recursively emit nested
    /// bundles. Otherwise we stop traversal once a bundle is encountered.
    pub fn nested_bundles(&self, descend: bool) -> Result<Vec<(String, Self)>> {
        let mut bundles = vec![];

        let mut poisoned_prefixes = HashSet::new();

        for entry in walkdir::WalkDir::new(&self.root).sort_by_file_name() {
            let entry = entry?;

            let path = entry.path();

            // Ignore self.
            if path == self.root {
                continue;
            }

            // A nested bundle must be a directory.
            if !path.is_dir() || path.is_symlink() {
                continue;
            }

            // This directory is inside a directory that has already been searched for
            // nested bundles. So ignore.
            if poisoned_prefixes
                .iter()
                .any(|prefix| path.strip_prefix(prefix).is_ok())
            {
                continue;
            }

            let root_relative = path.strip_prefix(&self.root)?.to_string_lossy();

            // Some bundle types have known child directories that themselves
            // can't be bundles. Exclude those from the search.
            match self.package_type {
                BundlePackageType::Framework => {
                    // Resources and Versions are known directories under frameworks.
                    // They can't be bundles.
                    if matches!(root_relative.as_ref(), "Resources" | "Versions") {
                        continue;
                    }
                }
                _ => {
                    if root_relative == "Contents" {
                        continue;
                    }
                }
            }

            // If we got here, test for bundle-ness by using our constructor.
            let bundle = match Self::new_from_path(path) {
                Ok(bundle) => bundle,
                Err(_) => {
                    continue;
                }
            };

            bundles.push((root_relative.to_string(), bundle.clone()));

            if descend {
                for (path, nested) in bundle.nested_bundles(true)? {
                    bundles.push((format!("{}/{}", root_relative, path), nested));
                }
            }

            poisoned_prefixes.insert(path.to_path_buf());
        }

        Ok(bundles)
    }

    /// Resolve the versions present within a framework.
    ///
    /// Does not emit versions that are symlinks.
    pub fn framework_versions(&self) -> Result<Vec<String>> {
        if self.package_type != BundlePackageType::Framework {
            return Ok(vec![]);
        }

        let mut res = vec![];

        for entry in std::fs::read_dir(self.root.join("Versions"))? {
            let entry = entry?;
            let metadata = entry.metadata()?;

            if metadata.is_dir() && !metadata.is_symlink() {
                res.push(entry.file_name().to_string_lossy().to_string());
            }
        }

        // Be deterministic.
        res.sort();

        Ok(res)
    }

    /// Whether this bundle is a version within a framework bundle.
    ///
    /// This is true if we are a framework bundle under a `Versions` directory.
    pub fn is_framework_version(&self) -> bool {
        if self.package_type == BundlePackageType::Framework {
            if let Some(parent) = self.root.parent() {
                if let Some(file_name) = parent.file_name() {
                    file_name == "Versions"
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        }
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

    /// Whether this is the `_CodeSignature/CodeResources` XML plist file.
    pub fn is_code_resources_xml_plist(&self) -> bool {
        self.absolute_path == self.bundle.resolve_path("_CodeSignature/CodeResources")
    }

    /// Whether this is the `CodeResources` file holding the notarization ticket.
    pub fn is_notarization_ticket(&self) -> bool {
        self.absolute_path == self.bundle.resolve_path("CodeResources")
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
    ///
    /// Does not follow symlinks.
    pub fn metadata(&self) -> Result<std::fs::Metadata> {
        Ok(self.absolute_path.symlink_metadata()?)
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

#[cfg(test)]
mod test {
    use {super::*, std::fs::create_dir_all};

    fn temp_dir() -> Result<(tempfile::TempDir, PathBuf)> {
        let td = tempfile::Builder::new()
            .prefix("apple-bundles-")
            .tempdir()?;
        let path = td.path().to_path_buf();

        Ok((td, path))
    }

    #[test]
    fn app_simple() -> Result<()> {
        let (_temp, td) = temp_dir()?;

        // Empty directory fails.
        let root = td.join("MyApp.app");
        create_dir_all(&root)?;
        assert!(DirectoryBundle::new_from_path(&root).is_err());

        // Empty Contents/ fails.
        let contents = root.join("Contents");
        create_dir_all(&contents)?;
        assert!(DirectoryBundle::new_from_path(&root).is_err());

        // Empty Info.plist fails.
        let plist_path = contents.join("Info.plist");
        std::fs::write(&plist_path, &[])?;
        assert!(DirectoryBundle::new_from_path(&root).is_err());

        // Empty plist dictionary works.
        let empty = plist::Value::from(plist::Dictionary::new());
        empty.to_file_xml(&plist_path)?;
        let bundle = DirectoryBundle::new_from_path(&root)?;

        assert_eq!(bundle.package_type, BundlePackageType::App);
        assert_eq!(bundle.name(), "MyApp.app");
        assert!(!bundle.shallow());
        assert_eq!(bundle.identifier()?, None);
        assert!(bundle.nested_bundles(true)?.is_empty());

        Ok(())
    }

    #[test]
    fn framework() -> Result<()> {
        let (_temp, td) = temp_dir()?;

        // Empty directory fails.
        let root = td.join("MyFramework.framework");
        create_dir_all(&root)?;
        assert!(DirectoryBundle::new_from_path(&root).is_err());

        // Empty Resources/ fails.
        let resources = root.join("Resources");
        create_dir_all(&resources)?;
        assert!(DirectoryBundle::new_from_path(&root).is_err());

        // Empty Info.plist file fails.
        let plist_path = resources.join("Info.plist");
        std::fs::write(&plist_path, &[])?;
        assert!(DirectoryBundle::new_from_path(&root).is_err());

        // Empty plist dictionary works.
        let empty = plist::Value::from(plist::Dictionary::new());
        empty.to_file_xml(&plist_path)?;
        let bundle = DirectoryBundle::new_from_path(&root)?;

        assert_eq!(bundle.package_type, BundlePackageType::Framework);
        assert_eq!(bundle.name(), "MyFramework.framework");
        assert!(bundle.shallow());
        assert_eq!(bundle.identifier()?, None);
        assert!(bundle.nested_bundles(true)?.is_empty());

        Ok(())
    }

    #[test]
    fn framework_in_app() -> Result<()> {
        let (_temp, td) = temp_dir()?;

        let root = td.join("MyApp.app");
        let contents = root.join("Contents");
        create_dir_all(&contents)?;

        let app_info_plist = contents.join("Info.plist");
        let empty = plist::Value::Dictionary(plist::Dictionary::new());
        empty.to_file_xml(&app_info_plist)?;

        let frameworks = contents.join("Frameworks");
        let framework = frameworks.join("MyFramework.framework");
        let resources = framework.join("Resources");
        create_dir_all(&resources)?;
        let versions = framework.join("Versions");
        create_dir_all(&versions)?;
        let framework_info_plist = resources.join("Info.plist");
        empty.to_file_xml(&framework_info_plist)?;
        let framework_resource_file_root = resources.join("root00.txt");
        std::fs::write(&framework_resource_file_root, &[])?;

        let framework_child = resources.join("child_dir");
        create_dir_all(&framework_child)?;
        let framework_resource_file_child = framework_child.join("child00.txt");
        std::fs::write(&framework_resource_file_child, &[])?;

        let a_resources = versions.join("A").join("Resources");
        create_dir_all(&a_resources)?;
        let b_resources = versions.join("B").join("Resources");
        create_dir_all(&b_resources)?;
        let a_plist = a_resources.join("Info.plist");
        empty.to_file_xml(&a_plist)?;
        let b_plist = b_resources.join("Info.plist");
        empty.to_file_xml(&b_plist)?;

        let bundle = DirectoryBundle::new_from_path(&root)?;

        let nested = bundle.nested_bundles(true)?;
        assert_eq!(nested.len(), 3);
        assert_eq!(
            nested
                .iter()
                .map(|x| x.0.replace("\\", "/"))
                .collect::<Vec<_>>(),
            vec![
                "Contents/Frameworks/MyFramework.framework",
                "Contents/Frameworks/MyFramework.framework/Versions/A",
                "Contents/Frameworks/MyFramework.framework/Versions/B",
            ]
        );

        assert_eq!(nested[0].1.framework_versions()?, vec!["A", "B"]);

        Ok(())
    }
}
