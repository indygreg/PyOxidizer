// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{
        io::{ContentDigest, DigestingReader, DigestingWriter},
        repository::{
            RepositoryPathVerification, RepositoryPathVerificationState, RepositoryWrite,
            RepositoryWriteError, RepositoryWriter,
        },
    },
    async_trait::async_trait,
    futures::{AsyncRead, AsyncReadExt},
    std::{
        borrow::Cow,
        path::{Path, PathBuf},
        pin::Pin,
    },
};

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
        expected_content: Option<(usize, ContentDigest)>,
    ) -> Result<RepositoryPathVerification<'path>, RepositoryWriteError> {
        let dest_path = self.root_dir.join(path);

        let metadata = async_std::fs::metadata(&dest_path)
            .await
            .map_err(|e| RepositoryWriteError::io_path(path, e))?;

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
                        .map_err(|e| RepositoryWriteError::io_path(path, e))?;

                    let mut remaining = expected_size;
                    let mut reader = DigestingReader::new(f);
                    let mut buf = [0u8; 16384];

                    loop {
                        let size = reader
                            .read(&mut buf[..])
                            .await
                            .map_err(|e| RepositoryWriteError::io_path(path, e))?;

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
    ) -> Result<RepositoryWrite<'path>, RepositoryWriteError> {
        let dest_path = self.root_dir.join(path.as_ref());

        if let Some(parent) = dest_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| RepositoryWriteError::IoPath(format!("{}", parent.display()), e))?;
        }

        let fh = std::fs::File::create(&dest_path)
            .map_err(|e| RepositoryWriteError::IoPath(format!("{}", dest_path.display()), e))?;

        let mut writer = DigestingWriter::new(futures::io::AllowStdIo::new(fh));

        let bytes_written = futures::io::copy(reader, &mut writer)
            .await
            .map_err(|e| RepositoryWriteError::IoPath(format!("{}", dest_path.display()), e))?;

        let digests = writer.finish().1;

        Ok(RepositoryWrite {
            path,
            bytes_written,
            digests,
        })
    }
}
