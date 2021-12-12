// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Error handling. */

use {thiserror::Error, tugger_file_manifest::FileManifestError};

/// Primary crate error type.
#[derive(Debug, Error)]
pub enum DebianError {
    #[error("file manifest error: {0}")]
    FileManifestError(#[from] FileManifestError),

    #[cfg(feature = "http")]
    #[error("URL error: {0:?}")]
    Url(#[from] url::ParseError),

    #[error("hex parsing error: {0:?}")]
    Hex(#[from] hex::FromHexError),

    #[error("PGP error: {0:?}")]
    Pgp(#[from] pgp::errors::Error),

    #[error("date parsing error: {0:?}")]
    DateParse(#[from] mailparse::MailParseError),

    #[cfg(feature = "http")]
    #[error("HTTP error: {0:?}")]
    Reqwest(#[from] reqwest::Error),

    #[error("I/O error: {0:?}")]
    Io(#[from] std::io::Error),

    #[error("integer parsing error: {0:?}")]
    ParseInt(#[from] std::num::ParseIntError),

    #[error("control file parse error: {0}")]
    ControlParseError(String),

    #[error("Control file lacks a paragraph")]
    ControlFileNoParagraph,

    #[error("Control file not found")]
    ControlFileNotFound,

    #[error("unknown entry in binary package archive: {0}")]
    DebUnknownBinaryPackageEntry(String),

    #[error("unknown compression in deb archive file: {0}")]
    DebUnknownCompression(String),

    #[error("release file does not contain supported checksum flavor")]
    RepositoryReadReleaseNoKnownChecksum,

    #[error("could not find Contents indices entry in Release file")]
    RepositoryReadContentsIndicesEntryNotFound,

    #[error("could not find packages indices entry in Release file")]
    RepositoryReadPackagesIndicesEntryNotFound,

    #[error("No packages indices for checksum {0}")]
    RepositoryNoPackagesIndices(&'static str),

    #[error("repository I/O error on path {0}: {1:?}")]
    RepositoryIoPath(String, std::io::Error),

    #[error("attempting to add package to undefined component: {0}")]
    RepositoryBuildUnknownComponent(String),

    #[error("attempting to add package to undefined architecture: {0}")]
    RepositoryBuildUnknownArchitecture(String),

    #[error("pool layout cannot be changed after content is indexed")]
    RepositoryBuildPoolLayoutImmutable,

    #[error(".deb not available: {0}")]
    RepositoryBuildDebNotAvailable(&'static str),

    #[error("expected 1 paragraph in control file; got {0}")]
    ReleaseControlParagraphMismatch(usize),

    #[error("digest missing from index entry")]
    ReleaseMissingDigest,

    #[error("size missing from index entry")]
    ReleaseMissingSize,

    #[error("path missing from index entry")]
    ReleaseMissingPath,

    #[error("index entry path unexpectedly has spaces: {0}")]
    ReleasePathWithSpaces(String),

    #[error("No PGP signatures found")]
    ReleaseNoSignatures,

    #[error("No PGP signatures found from the specified key")]
    ReleaseNoSignaturesByKey,

    #[error("failed to parse dependency expression: {0}")]
    DependencyParse(String),

    #[error("unknown binary dependency field: {0}")]
    UnknownBinaryDependencyField(String),

    #[error("the epoch component has non-digit characters: {0}")]
    EpochNonNumeric(String),

    #[error("upstream_version component has illegal character: {0}")]
    UpstreamVersionIllegalChar(String),

    #[error("debian_revision component has illegal character: {0}")]
    DebianRevisionIllegalChar(String),

    #[error("required field missing in binary package control file: {0}")]
    BinaryPackageControlRequiredFiledMissing(&'static str),
}

/// Result wrapper for this crate.
pub type Result<T> = std::result::Result<T, DebianError>;
