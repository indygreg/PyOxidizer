// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! XAR XML table of contents data structure.

use crate::Error;
use {
    crate::XarResult,
    chrono::{DateTime, Utc},
    serde::{Deserialize, Serialize},
    std::{
        fmt::{Display, Formatter},
        io::Read,
        ops::{Deref, DerefMut},
    },
    x509_certificate::{CapturedX509Certificate, X509CertificateError},
};

/// An XML table of contents in a XAR file.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TableOfContents {
    toc: XarToC,
}

impl Deref for TableOfContents {
    type Target = XarToC;

    fn deref(&self) -> &Self::Target {
        &self.toc
    }
}

impl DerefMut for TableOfContents {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.toc
    }
}

impl TableOfContents {
    /// Parse XML table of contents from a reader.
    pub fn from_reader(reader: impl Read) -> XarResult<Self> {
        Ok(serde_xml_rs::from_reader(reader)?)
    }

    /// Resolve the complete list of files.
    ///
    /// Files are sorted by their numerical ID, which should hopefully also
    /// be the order that file data occurs in the heap. Each elements consists of
    /// the full filename and the <file> record.
    pub fn files(&self) -> XarResult<Vec<(String, File)>> {
        let mut files = self
            .toc
            .files
            .iter()
            .map(|f| f.files(None))
            .collect::<XarResult<Vec<_>>>()?
            .into_iter()
            .flat_map(|x| x.into_iter())
            .collect::<Vec<_>>();

        files.sort_by(|a, b| a.1.id.cmp(&b.1.id));

        Ok(files)
    }
}

/// The main data structure inside a table of contents.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct XarToC {
    pub creation_time: String,
    pub checksum: Checksum,
    #[serde(rename = "file")]
    pub files: Vec<File>,
    pub signature: Option<Signature>,
    pub x_signature: Option<Signature>,
}

impl XarToC {
    /// Signatures present in the table of contents.
    pub fn signatures(&self) -> Vec<&Signature> {
        let mut res = vec![];
        if let Some(sig) = &self.signature {
            res.push(sig);
        }
        if let Some(sig) = &self.x_signature {
            res.push(sig);
        }

        res
    }

    /// Attempt to find a signature given a signature style.
    pub fn find_signature(&self, style: SignatureStyle) -> Option<&Signature> {
        self.signatures().into_iter().find(|sig| sig.style == style)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Checksum {
    /// The digest format used.
    pub style: ChecksumType,

    /// Offset within heap of the checksum data.
    pub offset: u64,

    /// Size of checksum data.
    pub size: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChecksumType {
    None,
    Sha1,
    Sha256,
    Sha512,
    Md5,
}

impl Display for ChecksumType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => f.write_str("none"),
            Self::Sha1 => f.write_str("SHA-1"),
            Self::Sha256 => f.write_str("SHA-256"),
            Self::Sha512 => f.write_str("SHA-512"),
            Self::Md5 => f.write_str("MD5"),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct File {
    pub id: u64,
    pub ctime: Option<DateTime<Utc>>,
    pub mtime: Option<DateTime<Utc>>,
    pub atime: Option<DateTime<Utc>>,
    /// Filename.
    ///
    /// There should only be a single element. However, some Apple tools can
    /// emit multiple <name> elements.
    #[serde(rename = "name")]
    pub names: Vec<String>,
    #[serde(rename = "type")]
    pub file_type: FileType,
    pub mode: Option<u32>,
    pub deviceno: Option<u32>,
    pub inode: Option<u64>,
    pub uid: Option<u32>,
    pub gid: Option<u32>,
    pub user: Option<String>,
    pub group: Option<String>,
    pub size: Option<u64>,
    pub data: Option<FileData>,
    pub ea: Option<Ea>,
    #[serde(rename = "FinderCreateTime")]
    pub finder_create_time: Option<FinderCreateTime>,
    #[serde(default, rename = "file")]
    pub files: Vec<File>,
}

impl File {
    pub fn files(&self, directory: Option<&str>) -> XarResult<Vec<(String, File)>> {
        let name = self
            .names
            .iter()
            .last()
            .ok_or(Error::TableOfContentsCorrupted("missing file name"))?;

        let full_path = if let Some(d) = directory {
            format!("{}/{}", d, name)
        } else {
            name.clone()
        };

        let mut files = vec![(full_path.clone(), self.clone())];

        for f in &self.files {
            files.extend(f.files(Some(&full_path))?);
        }

        Ok(files)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FileType {
    File,
    Directory,
    HardLink,
    Link,
}

impl Display for FileType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            FileType::File => f.write_str("file"),
            FileType::Directory => f.write_str("directory"),
            FileType::HardLink => f.write_str("hardlink"),
            FileType::Link => f.write_str("symlink"),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct FileData {
    pub offset: u64,
    pub size: u64,
    pub length: u64,
    pub extracted_checksum: FileChecksum,
    pub archived_checksum: FileChecksum,
    pub encoding: FileEncoding,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FileChecksum {
    pub style: ChecksumType,
    #[serde(rename = "$value")]
    pub checksum: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FileEncoding {
    pub style: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Ea {
    pub name: String,
    pub offset: u64,
    pub size: u64,
    pub length: u64,
    pub extracted_checksum: FileChecksum,
    pub archived_checksum: FileChecksum,
    pub encoding: FileEncoding,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FinderCreateTime {
    pub nanoseconds: u64,
    pub time: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Signature {
    pub style: SignatureStyle,
    pub offset: u64,
    pub size: u64,
    #[serde(rename = "KeyInfo")]
    pub key_info: KeyInfo,
}

impl Signature {
    /// Obtained parsed X.509 certificates.
    pub fn x509_certificates(&self) -> XarResult<Vec<CapturedX509Certificate>> {
        self.key_info.x509_certificates()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum SignatureStyle {
    /// Cryptographic message syntax.
    Cms,

    /// RSA signature.
    Rsa,
}

impl Display for SignatureStyle {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cms => f.write_str("CMS"),
            Self::Rsa => f.write_str("RSA"),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct KeyInfo {
    #[serde(rename = "X509Data")]
    pub x509_data: X509Data,
}

impl KeyInfo {
    /// Obtain parsed X.509 certificates.
    pub fn x509_certificates(&self) -> XarResult<Vec<CapturedX509Certificate>> {
        self.x509_data.x509_certificates()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct X509Data {
    #[serde(rename = "X509Certificate")]
    pub x509_certificate: Vec<String>,
}

impl X509Data {
    /// Obtain parsed X.509 certificates.
    pub fn x509_certificates(&self) -> XarResult<Vec<CapturedX509Certificate>> {
        Ok(self
            .x509_certificate
            .iter()
            .map(|data| {
                // The data in the XML isn't armored. So we add armoring so it can
                // be decoded by the pem crate.
                let data = format!(
                    "-----BEGIN CERTIFICATE-----\r\n{}\r\n-----END CERTIFICATE-----\r\n",
                    data
                );

                CapturedX509Certificate::from_pem(data)
            })
            .collect::<Result<Vec<_>, X509CertificateError>>()?)
    }
}
