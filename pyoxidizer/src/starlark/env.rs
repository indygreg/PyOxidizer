// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::py_packaging::distribution::DistributionCache,
    anyhow::{Context, Result},
    starlark::{
        environment::{Environment, EnvironmentError, TypeValues},
        values::{
            error::{RuntimeError, ValueError},
            none::NoneType,
            {Mutable, TypedValue, Value, ValueResult},
        },
    },
    starlark_dialect_build_targets::{get_context_value, EnvironmentContext},
    std::{
        collections::HashMap,
        path::{Path, PathBuf},
        sync::Arc,
    },
    tugger::starlark::TuggerContext,
};

/// Holds state for evaluating a Starlark config file.
#[derive(Debug)]
pub struct PyOxidizerEnvironmentContext {
    /// PyOxidizer's run-time environment.
    env: crate::environment::Environment,

    /// Whether executing in verbose mode.
    pub verbose: bool,

    /// Directory the environment should be evaluated from.
    ///
    /// Typically used to resolve filenames.
    pub cwd: PathBuf,

    /// Path to the configuration file.
    pub config_path: PathBuf,

    /// Host triple we are building from.
    pub build_host_triple: String,

    /// Target triple we are building for.
    pub build_target_triple: String,

    /// Whether we are building a debug or release binary.
    pub build_release: bool,

    /// Optimization level when building binaries.
    pub build_opt_level: String,

    /// Cache of ready-to-clone Python distribution objects.
    ///
    /// This exists because constructing a new instance can take a
    /// few seconds in debug builds. And this adds up, especially in tests!
    pub distribution_cache: Arc<DistributionCache>,

    /// Extra variables to inject into Starlark environment.
    extra_vars: HashMap<String, Option<String>>,
}

impl PyOxidizerEnvironmentContext {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        env: &crate::environment::Environment,
        verbose: bool,
        config_path: &Path,
        build_host_triple: &str,
        build_target_triple: &str,
        build_release: bool,
        build_opt_level: &str,
        distribution_cache: Option<Arc<DistributionCache>>,
        extra_vars: HashMap<String, Option<String>>,
    ) -> Result<PyOxidizerEnvironmentContext> {
        let parent = config_path
            .parent()
            .with_context(|| "resolving parent directory of config".to_string())?;

        let parent = if parent.is_relative() {
            std::env::current_dir()?.join(parent)
        } else {
            parent.to_path_buf()
        };

        let distribution_cache = distribution_cache.unwrap_or_else(|| {
            Arc::new(DistributionCache::new(Some(
                &env.python_distributions_dir(),
            )))
        });

        Ok(PyOxidizerEnvironmentContext {
            env: env.clone(),
            verbose,
            cwd: parent,
            config_path: config_path.to_path_buf(),
            build_host_triple: build_host_triple.to_string(),
            build_target_triple: build_target_triple.to_string(),
            build_release,
            build_opt_level: build_opt_level.to_string(),
            distribution_cache,
            extra_vars,
        })
    }

    pub fn env(&self) -> &crate::environment::Environment {
        &self.env
    }

    pub fn build_path(&self, type_values: &TypeValues) -> Result<PathBuf, ValueError> {
        let build_targets_context_value = get_context_value(type_values)?;
        let context = build_targets_context_value
            .downcast_ref::<EnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        Ok(context.build_path().to_path_buf())
    }

    pub fn python_distributions_path(&self) -> Result<PathBuf, ValueError> {
        Ok(self.env.python_distributions_dir())
    }

    pub fn get_output_path(
        &self,
        type_values: &TypeValues,
        target: &str,
    ) -> Result<PathBuf, ValueError> {
        let build_targets_context_value = get_context_value(type_values)?;
        let context = build_targets_context_value
            .downcast_ref::<EnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        Ok(context.target_build_path(target))
    }
}

impl TypedValue for PyOxidizerEnvironmentContext {
    type Holder = Mutable<PyOxidizerEnvironmentContext>;
    const TYPE: &'static str = "EnvironmentContext";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }
}

/// Starlark type holding context for PyOxidizer.
#[derive(Default)]
pub struct PyOxidizerContext {}

impl TypedValue for PyOxidizerContext {
    type Holder = Mutable<PyOxidizerContext>;
    const TYPE: &'static str = "PyOxidizer";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }
}

/// Obtain the PyOxidizerContext for the Starlark execution environment.
pub fn get_context(type_values: &TypeValues) -> ValueResult {
    type_values
        .get_type_value(&Value::new(PyOxidizerContext::default()), "CONTEXT")
        .ok_or_else(|| {
            ValueError::from(RuntimeError {
                code: "PYOXIDIZER",
                message: "Unable to resolve context (this should never happen)".to_string(),
                label: "".to_string(),
            })
        })
}

/// Obtain a Starlark environment for evaluating PyOxidizer configurations.
pub fn register_starlark_dialect(
    env: &mut Environment,
    type_values: &mut TypeValues,
) -> Result<(), EnvironmentError> {
    starlark_dialect_build_targets::register_starlark_dialect(env, type_values)?;
    tugger::starlark::register_starlark_dialect(env, type_values)?;
    super::file_resource::file_resource_env(env, type_values);
    super::python_distribution::python_distribution_module(env, type_values);
    super::python_embedded_resources::python_embedded_resources_module(env, type_values);
    super::python_executable::python_executable_env(env, type_values);
    super::python_packaging_policy::python_packaging_policy_module(env, type_values);

    Ok(())
}

pub fn populate_environment(
    env: &mut Environment,
    type_values: &mut TypeValues,
    context: PyOxidizerEnvironmentContext,
    resolve_targets: Option<Vec<String>>,
    build_script_mode: bool,
) -> Result<(), EnvironmentError> {
    let mut build_targets_context = EnvironmentContext::new(context.cwd.clone());

    if let Some(targets) = resolve_targets {
        build_targets_context.set_resolve_targets(targets);
    }

    build_targets_context.build_script_mode = build_script_mode;

    build_targets_context.set_target_build_path_prefix(Some(
        PathBuf::from(&context.build_target_triple).join(if context.build_release {
            "release"
        } else {
            "debug"
        }),
    ));

    let tugger_context = TuggerContext::new();

    starlark_dialect_build_targets::populate_environment(env, type_values, build_targets_context)?;
    tugger::starlark::populate_environment(env, type_values, tugger_context)?;

    let mut vars = starlark::values::dict::Dictionary::default();

    for (k, v) in context.extra_vars.iter() {
        vars.insert(
            Value::from(k.as_str()),
            match v {
                Some(v) => Value::from(v.as_str()),
                None => Value::from(NoneType::None),
            },
        )
        .expect("error inserting variable; this should not happen");
    }

    env.set("VARS", Value::try_from(vars.get_content().clone()).unwrap())?;
    env.set("CWD", Value::from(context.cwd.display().to_string()))?;
    env.set(
        "CONFIG_PATH",
        Value::from(context.config_path.display().to_string()),
    )?;
    env.set(
        "BUILD_TARGET_TRIPLE",
        Value::from(context.build_target_triple.clone()),
    )?;

    env.set("CONTEXT", Value::new(context))?;

    // We alias various globals as PyOxidizer.* attributes so they are
    // available via the type object API. This is a bit hacky. But it allows
    // Rust code with only access to the TypeValues dictionary to retrieve
    // these globals.
    for f in &["CONTEXT", "CWD", "CONFIG_PATH", "BUILD_TARGET_TRIPLE"] {
        type_values.add_type_value(PyOxidizerContext::TYPE, f, env.get(f)?);
    }

    Ok(())
}

#[cfg(test)]
pub mod tests {
    use crate::{environment::default_target_triple, starlark::testutil::*};

    #[test]
    fn test_cwd() {
        let cwd = starlark_ok("CWD");
        let pwd = std::env::current_dir().unwrap();
        assert_eq!(cwd.to_str(), pwd.display().to_string());
    }

    #[test]
    fn test_build_target() {
        let target = starlark_ok("BUILD_TARGET_TRIPLE");
        assert_eq!(target.to_str(), default_target_triple());
    }

    #[test]
    fn test_print() {
        starlark_ok("print('hello, world')");
    }
}
