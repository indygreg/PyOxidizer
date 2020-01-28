// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    super::env::{global_environment, EnvironmentContext},
    anyhow::{anyhow, Result},
    codemap::CodeMap,
    codemap_diagnostic::{Diagnostic, Level},
    starlark::environment::Environment,
    std::path::Path,
    std::sync::{Arc, Mutex},
};

/// Represents the result of evaluating a Starlark environment.
pub struct EvalResult {
    pub env: Environment,

    pub context: EnvironmentContext,
}

/// Evaluate a Starlark configuration file, returning a low-level result.
pub fn evaluate_file(
    logger: &slog::Logger,
    config_path: &Path,
    build_target_triple: &str,
    release: bool,
    verbose: bool,
    resolve_targets: Option<Vec<String>>,
    build_script_mode: bool,
) -> Result<EvalResult, Diagnostic> {
    let context = EnvironmentContext::new(
        logger,
        verbose,
        config_path,
        crate::project_building::HOST,
        build_target_triple,
        release,
        // TODO this should be an argument.
        "0",
        resolve_targets,
        build_script_mode,
    )
    .or_else(|e| {
        Err(Diagnostic {
            level: Level::Error,
            message: e.to_string(),
            code: Some("environment".to_string()),
            spans: vec![],
        })
    })?;

    let mut env = global_environment(&context).or_else(|_| {
        Err(Diagnostic {
            level: Level::Error,
            message: "error creating environment".to_string(),
            code: Some("environment".to_string()),
            spans: vec![],
        })
    })?;

    let map = Arc::new(Mutex::new(CodeMap::new()));
    starlark::eval::simple::eval_file(&map, &config_path.display().to_string(), false, &mut env)
        .or_else(|e| {
            let mut msg = Vec::new();
            let raw_map = map.lock().unwrap();
            {
                let mut emitter = codemap_diagnostic::Emitter::vec(&mut msg, Some(&raw_map));
                emitter.emit(&[e.clone()]);
            }

            slog::error!(logger, "{}", String::from_utf8_lossy(&msg));

            Err(e)
        })?;

    // The EnvironmentContext is cloned as part of evaluation, which is a bit wonky.
    // TODO avoid this clone.
    let env_context = env.get("CONTEXT").or_else(|_| {
        Err(Diagnostic {
            level: Level::Error,
            message: "CONTEXT not defined".to_string(),
            code: Some("environment".to_string()),
            spans: vec![],
        })
    })?;

    Ok(EvalResult {
        env,
        context: env_context.downcast_apply(|x: &EnvironmentContext| x.clone()),
    })
}

/// Evaluate a Starlark configuration file and return its result.
pub fn eval_starlark_config_file(
    logger: &slog::Logger,
    path: &Path,
    build_target_triple: &str,
    release: bool,
    verbose: bool,
    resolve_targets: Option<Vec<String>>,
    build_script_mode: bool,
) -> Result<EvalResult> {
    crate::starlark::eval::evaluate_file(
        logger,
        path,
        build_target_triple,
        release,
        verbose,
        resolve_targets,
        build_script_mode,
    )
    .or_else(|d| Err(anyhow!(d.message)))
}
