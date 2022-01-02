// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! RPM repository interaction.

This crate facilitates interacting with RPM package repositories.

RPM repositories are defined by a base URL. Under that base URL is typically a
`repodata` directory containing a `repomd.xml` file. This `repomd.xml` file
(represented by [metadata::repomd::RepoMd]) describes other _metadata_
files constituting the repository.

Files and data structures in the `repodata` directory are defined in the
[metadata] module tree.

The [RepositoryRootReader] trait defines a generic read interface bound to a
base URL. The [MetadataReader] trait defines an interface to repository metadata
via a parsed `repomd.xml` file.

Concrete repository readers exist. [http::HttpRepositoryClient] provides a reader
for repositories accessed via HTTP.

*/

pub mod error;
pub mod http;
pub mod io;
pub mod metadata;

pub use crate::error::{Result, RpmRepositoryError};

use {
    crate::{
        io::{read_decompressed, Compression, ContentDigest, ContentValidatingReader},
        metadata::{
            primary::Primary,
            repomd::{RepoMd, RepoMdData},
        },
    },
    futures::{AsyncRead, AsyncReadExt},
    std::{future::Future, pin::Pin},
};

/// Path based content fetching.
pub trait DataResolver: Sync {
    /// Get the content of a relative path as an async reader.
    #[allow(clippy::type_complexity)]
    fn get_path(
        &self,
        path: String,
    ) -> Pin<Box<dyn Future<Output = Result<Pin<Box<dyn AsyncRead + Send>>>> + Send + '_>>;

    /// Obtain a reader that performs content integrity checking.
    ///
    /// Because content digests can only be computed once all content is read, the reader
    /// emits data as it is streaming but only compares the cryptographic digest once all
    /// data has been read. If there is a content digest mismatch, an error will be raised
    /// once the final byte is read.
    ///
    /// Validation only occurs if the stream is read to completion. Failure to read the
    /// entire stream could result in reading of unexpected content.
    #[allow(clippy::type_complexity)]
    fn get_path_with_digest_verification(
        &self,
        path: String,
        expected_size: u64,
        expected_digest: ContentDigest,
    ) -> Pin<Box<dyn Future<Output = Result<Pin<Box<dyn AsyncRead + Send>>>> + Send + '_>> {
        async fn run(
            slf: &(impl DataResolver + ?Sized),
            path: String,
            expected_size: u64,
            expected_digest: ContentDigest,
        ) -> Result<Pin<Box<dyn AsyncRead + Send>>> {
            Ok(Box::pin(ContentValidatingReader::new(
                slf.get_path(path).await?,
                expected_size,
                expected_digest,
            )))
        }

        Box::pin(run(self, path, expected_size, expected_digest))
    }

    /// Get the content of a relative path, transparently applying decompression.
    #[allow(clippy::type_complexity)]
    fn get_path_decompressed(
        &self,
        path: String,
        compression: Compression,
    ) -> Pin<Box<dyn Future<Output = Result<Pin<Box<dyn AsyncRead + Send>>>> + Send + '_>> {
        async fn run(
            slf: &(impl DataResolver + ?Sized),
            path: String,
            compression: Compression,
        ) -> Result<Pin<Box<dyn AsyncRead + Send>>> {
            let reader = slf.get_path(path).await?;

            Ok(read_decompressed(
                Box::pin(futures::io::BufReader::new(reader)),
                compression,
            ))
        }

        Box::pin(run(self, path, compression))
    }

    /// A combination of both [Self::get_path_decompressed()] and [Self::get_path_with_digest_verification()].
    #[allow(clippy::type_complexity)]
    fn get_path_decompressed_with_digest_verification(
        &self,
        path: String,
        compression: Compression,
        expected_size: u64,
        expected_digest: ContentDigest,
    ) -> Pin<Box<dyn Future<Output = Result<Pin<Box<dyn AsyncRead + Send>>>> + Send + '_>> {
        async fn run(
            slf: &(impl DataResolver + ?Sized),
            path: String,
            compression: Compression,
            expected_size: u64,
            expected_digest: ContentDigest,
        ) -> Result<Pin<Box<dyn AsyncRead + Send>>> {
            let reader = slf
                .get_path_with_digest_verification(path, expected_size, expected_digest)
                .await?;

            Ok(read_decompressed(
                Box::pin(futures::io::BufReader::new(reader)),
                compression,
            ))
        }

        Box::pin(run(self, path, compression, expected_size, expected_digest))
    }
}

/// A read-only interface for the root of an RPM repository.
pub trait RepositoryRootReader: DataResolver + Sync {
    /// Obtain the URL to which this reader is bound.
    fn url(&self) -> Result<url::Url>;

    #[allow(clippy::type_complexity)]
    fn metadata_reader(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn MetadataReader>>> + Send + '_>>;

    /// Fetch and parse a `repomd.xml` file given the relative path to that file.
    fn fetch_repomd(
        &self,
        path: String,
    ) -> Pin<Box<dyn Future<Output = Result<RepoMd>> + Send + '_>> {
        async fn run(slf: &(impl RepositoryRootReader + ?Sized), path: String) -> Result<RepoMd> {
            let mut reader = slf.get_path(path.clone()).await?;

            let mut data = vec![];
            reader
                .read_to_end(&mut data)
                .await
                .map_err(|e| RpmRepositoryError::IoPath(path, e))?;

            RepoMd::from_reader(std::io::Cursor::new(data))
        }

        Box::pin(run(self, path))
    }
}

/// A read-only interface for metadata in an RPM repository.
///
/// This essentially provides methods for retrieving and parsing content
/// from the `repodata` directory.
pub trait MetadataReader: DataResolver + Sync {
    /// Obtain the base URL to which this instance is bound.
    fn url(&self) -> Result<url::Url>;

    /// Obtain the path relative to the repository root this instance is bound to.
    fn root_relative_path(&self) -> &str;

    /// Obtain the raw parsed `repomd.xml` data structure.
    fn repomd(&self) -> &RepoMd;

    #[allow(clippy::type_complexity)]
    fn fetch_data_file<'slf>(
        &'slf self,
        data: &'slf RepoMdData,
    ) -> Pin<Box<dyn Future<Output = Result<Pin<Box<dyn AsyncRead + Send>>>> + Send + 'slf>> {
        async fn run(
            slf: &(impl MetadataReader + ?Sized),
            data: &RepoMdData,
        ) -> Result<Pin<Box<dyn AsyncRead + Send>>> {
            let path = data.location.href.as_str();

            let expected_size = data.size.ok_or(RpmRepositoryError::MetadataMissingSize)?;
            let expected_digest = ContentDigest::try_from(data.checksum.clone())?;

            let compression = match path {
                _ if path.ends_with(".gz") => Compression::Gzip,
                _ if path.ends_with(".xz") => Compression::Xz,
                _ => Compression::None,
            };

            slf.get_path_decompressed_with_digest_verification(
                path.to_string(),
                compression,
                expected_size,
                expected_digest,
            )
            .await
        }

        Box::pin(run(self, data))
    }

    #[allow(clippy::type_complexity)]
    fn primary_packages(&self) -> Pin<Box<dyn Future<Output = Result<Primary>> + Send + '_>> {
        async fn run(slf: &(impl MetadataReader + ?Sized)) -> Result<Primary> {
            let primary = slf
                .repomd()
                .data
                .iter()
                .find(|entry| entry.data_type == "primary")
                .ok_or(RpmRepositoryError::MetadataFileNotFound("primary"))?;

            let mut reader = slf.fetch_data_file(primary).await?;
            let mut data = vec![];

            reader
                .read_to_end(&mut data)
                .await
                .map_err(|e| RpmRepositoryError::IoPath(primary.location.href.clone(), e))?;

            Primary::from_reader(std::io::Cursor::new(data))
        }

        Box::pin(run(self))
    }
}
