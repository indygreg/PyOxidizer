// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Functionality for signing binaries on Windows. */

use std::path::{Path, PathBuf};

/// Represents an x509 signing certificate backed by a file.
#[derive(Clone, Debug)]
pub struct FileBasedX509SigningCertificate {
    /// Path to the certificate file.
    path: PathBuf,
    /// Password used to unlock the certificate.
    password: Option<String>,
}

impl FileBasedX509SigningCertificate {
    /// Construct an instance from a path.
    ///
    /// No validation is done that the path exists.
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            password: None,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn password(&self) -> &Option<String> {
        &self.password
    }

    pub fn set_password(&mut self, password: impl ToString) {
        self.password = Some(password.to_string());
    }
}

/// Represents an x509 certificate used to sign binaries on Windows.
#[derive(Clone, Debug)]
pub enum X509SigningCertificate {
    /// Select the best available signing certificate.
    Auto,

    /// An x509 certificate backed by a filesystem file.
    File(FileBasedX509SigningCertificate),

    /// An x509 certificate specified by its subject name or substring thereof.
    SubjectName(String),
}

impl From<FileBasedX509SigningCertificate> for X509SigningCertificate {
    fn from(v: FileBasedX509SigningCertificate) -> Self {
        Self::File(v)
    }
}
