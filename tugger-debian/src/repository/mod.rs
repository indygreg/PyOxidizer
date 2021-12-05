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
        repository::release::{ChecksumType, PackagesFileEntry, ReleaseError, ReleaseFile},
    },
    async_compression::futures::bufread::{BzDecoder, GzipDecoder, LzmaDecoder, XzDecoder},
    async_trait::async_trait,
    futures::{AsyncBufRead, AsyncRead, AsyncReadExt},
    pin_project::pin_project,
    std::{
        pin::Pin,
        task::{Context, Poll},
    },
    thiserror::Error,
};

pub mod builder;
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

    #[error("URL error: {0:?}")]
    Url(#[from] url::ParseError),
}

async fn get_path_decoded(
    stream: Pin<Box<dyn AsyncBufRead + Send>>,
    compression: IndexFileCompression,
) -> Result<Pin<Box<dyn AsyncRead + Send>>, RepositoryReadError> {
    Ok(match compression {
        IndexFileCompression::None => Box::pin(stream),
        IndexFileCompression::Gzip => Box::pin(GzipDecoder::new(stream)),
        IndexFileCompression::Xz => Box::pin(XzDecoder::new(stream)),
        IndexFileCompression::Bzip2 => Box::pin(BzDecoder::new(stream)),
        IndexFileCompression::Lzma => Box::pin(LzmaDecoder::new(stream)),
    })
}

/// An adapter for [AsyncRead] streams that validates source size and digest.
#[pin_project]
struct DigestValidatingStreamReader<R> {
    hasher: Option<Box<dyn pgp::crypto::Hasher + Send>>,
    expected_size: usize,
    expected_digest: Vec<u8>,
    #[pin]
    source: R,
    bytes_read: usize,
}

impl<R> DigestValidatingStreamReader<R> {
    fn new(
        source: R,
        expected_size: usize,
        digest_type: ChecksumType,
        expected_digest: Vec<u8>,
    ) -> Self {
        Self {
            hasher: Some(digest_type.new_hasher()),
            expected_size,
            expected_digest,
            source,
            bytes_read: 0,
        }
    }
}

impl<R> AsyncRead for DigestValidatingStreamReader<R>
where
    R: AsyncRead + Unpin,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        let mut this = self.project();

        match this.source.as_mut().poll_read(cx, buf) {
            Poll::Ready(Ok(size)) => {
                if size > 0 {
                    if let Some(hasher) = this.hasher.as_mut() {
                        hasher.update(&buf[0..size]);
                    } else {
                        panic!("hasher destroyed prematurely");
                    }

                    *this.bytes_read += size;
                }

                match this.bytes_read.cmp(&this.expected_size) {
                    std::cmp::Ordering::Equal => {
                        if let Some(hasher) = this.hasher.take() {
                            let got_digest = hasher.finish();

                            if &got_digest != this.expected_digest {
                                return Poll::Ready(Err(std::io::Error::new(
                                    std::io::ErrorKind::Other,
                                    format!(
                                        "digest mismatch of retrieved content: expected {}, got {}",
                                        hex::encode(this.expected_digest),
                                        hex::encode(got_digest)
                                    ),
                                )));
                            }
                        }
                    }
                    std::cmp::Ordering::Greater => {
                        return Poll::Ready(Err(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            format!(
                                "extra bytes read: expected {}; got {}",
                                this.expected_size, this.bytes_read
                            ),
                        )));
                    }
                    std::cmp::Ordering::Less => {}
                }

                Poll::Ready(Ok(size))
            }
            res => res,
        }
    }
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
        checksum: ChecksumType,
        digest: Vec<u8>,
    ) -> Result<Pin<Box<dyn AsyncRead>>, RepositoryReadError> {
        Ok(Box::pin(DigestValidatingStreamReader::new(
            self.get_path(path).await?,
            expected_size,
            checksum,
            digest,
        )))
    }

    /// Get the content of a relative path with decompression transparently applied.
    async fn get_path_decoded(
        &self,
        path: &str,
        compression: IndexFileCompression,
    ) -> Result<Pin<Box<dyn AsyncRead + Send>>, RepositoryReadError> {
        get_path_decoded(self.get_path(path).await?, compression).await
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
        checksum: ChecksumType,
        digest: Vec<u8>,
    ) -> Result<Pin<Box<dyn AsyncRead + Send>>, RepositoryReadError> {
        Ok(Box::pin(DigestValidatingStreamReader::new(
            self.get_path(path).await?,
            expected_size,
            checksum,
            digest,
        )))
    }

    /// Get the content of a relative path with decompression transparently applied.
    async fn get_path_decoded(
        &self,
        path: &str,
        compression: IndexFileCompression,
    ) -> Result<Pin<Box<dyn AsyncRead + Send>>, RepositoryReadError> {
        get_path_decoded(self.get_path(path).await?, compression).await
    }

    /// Like [Self::get_path_decoded()] but also perform content integrity verification.
    ///
    /// The digest is matched against the original fetched content, before decompression.
    async fn get_path_decoded_with_digest_verification(
        &self,
        path: &str,
        compression: IndexFileCompression,
        expected_size: usize,
        checksum: ChecksumType,
        digest: Vec<u8>,
    ) -> Result<Pin<Box<dyn AsyncRead + Send>>, RepositoryReadError> {
        let reader = self
            .get_path_with_digest_verification(path, expected_size, checksum, digest)
            .await?;

        get_path_decoded(Box::pin(futures::io::BufReader::new(reader)), compression).await
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
                self.retrieve_checksum()?,
                entry.entry.digest_bytes()?,
            )
            .await?,
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
