// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{
        binary_package_control::BinaryPackageControlFile,
        control::{ControlError, ControlParagraphAsyncReader},
        repository::{
            release::{ChecksumType, PackagesFileEntry, ReleaseError, ReleaseFile},
            IndexFileCompression,
        },
    },
    async_compression::futures::bufread::{BzDecoder, GzipDecoder, LzmaDecoder, XzDecoder},
    futures::{stream::TryStreamExt, AsyncRead},
    reqwest::{Client, IntoUrl, Response, Url},
    std::{io::Cursor, pin::Pin},
    thiserror::Error,
};

#[derive(Debug, Error)]
pub enum HttpError {
    #[error("I/O error: {0:?}")]
    Io(#[from] std::io::Error),

    #[error("HTTP error: {0:?}")]
    Reqwest(#[from] reqwest::Error),

    #[error("URL error: {0:?}")]
    Url(#[from] url::ParseError),

    #[error("Control file error: {0:?}")]
    Control(#[from] ControlError),

    #[error("Release file error: {0:?}")]
    Release(#[from] ReleaseError),

    #[error("Release file does not contain supported checksum flavor")]
    NoKnownChecksum,

    #[error("No packages indices for checksum {0}")]
    NoPackagesIndices(&'static str),

    #[error("Could not find packages indices entry")]
    PackagesIndicesEntryNotFound,
}

/// Client for a Debian repository served via HTTP.
///
/// Instances are bound to a base URL, which represents the base directory.
/// That URL should have an `InRelease` or `Release` file under it. From
/// that main entrypoint, all other repository state can be discovered and
/// retrieved.
#[derive(Debug)]
pub struct HttpRepositoryClient {
    /// HTTP client to use.
    client: Client,

    base_url: Url,
}

impl HttpRepositoryClient {
    /// Construct an instance bound to the specified URL.
    pub fn new(url: impl IntoUrl) -> Result<Self, HttpError> {
        Self::new_client(Client::default(), url)
    }

    /// Construct an instance using the given [Client] and URL.
    ///
    /// The URL should have an `InRelease` or `Release` file under it.
    pub fn new_client(client: Client, url: impl IntoUrl) -> Result<Self, HttpError> {
        let base_url = url.into_url()?;

        Ok(Self { client, base_url })
    }

    /// Perform an HTTP GET to the repository.
    pub async fn get_path(&self, path: &str) -> Result<Response, HttpError> {
        let url = self.base_url.join(path)?;

        let res = self.client.get(url).send().await?;

        Ok(res.error_for_status()?)
    }

    pub async fn get_path_stream_decompressed(
        &self,
        path: &str,
        compression: IndexFileCompression,
    ) -> Result<Pin<Box<dyn AsyncRead>>, HttpError> {
        let res = self.get_path(path).await?;

        let stream = res
            .bytes_stream()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("{:?}", e)));

        Ok(match compression {
            IndexFileCompression::None => Box::pin(stream.into_async_read()),
            IndexFileCompression::Gzip => Box::pin(GzipDecoder::new(stream.into_async_read())),
            IndexFileCompression::Xz => Box::pin(XzDecoder::new(stream.into_async_read())),
            IndexFileCompression::Bzip2 => Box::pin(BzDecoder::new(stream.into_async_read())),
            IndexFileCompression::Lzma => Box::pin(LzmaDecoder::new(stream.into_async_read())),
        })
    }

    /// Fetch and parse the `InRelease` file from the repository.
    ///
    /// Returns a new object bound to the parsed `InRelease` file.
    pub async fn fetch_inrelease(&self) -> Result<HttpReleaseClient<'_>, HttpError> {
        let res = self.get_path("InRelease").await?;

        let data = res.bytes().await?;

        let release = ReleaseFile::from_armored_reader(Cursor::new(data))?;

        // Determine which checksum flavor to fetch from the strongest present.
        let fetch_checksum = &[ChecksumType::Sha256, ChecksumType::Sha1, ChecksumType::Md5]
            .iter()
            .find(|variant| release.first_field(variant.field_name()).is_some())
            .ok_or(HttpError::NoKnownChecksum)?;

        let fetch_compression = IndexFileCompression::Xz;

        Ok(HttpReleaseClient {
            base_client: self,
            release,
            fetch_checksum: **fetch_checksum,
            fetch_compression,
        })
    }
}

/// Repository HTTP client bound to a parsed `Release` or `InRelease` file.
pub struct HttpReleaseClient<'client> {
    base_client: &'client HttpRepositoryClient,
    release: ReleaseFile<'static>,
    /// Which checksum flavor to fetch and verify.
    fetch_checksum: ChecksumType,
    fetch_compression: IndexFileCompression,
}

impl<'client> AsRef<ReleaseFile<'static>> for HttpReleaseClient<'client> {
    fn as_ref(&self) -> &ReleaseFile<'static> {
        &self.release
    }
}

impl<'client> HttpReleaseClient<'client> {
    /// Fetch a `Packages` file and convert it to a stream of [BinaryPackageControlFile] instances.
    pub async fn fetch_packages(
        &self,
        component: &str,
        arch: &str,
        is_installer: bool,
    ) -> Result<Vec<BinaryPackageControlFile<'static>>, HttpError> {
        let entry = self
            .release
            .find_packages_indices(
                self.fetch_checksum,
                self.fetch_compression,
                component,
                arch,
                is_installer,
            )
            .ok_or(HttpError::PackagesIndicesEntryNotFound)?;
        let path = entry.entry.path;

        // TODO perform digest verification.
        // TODO make this stream output.

        let mut reader = ControlParagraphAsyncReader::new(futures::io::BufReader::new(
            self.base_client
                .get_path_stream_decompressed(path, entry.compression)
                .await?,
        ));

        let mut res = vec![];

        while let Some(paragraph) = reader.read_paragraph().await? {
            res.push(BinaryPackageControlFile::from(paragraph));
        }

        Ok(res)
    }

    /// Obtain all file entries for `Packages*` files matching our fetch criteria.
    pub fn packages_indices_entries(&self) -> Result<Vec<PackagesFileEntry>, HttpError> {
        Ok(
            if let Some(entries) = self.release.iter_packages_indices(self.fetch_checksum) {
                entries
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter()
                    .filter(|entry| entry.compression == self.fetch_compression)
                    .collect::<Vec<_>>()
            } else {
                vec![]
            },
        )
    }
}

#[cfg(test)]
mod test {
    use {super::*, crate::error::Result};

    const BULLSEYE_URL: &str =
        "http://snapshot.debian.org/archive/debian/20211120T085721Z/dists/bullseye/";

    #[tokio::test]
    async fn bullseye_release() -> Result<()> {
        let repo = HttpRepositoryClient::new(BULLSEYE_URL)?;

        let release = repo.fetch_inrelease().await?;

        let packages = release.fetch_packages("main", "amd64", false).await?;
        assert_eq!(packages.len(), 58606);

        let p = &packages[0];
        assert_eq!(p.package()?, "0ad");
        assert_eq!(
            p.first_field_str("SHA256"),
            Some("610e9f9c41be18af516dd64a6dc1316dbfe1bb8989c52bafa556de9e381d3e29")
        );

        let p = &packages[packages.len() - 1];
        assert_eq!(p.package()?, "python3-zzzeeksphinx");
        assert_eq!(
            p.first_field_str("SHA256"),
            Some("6e35f5805e808c19becd3b9ce25c4cf40c41aa0cf5d81fab317198ded917fec1")
        );

        // Make sure dependency syntax parsing works.
        for p in &packages {
            if let Some(deps) = p.depends() {
                deps?;
            }
            if let Some(deps) = p.recommends() {
                deps?;
            }
            if let Some(deps) = p.suggests() {
                deps?;
            }
            if let Some(deps) = p.enhances() {
                deps?;
            }
            if let Some(deps) = p.pre_depends() {
                deps?;
            }
        }

        Ok(())
    }
}
