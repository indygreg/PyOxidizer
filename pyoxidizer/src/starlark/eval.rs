// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{
        environment::default_target_triple,
        py_packaging::distribution::DistributionCache,
        starlark::env::{
            populate_environment, register_starlark_dialect, PyOxidizerContext,
            PyOxidizerEnvironmentContext,
        },
    },
    anyhow::{anyhow, Result},
    codemap::CodeMap,
    codemap_diagnostic::{Diagnostic, Emitter},
    log::error,
    starlark::{
        environment::{Environment, EnvironmentError, TypeValues},
        eval::call_stack::CallStack,
        syntax::dialect::Dialect,
        values::{
            error::{RuntimeError, ValueError},
            Value, ValueResult,
        },
    },
    starlark_dialect_build_targets::{
        build_target, run_target, EnvironmentContext, ResolvedTarget,
    },
    std::{
        collections::HashMap,
        path::{Path, PathBuf},
        sync::{Arc, Mutex},
    },
};

/// Builder type to construct `EvaluationContext` instances.
pub struct EvaluationContextBuilder {
    env: crate::environment::Environment,
    config_path: PathBuf,
    build_target_triple: String,
    release: bool,
    verbose: bool,
    resolve_targets: Option<Vec<String>>,
    build_script_mode: bool,
    build_opt_level: String,
    distribution_cache: Option<Arc<DistributionCache>>,
    extra_vars: HashMap<String, Option<String>>,
}

impl EvaluationContextBuilder {
    pub fn new(
        env: &crate::environment::Environment,
        config_path: impl AsRef<Path>,
        build_target_triple: impl ToString,
    ) -> Self {
        Self {
            env: env.clone(),
            config_path: config_path.as_ref().to_path_buf(),
            build_target_triple: build_target_triple.to_string(),
            release: false,
            verbose: false,
            resolve_targets: None,
            build_script_mode: false,
            build_opt_level: "0".to_string(),
            distribution_cache: None,
            extra_vars: HashMap::new(),
        }
    }

    /// Transform self into an `EvaluationContext`.
    pub fn into_context(self) -> Result<EvaluationContext> {
        EvaluationContext::from_builder(self)
    }

    #[must_use]
    pub fn config_path(mut self, value: impl AsRef<Path>) -> Self {
        self.config_path = value.as_ref().to_path_buf();
        self
    }

    #[must_use]
    pub fn build_target_triple(mut self, value: impl ToString) -> Self {
        self.build_target_triple = value.to_string();
        self
    }

    #[must_use]
    pub fn release(mut self, value: bool) -> Self {
        self.release = value;
        self
    }

    #[must_use]
    pub fn verbose(mut self, value: bool) -> Self {
        self.verbose = value;
        self
    }

    #[must_use]
    pub fn resolve_targets_optional(mut self, targets: Option<Vec<impl ToString>>) -> Self {
        self.resolve_targets =
            targets.map(|targets| targets.iter().map(|x| x.to_string()).collect());
        self
    }

    #[must_use]
    pub fn resolve_targets(mut self, targets: Vec<String>) -> Self {
        self.resolve_targets = Some(targets);
        self
    }

    #[must_use]
    pub fn resolve_target_optional(mut self, target: Option<impl ToString>) -> Self {
        self.resolve_targets = target.map(|target| vec![target.to_string()]);
        self
    }

    #[must_use]
    pub fn resolve_target(mut self, target: impl ToString) -> Self {
        self.resolve_targets = Some(vec![target.to_string()]);
        self
    }

    #[must_use]
    pub fn build_script_mode(mut self, value: bool) -> Self {
        self.build_script_mode = value;
        self
    }

    #[must_use]
    pub fn distribution_cache(mut self, cache: Arc<DistributionCache>) -> Self {
        self.distribution_cache = Some(cache);
        self
    }

    #[must_use]
    pub fn extra_vars(mut self, extra_vars: HashMap<String, Option<String>>) -> Self {
        self.extra_vars = extra_vars;
        self
    }
}

/// Interface to evaluate Starlark configuration files.
///
/// This type provides the primary interface for evaluating Starlark
/// configuration files.
///
/// Instances should be constructed from `EvaluationContextBuilder` instances, as
/// the number of parameters to construct an evaluation context is significant.
pub struct EvaluationContext {
    parent_env: Environment,
    child_env: Environment,
    type_values: TypeValues,
}

impl TryFrom<EvaluationContextBuilder> for EvaluationContext {
    type Error = anyhow::Error;

    fn try_from(value: EvaluationContextBuilder) -> Result<Self, Self::Error> {
        Self::from_builder(value)
    }
}

impl EvaluationContext {
    pub fn from_builder(builder: EvaluationContextBuilder) -> Result<Self> {
        let context = PyOxidizerEnvironmentContext::new(
            &builder.env,
            builder.verbose,
            &builder.config_path,
            default_target_triple(),
            &builder.build_target_triple,
            builder.release,
            &builder.build_opt_level,
            builder.distribution_cache,
            builder.extra_vars,
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
            builder.resolve_targets,
            builder.build_script_mode,
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
            let mut msg = Vec::new();
            let raw_map = map.lock().unwrap();
            {
                let mut emitter = codemap_diagnostic::Emitter::vec(&mut msg, Some(&raw_map));
                emitter.emit(&[e.clone()]);
            }

            error!("{}", String::from_utf8_lossy(&msg));

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
            map,
            path,
            code,
            Dialect::Bzl,
            &mut self.child_env,
            &self.type_values,
            file_loader_env,
        )
    }

    /// Evaluate code as if it is executing from a path.
    pub fn eval_code_with_path(&mut self, path: &str, code: &str) -> Result<Value> {
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

    /// Evaluate code with a placeholder value for the filename.
    pub fn eval(&mut self, code: &str) -> Result<Value> {
        self.eval_code_with_path("<no_file>", code)
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

    pub fn target_build_path(&self, target: &str) -> Result<PathBuf> {
        let context_value = self.build_targets_context_value()?;
        let context = context_value.downcast_ref::<EnvironmentContext>().unwrap();

        Ok(context.target_build_path(target))
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
        let mut call_stack = CallStack::default();

        build_target(
            &mut self.child_env,
            &self.type_values,
            &mut call_stack,
            target,
        )
    }

    pub fn run_target(&mut self, target: Option<&str>) -> Result<()> {
        let mut call_stack = CallStack::default();

        run_target(
            &mut self.child_env,
            &self.type_values,
            &mut call_stack,
            target,
        )
    }
}

#[cfg(test)]
mod tests {
    use {super::*, crate::testutil::*, starlark::values::dict::Dictionary};

    #[test]
    fn test_load() -> Result<()> {
        let env = get_env()?;
        let temp_dir = env.temporary_directory("pyoxidizer-test")?;

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

        let mut context = EvaluationContextBuilder::new(
            &env,
            main_path.clone(),
            default_target_triple().to_string(),
        )
        .verbose(true)
        .into_context()?;
        context.evaluate_file(&main_path)?;

        temp_dir.close()?;

        Ok(())
    }

    #[test]
    fn test_register_target() -> Result<()> {
        let env = get_env()?;
        let temp_dir = env.temporary_directory("pyoxidizer-test")?;

        let config_path = temp_dir.path().join("pyoxidizer.bzl");
        std::fs::write(&config_path, "def make_dist():\n    return default_python_distribution()\nregister_target('dist', make_dist)\n".as_bytes())?;

        let mut context: EvaluationContext = EvaluationContextBuilder::new(
            &env,
            config_path.clone(),
            default_target_triple().to_string(),
        )
        .verbose(true)
        .into_context()?;
        context.evaluate_file(&config_path)?;

        temp_dir.close()?;

        Ok(())
    }

    #[test]
    fn extra_vars() -> Result<()> {
        let env = get_env()?;
        let temp_dir = env.temporary_directory("pyoxidizer-test")?;

        let config_path = temp_dir.path().join("pyoxidizer.bzl");
        std::fs::write(&config_path, "my_var_copy = my_var\nempty_copy = empty\n")?;

        let mut extra_vars = HashMap::new();
        extra_vars.insert("my_var".to_string(), Some("my_value".to_string()));
        extra_vars.insert("empty".to_string(), None);

        let context =
            EvaluationContextBuilder::new(&env, config_path, default_target_triple().to_string())
                .extra_vars(extra_vars)
                .into_context()?;

        let vars_value = context.get_var("VARS").unwrap();
        assert_eq!(vars_value.get_type(), "dict");
        let vars = vars_value.downcast_ref::<Dictionary>().unwrap();

        let v = vars.get(&Value::from("my_var")).unwrap().unwrap();
        assert_eq!(v.to_string(), "my_value");

        let v = vars.get(&Value::from("empty")).unwrap().unwrap();
        assert_eq!(v.get_type(), "NoneType");

        temp_dir.close()?;

        Ok(())
    }
}
