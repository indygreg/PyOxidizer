// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    super::env::{get_context, global_environment, PyOxidizerEnvironmentContext},
    anyhow::{anyhow, Result},
    codemap::CodeMap,
    codemap_diagnostic::{Diagnostic, Level},
    starlark::{
        environment::{Environment, TypeValues},
        syntax::dialect::Dialect,
        values::Value,
    },
    starlark_dialect_build_targets::ResolvedTarget,
    std::{
        path::Path,
        sync::{Arc, Mutex},
    },
};

/// Represents a running Starlark environment.
pub struct EvaluationContext {
    pub env: Environment,
    type_values: TypeValues,
}

impl EvaluationContext {
    fn context_value(&self) -> Result<Value> {
        get_context(&self.type_values).map_err(|_| anyhow!("could not obtain context"))
    }

    pub fn default_target(&self) -> Result<Option<String>> {
        let raw_context = self.context_value()?;
        let context = raw_context
            .downcast_ref::<PyOxidizerEnvironmentContext>()
            .ok_or_else(|| anyhow!("context has incorrect type"))?;

        Ok(context.core.default_target().map(|x| x.to_string()))
    }

    pub fn target_names(&self) -> Result<Vec<String>> {
        let raw_context = self.context_value()?;
        let context = raw_context
            .downcast_ref::<PyOxidizerEnvironmentContext>()
            .ok_or_else(|| anyhow!("context has incorrect type"))?;

        Ok(context
            .core
            .targets()
            .keys()
            .map(|x| x.to_string())
            .collect::<Vec<_>>())
    }

    /// Obtain targets that should be resolved.
    pub fn targets_to_resolve(&self) -> Result<Vec<String>> {
        let raw_context = self.context_value()?;
        let context = raw_context
            .downcast_ref::<PyOxidizerEnvironmentContext>()
            .ok_or_else(|| anyhow!("context has incorrect type"))?;

        Ok(context.core.targets_to_resolve())
    }

    pub fn build_resolved_target(&mut self, target: &str) -> Result<ResolvedTarget> {
        let raw_context = self.context_value()?;
        let mut context = raw_context
            .downcast_mut::<PyOxidizerEnvironmentContext>()
            .map_err(|_| anyhow!("unable to obtain mutable context"))?
            .ok_or_else(|| anyhow!("context has incorrect type"))?;

        context.build_resolved_target(target)
    }

    pub fn run_target(&mut self, target: Option<&str>) -> Result<()> {
        let raw_context = self.context_value()?;
        let mut context = raw_context
            .downcast_mut::<PyOxidizerEnvironmentContext>()
            .map_err(|_| anyhow!("unable to obtain mutable context"))?
            .ok_or_else(|| anyhow!("context has incorrect type"))?;

        context.run_target(target)
    }
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
) -> Result<EvaluationContext, Diagnostic> {
    let context = PyOxidizerEnvironmentContext::new(
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
        None,
    )
    .map_err(|e| Diagnostic {
        level: Level::Error,
        message: e.to_string(),
        code: Some("environment".to_string()),
        spans: vec![],
    })?;

    let (mut env, type_values) = global_environment(&context).map_err(|_| Diagnostic {
        level: Level::Error,
        message: "error creating environment".to_string(),
        code: Some("environment".to_string()),
        spans: vec![],
    })?;

    let map = Arc::new(Mutex::new(CodeMap::new()));
    let file_loader_env = env.clone();
    starlark::eval::simple::eval_file(
        &map,
        &config_path.display().to_string(),
        Dialect::Bzl,
        &mut env,
        &type_values,
        file_loader_env,
    )
    .map_err(|e| {
        let mut msg = Vec::new();
        let raw_map = map.lock().unwrap();
        {
            let mut emitter = codemap_diagnostic::Emitter::vec(&mut msg, Some(&raw_map));
            emitter.emit(&[e.clone()]);
        }

        slog::error!(logger, "{}", String::from_utf8_lossy(&msg));

        e
    })?;

    Ok(EvaluationContext { env, type_values })
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
) -> Result<EvaluationContext> {
    crate::starlark::eval::evaluate_file(
        logger,
        path,
        build_target_triple,
        release,
        verbose,
        resolve_targets,
        build_script_mode,
    )
    .map_err(|d| anyhow!(d.message))
}
