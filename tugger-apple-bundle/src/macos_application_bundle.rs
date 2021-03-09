// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! macOS Application Bundles

See https://developer.apple.com/library/archive/documentation/CoreFoundation/Conceptual/CFBundles/BundleTypes/BundleTypes.html#//apple_ref/doc/uid/10000123i-CH101-SW1
for documentation of the macOS Application Bundle format.
*/

use {
    anyhow::{anyhow, Context, Result},
    std::path::{Path, PathBuf},
    tugger_file_manifest::{FileData, FileEntry, FileManifest, FileManifestError},
};

/// Primitive used to iteratively construct a macOS Application Bundle.
///
/// Under the hood, the builder maintains a list of files that will constitute
/// the final, materialized bundle. There is a low-level `add_file()` API for
/// adding a file at an explicit path within the bundle. This gives you full
/// control over the content of the bundle.
///
/// There are also a number of high-level APIs for performing common tasks, such
/// as defining required bundle metadata for the `Contents/Info.plist` file and
/// adding files to specific locations. There are even APIs for performing
/// lower-level manipulation of certain files, such as adding keys to the
/// `Content/Info.plist` file.
///
/// Apple's documentation on the
/// [bundle format](https://developer.apple.com/library/archive/documentation/CoreFoundation/Conceptual/CFBundles/BundleTypes/BundleTypes.html#//apple_ref/doc/uid/10000123i-CH101-SW1)
/// is very comprehensive and can answer many questions. The most important
/// takeaways are:
///
/// 1. The `Contents/Info.plist` must contain some required keys defining the
///    bundle. Call `set_info_plist_required_keys()` to ensure these are
///    defined.
/// 2. There must be an executable file in the `Contents/MacOS` directory. Add
///    one via `add_file_macos()`.
///
/// This type attempts to prevent some misuse (such as validating `Info.plist`
/// content) but it cannot prevent all misconfigurations.
///
/// # Examples
///
/// ```
/// use tugger_apple_bundle::MacOsApplicationBundleBuilder;
/// use tugger_file_manifest::FileEntry;
///
/// # fn main() -> anyhow::Result<()> {
/// let mut builder = MacOsApplicationBundleBuilder::new("MyProgram")?;
///
/// // Populate some required keys in Contents/Info.plist.
/// builder.set_info_plist_required_keys("My Program", "com.example.my_program", "0.1", "mypg", "MyProgram")?;
///
/// // Add an executable file providing our main application.
/// builder.add_file_macos("MyProgram", FileEntry {
///     data: b"#!/bin/sh\necho 'hello world'\n".to_vec().into(),
///     executable: true,
/// })?;
/// # Ok(())
/// # }
/// ```
pub struct MacOsApplicationBundleBuilder {
    /// Files constituting the application bundle.
    files: FileManifest,
}

impl MacOsApplicationBundleBuilder {
    /// Create a new macOS Application Bundle builder.
    ///
    /// The bundle will be populated with a skeleton `Contents/Info.plist` file
    /// defining the bundle name passed.
    pub fn new(bundle_name: impl ToString) -> Result<Self> {
        let mut instance = Self {
            files: FileManifest::default(),
        };

        instance
            .set_info_plist_key("CFBundleName", bundle_name.to_string())
            .context("setting CFBundleName")?;

        // This is an application bundle, so CFBundlePackageType is constant.
        instance
            .set_info_plist_key("CFBundlePackageType", "APPL")
            .context("setting CFBundlePackageType")?;

        Ok(instance)
    }

    /// Obtain the raw FileManifest backing this builder.
    pub fn files(&self) -> &FileManifest {
        &self.files
    }

    /// Obtain the name of the bundle.
    ///
    /// This will parse the stored `Contents/Info.plist` and return the
    /// value of the `CFBundleName` key.
    ///
    /// This will error if the stored `Info.plist` is malformed, is missing
    /// a key, or the key has the wrong type. Errors should only happen if
    /// the file was explicitly stored or the value of this key was explicitly
    /// defined to the wrong type.
    pub fn bundle_name(&self) -> Result<String> {
        Ok(self
            .get_info_plist_key("CFBundleName")
            .context("resolving CFBundleName")?
            .ok_or_else(|| anyhow!("CFBundleName key not defined"))?
            .as_string()
            .ok_or_else(|| anyhow!("CFBundleName is not a string"))?
            .to_string())
    }

    /// Obtain the parsed content of the `Contents/Info.plist` file.
    ///
    /// Returns `Some(T)` if a `Contents/Info.plist` is defined or `None` if
    /// not.
    ///
    /// Returns `Err` if the file content could not be resolved or fails to parse
    /// as a plist dictionary.
    pub fn info_plist(&self) -> Result<Option<plist::Dictionary>> {
        if let Some(entry) = self.files.get("Contents/Info.plist") {
            let data = entry.data.resolve().context("resolving file content")?;
            let cursor = std::io::Cursor::new(data);

            let value = plist::Value::from_reader_xml(cursor).context("parsing plist")?;

            if let Some(dict) = value.into_dictionary() {
                Ok(Some(dict))
            } else {
                Err(anyhow!("parsed plist is not a dictionary"))
            }
        } else {
            Ok(None)
        }
    }

    /// Add a file to this application bundle.
    ///
    /// The path specified will be added without any checking, replacing
    /// an existing file at that path, if present.
    pub fn add_file(
        &mut self,
        path: impl AsRef<Path>,
        entry: impl Into<FileEntry>,
    ) -> Result<(), FileManifestError> {
        self.files.add_file_entry(path, entry)
    }

    /// Set the content of `Contents/Info.plist` using a `plist::Dictionary`.
    ///
    /// This allows you to define the `Info.plist` file with some validation
    /// since it goes through a plist serialization API, which should produce a
    /// valid plist file (although the contents of the plist may be invalid
    /// for an application bundle).
    pub fn set_info_plist_from_dictionary(&mut self, value: plist::Dictionary) -> Result<()> {
        let mut data: Vec<u8> = vec![];

        let value = plist::Value::from(value);

        value
            .to_writer_xml(&mut data)
            .context("serializing plist dictionary to XML")?;

        Ok(self.add_file(
            "Contents/Info.plist",
            FileEntry {
                data: data.into(),
                executable: false,
            },
        )?)
    }

    /// Obtain the value of a key in the `Contents/Info.plist` file.
    ///
    /// Returns `Some(Value)` if the key exists, `None` otherwise.
    ///
    /// May error if the stored `Contents/Info.plist` file is malformed.
    pub fn get_info_plist_key(&self, key: &str) -> Result<Option<plist::Value>> {
        Ok(
            if let Some(dict) = self.info_plist().context("parsing Info.plist")? {
                dict.get(key).cloned()
            } else {
                None
            },
        )
    }

    /// Set the value of a key in the `Contents/Info.plist` file.
    ///
    /// This API can be used to iteratively build up the `Info.plist` file by
    /// setting keys in it.
    ///
    /// If an existing key is replaced, `Some(Value)` will be returned.
    pub fn set_info_plist_key(
        &mut self,
        key: impl ToString,
        value: impl Into<plist::Value>,
    ) -> Result<Option<plist::Value>> {
        let mut dict = if let Some(dict) = self.info_plist().context("retrieving Info.plist")? {
            dict
        } else {
            plist::Dictionary::new()
        };

        let old = dict.insert(key.to_string(), value.into());

        self.set_info_plist_from_dictionary(dict)
            .context("replacing Info.plist dictionary")?;

        Ok(old)
    }

    /// Defines required keys in the `Contents/Info.plist` file.
    ///
    /// The following keys are set:
    ///
    /// `display_name` sets `CFBundleDisplayName`, the bundle display name.
    /// `identifier` sets `CFBundleIdentifier`, the bundle identifier.
    /// `version` sets `CFBundleVersion`, the bundle version string.
    /// `signature` sets `CFBundleSignature`, the bundle creator OS type code.
    /// `executable` sets `CFBundleExecutable`, the name of the main executable file.
    pub fn set_info_plist_required_keys(
        &mut self,
        display_name: impl ToString,
        identifier: impl ToString,
        version: impl ToString,
        signature: impl ToString,
        executable: impl ToString,
    ) -> Result<()> {
        let signature = signature.to_string();

        if signature.len() != 4 {
            return Err(anyhow!(
                "signature must be exactly 4 characters; got {}",
                signature
            ));
        }

        self.set_info_plist_key("CFBundleDisplayName", display_name.to_string())
            .context("setting CFBundleDisplayName")?;
        self.set_info_plist_key("CFBundleIdentifier", identifier.to_string())
            .context("setting CFBundleIdentifier")?;
        self.set_info_plist_key("CFBundleVersion", version.to_string())
            .context("setting CFBundleVersion")?;
        self.set_info_plist_key("CFBundleSignature", signature)
            .context("setting CFBundleSignature")?;
        self.set_info_plist_key("CFBundleExecutable", executable.to_string())
            .context("setting CFBundleExecutable")?;

        Ok(())
    }

    /// Add the icon for the bundle.
    ///
    /// This will materialize the passed raw image data (can be multiple formats)
    /// into the `Contents/Resources/<BundleName>.icns` file.
    pub fn add_icon(&mut self, data: impl Into<FileData>) -> Result<()> {
        Ok(self.add_file_resources(
            format!(
                "{}.icns",
                self.bundle_name().context("resolving bundle name")?
            ),
            FileEntry {
                data: data.into(),
                executable: false,
            },
        )?)
    }

    /// Add a file to the `Contents/MacOS/` directory.
    ///
    /// The passed path will be prefixed with `Contents/MacOS/`.
    pub fn add_file_macos(
        &mut self,
        path: impl AsRef<Path>,
        entry: impl Into<FileEntry>,
    ) -> Result<(), FileManifestError> {
        self.add_file(PathBuf::from("Contents/MacOS").join(path), entry)
    }

    /// Add a file to the `Contents/Resources/` directory.
    ///
    /// The passed path will be prefixed with `Contents/Resources/`
    pub fn add_file_resources(
        &mut self,
        path: impl AsRef<Path>,
        entry: impl Into<FileEntry>,
    ) -> Result<(), FileManifestError> {
        self.add_file(PathBuf::from("Contents/Resources").join(path), entry)
    }

    /// Add a localized resources file.
    ///
    /// This is a convenience wrapper to `add_file_resources()` which automatically
    /// places the file in the appropriate directory given the name of a locale.
    pub fn add_localized_resources_file(
        &mut self,
        locale: impl ToString,
        path: impl AsRef<Path>,
        entry: impl Into<FileEntry>,
    ) -> Result<(), FileManifestError> {
        self.add_file_resources(
            PathBuf::from(format!("{}.lproj", locale.to_string())).join(path),
            entry,
        )
    }

    /// Add a file to the `Contents/Frameworks/` directory.
    ///
    /// The passed path will be prefixed with `Contents/Frameworks/`.
    pub fn add_file_frameworks(
        &mut self,
        path: impl AsRef<Path>,
        entry: impl Into<FileEntry>,
    ) -> Result<(), FileManifestError> {
        self.add_file(PathBuf::from("Contents/Frameworks").join(path), entry)
    }

    /// Add a file to the `Contents/Plugins/` directory.
    ///
    /// The passed path will be prefixed with `Contents/Plugins/`.
    pub fn add_file_plugins(
        &mut self,
        path: impl AsRef<Path>,
        entry: impl Into<FileEntry>,
    ) -> Result<(), FileManifestError> {
        self.add_file(PathBuf::from("Contents/Plugins").join(path), entry)
    }

    /// Add a file to the `Contents/SharedSupport/` directory.
    ///
    /// The passed path will be prefixed with `Contents/SharedSupport/`.
    pub fn add_file_shared_support(
        &mut self,
        path: impl AsRef<Path>,
        entry: impl Into<FileEntry>,
    ) -> Result<(), FileManifestError> {
        self.add_file(PathBuf::from("Contents/SharedSupport").join(path), entry)
    }

    /// Materialize this bundle to the specified directory.
    ///
    /// All files comprising this bundle will be written to a directory named
    /// `<bundle_name>.app` in the directory specified. The path of this directory
    /// will be returned.
    ///
    /// If the destination bundle directory exists, existing files will be
    /// overwritten. Files already in the destination not defined in this
    /// builder will not be touched.
    pub fn materialize_bundle(&self, dest_dir: impl AsRef<Path>) -> Result<PathBuf> {
        let bundle_name = self.bundle_name().context("resolving bundle name")?;
        let bundle_dir = dest_dir.as_ref().join(format!("{}.app", bundle_name));

        self.files
            .materialize_files(&bundle_dir)
            .context("materializing FileManifest")?;

        Ok(bundle_dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_plist() -> Result<()> {
        let builder = MacOsApplicationBundleBuilder::new("MyProgram")?;

        let entries = builder.files().iter_entries().collect::<Vec<_>>();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, &PathBuf::from("Contents/Info.plist"));

        let mut dict = plist::Dictionary::new();
        dict.insert("CFBundleName".to_string(), "MyProgram".to_string().into());
        dict.insert("CFBundlePackageType".to_string(), "APPL".to_string().into());

        assert_eq!(builder.info_plist()?, Some(dict));
        assert!(String::from_utf8(entries[0].1.data.resolve()?)?
            .starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));

        Ok(())
    }

    #[test]
    fn plist_set() -> Result<()> {
        let mut builder = MacOsApplicationBundleBuilder::new("MyProgram")?;

        builder.set_info_plist_required_keys(
            "My Program",
            "com.example.my_program",
            "0.1",
            "mypg",
            "MyProgram",
        )?;

        let dict = builder.info_plist()?.unwrap();
        assert_eq!(
            dict.get("CFBundleDisplayName"),
            Some(&plist::Value::from("My Program"))
        );
        assert_eq!(
            dict.get("CFBundleIdentifier"),
            Some(&plist::Value::from("com.example.my_program"))
        );
        assert_eq!(
            dict.get("CFBundleVersion"),
            Some(&plist::Value::from("0.1"))
        );
        assert_eq!(
            dict.get("CFBundleSignature"),
            Some(&plist::Value::from("mypg"))
        );
        assert_eq!(
            dict.get("CFBundleExecutable"),
            Some(&plist::Value::from("MyProgram"))
        );

        Ok(())
    }

    #[test]
    fn add_icon() -> Result<()> {
        let mut builder = MacOsApplicationBundleBuilder::new("MyProgram")?;

        builder.add_icon(vec![42])?;

        let entries = builder.files.iter_entries().collect::<Vec<_>>();
        assert_eq!(entries.len(), 2);
        assert_eq!(
            entries[1].0,
            &PathBuf::from("Contents/Resources/MyProgram.icns")
        );

        Ok(())
    }

    #[test]
    fn add_file_macos() -> Result<()> {
        let mut builder = MacOsApplicationBundleBuilder::new("MyProgram")?;

        builder.add_file_macos(
            "MyProgram",
            FileEntry {
                data: vec![42].into(),
                executable: true,
            },
        )?;

        let entries = builder.files.iter_entries().collect::<Vec<_>>();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].0, &PathBuf::from("Contents/MacOS/MyProgram"));

        Ok(())
    }

    #[test]
    fn add_localized_resources_file() -> Result<()> {
        let mut builder = MacOsApplicationBundleBuilder::new("MyProgram")?;

        builder.add_localized_resources_file(
            "it",
            "resource",
            FileEntry {
                data: vec![42].into(),
                executable: false,
            },
        )?;

        let entries = builder.files.iter_entries().collect::<Vec<_>>();
        assert_eq!(entries.len(), 2);
        assert_eq!(
            entries[1].0,
            &PathBuf::from("Contents/Resources/it.lproj/resource")
        );

        Ok(())
    }
}
