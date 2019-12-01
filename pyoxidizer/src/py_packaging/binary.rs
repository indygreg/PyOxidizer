// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::config::{EmbeddedPythonConfig, RunMode};
use super::distribution::ParsedPythonDistribution;
use super::embedded_resource::EmbeddedPythonResourcesPrePackaged;

/// A self-contained Python executable before it is compiled.
#[derive(Debug, Clone)]
pub struct PreBuiltPythonExecutable {
    pub distribution: ParsedPythonDistribution,
    pub resources: EmbeddedPythonResourcesPrePackaged,
    pub config: EmbeddedPythonConfig,
    pub run_mode: RunMode,
}
