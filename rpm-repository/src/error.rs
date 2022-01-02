// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use thiserror::Error;

/// Error type for this crate.
#[derive(Debug, Error)]
pub enum RpmRepositoryError {
    #[error("URL parse error: {0:?}")]
    UrlParse(#[from] url::ParseError),

    #[error("HTTP error: {0:?}")]
    Http(#[from] reqwest::Error),

    #[error("XML error: {0:?}")]
    Xml(#[from] serde_xml_rs::Error),

    #[error("repository I/O error on path {0}: {1:?}")]
    IoPath(String, std::io::Error),

    #[error("invalid hex in content digest: {0}; {1:?}")]
    ContentDigestBadHex(String, hex::FromHexError),

    #[error("unknown content digest format: {0}")]
    UnknownDigestFormat(String),

    #[error("repository metadata entry not found: {0}")]
    MetadataFileNotFound(&'static str),

    #[error("unexpected data path: {0}")]
    UnexpectedDataPath(String),

    #[error("content size missing from metadata entry")]
    MetadataMissingSize,
}

/// Result type for this crate.
pub type Result<T> = std::result::Result<T, RpmRepositoryError>;
