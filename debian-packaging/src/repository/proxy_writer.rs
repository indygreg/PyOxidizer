// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! A repository writer that doesn't actually write. */

use {
    crate::{
        error::{DebianError, Result},
        io::ContentDigest,
        repository::{
            RepositoryPathVerification, RepositoryPathVerificationState, RepositoryWrite,
            RepositoryWriter,
        },
    },
    async_trait::async_trait,
    futures::AsyncRead,
    std::{borrow::Cow, pin::Pin, sync::Mutex},
};

/// How [RepositoryWriter::verify_path()] should behave for [ProxyWriter] instances.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProxyVerifyBehavior {
    /// Always call the inner [RepositoryWriter::verify_path()].
    Proxy,
    AlwaysExistsNoIntegrityCheck,
    AlwaysExistsIntegrityVerified,
    AlwaysExistsIntegrityMismatch,
    AlwaysMissing,
}

/// A [RepositoryWriter] that proxies operations to an inner writer.
///
/// The behavior of each I/O operation can be configured to facilitate customizing
/// behavior. It also records operations performed. This makes this type useful as part
/// of testing and simulating what would  occur.
pub struct ProxyWriter<W> {
    inner: W,
    verify_behavior: ProxyVerifyBehavior,
    /// List of paths that were written.
    path_writes: Mutex<Vec<String>>,
}

impl<W: RepositoryWriter + Send> ProxyWriter<W> {
    /// Construct a new instance by wrapping an existing writer.
    ///
    /// The default behavior for path verification is to call the inner writer.
    pub fn new(writer: W) -> Self {
        Self {
            inner: writer,
            verify_behavior: ProxyVerifyBehavior::Proxy,
            path_writes: Mutex::new(vec![]),
        }
    }

    /// Return the inner writer, consuming self.
    pub fn into_inner(self) -> W {
        self.inner
    }

    /// Set the behavior for [RepositoryWriter::verify_path()].
    pub fn set_verify_behavior(&mut self, behavior: ProxyVerifyBehavior) {
        self.verify_behavior = behavior;
    }
}

#[async_trait]
impl<W: RepositoryWriter + Send> RepositoryWriter for ProxyWriter<W> {
    async fn verify_path<'path>(
        &self,
        path: &'path str,
        expected_content: Option<(u64, ContentDigest)>,
    ) -> Result<RepositoryPathVerification<'path>> {
        match self.verify_behavior {
            ProxyVerifyBehavior::Proxy => self.inner.verify_path(path, expected_content).await,
            ProxyVerifyBehavior::AlwaysExistsIntegrityVerified => Ok(RepositoryPathVerification {
                path,
                state: RepositoryPathVerificationState::ExistsIntegrityVerified,
            }),
            ProxyVerifyBehavior::AlwaysExistsNoIntegrityCheck => Ok(RepositoryPathVerification {
                path,
                state: RepositoryPathVerificationState::ExistsNoIntegrityCheck,
            }),
            ProxyVerifyBehavior::AlwaysExistsIntegrityMismatch => Ok(RepositoryPathVerification {
                path,
                state: RepositoryPathVerificationState::ExistsIntegrityMismatch,
            }),
            ProxyVerifyBehavior::AlwaysMissing => Ok(RepositoryPathVerification {
                path,
                state: RepositoryPathVerificationState::Missing,
            }),
        }
    }

    async fn write_path<'path, 'reader>(
        &self,
        path: Cow<'path, str>,
        reader: Pin<Box<dyn AsyncRead + Send + 'reader>>,
    ) -> Result<RepositoryWrite<'path>> {
        let res = self.inner.write_path(path.clone(), reader).await?;

        self.path_writes
            .lock()
            .map_err(|_| {
                DebianError::RepositoryIoPath(
                    path.to_string(),
                    std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "error acquiring write paths mutex",
                    ),
                )
            })?
            .push(path.to_string());

        Ok(res)
    }
}
