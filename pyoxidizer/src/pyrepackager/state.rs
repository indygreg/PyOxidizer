// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

use super::config::Config;
use crate::pypackaging::distribution::LicenseInfo;
use crate::pypackaging::resource::AppRelativeResources;

/// Holds state needed to perform packaging.
///
/// Instances are serialized to disk during builds and read during
/// packaging.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PackagingState {
    pub app_relative_resources: BTreeMap<String, AppRelativeResources>,
    pub license_files_path: Option<String>,
    pub license_infos: BTreeMap<String, Vec<LicenseInfo>>,
}

/// Represents environment for a build.
pub struct BuildContext {
    /// Path to Rust project.
    pub project_path: PathBuf,

    /// Path to PyOxidizer configuration file.
    pub config_path: PathBuf,

    /// Path to directory containing PyOxidizer configuration file.
    pub config_parent_path: PathBuf,

    /// Parsed PyOxidizer configuration file.
    pub config: Config,

    /// Parsed Cargo.toml for Rust project.
    pub cargo_config: cargo_toml::Manifest,

    /// Whether to operate in verbose mode.
    pub verbose: bool,

    /// Path to main build directory where all state is stored.
    pub build_path: PathBuf,

    /// Name of application/binary being built.
    pub app_name: String,

    /// Path containing build/packaged application and all supporting files.
    pub app_path: PathBuf,

    /// Path to application executable in its installed/packaged directory.
    pub app_exe_path: PathBuf,

    /// Path where distribution files should be written.
    pub distributions_path: PathBuf,

    /// Rust target triple for build host.
    pub host_triple: String,

    /// Rust target triple for build target.
    pub target_triple: String,

    /// Whether compiling a release build.
    pub release: bool,

    /// Main output path for Rust build artifacts.
    ///
    /// Should be passed as --target to cargo build.
    pub target_base_path: PathBuf,

    /// Rust build artifact output path for this target.
    pub target_triple_base_path: PathBuf,

    /// Path to extracted Python distribution.
    pub python_distribution_path: PathBuf,

    /// Rust build artifact output path for the application crate.
    pub app_target_path: PathBuf,

    /// Application executable in its Rust target directory.
    pub app_exe_target_path: PathBuf,

    /// Path where PyOxidizer should write its build artifacts.
    pub pyoxidizer_artifacts_path: PathBuf,

    /// State used for packaging.
    pub packaging_state: Option<PackagingState>,
}
