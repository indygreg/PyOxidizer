// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use anyhow::{Context, Result};
use starlark::values::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Holds state for evaluating app packaging.
#[derive(Debug, Clone)]
pub struct EnvironmentContext {
    pub logger: slog::Logger,

    /// Directory the environment should be evaluated from.
    ///
    /// Typically used to resolve filenames.
    pub cwd: PathBuf,

    /// Path to the configuration file.
    pub config_path: PathBuf,

    /// Target triple we are building for.
    pub build_target_triple: String,

    /// Base directory to use for build state.
    pub build_path: PathBuf,

    /// Path where Python distributions are written.
    pub python_distributions_path: PathBuf,

    /// Where to automatically write artifacts for built executables.
    pub write_artifacts_path: Option<PathBuf>,

    /// Registered build targets.
    ///
    /// A target consists of a name and a Starlark callable.
    pub targets: BTreeMap<String, Value>,

    /// Order targets are registered in.
    pub targets_order: Vec<String>,
}

impl EnvironmentContext {
    pub fn new(
        logger: &slog::Logger,
        config_path: &Path,
        build_target_triple: &str,
        write_artifacts_path: Option<&Path>,
    ) -> Result<EnvironmentContext> {
        let parent = config_path
            .parent()
            .with_context(|| "resolving parent directory of config".to_string())?;

        let build_path = parent.join("build");

        Ok(EnvironmentContext {
            logger: logger.clone(),
            cwd: parent.to_path_buf(),
            config_path: config_path.to_path_buf(),
            build_target_triple: build_target_triple.to_string(),
            build_path: build_path.clone(),
            python_distributions_path: build_path.join("python_distributions"),
            write_artifacts_path: match write_artifacts_path {
                Some(p) => Some(p.to_path_buf()),
                None => None,
            },
            targets: BTreeMap::new(),
            targets_order: Vec::new(),
        })
    }

    pub fn set_build_path(&mut self, path: &Path) {
        self.build_path = path.to_path_buf();
        self.python_distributions_path = path.join("python_distributions");
    }

    pub fn register_target(&mut self, target: String, callable: Value) {
        if !self.targets.contains_key(&target) {
            self.targets_order.push(target.clone());
        }

        self.targets.insert(target, callable);
    }

    pub fn default_target(&self) -> Option<String> {
        if self.targets_order.is_empty() {
            None
        } else {
            Some(self.targets_order[0].clone())
        }
    }
}
