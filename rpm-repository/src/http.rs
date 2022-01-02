// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{
        error::{Result, RpmRepositoryError},
        metadata::repomd::RepoMd,
        DataResolver, MetadataReader, RepositoryRootReader,
    },
    futures::{AsyncRead, TryStreamExt},
    reqwest::{Client, ClientBuilder, IntoUrl, StatusCode, Url},
    std::{future::Future, pin::Pin},
};

/// Default HTTP user agent string.
pub const USER_AGENT: &str = "rpm-repository Rust crate (https://crates.io/crates/rpm-repository)";

async fn fetch_url(
    client: &Client,
    root_url: &Url,
    path: &str,
) -> Result<Pin<Box<dyn AsyncRead + Send>>> {
    let request_url = root_url.join(path)?;

    let res = client.get(request_url.clone()).send().await.map_err(|e| {
        RpmRepositoryError::IoPath(
            path.to_string(),
            std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("error sending HTTP request: {:?}", e),
            ),
        )
    })?;

    let res = res.error_for_status().map_err(|e| {
        if e.status() == Some(StatusCode::NOT_FOUND) {
            RpmRepositoryError::IoPath(
                path.to_string(),
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("HTTP 404 for {}", request_url),
                ),
            )
        } else {
            RpmRepositoryError::IoPath(
                path.to_string(),
                std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("bad HTTP status code: {:?}", e),
                ),
            )
        }
    })?;

    Ok(Box::pin(
        res.bytes_stream()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("{:?}", e)))
            .into_async_read(),
    ))
}

/// Client for RPM repositories served via HTTP.
///
/// Instances are bound to a base URL, which represents the base directory.
#[derive(Debug)]
pub struct HttpRepositoryClient {
    /// HTTP client to use.
    client: Client,

    /// Base URL for this repository.
    root_url: Url,
}

impl HttpRepositoryClient {
    /// Construct an instance bound to the specified URL.
    pub fn new(url: impl IntoUrl) -> Result<Self> {
        let builder = ClientBuilder::new().user_agent(USER_AGENT);

        Self::new_client(builder.build()?, url)
    }

    pub fn new_client(client: Client, url: impl IntoUrl) -> Result<Self> {
        let mut root_url = url.into_url()?;

        // Trailing URLs are significant to the Url type when we .join(). So ensure
        // the URL has a trailing path.
        if !root_url.path().ends_with('/') {
            root_url.set_path(&format!("{}/", root_url.path()));
        }

        Ok(Self { client, root_url })
    }
}

impl DataResolver for HttpRepositoryClient {
    #[allow(clippy::type_complexity)]
    fn get_path(
        &self,
        path: String,
    ) -> Pin<Box<dyn Future<Output = Result<Pin<Box<dyn AsyncRead + Send>>>> + Send + '_>> {
        async fn run(
            slf: &HttpRepositoryClient,
            path: String,
        ) -> Result<Pin<Box<dyn AsyncRead + Send>>> {
            fetch_url(&slf.client, &slf.root_url, &path).await
        }

        Box::pin(run(self, path))
    }
}

impl RepositoryRootReader for HttpRepositoryClient {
    fn url(&self) -> Result<Url> {
        Ok(self.root_url.clone())
    }

    #[allow(clippy::type_complexity)]
    fn metadata_reader(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn MetadataReader>>> + Send + '_>> {
        async fn run(slf: &HttpRepositoryClient) -> Result<Box<dyn MetadataReader>> {
            let relative_path = "repodata".to_string();

            let root_url = slf.root_url.join(&relative_path)?;

            let repomd = slf
                .fetch_repomd(format!("{}/repomd.xml", relative_path))
                .await?;

            Ok(Box::new(HttpMetadataClient {
                client: slf.client.clone(),
                root_url,
                relative_path,
                repomd,
            }))
        }

        Box::pin(run(self))
    }
}

/// Repository HTTP client bound to a parsed `repomd.xml` file.
pub struct HttpMetadataClient {
    client: Client,
    root_url: Url,
    relative_path: String,
    repomd: RepoMd,
}

impl DataResolver for HttpMetadataClient {
    #[allow(clippy::type_complexity)]
    fn get_path(
        &self,
        path: String,
    ) -> Pin<Box<dyn Future<Output = Result<Pin<Box<dyn AsyncRead + Send>>>> + Send + '_>> {
        async fn run(
            slf: &HttpMetadataClient,
            path: String,
        ) -> Result<Pin<Box<dyn AsyncRead + Send>>> {
            fetch_url(&slf.client, &slf.root_url, &path).await
        }

        Box::pin(run(self, path))
    }
}

impl MetadataReader for HttpMetadataClient {
    fn url(&self) -> Result<Url> {
        Ok(self.root_url.clone())
    }

    fn root_relative_path(&self) -> &str {
        &self.relative_path
    }

    fn repomd(&self) -> &RepoMd {
        &self.repomd
    }
}

#[cfg(test)]
mod test {
    use super::*;

    const FEDORA_35_URL: &str =
        "https://download-ib01.fedoraproject.org/pub/fedora/linux/releases/35/Server/x86_64/os";

    #[tokio::test]
    async fn fedora_35() -> Result<()> {
        let root = HttpRepositoryClient::new(FEDORA_35_URL)?;

        let metadata = root.metadata_reader().await?;

        let primary = metadata.primary_packages().await?;

        let zlib = primary
            .packages
            .iter()
            .find(|entry| entry.name == "zlib")
            .unwrap();

        assert_eq!(zlib.package_type, "rpm");
        // This could change if a new version is released.
        assert!(zlib.version.version.starts_with("1.2"));

        Ok(())
    }
}
