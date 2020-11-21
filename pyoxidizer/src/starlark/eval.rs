// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{
        py_packaging::distribution::DistributionCache,
        starlark::env::{
            populate_environment, register_starlark_dialect, PyOxidizerContext,
            PyOxidizerEnvironmentContext,
        },
    },
    anyhow::{anyhow, Result},
    codemap::CodeMap,
    codemap_diagnostic::{Diagnostic, Emitter},
    linked_hash_map::LinkedHashMap,
    starlark::{
        environment::{Environment, EnvironmentError, TypeValues},
        eval::call_stack::CallStack,
        syntax::dialect::Dialect,
        values::{
            error::{RuntimeError, ValueError},
            Value, ValueResult,
        },
    },
    starlark_dialect_build_targets::{EnvironmentContext, ResolvedTarget, ResolvedTargetValue},
    std::{
        path::{Path, PathBuf},
        sync::{Arc, Mutex},
    },
};

/// Represents a running Starlark environment.
pub struct EvaluationContext {
    parent_env: Environment,
    child_env: Environment,
    type_values: TypeValues,
}

impl EvaluationContext {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        logger: &slog::Logger,
        config_path: &Path,
        build_target_triple: &str,
        release: bool,
        verbose: bool,
        resolve_targets: Option<Vec<String>>,
        build_script_mode: bool,
        build_opt_level: &str,
        distribution_cache: Option<Arc<DistributionCache>>,
    ) -> Result<Self> {
        let context = PyOxidizerEnvironmentContext::new(
            logger,
            verbose,
            config_path,
            crate::project_building::HOST,
            build_target_triple,
            release,
            build_opt_level,
            distribution_cache,
        )?;

        let (mut parent_env, mut type_values) = starlark::stdlib::global_environment();

        register_starlark_dialect(&mut parent_env, &mut type_values)
            .map_err(|e| anyhow!("error creating Starlark environment: {:?}", e))?;

        // All variables go in a child environment. Upon calling child(), the parent
        // environment is frozen and no new changes are allowed.
        let mut child_env = parent_env.child("pyoxidizer");

        populate_environment(
            &mut child_env,
            &mut type_values,
            context,
            resolve_targets,
            build_script_mode,
        )
        .map_err(|e| anyhow!("error populating Starlark environment: {:?}", e))?;

        Ok(Self {
            parent_env,
            child_env,
            type_values,
        })
    }

    /// Obtain a named variable from the Starlark environment.
    pub fn get_var(&self, name: &str) -> Result<Value, EnvironmentError> {
        self.child_env.get(name)
    }

    /// Set a named variables in the Starlark environment.
    pub fn set_var(&mut self, name: &str, value: Value) -> Result<(), EnvironmentError> {
        self.child_env.set(name, value)
    }

    /// Evaluate a Starlark configuration file, returning a Diagnostic on error.
    pub fn evaluate_file_diagnostic(&mut self, config_path: &Path) -> Result<(), Diagnostic> {
        let map = Arc::new(Mutex::new(CodeMap::new()));
        let file_loader_env = self.parent_env.clone();

        starlark::eval::simple::eval_file(
            &map,
            &config_path.display().to_string(),
            Dialect::Bzl,
            &mut self.child_env,
            &self.type_values,
            file_loader_env,
        )
        .map_err(|e| {
            if let Ok(raw_context) = self.build_targets_context_value() {
                if let Some(context) = raw_context.downcast_ref::<EnvironmentContext>() {
                    let mut msg = Vec::new();
                    let raw_map = map.lock().unwrap();
                    {
                        let mut emitter =
                            codemap_diagnostic::Emitter::vec(&mut msg, Some(&raw_map));
                        emitter.emit(&[e.clone()]);
                    }

                    slog::error!(context.logger(), "{}", String::from_utf8_lossy(&msg));
                }
            }

            e
        })?;

        Ok(())
    }

    /// Evaluate a Starlark configuration file, returning an anyhow Result.
    pub fn evaluate_file(&mut self, config_path: &Path) -> Result<()> {
        self.evaluate_file_diagnostic(config_path)
            .map_err(|d| anyhow!(d.message))
    }

    /// Evaluate code, returning a `Diagnostic` on error.
    pub fn eval_diagnostic(
        &mut self,
        map: &Arc<Mutex<CodeMap>>,
        path: &str,
        code: &str,
    ) -> Result<Value, Diagnostic> {
        let file_loader_env = self.child_env.clone();

        starlark::eval::simple::eval(
            &map,
            path,
            code,
            Dialect::Bzl,
            &mut self.child_env,
            &self.type_values,
            file_loader_env,
        )
    }

    pub fn eval(&mut self, path: &str, code: &str) -> Result<Value> {
        let map = std::sync::Arc::new(std::sync::Mutex::new(CodeMap::new()));

        self.eval_diagnostic(&map, path, code)
            .map_err(|diagnostic| {
                let cloned_map_lock = Arc::clone(&map);
                let unlocked_map = cloned_map_lock.lock().unwrap();

                let mut buffer = vec![];
                Emitter::vec(&mut buffer, Some(&unlocked_map)).emit(&[diagnostic]);

                anyhow!(
                    "error running '{}': {}",
                    code,
                    String::from_utf8_lossy(&buffer)
                )
            })
    }

    /// Obtain the `Value` for the build targets context.
    fn build_targets_context_value(&self) -> Result<Value> {
        starlark_dialect_build_targets::get_context_value(&self.type_values)
            .map_err(|_| anyhow!("could not obtain build targets context"))
    }

    /// Obtain the `Value` for the PyOxidizerContext.
    pub fn pyoxidizer_context_value(&self) -> ValueResult {
        self.type_values
            .get_type_value(&Value::new(PyOxidizerContext::default()), "CONTEXT")
            .ok_or_else(|| {
                ValueError::from(RuntimeError {
                    code: "PYOXIDIZER",
                    message: "Unable to resolve context (this should never happen)".to_string(),
                    label: "".to_string(),
                })
            })
    }

    pub fn build_path(&self) -> Result<PathBuf, ValueError> {
        let pyoxidizer_context_value = self.pyoxidizer_context_value()?;
        let pyoxidizer_context = pyoxidizer_context_value
            .downcast_ref::<PyOxidizerEnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        pyoxidizer_context.build_path(&self.type_values)
    }

    pub fn default_target(&self) -> Result<Option<String>> {
        let raw_context = self.build_targets_context_value()?;
        let context = raw_context
            .downcast_ref::<EnvironmentContext>()
            .ok_or_else(|| anyhow!("context has incorrect type"))?;

        Ok(context.default_target().map(|x| x.to_string()))
    }

    pub fn target_names(&self) -> Result<Vec<String>> {
        let raw_context = self.build_targets_context_value()?;
        let context = raw_context
            .downcast_ref::<EnvironmentContext>()
            .ok_or_else(|| anyhow!("context has incorrect type"))?;

        Ok(context
            .targets()
            .keys()
            .map(|x| x.to_string())
            .collect::<Vec<_>>())
    }

    /// Obtain targets that should be resolved.
    pub fn targets_to_resolve(&self) -> Result<Vec<String>> {
        let raw_context = self.build_targets_context_value()?;
        let context = raw_context
            .downcast_ref::<EnvironmentContext>()
            .ok_or_else(|| anyhow!("context has incorrect type"))?;

        Ok(context.targets_to_resolve())
    }

    pub fn build_resolved_target(&mut self, target: &str) -> Result<ResolvedTarget> {
        let resolved_value = {
            let raw_context = self.build_targets_context_value()?;
            let context = raw_context
                .downcast_ref::<EnvironmentContext>()
                .ok_or_else(|| anyhow!("context has incorrect type"))?;

            let v = if let Some(t) = context.get_target(target) {
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

            v
        };

        let build = self
            .type_values
            .get_type_value(&resolved_value, "build")
            .ok_or_else(|| anyhow!("{} does not implement build()", resolved_value.get_type()))?;

        let mut call_stack = CallStack::default();

        let resolved_target_value = build
            .call(
                &mut call_stack,
                &self.type_values,
                vec![resolved_value, Value::from(target)],
                LinkedHashMap::new(),
                None,
                None,
            )
            .map_err(|e| anyhow!("error calling build(): {:?}", e))?;

        let resolved_target = resolved_target_value
            .downcast_ref::<ResolvedTargetValue>()
            .unwrap();

        let raw_context = self.build_targets_context_value()?;
        let mut context = raw_context
            .downcast_mut::<EnvironmentContext>()
            .map_err(|_| anyhow!("unable to obtain mutable context"))?
            .ok_or_else(|| anyhow!("context has incorrect type"))?;

        context.get_target_mut(target).unwrap().built_target = Some(resolved_target.inner.clone());

        Ok(resolved_target.inner.clone())
    }

    /// Evaluate a target and run it, if possible.
    pub fn run_resolved_target(&mut self, target: &str) -> Result<()> {
        let resolved_target = self.build_resolved_target(target)?;

        resolved_target.run()
    }

    pub fn run_target(&mut self, target: Option<&str>) -> Result<()> {
        let target = {
            // Block to avoid nested borrow of this Value.
            let raw_context = self.build_targets_context_value()?;
            let context = raw_context
                .downcast_ref::<EnvironmentContext>()
                .ok_or_else(|| anyhow!("context has incorrect type"))?;

            if let Some(t) = target {
                t.to_string()
            } else if let Some(t) = context.default_target() {
                t.to_string()
            } else {
                return Err(anyhow!("unable to determine target to run"));
            }
        };

        self.run_resolved_target(&target)
    }
}

#[cfg(test)]
mod tests {
    use {super::*, crate::testutil::*};

    #[test]
    fn test_load() -> Result<()> {
        let temp_dir = tempdir::TempDir::new("pyoxidizer-test")?;
        let logger = get_logger()?;

        let load_path = temp_dir.path().join("load.bzl");
        std::fs::write(
            &load_path,
            "def make_dist():\n    return default_python_distribution()\n".as_bytes(),
        )?;

        let main_path = temp_dir.path().join("main.bzl");
        std::fs::write(
            &main_path,
            format!(
                "load('{}', 'make_dist')\nmake_dist()\n",
                load_path.display().to_string().escape_default()
            )
            .as_bytes(),
        )?;

        let mut context = EvaluationContext::new(
            &logger,
            &main_path,
            env!("HOST"),
            false,
            true,
            None,
            false,
            "0",
            None,
        )?;

        context.evaluate_file(&main_path)?;

        Ok(())
    }

    #[test]
    fn test_register_target() -> Result<()> {
        let temp_dir = tempdir::TempDir::new("pyoxidizer-test")?;
        let logger = get_logger()?;

        let config_path = temp_dir.path().join("pyoxidizer.bzl");
        std::fs::write(&config_path, "def make_dist():\n    return default_python_distribution()\nregister_target('dist', make_dist)\n".as_bytes())?;

        let mut context = EvaluationContext::new(
            &logger,
            &config_path,
            env!("HOST"),
            false,
            true,
            None,
            false,
            "0",
            None,
        )?;

        context.evaluate_file(&config_path)?;

        Ok(())
    }
}
