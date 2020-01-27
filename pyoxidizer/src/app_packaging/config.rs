// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use anyhow::{anyhow, Result};
use slog::warn;
use std::env;
use std::path::{Path, PathBuf};

use crate::starlark::eval::EvalResult;

/// Find a pyoxidizer.toml configuration file by walking directory ancestry.
pub fn find_pyoxidizer_config_file(start_dir: &Path) -> Option<PathBuf> {
    for test_dir in start_dir.ancestors() {
        let candidate = test_dir.to_path_buf().join("pyoxidizer.bzl");

        if candidate.exists() {
            return Some(candidate);
        }
    }

    None
}

/// Find a PyOxidizer configuration file from walking the filesystem or an
/// environment variable override.
pub fn find_pyoxidizer_config_file_env(logger: &slog::Logger, start_dir: &Path) -> Option<PathBuf> {
    match env::var("PYOXIDIZER_CONFIG") {
        Ok(config_env) => {
            warn!(
                logger,
                "using PyOxidizer config file from PYOXIDIZER_CONFIG: {}", config_env
            );
            Some(PathBuf::from(config_env))
        }
        Err(_) => find_pyoxidizer_config_file(start_dir),
    }
}

/// Evaluate a Starlark configuration file and return its result.
pub fn eval_starlark_config_file(
    logger: &slog::Logger,
    path: &Path,
    build_target_triple: &str,
    release: bool,
    verbose: bool,
    resolve_targets: Option<Vec<String>>,
) -> Result<EvalResult> {
    crate::starlark::eval::evaluate_file(
        logger,
        path,
        build_target_triple,
        release,
        verbose,
        resolve_targets,
    )
    .or_else(|d| Err(anyhow!(d.message)))
}
