// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::path::PathBuf;

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
}
