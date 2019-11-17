// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::path::{Path, PathBuf};

/// Holds state for evaluating app packaging.
#[derive(Debug, Clone)]
pub struct EnvironmentContext {
    /// Directory the environment should be evaluated from.
    ///
    /// Typically used to resolve filenames.
    pub cwd: PathBuf,

    /// Path to the configuration file.
    pub config_path: PathBuf,

    /// Target triple we are building for.
    pub build_target: String,

    /// Base directory to use for build state.
    pub build_path: PathBuf,

    /// Path where Python distributions are written.
    pub python_distributions_path: PathBuf,
}

impl EnvironmentContext {
    pub fn new(config_path: &Path, build_target: &str) -> Result<EnvironmentContext, String> {
        let parent = config_path
            .parent()
            .ok_or("unable to resolve parent directory of config".to_string())?;

        let build_path = parent.join("build");

        Ok(EnvironmentContext {
            cwd: parent.to_path_buf(),
            config_path: config_path.to_path_buf(),
            build_target: build_target.to_string(),
            build_path: build_path.clone(),
            python_distributions_path: build_path.join("python_distributions"),
        })
    }

    pub fn set_build_path(&mut self, path: &Path) {
        self.build_path = path.to_path_buf();
        self.python_distributions_path = path.join("python_distributions");
    }
}
