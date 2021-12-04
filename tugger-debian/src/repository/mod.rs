// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Debian repository primitives.

A Debian repository is a collection of files holding packages and other
support primitives. See <https://wiki.debian.org/DebianRepository/Format>
for the canonical definition of a Debian repository.
*/

use thiserror::Error;

#[cfg(feature = "async")]
use {
    async_compression::futures::bufread::{BzDecoder, GzipDecoder, LzmaDecoder, XzDecoder},
    async_trait::async_trait,
    futures::{AsyncBufRead, AsyncRead},
    std::pin::Pin,
};

pub mod builder;
#[cfg(feature = "async")]
pub mod filesystem;
#[cfg(feature = "http")]
pub mod http;
pub mod release;

/// Compression format used by index files.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum IndexFileCompression {
    /// No compression (no extension).
    None,

    /// XZ compression (.xz extension).
    Xz,

    /// Gzip compression (.gz extension).
    Gzip,

    /// Bzip2 compression (.bz2 extension).
    Bzip2,

    /// LZMA compression (.lzma extension).
    Lzma,
}

impl IndexFileCompression {
    /// Filename extension for files compressed in this format.
    pub fn extension(&self) -> &'static str {
        match self {
            Self::None => "",
            Self::Xz => ".xz",
            Self::Gzip => ".gz",
            Self::Bzip2 => ".bz2",
            Self::Lzma => ".lzma",
        }
    }
}

/// Errors related to reading from repositories.
#[derive(Debug, Error)]
pub enum RepositoryReadError {
    #[error("I/O error reading path {0}: {0:?}")]
    IoPath(String, std::io::Error),
}

/// Provides a transport-agnostic mechanism for reading from Debian repositories.
///
/// This trait essentially abstracts I/O for reading files in Debian repositories.
#[cfg_attr(feature = "async", async_trait)]
pub trait RepositoryReader {
    /// Get the content of a relative path as an async reader.
    ///
    /// This obtains a reader for path data and returns the raw data without any
    /// decoding applied.
    #[cfg(feature = "async")]
    async fn get_path(&self, path: &str)
        -> Result<Pin<Box<dyn AsyncBufRead>>, RepositoryReadError>;

    /// Get the content of a relative path with decompression transparently applied.
    #[cfg(feature = "async")]
    async fn get_path_decoded(
        &self,
        path: &str,
        compression: IndexFileCompression,
    ) -> Result<Pin<Box<dyn AsyncRead>>, RepositoryReadError> {
        let stream = self.get_path(path).await?;

        Ok(match compression {
            IndexFileCompression::None => Box::pin(stream),
            IndexFileCompression::Gzip => Box::pin(GzipDecoder::new(stream)),
            IndexFileCompression::Xz => Box::pin(XzDecoder::new(stream)),
            IndexFileCompression::Bzip2 => Box::pin(BzDecoder::new(stream)),
            IndexFileCompression::Lzma => Box::pin(LzmaDecoder::new(stream)),
        })
    }
}

/// Errors related to writing to repositories.
#[derive(Debug, Error)]
pub enum RepositoryWriteError {
    #[error("I/O error write path {0}: {0:?}")]
    IoPath(String, std::io::Error),
}

#[cfg_attr(feature = "async", async_trait)]
pub trait RepositoryWriter {
    /// Write data to a given path.
    ///
    /// The data to write is provided by an [AsyncRead] reader.
    #[cfg(feature = "async")]
    async fn write_path(
        &self,
        path: &str,
        reader: Pin<Box<dyn AsyncRead + Send>>,
    ) -> Result<u64, RepositoryWriteError>;
}
