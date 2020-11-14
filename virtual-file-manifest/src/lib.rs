// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::{
    borrow::Cow,
    convert::TryFrom,
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[cfg(unix)]
pub fn is_executable(metadata: &std::fs::Metadata) -> bool {
    let permissions = metadata.permissions();
    permissions.mode() & 0o111 != 0
}

#[cfg(windows)]
pub fn is_executable(_metadata: &std::fs::Metadata) -> bool {
    false
}

#[cfg(unix)]
pub fn set_executable(file: &mut std::fs::File) -> Result<(), std::io::Error> {
    let mut permissions = file.metadata()?.permissions();
    permissions.set_mode(0o770);
    file.set_permissions(permissions)?;
    Ok(())
}

#[cfg(windows)]
pub fn set_executable(_file: &mut std::fs::File) -> Result<(), std::io::Error> {
    Ok(())
}

/// Represents an abstract location for binary data.
///
/// Data can be backed by the filesystem or in memory.
#[derive(Clone, Debug, PartialEq)]
pub enum FileData<'a> {
    Path(PathBuf),
    Memory(Cow<'a, [u8]>),
}

impl<'a> FileData<'a> {
    /// Resolve the data for this instance.
    ///
    /// If backed by a file, the file will be read.
    pub fn resolve(&self) -> Result<Cow<'a, [u8]>, std::io::Error> {
        match self {
            Self::Path(p) => {
                let data = std::fs::read(p)?;

                Ok(Cow::Owned(data))
            }
            Self::Memory(data) => Ok(data.clone()),
        }
    }

    /// Convert this instance to a memory variant.
    ///
    /// This ensures any file-backed data is present in memory.
    pub fn to_memory(&self) -> Result<Self, std::io::Error> {
        Ok(Self::Memory(self.resolve()?))
    }
}

impl<'a> From<&Path> for FileData<'a> {
    fn from(path: &Path) -> Self {
        Self::Path(path.to_path_buf())
    }
}

impl<'a> From<Vec<u8>> for FileData<'a> {
    fn from(data: Vec<u8>) -> Self {
        Self::Memory(Cow::Owned(data))
    }
}

impl<'a> From<&'a [u8]> for FileData<'a> {
    fn from(data: &'a [u8]) -> Self {
        Self::Memory(Cow::Borrowed(data))
    }
}

/// Represents a virtual file, without an associated path.
#[derive(Clone, Debug, PartialEq)]
pub struct FileEntry<'a> {
    /// The content of the file.
    pub data: FileData<'a>,
    /// Whether the file is executable.
    pub executable: bool,
}

impl<'a> TryFrom<&Path> for FileEntry<'a> {
    type Error = std::io::Error;

    fn try_from(path: &Path) -> Result<Self, Self::Error> {
        let metadata = std::fs::metadata(path)?;
        let executable = is_executable(&metadata);

        Ok(Self {
            data: FileData::from(path),
            executable,
        })
    }
}
