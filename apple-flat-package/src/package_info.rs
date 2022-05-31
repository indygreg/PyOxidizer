// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! `PkgInfo` XML files.

use {
    crate::{distribution::Bundle, PkgResult},
    serde::{Deserialize, Serialize},
    std::io::Read,
};

/// Provides information about the package to install.
///
/// This includes authentication requirements, behavior after installation, etc.
/// See the fields for more descriptions.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PackageInfo {
    /// Authentication requirements for the package install.
    ///
    /// Values include `none` and `root`.
    pub auth: String,

    #[serde(rename = "deleteObsoleteLanguages")]
    pub delete_obsolete_languages: Option<bool>,

    /// Whether symlinks found at install time should be resolved instead of being replaced by a
    /// real file or directory.
    #[serde(rename = "followSymLinks")]
    pub follow_symlinks: Option<bool>,

    /// Format version of the package.
    ///
    /// Value is likely `2`.
    pub format_version: u8,

    /// Identifies the tool that assembled this package.
    pub generator_version: Option<String>,

    /// Uniform type identifier that defines the package.
    ///
    /// Should ideally be unique to this package.
    pub identifier: String,

    /// Default location where the payload hierarchy should be installed.
    pub install_location: Option<String>,

    /// Defines minimum OS version on which the package can be installed.
    #[serde(rename = "minimumSystemVersion")]
    pub minimum_system_version: Option<bool>,

    /// Defines if permissions of existing directories should be updated with ones from the payload.
    pub overwrite_permissions: Option<bool>,

    /// Action to perform after install.
    ///
    /// Potential values can include `logout`, `restart`, and `shutdown`.
    pub postinstall_action: Option<String>,

    /// Preserve extended attributes on files.
    pub preserve_xattr: Option<bool>,

    /// Unknown.
    ///
    /// Probably has something to do with whether the installation tree can be relocated
    /// without issue.
    pub relocatable: Option<bool>,

    /// Whether items in the package should be compressed after installation.
    #[serde(rename = "useHFSPlusCompression")]
    pub use_hfs_plus_compression: Option<bool>,

    /// Version of the package.
    ///
    /// This is the version of the package itself, not the version of the application
    /// being installed.
    pub version: u32,

    // End of attributes. Beginning of elements.
    #[serde(default)]
    pub atomic_update_bundle: Vec<BundleRef>,

    /// Versioning information about bundles within the payload.
    #[serde(default)]
    pub bundle: Vec<Bundle>,

    #[serde(default)]
    pub bundle_version: Vec<BundleRef>,

    /// Files to not obsolete during install.
    #[serde(default)]
    pub dont_obsolete: Vec<File>,

    /// Installs to process at next startup.
    #[serde(default)]
    pub install_at_startup: Vec<File>,

    /// Files to be patched.
    #[serde(default)]
    pub patch: Vec<File>,

    /// Provides information on the content being installed.
    pub payload: Option<Payload>,

    #[serde(default)]
    pub relocate: Vec<BundleRef>,

    /// Scripts to run before and after install.
    #[serde(default)]
    pub scripts: Vec<Script>,

    #[serde(default)]
    pub strict_identifiers: Vec<BundleRef>,

    #[serde(default)]
    pub update_bundle: Vec<BundleRef>,

    #[serde(default)]
    pub upgrade_bundle: Vec<BundleRef>,
}

impl Default for PackageInfo {
    fn default() -> Self {
        Self {
            auth: "none".into(),
            delete_obsolete_languages: None,
            follow_symlinks: None,
            format_version: 2,
            generator_version: Some("rust-apple-flat-package".to_string()),
            identifier: "".to_string(),
            install_location: None,
            minimum_system_version: None,
            overwrite_permissions: None,
            postinstall_action: None,
            preserve_xattr: None,
            relocatable: None,
            use_hfs_plus_compression: None,
            version: 0,
            atomic_update_bundle: vec![],
            bundle: vec![],
            bundle_version: vec![],
            dont_obsolete: vec![],
            install_at_startup: vec![],
            patch: vec![],
            payload: None,
            relocate: vec![],
            scripts: vec![],
            strict_identifiers: vec![],
            update_bundle: vec![],
            upgrade_bundle: vec![],
        }
    }
}

impl PackageInfo {
    /// Parse Distribution XML from a reader.
    pub fn from_reader(reader: impl Read) -> PkgResult<Self> {
        let mut de = serde_xml_rs::Deserializer::new_from_reader(reader);

        Ok(Self::deserialize(&mut de)?)
    }

    /// Parse Distribution XML from a string.
    pub fn from_xml(s: &str) -> PkgResult<Self> {
        let mut de = serde_xml_rs::Deserializer::new_from_reader(s.as_bytes())
            .non_contiguous_seq_elements(true);

        Ok(Self::deserialize(&mut de)?)
    }
}

/// File record.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct File {
    /// File path.
    pub path: String,

    /// Required SHA-1 of file.
    pub required_sha1: Option<String>,

    /// SHA-1 of file.
    pub sha1: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Payload {
    #[serde(rename = "numberOfFiles")]
    pub number_of_files: u64,
    #[serde(rename = "installKBytes")]
    pub install_kbytes: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BundleRef {
    pub id: Option<String>,
}

/// An entry in <scripts>.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum Script {
    #[serde(rename = "preinstall")]
    PreInstall(PreInstall),

    #[serde(rename = "postinstall")]
    PostInstall(PostInstall),
}

/// A script to run before install.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PreInstall {
    /// Name of script to run.
    pub file: String,

    /// ID of bundle element to run before.
    pub component_id: Option<String>,
}

/// A script to run after install.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PostInstall {
    /// Name of script to run.
    pub file: String,

    /// ID of bundle element to run after.
    pub component_id: Option<String>,
}
