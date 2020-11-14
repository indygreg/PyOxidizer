// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::{
    borrow::Cow,
    path::{Path, PathBuf},
};

/// Represents an abstract location for binary data.
///
/// Data can be backed by the filesystem or in memory.
#[derive(Clone, Debug, PartialEq)]
pub enum DataLocation<'a> {
    Path(PathBuf),
    Memory(Cow<'a, [u8]>),
}

impl<'a> DataLocation<'a> {
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

impl<'a> From<&Path> for DataLocation<'a> {
    fn from(path: &Path) -> Self {
        Self::Path(path.to_path_buf())
    }
}

impl<'a> From<Vec<u8>> for DataLocation<'a> {
    fn from(data: Vec<u8>) -> Self {
        Self::Memory(Cow::Owned(data))
    }
}

impl<'a> From<&'a [u8]> for DataLocation<'a> {
    fn from(data: &'a [u8]) -> Self {
        Self::Memory(Cow::Borrowed(data))
    }
}
