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
        binary_package_control::BinaryPackageControlFile,
        binary_package_list::BinaryPackageList,
        control::{ControlError, ControlParagraphAsyncReader},
        io::{
            drain_reader, read_decompressed, Compression, ContentDigest, ContentValidatingReader,
        },
        repository::{
            contents::{ContentsError, ContentsFile, ContentsFileAsyncReader},
            release::{
                ChecksumType, ContentsFileEntry, PackagesFileEntry, ReleaseError, ReleaseFile,
                SourcesFileEntry,
            },
        },
    },
    async_trait::async_trait,
    futures::{AsyncBufRead, AsyncRead, AsyncReadExt},
    std::pin::Pin,
    thiserror::Error,
};

pub mod builder;
pub mod contents;
pub mod filesystem;
#[cfg(feature = "http")]
pub mod http;
pub mod release;

/// Errors related to reading from repositories.
#[derive(Debug, Error)]
pub enum RepositoryReadError {
    #[error("I/O error reading path {0}: {0:?}")]
    IoPath(String, std::io::Error),

    #[error("Release file does not contain supported checksum flavor")]
    NoKnownChecksum,

    #[error("Could not find Contents indices entry")]
    ContentsIndicesEntryNotFound,

    #[error("Could not find packages indices entry")]
    PackagesIndicesEntryNotFound,

    #[error("Contents file error: {0:?}")]
    Contents(#[from] ContentsError),

    #[error("Control file error: {0:?}")]
    Control(#[from] ControlError),

    #[error("Release file error: {0:?}")]
    Release(#[from] ReleaseError),

    #[error("URL error: {0:?}")]
    Url(#[from] url::ParseError),
}

/// Debian repository reader bound to the root of the repository.
///
/// This trait facilitates access to *pool* as well as to multiple
/// *releases* within the repository.
#[async_trait]
pub trait RepositoryRootReader {
    /// Obtain the URL to which this reader is bound.  
    fn url(&self) -> &url::Url;

    /// Get the content of a relative path as an async reader.
    ///
    /// This obtains a reader for path data and returns the raw data without any
    /// decoding applied.
    async fn get_path(
        &self,
        path: &str,
    ) -> Result<Pin<Box<dyn AsyncBufRead + Send>>, RepositoryReadError>;

    /// Obtain a reader that performs content integrity checking.
    ///
    /// Because content digests can only be computed once all content is read, the reader
    /// emits data as it is streaming but only compares the cryptographic digest once all
    /// data has been read. If there is a content digest mismatch, an error will be raised
    /// once the final byte is read.
    ///
    /// Validation only occurs if the stream is read to completion. Failure to read the
    /// entire stream could result in reading of unexpected content.
    async fn get_path_with_digest_verification(
        &self,
        path: &str,
        expected_size: usize,
        expected_digest: ContentDigest,
    ) -> Result<Pin<Box<dyn AsyncRead>>, RepositoryReadError> {
        Ok(Box::pin(ContentValidatingReader::new(
            self.get_path(path).await?,
            expected_size,
            expected_digest,
        )))
    }

    /// Get the content of a relative path with decompression transparently applied.
    async fn get_path_decoded(
        &self,
        path: &str,
        compression: Compression,
    ) -> Result<Pin<Box<dyn AsyncRead + Send>>, RepositoryReadError> {
        read_decompressed(self.get_path(path).await?, compression)
            .await
            .map_err(|e| RepositoryReadError::IoPath(path.to_string(), e))
    }

    /// Obtain a [ReleaseReader] for a given distribution.
    ///
    /// This assumes the `InRelease` file is located in `dists/{distribution}/`. This is the case
    /// for most repositories.
    async fn release_reader(
        &self,
        distribution: &str,
    ) -> Result<Box<dyn ReleaseReader>, RepositoryReadError> {
        self.release_reader_with_distribution_path(&format!(
            "dists/{}",
            distribution.trim_matches('/')
        ))
        .await
    }

    /// Obtain a [ReleaseReader] given a distribution path.
    ///
    /// Typically distributions exist at `dists/<distribution>/`. However, this may not
    /// always be the case. This method allows explicitly passing in the relative path
    /// holding the `InRelease` file.
    async fn release_reader_with_distribution_path(
        &self,
        path: &str,
    ) -> Result<Box<dyn ReleaseReader>, RepositoryReadError>;

    /// Fetch and parse an `InRelease` file at the relative path specified.
    ///
    /// `path` is typically a value like `dists/<distribution>/InRelease`. e.g.
    /// `dists/bullseye/InRelease`.
    ///
    /// The default implementation of this trait should be sufficient for most types.
    async fn fetch_inrelease(
        &self,
        path: &str,
    ) -> Result<ReleaseFile<'static>, RepositoryReadError> {
        let mut reader = self.get_path(path).await?;

        let mut data = vec![];
        reader
            .read_to_end(&mut data)
            .await
            .map_err(|e| RepositoryReadError::IoPath(path.to_string(), e))?;

        Ok(ReleaseFile::from_armored_reader(std::io::Cursor::new(
            data,
        ))?)
    }
}

/// Provides a transport-agnostic mechanism for reading from a parsed `[In]Release` file.
#[async_trait]
pub trait ReleaseReader: Sync {
    /// Obtain the base URL to which this instance is bound.
    fn url(&self) -> &url::Url;

    /// Get the content of a relative path as an async reader.
    ///
    /// This obtains a reader for path data and returns the raw data without any
    /// decoding applied.
    async fn get_path(
        &self,
        path: &str,
    ) -> Result<Pin<Box<dyn AsyncBufRead + Send>>, RepositoryReadError>;

    /// Obtain a reader that performs content integrity checking.
    ///
    /// Because content digests can only be computed once all content is read, the reader
    /// emits data as it is streaming but only compares the cryptographic digest once all
    /// data has been read. If there is a content digest mismatch, an error will be raised
    /// once the final byte is read.
    ///
    /// Validation only occurs if the stream is read to completion. Failure to read the
    /// entire stream could result in reading of unexpected content.
    async fn get_path_with_digest_verification(
        &self,
        path: &str,
        expected_size: usize,
        expected_digest: ContentDigest,
    ) -> Result<Pin<Box<dyn AsyncRead + Send>>, RepositoryReadError> {
        Ok(Box::pin(ContentValidatingReader::new(
            self.get_path(path).await?,
            expected_size,
            expected_digest,
        )))
    }

    /// Get the content of a relative path with decompression transparently applied.
    async fn get_path_decoded(
        &self,
        path: &str,
        compression: Compression,
    ) -> Result<Pin<Box<dyn AsyncRead + Send>>, RepositoryReadError> {
        read_decompressed(self.get_path(path).await?, compression)
            .await
            .map_err(|e| RepositoryReadError::IoPath(path.to_string(), e))
    }

    /// Like [Self::get_path_decoded()] but also perform content integrity verification.
    ///
    /// The digest is matched against the original fetched content, before decompression.
    async fn get_path_decoded_with_digest_verification(
        &self,
        path: &str,
        compression: Compression,
        expected_size: usize,
        expected_digest: ContentDigest,
    ) -> Result<Pin<Box<dyn AsyncRead + Send>>, RepositoryReadError> {
        let reader = self
            .get_path_with_digest_verification(path, expected_size, expected_digest)
            .await?;

        read_decompressed(Box::pin(futures::io::BufReader::new(reader)), compression)
            .await
            .map_err(|e| RepositoryReadError::IoPath(path.to_string(), e))
    }

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
    fn preferred_compression(&self) -> Compression;

    /// Set the preferred compression format for retrieved index files.
    ///
    /// Index files are often published in multiple compression formats, including no
    /// compression. This function can be used to instruct the reader which compression
    /// format to prefer.
    fn set_preferred_compression(&mut self, compression: Compression);

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

    /// Resolve indices for `Contents` files.
    ///
    /// Only entries for the checksum as defined by [Self::retrieve_checksum()] are returned.
    ///
    /// Multiple entries for the same logical file with varying compression formats may be
    /// returned.
    fn contents_indices_entries(&self) -> Result<Vec<ContentsFileEntry>, RepositoryReadError> {
        Ok(
            if let Some(entries) = self
                .release_file()
                .iter_contents_indices(self.retrieve_checksum()?)
            {
                entries.collect::<Result<Vec<_>, _>>()?
            } else {
                vec![]
            },
        )
    }

    /// Resolve indices for `Sources` file.
    ///
    /// Only entries for the checksum as defined by [Self::retrieve_checksum()] are returned.
    ///
    /// Multiple entries for the same logical file with varying compression formats may be
    /// returned.
    fn sources_indices_entries(&self) -> Result<Vec<SourcesFileEntry>, RepositoryReadError> {
        Ok(
            if let Some(entries) = self
                .release_file()
                .iter_sources_indices(self.retrieve_checksum()?)
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
            for compression in Compression::default_preferred_order() {
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

        let mut reader = ControlParagraphAsyncReader::new(futures::io::BufReader::new(
            self.get_path_decoded_with_digest_verification(
                &path,
                entry.compression,
                entry.entry.size,
                entry.entry.digest.as_content_digest()?,
            )
            .await?,
        ));

        let mut res = BinaryPackageList::default();

        while let Some(paragraph) = reader.read_paragraph().await? {
            res.push(BinaryPackageControlFile::from(paragraph));
        }

        Ok(res)
    }

    /// Resolve a reference to a `Contents` file to fetch given search criteria.
    ///
    /// This will attempt to find the entry for a `Contents` file given search criteria.
    fn contents_entry(
        &self,
        component: &str,
        architecture: &str,
        is_installer: bool,
    ) -> Result<ContentsFileEntry, RepositoryReadError> {
        let entries = self
            .contents_indices_entries()?
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
            for compression in Compression::default_preferred_order() {
                if let Some(entry) = entries
                    .iter()
                    .find(|entry| entry.compression == compression)
                {
                    return Ok(entry.clone());
                }
            }

            Err(RepositoryReadError::ContentsIndicesEntryNotFound)
        }
    }

    async fn resolve_contents(
        &self,
        component: &str,
        architecture: &str,
        is_installer: bool,
    ) -> Result<ContentsFile, RepositoryReadError> {
        let release = self.release_file();
        let entry = self.contents_entry(component, architecture, is_installer)?;

        let path = if release.acquire_by_hash().unwrap_or_default() {
            entry.entry.by_hash_path()
        } else {
            entry.entry.path.to_string()
        };

        let reader = self
            .get_path_decoded_with_digest_verification(
                &path,
                entry.compression,
                entry.entry.size,
                entry.entry.digest.as_content_digest()?,
            )
            .await?;

        let mut reader = ContentsFileAsyncReader::new(futures::io::BufReader::new(reader));
        reader.read_all().await?;

        let (contents, reader) = reader.consume();

        drain_reader(reader)
            .await
            .map_err(|e| RepositoryReadError::IoPath(path, e))?;

        Ok(contents)
    }
}

/// Errors related to writing to repositories.
#[derive(Debug, Error)]
pub enum RepositoryWriteError {
    #[error("I/O error write path {0}: {0:?}")]
    IoPath(String, std::io::Error),
}

#[async_trait]
pub trait RepositoryWriter {
    /// Write data to a given path.
    ///
    /// The data to write is provided by an [AsyncRead] reader.
    async fn write_path(
        &self,
        path: &str,
        reader: Pin<Box<dyn AsyncRead + Send>>,
    ) -> Result<u64, RepositoryWriteError>;
}
