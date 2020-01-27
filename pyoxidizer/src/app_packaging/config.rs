// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use anyhow::{anyhow, Result};
use std::path::Path;

use crate::starlark::eval::EvalResult;

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
