// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! A special repository writer that writes to a black hole. */

use {
    crate::{
        error::Result,
        io::ContentDigest,
        repository::{
            RepositoryPathVerification, RepositoryPathVerificationState, RepositoryWrite,
            RepositoryWriter,
        },
    },
    async_trait::async_trait,
    futures::AsyncRead,
    std::{borrow::Cow, pin::Pin},
};

/// How [RepositoryWriter::verify_path()] should behave for [SinkWriter] instances.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SinkWriterVerifyBehavior {
    /// Path exists but an integrity check was not performed.
    ExistsNoIntegrityCheck,
    /// Path exists and its integrity was verified.
    ExistsIntegrityVerified,
    /// Path exists but its integrity doesn't match expectations.
    ExistsIntegrityMismatch,
    /// Path does not exist.
    Missing,
}

impl From<SinkWriterVerifyBehavior> for RepositoryPathVerificationState {
    fn from(v: SinkWriterVerifyBehavior) -> Self {
        match v {
            SinkWriterVerifyBehavior::ExistsNoIntegrityCheck => Self::ExistsNoIntegrityCheck,
            SinkWriterVerifyBehavior::ExistsIntegrityVerified => Self::ExistsIntegrityVerified,
            SinkWriterVerifyBehavior::ExistsIntegrityMismatch => Self::ExistsIntegrityMismatch,
            SinkWriterVerifyBehavior::Missing => Self::Missing,
        }
    }
}

/// A [RepositoryWriter] that writes data to a black hole.
pub struct SinkWriter {
    verify_behavior: SinkWriterVerifyBehavior,
}

impl Default for SinkWriter {
    fn default() -> Self {
        Self {
            verify_behavior: SinkWriterVerifyBehavior::Missing,
        }
    }
}

#[async_trait]
impl RepositoryWriter for SinkWriter {
    async fn verify_path<'path>(
        &self,
        path: &'path str,
        _expected_content: Option<(u64, ContentDigest)>,
    ) -> Result<RepositoryPathVerification<'path>> {
        Ok(RepositoryPathVerification {
            path,
            state: self.verify_behavior.into(),
        })
    }

    async fn write_path<'path, 'reader>(
        &self,
        path: Cow<'path, str>,
        reader: Pin<Box<dyn AsyncRead + Send + 'reader>>,
    ) -> Result<RepositoryWrite<'path>> {
        let mut writer = futures::io::sink();
        let bytes_written = futures::io::copy(reader, &mut writer).await?;

        Ok(RepositoryWrite {
            path,
            bytes_written,
        })
    }
}
