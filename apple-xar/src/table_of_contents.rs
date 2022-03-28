// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! XAR XML table of contents data structure.

use {
    crate::{format::XarChecksum, Error, XarResult},
    digest::DynDigest,
    serde::Deserialize,
    std::{
        fmt::{Display, Formatter},
        io::{Read, Write},
        ops::{Deref, DerefMut},
    },
    x509_certificate::{CapturedX509Certificate, X509CertificateError},
    xml::{
        common::XmlVersion,
        writer::{EmitterConfig, EventWriter, XmlEvent},
    },
};

/// An XML table of contents in a XAR file.
#[derive(Clone, Debug, Deserialize)]
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

    pub fn to_xml(&self) -> XarResult<Vec<u8>> {
        let mut emitter = EmitterConfig::new().create_writer(std::io::BufWriter::new(vec![]));
        self.write_xml(&mut emitter)?;

        emitter
            .into_inner()
            .into_inner()
            .map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))
    }

    pub fn write_xml<W: Write>(&self, writer: &mut EventWriter<W>) -> XarResult<()> {
        writer.write(XmlEvent::StartDocument {
            version: XmlVersion::Version10,
            encoding: Some("UTF-8"),
            standalone: None,
        })?;

        writer.write(XmlEvent::start_element("xar"))?;
        writer.write(XmlEvent::start_element("toc"))?;

        writer.write(XmlEvent::start_element("creation-time"))?;
        writer.write(XmlEvent::characters(&self.creation_time))?;
        writer.write(XmlEvent::end_element())?;

        self.checksum.write_xml(writer)?;

        if let Some(sig) = &self.signature {
            sig.write_xml(writer, "signature")?;
        }
        if let Some(sig) = &self.x_signature {
            sig.write_xml(writer, "x-signature")?;
        }

        for file in &self.files {
            file.write_xml(writer)?;
        }

        writer.write(XmlEvent::end_element().name("toc"))?;
        writer.write(XmlEvent::end_element().name("xar"))?;

        Ok(())
    }
}

/// The main data structure inside a table of contents.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
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

    pub fn visit_files_mut(&mut self, cb: &dyn Fn(&mut File)) {
        for file in self.files.iter_mut() {
            cb(file);
            file.visit_files_mut(cb);
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checksum {
    /// The digest format used.
    pub style: ChecksumType,

    /// Offset within heap of the checksum data.
    pub offset: u64,

    /// Size of checksum data.
    pub size: u64,
}

impl Checksum {
    pub fn write_xml<W: Write>(&self, writer: &mut EventWriter<W>) -> XarResult<()> {
        writer.write(XmlEvent::start_element("checksum").attr("style", &self.style.to_string()))?;
        writer.write(XmlEvent::start_element("offset"))?;
        writer.write(XmlEvent::characters(&format!("{}", self.offset)))?;
        writer.write(XmlEvent::end_element())?;
        writer.write(XmlEvent::start_element("size"))?;
        writer.write(XmlEvent::characters(&format!("{}", self.size)))?;
        writer.write(XmlEvent::end_element())?;
        writer.write(XmlEvent::end_element().name("checksum"))?;

        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
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
            Self::Sha1 => f.write_str("sha1"),
            Self::Sha256 => f.write_str("sha256"),
            Self::Sha512 => f.write_str("sha512"),
            Self::Md5 => f.write_str("md5"),
        }
    }
}

impl TryFrom<XarChecksum> for ChecksumType {
    type Error = Error;

    fn try_from(v: XarChecksum) -> Result<Self, Self::Error> {
        match v {
            XarChecksum::None => Ok(Self::None),
            XarChecksum::Sha1 => Ok(Self::Sha1),
            XarChecksum::Md5 => Ok(Self::Md5),
            XarChecksum::Sha256 => Ok(Self::Sha256),
            XarChecksum::Sha512 => Ok(Self::Sha512),
            XarChecksum::Other(_) => Err(Error::Unsupported("unknown checksum type")),
        }
    }
}

impl From<ChecksumType> for XarChecksum {
    fn from(v: ChecksumType) -> Self {
        match v {
            ChecksumType::None => Self::None,
            ChecksumType::Sha1 => Self::Sha1,
            ChecksumType::Sha256 => Self::Sha256,
            ChecksumType::Sha512 => Self::Sha512,
            ChecksumType::Md5 => Self::Md5,
        }
    }
}

impl ChecksumType {
    /// Digest a slice of data.
    pub fn digest_data(&self, data: &[u8]) -> XarResult<Vec<u8>> {
        let mut h: Box<dyn DynDigest> = match self {
            Self::None => return Err(Error::Unsupported("cannot digest None checksum")),
            Self::Md5 => Box::new(md5::Md5::default()),
            Self::Sha1 => Box::new(sha1::Sha1::default()),
            Self::Sha256 => Box::new(sha2::Sha256::default()),
            Self::Sha512 => Box::new(sha2::Sha512::default()),
        };

        h.update(data);

        Ok(h.finalize().to_vec())
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct File {
    pub id: u64,
    pub ctime: Option<String>,
    pub mtime: Option<String>,
    pub atime: Option<String>,
    /// Filename.
    ///
    /// There should only be a single element. However, some Apple tools can
    /// emit multiple <name> elements.
    #[serde(rename = "name")]
    pub names: Vec<String>,
    #[serde(rename = "type")]
    pub file_type: FileType,
    pub mode: Option<String>,
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

    pub fn visit_files_mut(&mut self, cb: &dyn Fn(&mut File)) {
        for f in self.files.iter_mut() {
            cb(f);
            f.visit_files_mut(cb)
        }
    }

    pub fn write_xml<W: Write>(&self, writer: &mut EventWriter<W>) -> XarResult<()> {
        writer.write(XmlEvent::start_element("file").attr("id", &format!("{}", self.id)))?;

        if let Some(data) = &self.data {
            data.write_xml(writer)?;
        }

        if let Some(fct) = &self.finder_create_time {
            writer.write(XmlEvent::start_element("FinderCreateTime"))?;

            writer.write(XmlEvent::start_element("nanoseconds"))?;
            writer.write(XmlEvent::characters(&format!("{}", fct.nanoseconds)))?;
            writer.write(XmlEvent::end_element())?;

            writer.write(XmlEvent::start_element("time"))?;
            writer.write(XmlEvent::characters(&fct.time))?;
            writer.write(XmlEvent::end_element())?;

            writer.write(XmlEvent::end_element())?;
        }

        if let Some(time) = &self.ctime {
            writer.write(XmlEvent::start_element("ctime"))?;
            writer.write(XmlEvent::characters(time))?;
            writer.write(XmlEvent::end_element())?;
        }

        if let Some(time) = &self.mtime {
            writer.write(XmlEvent::start_element("mtime"))?;
            writer.write(XmlEvent::characters(time))?;
            writer.write(XmlEvent::end_element())?;
        }

        if let Some(time) = &self.atime {
            writer.write(XmlEvent::start_element("atime"))?;
            writer.write(XmlEvent::characters(time))?;
            writer.write(XmlEvent::end_element())?;
        }

        if let Some(v) = &self.group {
            writer.write(XmlEvent::start_element("group"))?;
            writer.write(XmlEvent::characters(v))?;
            writer.write(XmlEvent::end_element())?;
        }

        if let Some(v) = &self.gid {
            writer.write(XmlEvent::start_element("gid"))?;
            writer.write(XmlEvent::characters(&format!("{}", v)))?;
            writer.write(XmlEvent::end_element())?;
        }

        if let Some(v) = &self.user {
            writer.write(XmlEvent::start_element("user"))?;
            writer.write(XmlEvent::characters(v))?;
            writer.write(XmlEvent::end_element())?;
        }

        if let Some(v) = &self.uid {
            writer.write(XmlEvent::start_element("uid"))?;
            writer.write(XmlEvent::characters(&format!("{}", v)))?;
            writer.write(XmlEvent::end_element())?;
        }

        if let Some(v) = &self.mode {
            writer.write(XmlEvent::start_element("mode"))?;
            writer.write(XmlEvent::characters(v))?;
            writer.write(XmlEvent::end_element())?;
        }

        if let Some(v) = &self.deviceno {
            writer.write(XmlEvent::start_element("deviceno"))?;
            writer.write(XmlEvent::characters(&format!("{}", v)))?;
            writer.write(XmlEvent::end_element())?;
        }

        if let Some(v) = &self.inode {
            writer.write(XmlEvent::start_element("inode"))?;
            writer.write(XmlEvent::characters(&format!("{}", v)))?;
            writer.write(XmlEvent::end_element())?;
        }

        if let Some(ea) = &self.ea {
            ea.write_xml(writer)?;
        }

        writer.write(XmlEvent::start_element("type"))?;
        writer.write(XmlEvent::characters(&self.file_type.to_string()))?;
        writer.write(XmlEvent::end_element())?;

        for name in &self.names {
            writer.write(XmlEvent::start_element("name"))?;
            writer.write(XmlEvent::characters(name))?;
            writer.write(XmlEvent::end_element())?;
        }

        for file in &self.files {
            file.write_xml(writer)?;
        }

        writer.write(XmlEvent::end_element().name("file"))?;

        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
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

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct FileData {
    pub offset: u64,
    pub size: u64,
    pub length: u64,
    pub extracted_checksum: FileChecksum,
    pub archived_checksum: FileChecksum,
    pub encoding: FileEncoding,
}

impl FileData {
    pub fn write_xml<W: Write>(&self, writer: &mut EventWriter<W>) -> XarResult<()> {
        writer.write(XmlEvent::start_element("data"))?;

        writer.write(XmlEvent::start_element("length"))?;
        writer.write(XmlEvent::characters(&format!("{}", self.length)))?;
        writer.write(XmlEvent::end_element())?;

        writer.write(XmlEvent::start_element("offset"))?;
        writer.write(XmlEvent::characters(&format!("{}", self.offset)))?;
        writer.write(XmlEvent::end_element())?;

        writer.write(XmlEvent::start_element("size"))?;
        writer.write(XmlEvent::characters(&format!("{}", self.size)))?;
        writer.write(XmlEvent::end_element())?;

        writer.write(XmlEvent::start_element("encoding").attr("style", &self.encoding.style))?;
        writer.write(XmlEvent::end_element())?;

        self.extracted_checksum
            .write_xml(writer, "extracted-checksum")?;
        self.archived_checksum
            .write_xml(writer, "archived-checksum")?;

        writer.write(XmlEvent::end_element().name("data"))?;

        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileChecksum {
    pub style: ChecksumType,
    #[serde(rename = "$value")]
    pub checksum: String,
}

impl FileChecksum {
    pub fn write_xml<W: Write>(&self, writer: &mut EventWriter<W>, name: &str) -> XarResult<()> {
        writer.write(XmlEvent::start_element(name).attr("style", &self.style.to_string()))?;
        writer.write(XmlEvent::characters(&self.checksum))?;
        writer.write(XmlEvent::end_element())?;

        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileEncoding {
    pub style: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Ea {
    pub name: String,
    pub offset: u64,
    pub size: u64,
    pub length: u64,
    pub extracted_checksum: FileChecksum,
    pub archived_checksum: FileChecksum,
    pub encoding: FileEncoding,
}

impl Ea {
    pub fn write_xml<W: Write>(&self, writer: &mut EventWriter<W>) -> XarResult<()> {
        writer.write(XmlEvent::start_element("ea"))?;

        writer.write(XmlEvent::start_element("name"))?;
        writer.write(XmlEvent::characters(&self.name))?;
        writer.write(XmlEvent::end_element())?;

        writer.write(XmlEvent::start_element("offset"))?;
        writer.write(XmlEvent::characters(&format!("{}", self.offset)))?;
        writer.write(XmlEvent::end_element())?;

        writer.write(XmlEvent::start_element("size"))?;
        writer.write(XmlEvent::characters(&format!("{}", self.size)))?;
        writer.write(XmlEvent::end_element())?;

        writer.write(XmlEvent::start_element("length"))?;
        writer.write(XmlEvent::characters(&format!("{}", self.length)))?;
        writer.write(XmlEvent::end_element())?;

        self.extracted_checksum
            .write_xml(writer, "extracted-checksum")?;
        self.archived_checksum
            .write_xml(writer, "archived-checksum")?;

        writer.write(XmlEvent::start_element("encoding").attr("style", &self.encoding.style))?;
        writer.write(XmlEvent::end_element())?;

        writer.write(XmlEvent::end_element())?;

        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FinderCreateTime {
    pub nanoseconds: u64,
    pub time: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
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

    pub fn write_xml<W: Write>(&self, writer: &mut EventWriter<W>, name: &str) -> XarResult<()> {
        writer.write(XmlEvent::start_element(name).attr("style", &self.style.to_string()))?;

        writer.write(XmlEvent::start_element("offset"))?;
        writer.write(XmlEvent::characters(&format!("{}", &self.offset)))?;
        writer.write(XmlEvent::end_element())?;

        writer.write(XmlEvent::start_element("size"))?;
        writer.write(XmlEvent::characters(&format!("{}", &self.size)))?;
        writer.write(XmlEvent::end_element())?;

        writer.write(
            XmlEvent::start_element("KeyInfo").ns("", "http://www.w3.org/2000/09/xmldsig#"),
        )?;
        writer.write(XmlEvent::start_element("X509Data"))?;

        for cert in &self.key_info.x509_data.x509_certificate {
            writer.write(XmlEvent::start_element("X509Certificate"))?;
            writer.write(XmlEvent::characters(cert))?;
            writer.write(XmlEvent::end_element())?;
        }

        writer.write(XmlEvent::end_element().name("X509Data"))?;
        writer.write(XmlEvent::end_element().name("KeyInfo"))?;

        writer.write(XmlEvent::end_element().name(name))?;

        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
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

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyInfo {
    #[serde(rename = "X509Data")]
    pub x509_data: X509Data,
}

impl KeyInfo {
    /// Construct an instance from an iterable of certificates.
    pub fn from_certificates<'a>(
        certs: impl Iterator<Item = &'a CapturedX509Certificate>,
    ) -> XarResult<Self> {
        Ok(Self {
            x509_data: X509Data {
                x509_certificate: certs
                    .map(|cert| {
                        let der = cert.encode_der()?;
                        let s = base64::encode(der);

                        let mut lines = vec![];

                        let mut remaining = s.as_str();

                        loop {
                            if remaining.len() > 72 {
                                let res = remaining.split_at(72);
                                lines.push(res.0);
                                remaining = res.1;
                            } else {
                                lines.push(remaining);
                                break;
                            }
                        }

                        Ok(lines.join("\n"))
                    })
                    .collect::<XarResult<Vec<_>>>()?,
            },
        })
    }

    /// Obtain parsed X.509 certificates.
    pub fn x509_certificates(&self) -> XarResult<Vec<CapturedX509Certificate>> {
        self.x509_data.x509_certificates()
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
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
