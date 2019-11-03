// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::env::{global_environment, EnvironmentContext};
use codemap::CodeMap;
use codemap_diagnostic::{Diagnostic, Level};
use starlark::environment::Environment;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Represents the result of evaluating a Starlark environment.
pub struct EvalResult {
    pub env: Environment,

    pub context: EnvironmentContext,
}

pub fn evaluate_file(path: &Path, context: &EnvironmentContext) -> Result<EvalResult, Diagnostic> {
    let mut env = global_environment(context).or_else(|_| {
        Err(Diagnostic {
            level: Level::Error,
            message: "error creating environment".to_string(),
            code: Some("environment".to_string()),
            spans: vec![],
        })
    })?;

    let map = Arc::new(Mutex::new(CodeMap::new()));
    starlark::eval::simple::eval_file(&map, &path.display().to_string(), false, &mut env)?;

    Ok(EvalResult {
        env,
        context: context.clone(),
    })
}
