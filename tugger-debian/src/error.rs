// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{
        binary_package_control::BinaryPackageControlError,
        control::ControlError,
        repository::{release::ReleaseError, RepositoryReadError},
    },
    thiserror::Error,
};

#[cfg(feature = "http")]
use crate::repository::http::HttpError;

#[derive(Debug, Error)]
pub enum DebianError {
    #[error("binary package control file error: {0:?}")]
    BinaryPackageControl(#[from] BinaryPackageControlError),

    #[error("control file error: {0:?}")]
    Control(#[from] ControlError),

    #[error("release file error: {0:?}")]
    Release(#[from] ReleaseError),

    #[error("repository read error: {0:?}")]
    RepositoryRead(#[from] RepositoryReadError),

    #[cfg(feature = "http")]
    #[error("HTTP error: {0:?}")]
    Http(#[from] HttpError),

    #[error("I/O error: {0:?}")]
    Io(#[from] std::io::Error),
}

/// Result wrapper for this crate.
pub type Result<T> = std::result::Result<T, DebianError>;
