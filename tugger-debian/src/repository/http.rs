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
    crate::repository::{
        release::{ReleaseError, ReleaseFile},
        IndexFileCompression, ReleaseReader, RepositoryReadError, RepositoryRootReader,
    },
    async_trait::async_trait,
    futures::{stream::TryStreamExt, AsyncBufRead},
    reqwest::{Client, IntoUrl, Url},
    std::pin::Pin,
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

    #[error("No packages indices for checksum {0}")]
    NoPackagesIndices(&'static str),

    #[error("Release file error: {0:?}")]
    Release(#[from] ReleaseError),
}

async fn fetch_url(
    client: &Client,
    root_url: &Url,
    path: &str,
) -> Result<Pin<Box<dyn AsyncBufRead + Send>>, RepositoryReadError> {
    let res = client.get(root_url.join(path)?).send().await.map_err(|e| {
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
}

#[async_trait]
impl RepositoryRootReader for HttpRepositoryClient {
    fn url(&self) -> &Url {
        &self.root_url
    }

    async fn get_path(
        &self,
        path: &str,
    ) -> Result<Pin<Box<dyn AsyncBufRead + Send>>, RepositoryReadError> {
        fetch_url(&self.client, &self.root_url, path).await
    }

    async fn release_reader_with_distribution_path(
        &self,
        path: &str,
    ) -> Result<Box<dyn ReleaseReader>, RepositoryReadError> {
        let distribution_path = path.trim_matches('/').to_string();
        let release_path = join_path(&distribution_path, "InRelease");
        let mut root_url = self.root_url.join(&distribution_path)?;

        // Trailing URLs are significant to the Url type when we .join(). So ensure
        // the URL has a trailing path.
        if !root_url.path().ends_with('/') {
            root_url.set_path(&format!("{}/", root_url.path()));
        }

        let release = self.fetch_inrelease(&release_path).await?;

        let fetch_compression = IndexFileCompression::default_preferred_order()
            .next()
            .expect("iterator should not be empty");

        Ok(Box::new(HttpReleaseClient {
            client: self.client.clone(),
            root_url,
            release,
            fetch_compression,
        }))
    }
}

fn join_path(a: &str, b: &str) -> String {
    format!("{}/{}", a.trim_matches('/'), b.trim_start_matches('/'))
}

/// Repository HTTP client bound to a parsed `Release` or `InRelease` file.
pub struct HttpReleaseClient {
    client: Client,
    root_url: Url,
    release: ReleaseFile<'static>,
    fetch_compression: IndexFileCompression,
}

#[async_trait]
impl ReleaseReader for HttpReleaseClient {
    fn url(&self) -> &Url {
        &self.root_url
    }

    async fn get_path(
        &self,
        path: &str,
    ) -> Result<Pin<Box<dyn AsyncBufRead + Send>>, RepositoryReadError> {
        fetch_url(&self.client, &self.root_url, path).await
    }

    fn release_file(&self) -> &ReleaseFile<'static> {
        &self.release
    }

    fn preferred_compression(&self) -> IndexFileCompression {
        self.fetch_compression
    }

    fn set_preferred_compression(&mut self, compression: IndexFileCompression) {
        self.fetch_compression = compression;
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

        let release = root.release_reader("bullseye").await?;

        let contents = release.contents_indices_entries()?;
        assert_eq!(contents.len(), 126);

        let packages = release.resolve_packages("main", "amd64", false).await?;
        assert_eq!(packages.len(), 58606);

        let sources = release.sources_indices_entries()?;
        assert_eq!(sources.len(), 9);

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
