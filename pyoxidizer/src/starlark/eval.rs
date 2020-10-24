// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::starlark::{
        env::{
            get_context, global_environment, PyOxidizerBuildContext, PyOxidizerEnvironmentContext,
        },
        file_resource::FileManifestValue,
        python_embedded_resources::PythonEmbeddedResources,
        python_executable::PythonExecutable,
    },
    anyhow::{anyhow, Context, Result},
    codemap::CodeMap,
    codemap_diagnostic::{Diagnostic, Level},
    starlark::{
        environment::{Environment, TypeValues},
        syntax::dialect::Dialect,
        values::Value,
    },
    starlark_dialect_build_targets::{BuildTarget, ResolvedTarget},
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

        let resolved_value = if let Some(t) = context.core.get_target(target) {
            if let Some(t) = &t.built_target {
                return Ok(t.clone());
            }

            if let Some(v) = &t.resolved_value {
                v.clone()
            } else {
                return Err(anyhow!("target {} is not resolved", target));
            }
        } else {
            return Err(anyhow!("target {} is not registered", target));
        };

        let output_path = context
            .build_path
            .join(&context.build_target_triple)
            .join(if context.build_release {
                "release"
            } else {
                "debug"
            })
            .join(target);

        std::fs::create_dir_all(&output_path).context("creating output path")?;

        let build_context = PyOxidizerBuildContext {
            logger: context.logger().clone(),
            host_triple: context.build_host_triple.clone(),
            target_triple: context.build_target_triple.clone(),
            release: context.build_release,
            opt_level: context.build_opt_level.clone(),
            output_path,
        };

        // TODO surely this can use dynamic dispatch.
        let resolved_target: ResolvedTarget = match resolved_value.get_type() {
            "FileManifest" => resolved_value
                .downcast_mut::<FileManifestValue>()
                .map_err(|_| anyhow!("object isn't mutable"))?
                .ok_or_else(|| anyhow!("invalid cast"))?
                .build(&build_context),
            "PythonExecutable" => resolved_value
                .downcast_mut::<PythonExecutable>()
                .map_err(|_| anyhow!("object isn't mutable"))?
                .ok_or_else(|| anyhow!("invalid cast"))?
                .build(&build_context),
            "PythonEmbeddedResources" => resolved_value
                .downcast_mut::<PythonEmbeddedResources>()
                .map_err(|_| anyhow!("object isn't mutable"))?
                .ok_or_else(|| anyhow!("invalid cast"))?
                .build(&build_context),
            _ => Err(anyhow!("could not determine type of target")),
        }?;

        context.core.get_target_mut(target).unwrap().built_target = Some(resolved_target.clone());

        Ok(resolved_target)
    }

    /// Evaluate a target and run it, if possible.
    pub fn run_resolved_target(&mut self, target: &str) -> Result<()> {
        let resolved_target = self.build_resolved_target(target)?;

        resolved_target.run()
    }

    pub fn run_target(&mut self, target: Option<&str>) -> Result<()> {
        let raw_context = self.context_value()?;
        let context = raw_context
            .downcast_ref::<PyOxidizerEnvironmentContext>()
            .ok_or_else(|| anyhow!("context has incorrect type"))?;

        let target = if let Some(t) = target {
            t.to_string()
        } else if let Some(t) = context.core.default_target() {
            t.to_string()
        } else {
            return Err(anyhow!("unable to determine target to run"));
        };

        self.run_resolved_target(&target)
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

    let (mut env, type_values) = global_environment(context).map_err(|_| Diagnostic {
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
