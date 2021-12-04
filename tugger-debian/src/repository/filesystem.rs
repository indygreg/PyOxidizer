// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::repository::{RepositoryWriteError, RepositoryWriter},
    async_trait::async_trait,
    futures::AsyncRead,
    std::{
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
    async fn write_path(
        &self,
        path: &str,
        reader: Pin<Box<dyn AsyncRead + Send>>,
    ) -> Result<u64, RepositoryWriteError> {
        let dest_path = self.root_dir.join(path);

        if let Some(parent) = dest_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| RepositoryWriteError::IoPath(format!("{}", parent.display()), e))?;
        }

        let fh = std::fs::File::create(&dest_path)
            .map_err(|e| RepositoryWriteError::IoPath(format!("{}", dest_path.display()), e))?;

        let mut writer = futures::io::AllowStdIo::new(fh);

        Ok(futures::io::copy(reader, &mut writer)
            .await
            .map_err(|e| RepositoryWriteError::IoPath(format!("{}", dest_path.display()), e))?)
    }
}
