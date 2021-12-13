// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Filesystem based Debian repositories. */

use {
    crate::{
        error::{DebianError, Result},
        io::{Compression, ContentDigest, DataResolver, DigestingReader},
        repository::{
            release::ReleaseFile, ReleaseReader, RepositoryPathVerification,
            RepositoryPathVerificationState, RepositoryRootReader, RepositoryWrite,
            RepositoryWriter,
        },
    },
    async_trait::async_trait,
    futures::{io::BufReader, AsyncBufRead, AsyncRead, AsyncReadExt},
    std::{
        borrow::Cow,
        path::{Path, PathBuf},
        pin::Pin,
    },
    url::Url,
};

/// A readable interface to a Debian repository backed by a filesystem.
#[derive(Clone, Debug)]
pub struct FilesystemRepositoryReader {
    root_dir: PathBuf,
}

impl FilesystemRepositoryReader {
    /// Construct a new instance, bound to the root directory specified.
    ///
    /// No validation of the passed path is performed.
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            root_dir: path.as_ref().to_path_buf(),
        }
    }
}

#[async_trait]
impl DataResolver for FilesystemRepositoryReader {
    async fn get_path(&self, path: &str) -> Result<Pin<Box<dyn AsyncBufRead + Send>>> {
        let path = self.root_dir.join(path);

        let f = std::fs::File::open(&path)
            .map_err(|e| DebianError::RepositoryIoPath(format!("{}", path.display()), e))?;

        Ok(Box::pin(BufReader::new(futures::io::AllowStdIo::new(f))))
    }
}

#[async_trait]
impl RepositoryRootReader for FilesystemRepositoryReader {
    fn url(&self) -> Result<Url> {
        Url::from_file_path(&self.root_dir)
            .map_err(|_| DebianError::Other("error converting filesystem path to URL".to_string()))
    }

    async fn release_reader_with_distribution_path(
        &self,
        path: &str,
    ) -> Result<Box<dyn ReleaseReader>> {
        let distribution_path = path.trim_matches('/').to_string();
        let release_path = format!("{}/InRelease", distribution_path);
        let distribution_dir = self.root_dir.join(&distribution_path);

        let release = self.fetch_inrelease(&release_path).await?;

        let fetch_compression = Compression::default_preferred_order()
            .next()
            .expect("iterator should not be empty");

        Ok(Box::new(FilesystemReleaseClient {
            distribution_dir,
            release,
            fetch_compression,
        }))
    }
}

pub struct FilesystemReleaseClient {
    distribution_dir: PathBuf,
    release: ReleaseFile<'static>,
    fetch_compression: Compression,
}

#[async_trait]
impl DataResolver for FilesystemReleaseClient {
    async fn get_path(&self, path: &str) -> Result<Pin<Box<dyn AsyncBufRead + Send>>> {
        let path = self.distribution_dir.join(path);

        let f = std::fs::File::open(&path)
            .map_err(|e| DebianError::RepositoryIoPath(format!("{}", path.display()), e))?;

        Ok(Box::pin(BufReader::new(futures::io::AllowStdIo::new(f))))
    }
}

#[async_trait]
impl ReleaseReader for FilesystemReleaseClient {
    fn url(&self) -> Result<Url> {
        Url::from_file_path(&self.distribution_dir)
            .map_err(|_| DebianError::Other("error converting filesystem path to URL".to_string()))
    }

    fn release_file(&self) -> &ReleaseFile<'static> {
        &self.release
    }

    fn preferred_compression(&self) -> Compression {
        self.fetch_compression
    }

    fn set_preferred_compression(&mut self, compression: Compression) {
        self.fetch_compression = compression;
    }
}

/// A writable Debian repository backed by a filesystem.
pub struct FilesystemRepositoryWriter {
    root_dir: PathBuf,
}

impl FilesystemRepositoryWriter {
    /// Construct a new instance, bound to the root directory specified.
    ///
    /// No validation of the passed path is performed. The directory does not need to exist.
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            root_dir: path.as_ref().to_path_buf(),
        }
    }
}

#[async_trait]
impl RepositoryWriter for FilesystemRepositoryWriter {
    async fn verify_path<'path>(
        &self,
        path: &'path str,
        expected_content: Option<(u64, ContentDigest)>,
    ) -> Result<RepositoryPathVerification<'path>> {
        let dest_path = self.root_dir.join(path);

        let metadata = async_std::fs::metadata(&dest_path)
            .await
            .map_err(|e| DebianError::RepositoryIoPath(path.to_string(), e))?;

        if metadata.is_file() {
            if let Some((expected_size, expected_digest)) = expected_content {
                if metadata.len() != expected_size as u64 {
                    Ok(RepositoryPathVerification {
                        path,
                        state: RepositoryPathVerificationState::ExistsIntegrityMismatch,
                    })
                } else {
                    let f = async_std::fs::File::open(&dest_path)
                        .await
                        .map_err(|e| DebianError::RepositoryIoPath(path.to_string(), e))?;

                    let mut remaining = expected_size;
                    let mut reader = DigestingReader::new(f);
                    let mut buf = [0u8; 16384];

                    loop {
                        let size = reader
                            .read(&mut buf[..])
                            .await
                            .map_err(|e| DebianError::RepositoryIoPath(path.to_string(), e))?
                            as u64;

                        if size >= remaining || size == 0 {
                            break;
                        }

                        remaining -= size;
                    }

                    let digest = reader.finish().1;

                    Ok(RepositoryPathVerification {
                        path,
                        state: if digest.matches_digest(&expected_digest) {
                            RepositoryPathVerificationState::ExistsIntegrityVerified
                        } else {
                            RepositoryPathVerificationState::ExistsIntegrityMismatch
                        },
                    })
                }
            } else {
                Ok(RepositoryPathVerification {
                    path,
                    state: RepositoryPathVerificationState::ExistsNoIntegrityCheck,
                })
            }
        } else {
            Ok(RepositoryPathVerification {
                path,
                state: RepositoryPathVerificationState::Missing,
            })
        }
    }

    async fn write_path<'path, 'reader>(
        &self,
        path: Cow<'path, str>,
        reader: Pin<Box<dyn AsyncRead + Send + 'reader>>,
    ) -> Result<RepositoryWrite<'path>> {
        let dest_path = self.root_dir.join(path.as_ref());

        if let Some(parent) = dest_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| DebianError::RepositoryIoPath(format!("{}", parent.display()), e))?;
        }

        let fh = std::fs::File::create(&dest_path)
            .map_err(|e| DebianError::RepositoryIoPath(format!("{}", dest_path.display()), e))?;

        let mut writer = futures::io::AllowStdIo::new(fh);

        let bytes_written = futures::io::copy(reader, &mut writer)
            .await
            .map_err(|e| DebianError::RepositoryIoPath(format!("{}", dest_path.display()), e))?;

        Ok(RepositoryWrite {
            path,
            bytes_written,
        })
    }
}
