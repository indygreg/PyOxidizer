// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! XAR file format */

pub mod format;
pub mod reader;
pub mod signing;
pub mod table_of_contents;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("(de)serialization error: {0}")]
    Scroll(#[from] scroll::Error),

    #[error("unable to parse checksum string: {0}")]
    BadChecksum(&'static str),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("decompression error: {0}")]
    Decompress(#[from] flate2::DecompressError),

    #[error("XML error: {0}")]
    SerdeXml(#[from] serde_xml_rs::Error),

    #[error("XML write error: {0}")]
    XmlWrite(#[from] xml::writer::Error),

    #[error("Invalid file ID")]
    InvalidFileId,

    #[error("table of contents is corrupted: {0}")]
    TableOfContentsCorrupted(&'static str),

    #[error("File has no data")]
    FileNoData,

    #[error("Unimplemented file encoding: {0}")]
    UnimplementedFileEncoding(String),

    #[error("Operation not supported: {0}")]
    Unsupported(&'static str),

    #[error("x509 certificate error: {0}")]
    X509Certificate(#[from] x509_certificate::X509CertificateError),

    #[error("CMS error: {0}")]
    Cms(#[from] cryptographic_message_syntax::CmsError),

    #[error("HTTP error: {0}")]
    Reqwest(#[from] reqwest::Error),
}

pub type XarResult<T> = std::result::Result<T, Error>;
