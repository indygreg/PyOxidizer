// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! `repomd.xml` file format. */

use {
    crate::{
        error::{Result, RpmRepositoryError},
        io::ContentDigest,
    },
    serde::{Deserialize, Serialize},
    std::io::Read,
};

/// A `repomd.xml` file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoMd {
    /// Revision of the repository.
    ///
    /// Often an integer-like value.
    pub revision: String,
    /// Describes additional primary data files constituting this repository.
    pub data: Vec<RepoMdData>,
}

impl RepoMd {
    /// Construct an instance by parsing XML from a reader.
    pub fn from_reader(reader: impl Read) -> Result<Self> {
        Ok(serde_xml_rs::from_reader(reader)?)
    }

    /// Construct an instance by parsing XML from a string.
    pub fn from_xml(s: &str) -> Result<Self> {
        Ok(serde_xml_rs::from_str(s)?)
    }
}

/// A `<data>` element in a `repomd.xml` file.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RepoMdData {
    /// The type of data.
    #[serde(rename = "type")]
    pub data_type: String,
    /// Content checksum of this file.
    pub checksum: Checksum,
    /// Where the file is located.
    pub location: Location,
    /// Size in bytes of the file as stored in the repository.
    pub size: Option<u64>,
    /// Time file was created/modified.
    pub timestamp: Option<u64>,
    /// Content checksum of the decoded (often decompressed) file.
    #[serde(rename = "open-checksum")]
    pub open_checksum: Option<Checksum>,
    /// Size in bytes of the decoded (often decompressed) file.
    #[serde(rename = "open-size")]
    pub open_size: Option<u64>,
    /// Content checksum of header data.
    #[serde(rename = "header-checksum")]
    pub header_checksum: Option<Checksum>,
    /// Size in bytes of the header.
    #[serde(rename = "header-size")]
    pub header_size: Option<u64>,
}

/// The content checksum of a `<data>` element.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Checksum {
    /// The name of the content digest.
    #[serde(rename = "type")]
    pub name: String,
    /// The hex encoded content digest.
    #[serde(rename = "$value")]
    pub value: String,
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

/// The location of a `<data>` element.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Location {
    pub href: String,
}

#[cfg(test)]
mod test {
    use super::*;

    const FEDORA_35_REPOMD_XML: &str = include_str!("../testdata/fedora-35-repodata.xml");

    #[test]
    fn fedora_35_parse() -> Result<()> {
        RepoMd::from_xml(FEDORA_35_REPOMD_XML)?;

        Ok(())
    }
}
