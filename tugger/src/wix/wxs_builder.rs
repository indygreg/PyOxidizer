// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::file_resource::FileContent,
    anyhow::{anyhow, Result},
    std::{
        collections::BTreeMap,
        convert::TryFrom,
        path::{Path, PathBuf},
    },
};

/// Entity representing the build context for a .wxs file.
#[derive(Debug)]
pub struct WxsBuilder {
    /// Relative path/filename of this wxs file.
    path: PathBuf,

    /// Raw content of the wxs file.
    data: Vec<u8>,

    /// Keys to define in the preprocessor when running candle.
    preprocessor_parameters: BTreeMap<String, String>,
}

impl WxsBuilder {
    /// Create a new instance from data.
    pub fn from_data<P: AsRef<Path>>(path: P, data: Vec<u8>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            data,
            preprocessor_parameters: BTreeMap::new(),
        }
    }

    /// Create a new instance from a filesystem file.
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let filename = path
            .as_ref()
            .file_name()
            .ok_or_else(|| anyhow!("unable to determine filename"))?;

        let content = FileContent::try_from(path.as_ref())?;

        Ok(Self {
            path: PathBuf::from(filename),
            data: content.data,
            preprocessor_parameters: BTreeMap::new(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn preprocessor_parameters(&self) -> impl Iterator<Item = (&String, &String)> {
        Box::new(self.preprocessor_parameters.iter())
    }

    /// Set a preprocessor parameter value.
    ///
    /// These are passed to `candle.exe`.
    pub fn set_preprocessor_parameter<S: ToString>(&mut self, key: S, value: S) {
        self.preprocessor_parameters
            .insert(key.to_string(), value.to_string());
    }
}
