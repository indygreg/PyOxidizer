// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! `primary.xml` file format. */

use {
    crate::{
        error::{Result, RpmRepositoryError},
        io::ContentDigest,
        metadata::repomd::Location,
    },
    serde::{Deserialize, Serialize},
    std::io::Read,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Primary {
    /// The number of packages expressed by this document.
    #[serde(rename = "packages")]
    pub count: usize,

    /// `<package>` elements in this document.
    #[serde(rename = "package")]
    pub packages: Vec<Package>,
}

impl Primary {
    /// Construct an instance by parsing XML from a reader.
    pub fn from_reader(reader: impl Read) -> Result<Self> {
        Ok(serde_xml_rs::from_reader(reader)?)
    }

    /// Construct an instance by parsing XML from a string.
    pub fn from_xml(s: &str) -> Result<Self> {
        Ok(serde_xml_rs::from_str(s)?)
    }
}

/// A package as advertised in a `primary.xml` file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    /// The type/flavor of a package.
    ///
    /// e.g. `rpm`.
    #[serde(rename = "type")]
    pub package_type: String,

    /// The name of the package.
    pub name: String,

    /// The machine architecture the package is targeting.
    pub arch: String,

    /// The package version.
    pub version: PackageVersion,

    /// Content digest of package file.
    pub checksum: Checksum,

    /// A text summary of the package.
    pub summary: String,

    /// A longer text description of the package.
    pub description: String,

    /// Name of entity that produced the package.
    pub packager: Option<String>,

    /// URL where additional package info can be obtained.
    pub url: Option<String>,

    /// Time the package was created.
    pub time: PackageTime,

    /// Describes sizes affiliated with the package.
    pub size: PackageSize,

    /// Where the package can be obtained from.
    pub location: Location,

    /// Additional metadata about the package.
    pub format: Option<PackageFormat>,
}

/// Describes a package version.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PackageVersion {
    /// When the version came into existence.
    pub epoch: u64,

    /// Version string.
    #[serde(rename = "ver")]
    pub version: String,

    /// Release string.
    #[serde(rename = "rel")]
    pub release: String,
}

/// Describes the content checksum of a package.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Checksum {
    /// Digest type.
    #[serde(rename = "type")]
    pub name: String,

    /// Hex encoded digest value.
    #[serde(rename = "$value")]
    pub value: String,

    #[serde(rename = "pkgid")]
    pub pkg_id: Option<String>,
}

impl TryFrom<Checksum> for ContentDigest {
    type Error = RpmRepositoryError;

    fn try_from(v: Checksum) -> std::result::Result<Self, Self::Error> {
        match v.name.as_str() {
            "sha1" => ContentDigest::sha1_hex(&v.value),
            "sha256" => ContentDigest::sha256_hex(&v.value),
            name => Err(RpmRepositoryError::UnknownDigestFormat(name.to_string())),
        }
    }
}

/// Times associated with a package.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PackageTime {
    pub file: u64,
    pub build: u64,
}

/// Sizes associated with a package.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PackageSize {
    pub package: u64,

    /// Total size in bytes when installed.
    pub installed: u64,

    /// Size in bytes of package archive.
    pub archive: u64,
}

/// Additional metadata about a package.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PackageFormat {
    /// The package's license.
    pub license: Option<String>,

    /// Vendor of package.
    pub vendor: Option<String>,
    pub group: Option<String>,

    /// Hostname of machine that built the package.
    #[serde(rename = "buildhost")]
    pub build_host: Option<String>,

    /// Name of RPM from which this package is derived.
    #[serde(rename = "sourcerpm")]
    pub source_rpm: Option<String>,

    /// File segment containing the header.
    #[serde(rename = "header-range")]
    pub header_range: Option<HeaderRange>,

    /// Packages that this package provides.
    pub provides: Option<Entries>,

    /// Packages that this package obsoletes.
    pub obsoletes: Option<Entries>,

    /// Packages that this package requires.
    pub requires: Option<Entries>,

    /// Packages that conflict with this one.
    pub conflicts: Option<Entries>,

    /// Packages that are suggested when this one is installed.
    pub suggests: Option<Entries>,

    /// Packages that are recommended when this one is installed.
    pub recommends: Option<Entries>,

    /// Packages that this package supplements.
    pub supplements: Option<Entries>,

    /// Files provided by this package.
    #[serde(default, rename = "file")]
    pub files: Vec<FileEntry>,
}

/// Describes the location of a header in a package.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct HeaderRange {
    /// Start offset in bytes.
    pub start: u64,

    /// End offset in bytes.
    pub end: u64,
}

/// A collection of [PackageEntry].
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Entries {
    #[serde(rename = "entry")]
    pub entries: Vec<PackageEntry>,
}

/// Describes a package relationship.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PackageEntry {
    /// Name of package.
    pub name: String,

    /// Version comparison flags.
    pub flags: Option<String>,

    /// Epoch value.
    pub epoch: Option<u64>,

    /// Version of package.
    #[serde(rename = "ver")]
    pub version: Option<String>,

    /// Release of package.
    #[serde(rename = "rel")]
    pub release: Option<String>,

    /// Whether this is a pre-release.
    pub pre: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FileEntry {
    /// Type of file.
    ///
    /// Missing value seems to imply regular file.
    #[serde(rename = "type")]
    pub file_type: Option<String>,

    #[serde(rename = "$value")]
    pub value: String,
}
