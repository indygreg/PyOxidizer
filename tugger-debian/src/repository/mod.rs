// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Debian repository primitives.

A Debian repository is a collection of files holding packages and other
support primitives. See <https://wiki.debian.org/DebianRepository/Format>
for the canonical definition of a Debian repository.
*/

use {
    crate::{
        control::ControlError,
        repository::release::{ChecksumType, PackagesFileEntry, ReleaseError, ReleaseFile},
    },
    thiserror::Error,
};

#[cfg(feature = "async")]
use {
    crate::{
        binary_package_control::BinaryPackageControlFile, binary_package_list::BinaryPackageList,
        control::ControlParagraphAsyncReader,
    },
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

    /// The default retrieval preference order for client.
    pub fn default_preferred_order() -> impl Iterator<Item = IndexFileCompression> {
        [Self::Xz, Self::Lzma, Self::Gzip, Self::Bzip2, Self::None].into_iter()
    }
}

/// Errors related to reading from repositories.
#[derive(Debug, Error)]
pub enum RepositoryReadError {
    #[error("I/O error reading path {0}: {0:?}")]
    IoPath(String, std::io::Error),

    #[error("Release file does not contain supported checksum flavor")]
    NoKnownChecksum,

    #[error("Could not find packages indices entry")]
    PackagesIndicesEntryNotFound,

    #[error("Control file error: {0:?}")]
    Control(#[from] ControlError),

    #[error("Release file error: {0:?}")]
    Release(#[from] ReleaseError),
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
    async fn get_path(
        &self,
        path: &str,
    ) -> Result<Pin<Box<dyn AsyncBufRead + Send>>, RepositoryReadError>;

    /// Get the content of a relative path with decompression transparently applied.
    #[cfg(feature = "async")]
    async fn get_path_decoded(
        &self,
        path: &str,
        compression: IndexFileCompression,
    ) -> Result<Pin<Box<dyn AsyncRead + Send>>, RepositoryReadError> {
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

/// Provides a transport-agnostic mechanism for reading from a parsed `[In]Release` file.
#[cfg_attr(feature = "async", async_trait)]
pub trait ReleaseReader: RepositoryReader {
    /// Obtain the parsed `[In]Release` file from which this reader is derived.
    fn release_file(&self) -> &ReleaseFile<'static>;

    /// Obtain the checksum flavor of content to retrieve.
    ///
    /// By default, this will prefer the strongest known checksum advertised in the
    /// release file.
    fn retrieve_checksum(&self) -> Result<ChecksumType, RepositoryReadError> {
        let release = self.release_file();

        let checksum = &[ChecksumType::Sha256, ChecksumType::Sha1, ChecksumType::Md5]
            .iter()
            .find(|variant| release.first_field(variant.field_name()).is_some())
            .ok_or(RepositoryReadError::NoKnownChecksum)?;

        Ok(**checksum)
    }

    /// Obtain the preferred compression format to retrieve index files in.
    fn preferred_compression(&self) -> IndexFileCompression;

    /// Set the preferred compression format for retrieved index files.
    ///
    /// Index files are often published in multiple compression formats, including no
    /// compression. This function can be used to instruct the reader which compression
    /// format to prefer.
    fn set_preferred_compression(&mut self, compression: IndexFileCompression);

    /// Obtain parsed `Packages` file entries within this Release file.
    ///
    /// Only entries for the checksum as defined by [Self::retrieve_checksum()] are returned.
    ///
    /// There may be multiple entries for a given logical `Packages` file corresponding
    /// to different compression formats. Use [Self::packages_entry()] to resolve the entry
    /// for the `Packages` file for the preferred configuration.
    fn packages_indices_entries(&self) -> Result<Vec<PackagesFileEntry>, RepositoryReadError> {
        Ok(
            if let Some(entries) = self
                .release_file()
                .iter_packages_indices(self.retrieve_checksum()?)
            {
                entries.collect::<Result<Vec<_>, _>>()?
            } else {
                vec![]
            },
        )
    }

    /// Resolve a reference to a `Packages` file to fetch given search criteria.
    ///
    /// This will find all entries defining the desired `Packages` file. It will filter
    /// through the [ChecksumType] as defined by [Self::retrieve_checksum()] and will prioritize
    /// the compression format according to [Self::preferred_compression()].
    fn packages_entry(
        &self,
        component: &str,
        architecture: &str,
        is_installer: bool,
    ) -> Result<PackagesFileEntry, RepositoryReadError> {
        let entries = self
            .packages_indices_entries()?
            .into_iter()
            .filter(|entry| {
                entry.component == component
                    && entry.architecture == architecture
                    && entry.is_installer == is_installer
            })
            .collect::<Vec<_>>();

        if let Some(entry) = entries
            .iter()
            .find(|entry| entry.compression == self.preferred_compression())
        {
            Ok(entry.clone())
        } else {
            for compression in IndexFileCompression::default_preferred_order() {
                if let Some(entry) = entries
                    .iter()
                    .find(|entry| entry.compression == compression)
                {
                    return Ok(entry.clone());
                }
            }

            Err(RepositoryReadError::PackagesIndicesEntryNotFound)
        }
    }

    /// Resolve packages given parameters to resolve a `Packages` file.
    #[cfg(feature = "async")]
    async fn resolve_packages(
        &self,
        component: &str,
        arch: &str,
        is_installer: bool,
    ) -> Result<BinaryPackageList<'static>, RepositoryReadError> {
        let release = self.release_file();

        let entry = self.packages_entry(component, arch, is_installer)?;

        let path = if release.acquire_by_hash().unwrap_or_default() {
            entry.entry.by_hash_path()
        } else {
            entry.entry.path.to_string()
        };

        // TODO perform digest verification.
        // TODO make this stream output.

        let mut reader = ControlParagraphAsyncReader::new(futures::io::BufReader::new(
            self.get_path_decoded(&path, entry.compression).await?,
        ));

        let mut res = BinaryPackageList::default();

        while let Some(paragraph) = reader.read_paragraph().await? {
            res.push(BinaryPackageControlFile::from(paragraph));
        }

        Ok(res)
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
