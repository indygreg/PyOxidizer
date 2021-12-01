// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Debian repository HTTP client.

This module provides functionality for interfacing with HTTP based Debian
repositories.

See <https://wiki.debian.org/DebianRepository/Format> for a definition of a
Debian repository layout. Essentially, there's a root URL. Under that root URL
are `dists/<distribution>/` directories. Each of these directories (which can
have multiple path separators) has an `InRelease` and/or `Release` file. These
files define the contents of a given *distribution*. This includes which
architectures are supported, what *components* are available, etc.

Our [HttpRepositoryClient] models a client bound to a root URL.

Our [HttpDistributionClient] models a client bound to a virtual sub-directory
under the root URL. You can obtain instances by calling [HttpRepositoryClient.distribution_client()].

The `InRelease`/`Release` files define the contents of a given *distribution*. Our
[HttpReleaseClient] models a client bound to a parsed file. You can obtain instances
by calling [HttpDistributionClient.fetch_inrelease()].
*/

use {
    crate::{
        binary_package_control::BinaryPackageControlFile,
        binary_package_list::BinaryPackageList,
        control::{ControlError, ControlParagraphAsyncReader},
        repository::{
            release::{ChecksumType, PackagesFileEntry, ReleaseError, ReleaseFile},
            IndexFileCompression, RepositoryReadError, RepositoryReader,
        },
    },
    async_trait::async_trait,
    futures::{stream::TryStreamExt, AsyncBufRead, AsyncReadExt},
    reqwest::{Client, IntoUrl, Url},
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

    #[error("Repository reading error: {0:?}")]
    RepositoryRead(#[from] RepositoryReadError),

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
///
/// Distributions (typically) exist in a `dists/<distribution>` directory.
/// Distributions have an `InRelease` and/or `Release` file under it.
#[derive(Debug)]
pub struct HttpRepositoryClient {
    /// HTTP client to use.
    client: Client,

    /// Base URL for this Debian archive.
    ///
    /// Contains both distributions and the files pool.
    root_url: Url,
}

impl HttpRepositoryClient {
    /// Construct an instance bound to the specified URL.
    pub fn new(url: impl IntoUrl) -> Result<Self, HttpError> {
        Self::new_client(Client::default(), url)
    }

    /// Construct an instance using the given [Client] and URL.
    ///
    /// The given URL should be the value that follows the
    /// `deb` line in apt sources files. e.g. for
    /// `deb https://deb.debian.org/debian stable main`, the value would be
    /// `https://deb.debian.org/debian`. The URL typically has a `dists/` directory
    /// underneath.
    pub fn new_client(client: Client, url: impl IntoUrl) -> Result<Self, HttpError> {
        let mut root_url = url.into_url()?;

        // Trailing URLs are significant to the Url type when we .join(). So ensure
        // the URL has a trailing path.
        if !root_url.path().ends_with('/') {
            root_url.set_path(&format!("{}/", root_url.path()));
        }

        Ok(Self { client, root_url })
    }

    /// Base URL for this fetcher.
    pub fn root_url(&self) -> &Url {
        &self.root_url
    }

    /// Obtain a [HttpDistributionClient] for a given distribution name/path.
    ///
    /// The returned client has its root URL set to `self.root_url().join("dists/{distribution}")`.
    pub fn distribution_client(&self, distribution: &str) -> HttpDistributionClient<'_> {
        HttpDistributionClient {
            root_client: self,
            distribution_path: format!("dists/{}", distribution.trim_matches('/')),
        }
    }

    /// Obtain a [HttpDistributionClient] for a given sub-directory.
    ///
    /// The root URL of the returned client is `self.root_url().join(path)`, without
    /// `dists/` prepended. This allows specifying non-standard paths to the distribution.
    pub fn distribution_client_raw_path(&self, path: &str) -> HttpDistributionClient<'_> {
        HttpDistributionClient {
            root_client: self,
            distribution_path: path.trim_matches('/').to_string(),
        }
    }
}

#[async_trait]
impl RepositoryReader for HttpRepositoryClient {
    async fn get_path(
        &self,
        path: &str,
    ) -> Result<Pin<Box<dyn AsyncBufRead>>, RepositoryReadError> {
        let res = self
            .client
            .get(self.root_url.join(path).map_err(|e| {
                RepositoryReadError::IoPath(
                    path.to_string(),
                    std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("error joining URL: {:?}", e),
                    ),
                )
            })?)
            .send()
            .await
            .map_err(|e| {
                RepositoryReadError::IoPath(
                    path.to_string(),
                    std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("error sending HTTP request: {:?}", e),
                    ),
                )
            })?;
        let res = res.error_for_status().map_err(|e| {
            RepositoryReadError::IoPath(
                path.to_string(),
                std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("bad HTTP status code: {:?}", e),
                ),
            )
        })?;

        Ok(Box::pin(
            res.bytes_stream()
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("{:?}", e)))
                .into_async_read(),
        ))
    }
}

fn join_path(a: &str, b: &str) -> String {
    format!("{}/{}", a.trim_matches('/'), b.trim_start_matches('/'))
}

/// An HTTP client bound to a specific distribution.
///
/// Debian repositories have the form `<root>/dists/<distribution>/` where the
/// *distribution* directory contains an `InRelease` and/or `Release` file.
///
/// This type models a client interface to a specific distribution path under a root
/// directory.
pub struct HttpDistributionClient<'client> {
    root_client: &'client HttpRepositoryClient,
    distribution_path: String,
}

#[async_trait]
impl<'client> RepositoryReader for HttpDistributionClient<'client> {
    async fn get_path(
        &self,
        path: &str,
    ) -> Result<Pin<Box<dyn AsyncBufRead>>, RepositoryReadError> {
        Ok(self
            .root_client
            .get_path(&join_path(&self.distribution_path, path))
            .await?)
    }
}

impl<'client> HttpDistributionClient<'client> {
    /// Fetch and parse the `InRelease` file from the repository.
    ///
    /// Returns a new object bound to the parsed `InRelease` file.
    pub async fn fetch_inrelease(&self) -> Result<HttpReleaseClient<'client>, HttpError> {
        let mut reader = self.get_path("InRelease").await?;

        let mut data = vec![];
        reader.read_to_end(&mut data).await?;

        let release = ReleaseFile::from_armored_reader(Cursor::new(data))?;

        // Determine which checksum flavor to fetch from the strongest present.
        let fetch_checksum = &[ChecksumType::Sha256, ChecksumType::Sha1, ChecksumType::Md5]
            .iter()
            .find(|variant| release.first_field(variant.field_name()).is_some())
            .ok_or(HttpError::NoKnownChecksum)?;

        let fetch_compression = IndexFileCompression::Xz;

        Ok(HttpReleaseClient {
            root_client: self.root_client,
            distribution_path: self.distribution_path.clone(),
            release,
            fetch_checksum: **fetch_checksum,
            fetch_compression,
        })
    }
}

/// Repository HTTP client bound to a parsed `Release` or `InRelease` file.
pub struct HttpReleaseClient<'client> {
    root_client: &'client HttpRepositoryClient,
    distribution_path: String,
    release: ReleaseFile<'static>,
    /// Which checksum flavor to fetch and verify.
    fetch_checksum: ChecksumType,
    fetch_compression: IndexFileCompression,
}

#[async_trait]
impl<'client> RepositoryReader for HttpReleaseClient<'client> {
    async fn get_path(
        &self,
        path: &str,
    ) -> Result<Pin<Box<dyn AsyncBufRead>>, RepositoryReadError> {
        Ok(self
            .root_client
            .get_path(&join_path(&self.distribution_path, path))
            .await?)
    }
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
    ) -> Result<BinaryPackageList<'static>, HttpError> {
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

        let path = if self.release.acquire_by_hash().unwrap_or_default() {
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
    use {
        super::*,
        crate::{
            dependency::BinaryDependency, dependency_resolution::DependencyResolver, error::Result,
        },
    };

    const BULLSEYE_URL: &str = "http://snapshot.debian.org/archive/debian/20211120T085721Z";

    #[tokio::test]
    async fn bullseye_release() -> Result<()> {
        let root = HttpRepositoryClient::new(BULLSEYE_URL)?;

        let dist = root.distribution_client("bullseye");

        let release = dist.fetch_inrelease().await?;

        let packages = release.fetch_packages("main", "amd64", false).await?;
        assert_eq!(packages.len(), 58606);

        let p = packages.iter().next().unwrap();
        assert_eq!(p.package()?, "0ad");
        assert_eq!(
            p.first_field_str("SHA256"),
            Some("610e9f9c41be18af516dd64a6dc1316dbfe1bb8989c52bafa556de9e381d3e29")
        );

        let p = packages.iter().last().unwrap();
        assert_eq!(p.package()?, "python3-zzzeeksphinx");
        assert_eq!(
            p.first_field_str("SHA256"),
            Some("6e35f5805e808c19becd3b9ce25c4cf40c41aa0cf5d81fab317198ded917fec1")
        );

        // Make sure dependency syntax parsing works.
        let mut resolver = DependencyResolver::default();
        resolver.load_binary_packages(packages.iter()).unwrap();

        for p in packages.iter() {
            resolver
                .find_direct_binary_package_dependencies(p, BinaryDependency::Depends)
                .unwrap();
        }

        let deps = resolver
            .find_transitive_binary_package_dependencies(
                p,
                [
                    BinaryDependency::Depends,
                    BinaryDependency::PreDepends,
                    BinaryDependency::Recommends,
                ]
                .into_iter(),
            )
            .unwrap();

        let sources = deps.packages_with_sources().collect::<Vec<_>>();
        assert_eq!(sources.len(), 128);

        Ok(())
    }
}
