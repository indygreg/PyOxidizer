// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{
        error::{DebianError, Result},
        io::{ContentDigest, MultiDigester},
        repository::{
            RepositoryPathVerification, RepositoryPathVerificationState, RepositoryWrite,
            RepositoryWriter,
        },
    },
    async_trait::async_trait,
    futures::{AsyncRead, AsyncReadExt as FuturesAsyncReadExt},
    rusoto_core::{ByteStream, Client, Region, RusotoError},
    rusoto_s3::{
        GetObjectError, GetObjectRequest, HeadObjectError, HeadObjectRequest, PutObjectRequest,
        S3Client, S3,
    },
    std::{borrow::Cow, pin::Pin},
    tokio::io::AsyncReadExt as TokioAsyncReadExt,
};

pub struct S3Writer {
    client: S3Client,
    bucket: String,
    key_prefix: Option<String>,
}

impl S3Writer {
    /// Create a new S3 writer bound to a named bucket with optional key prefix.
    ///
    /// This will construct a default AWS [Client].
    pub fn new(region: Region, bucket: impl ToString, key_prefix: Option<&str>) -> Self {
        Self {
            client: S3Client::new(region),
            bucket: bucket.to_string(),
            key_prefix: key_prefix.map(|x| x.trim_matches('/').to_string()),
        }
    }

    /// Create a new S3 writer bound to a named bucket, optional key prefix, with an AWS [Client].
    ///
    /// This is like [Self::new()] except the caller can pass in the AWS [Client] to use.
    pub fn new_with_client(
        client: Client,
        region: Region,
        bucket: impl ToString,
        key_prefix: Option<&str>,
    ) -> Self {
        Self {
            client: S3Client::new_with_client(client, region),
            bucket: bucket.to_string(),
            key_prefix: key_prefix.map(|x| x.trim_matches('/').to_string()),
        }
    }

    /// Compute the S3 key name given a repository relative path.
    pub fn path_to_key(&self, path: &str) -> String {
        if let Some(prefix) = &self.key_prefix {
            format!("{}/{}", prefix, path.trim_matches('/'))
        } else {
            path.trim_matches('/').to_string()
        }
    }
}

#[async_trait]
impl RepositoryWriter for S3Writer {
    async fn verify_path<'path>(
        &self,
        path: &'path str,
        expected_content: Option<(u64, ContentDigest)>,
    ) -> Result<RepositoryPathVerification<'path>> {
        if let Some((expected_size, expected_digest)) = expected_content {
            let req = GetObjectRequest {
                bucket: self.bucket.clone(),
                key: self.path_to_key(path),
                ..Default::default()
            };

            match self.client.get_object(req).await {
                Ok(output) => {
                    // Fast path without having to ready the body.
                    if let Some(cl) = output.content_length {
                        if cl as u64 != expected_size {
                            return Ok(RepositoryPathVerification {
                                path,
                                state: RepositoryPathVerificationState::ExistsIntegrityMismatch,
                            });
                        }
                    }

                    if let Some(body) = output.body {
                        let mut digester = MultiDigester::default();

                        let mut remaining = expected_size;
                        let mut reader = body.into_async_read();
                        let mut buf = [0u8; 16384];

                        loop {
                            let size = reader
                                .read(&mut buf[..])
                                .await
                                .map_err(|e| DebianError::RepositoryIoPath(path.to_string(), e))?;

                            digester.update(&buf[0..size]);

                            let size = size as u64;

                            if size >= remaining || size == 0 {
                                break;
                            }

                            remaining -= size;
                        }

                        let digests = digester.finish();

                        Ok(RepositoryPathVerification {
                            path,
                            state: if !digests.matches_digest(&expected_digest) {
                                RepositoryPathVerificationState::ExistsIntegrityMismatch
                            } else {
                                RepositoryPathVerificationState::ExistsIntegrityVerified
                            },
                        })
                    } else {
                        Ok(RepositoryPathVerification {
                            path,
                            state: RepositoryPathVerificationState::Missing,
                        })
                    }
                }
                Err(RusotoError::Service(GetObjectError::NoSuchKey(_))) => {
                    Ok(RepositoryPathVerification {
                        path,
                        state: RepositoryPathVerificationState::Missing,
                    })
                }
                Err(e) => Err(DebianError::RepositoryIoPath(
                    path.to_string(),
                    std::io::Error::new(std::io::ErrorKind::Other, format!("S3 error: {:?}", e)),
                )),
            }
        } else {
            let req = HeadObjectRequest {
                bucket: self.bucket.clone(),
                key: self.path_to_key(path),
                ..Default::default()
            };

            match self.client.head_object(req).await {
                Ok(_) => Ok(RepositoryPathVerification {
                    path,
                    state: RepositoryPathVerificationState::ExistsNoIntegrityCheck,
                }),
                Err(RusotoError::Service(HeadObjectError::NoSuchKey(_))) => {
                    Ok(RepositoryPathVerification {
                        path,
                        state: RepositoryPathVerificationState::Missing,
                    })
                }
                Err(e) => Err(DebianError::RepositoryIoPath(
                    path.to_string(),
                    std::io::Error::new(std::io::ErrorKind::Other, format!("S3 error: {:?}", e)),
                )),
            }
        }
    }

    async fn write_path<'path, 'reader>(
        &self,
        path: Cow<'path, str>,
        mut reader: Pin<Box<dyn AsyncRead + Send + 'reader>>,
    ) -> Result<RepositoryWrite<'path>> {
        // rusoto wants a Stream<Bytes>. There's no easy way to convert from an AsyncRead to a
        // Stream. So we just buffer content locally.
        // TODO implement this properly
        let mut buf = vec![];
        reader
            .read_to_end(&mut buf)
            .await
            .map_err(|e| DebianError::RepositoryIoPath(path.to_string(), e))?;

        let bytes_written = buf.len() as u64;
        let stream = futures::stream::once(async { Ok(bytes::Bytes::from(buf)) });

        let req = PutObjectRequest {
            bucket: self.bucket.clone(),
            key: self.path_to_key(path.as_ref()),
            body: Some(ByteStream::new(stream)),
            ..Default::default()
        };

        match self.client.put_object(req).await {
            Ok(_) => Ok(RepositoryWrite {
                path,
                bytes_written,
            }),
            Err(e) => Err(DebianError::RepositoryIoPath(
                path.to_string(),
                std::io::Error::new(std::io::ErrorKind::Other, format!("S3 error: {:?}", e)),
            )),
        }
    }
}
