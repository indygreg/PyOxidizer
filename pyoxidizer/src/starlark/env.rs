// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use anyhow::{anyhow, Context, Result};
use slog::warn;
use starlark::environment::{Environment, EnvironmentError};
use starlark::values::{default_compare, RuntimeError, TypedValue, Value, ValueError, ValueResult};
use starlark::{
    any, immutable, not_supported, starlark_fun, starlark_module, starlark_signature,
    starlark_signature_extraction, starlark_signatures,
};
use std::any::Any;
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use super::file_resource::FileManifest;
use super::target::{BuildTarget, ResolvedTarget};
use super::util::{required_str_arg, required_type_arg};
use crate::py_packaging::binary::PreBuiltPythonExecutable;

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

    /// List of targets to resolve.
    pub resolve_targets: Option<Vec<String>>,

    /// Targets that are fully resolved and their resolved value.
    pub resolved_targets: BTreeMap<String, Value>,
}

impl EnvironmentContext {
    pub fn new(
        logger: &slog::Logger,
        config_path: &Path,
        build_target_triple: &str,
        write_artifacts_path: Option<&Path>,
        resolve_targets: Option<Vec<String>>,
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
            resolve_targets,
            resolved_targets: BTreeMap::new(),
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

    pub fn targets_to_resolve(&self) -> Vec<String> {
        if let Some(targets) = &self.resolve_targets {
            targets.clone()
        } else if let Some(target) = self.default_target() {
            vec![target]
        } else {
            Vec::new()
        }
    }

    pub fn run_resolved_target(&self, target: &str) -> Result<()> {
        let v = if let Some(v) = self.resolved_targets.get(target) {
            Some(v.clone())
        } else {
            None
        }
        .ok_or_else(|| anyhow!("target {} was not resolved", target))?;

        let mut raw_value = v.0.borrow_mut();
        let raw_any = raw_value.as_any_mut();

        let resolved_target: ResolvedTarget = if raw_any.is::<FileManifest>() {
            raw_any.downcast_mut::<FileManifest>().unwrap().build()
        } else if raw_any.is::<PreBuiltPythonExecutable>() {
            raw_any
                .downcast_mut::<PreBuiltPythonExecutable>()
                .unwrap()
                .build()
        } else {
            Err(anyhow!("could not determine type of target"))
        }?;

        resolved_target.run()
    }
}

impl TypedValue for EnvironmentContext {
    immutable!();
    any!();
    not_supported!(binop);
    not_supported!(container);
    not_supported!(function);
    not_supported!(get_hash);
    not_supported!(to_int);

    fn to_str(&self) -> String {
        "EnvironmentContext".to_string()
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_type(&self) -> &'static str {
        "EnvironmentContext"
    }

    fn to_bool(&self) -> bool {
        true
    }

    fn compare(&self, other: &dyn TypedValue, _recursion: u32) -> Result<Ordering, ValueError> {
        default_compare(self, other)
    }
}

starlark_module! { global_module =>
    #[allow(clippy::ptr_arg)]
    register_target(env env, target, callable) {
        let target = required_str_arg("target", &target)?;
        required_type_arg("callable", "function", &callable)?;

        let mut context = env.get("CONTEXT").expect("CONTEXT not set");

        context.downcast_apply_mut(|x: &mut EnvironmentContext| {
            x.register_target(target.clone(), callable.clone())
        });

        Ok(Value::new(None))
    }

    #[allow(clippy::ptr_arg)]
    resolve_target(env env, call_stack cs, target) {
        let target = required_str_arg("target", &target)?;

        let mut context = env.get("CONTEXT").expect("CONTEXT not set");
        let logger = context.downcast_apply(|x: &EnvironmentContext| x.logger.clone());

        // If we have a resolved value for this target, return it.
        if let Some(resolved) = context.downcast_apply(|context: &EnvironmentContext| {
            if let Some(resolved) = context.resolved_targets.get(&target) {
                Some(resolved.clone())
            } else {
                None
            }
        }) {
            return Ok(resolved);
        }

        // Else resolve it.
        warn!(logger, "resolving target {}", target);
        let callable = context.downcast_apply(|x: &EnvironmentContext| {
            if let Some(value) = x.targets.get(&target) {
                Ok(value.clone())
            } else {
                Err(RuntimeError {
                    code: "PYOXIDIZER_BUILD",
                    message: format!("target {} does not exist", target),
                    label: "resolve_target()".to_string(),
                }.into())
            }
        })?;

        let res = callable.call(cs, env, Vec::new(), HashMap::new(), None, None)?;

        // TODO consider replacing the target's callable with a new function that
        // returns the resolved value. This will ensure a target function is only
        // called once.

        context.downcast_apply_mut(|context: &mut EnvironmentContext| {
            context.resolved_targets.insert(target.clone(), res.clone());
        });

        Ok(res)
    }

    #[allow(clippy::ptr_arg)]
    resolve_targets(env env, call_stack cs) {
        let context = env.get("CONTEXT").expect("CONTEXT not set");

        let targets = context.downcast_apply(|context: &EnvironmentContext| {
            context.targets_to_resolve()
        });

        println!("resolving {} targets", targets.len());
        for target in targets {
            let resolve = env.get("resolve_target").unwrap();

            resolve.call(cs, env.clone(), vec![Value::new(target)], HashMap::new(), None, None)?;
        }

        Ok(Value::new(None))
    }

    #[allow(clippy::ptr_arg)]
    set_build_path(env env, path) {
        let path = required_str_arg("path", &path)?;
        let mut context = env.get("CONTEXT").expect("CONTEXT not set");

        context.downcast_apply_mut(|x: &mut EnvironmentContext| {
            x.set_build_path(&PathBuf::from(&path));
        });

        Ok(Value::new(None))
    }
}

/// Obtain a Starlark environment for evaluating PyOxidizer configurations.
pub fn global_environment(context: &EnvironmentContext) -> Result<Environment, EnvironmentError> {
    let env = starlark::stdlib::global_environment();
    let env = global_module(env);
    let env = super::file_resource::file_resource_env(env);
    let env = super::python_distribution::python_distribution_module(env);
    let env = super::embedded_python_config::embedded_python_config_module(env);
    let env = super::python_executable::python_executable_env(env);
    let env = super::python_resource::python_resource_env(env);
    let env = super::python_run_mode::python_run_mode_env(env);

    env.set("CONTEXT", Value::new(context.clone()))?;

    env.set("CWD", Value::from(context.cwd.display().to_string()))?;
    env.set(
        "CONFIG_PATH",
        Value::from(context.config_path.display().to_string()),
    )?;
    env.set(
        "BUILD_TARGET_TRIPLE",
        Value::from(context.build_target_triple.clone()),
    )?;

    Ok(env)
}

#[cfg(test)]
pub mod tests {
    use super::super::testutil::*;
    use super::*;

    #[test]
    fn test_cwd() {
        let cwd = starlark_ok("CWD");
        let pwd = std::env::current_dir().unwrap();
        assert_eq!(cwd.to_str(), pwd.display().to_string());
    }

    #[test]
    fn test_build_target() {
        let target = starlark_ok("BUILD_TARGET_TRIPLE");
        assert_eq!(target.to_str(), crate::app_packaging::repackage::HOST);
    }

    #[test]
    fn test_register_target() {
        let mut env = starlark_env();
        starlark_eval_in_env(&mut env, "def foo(): pass").unwrap();
        starlark_eval_in_env(&mut env, "register_target('default', foo)").unwrap();

        let context = env.get("CONTEXT").unwrap();

        context.downcast_apply(|x: &EnvironmentContext| {
            assert_eq!(x.targets.len(), 1);
            assert!(x.targets.contains_key("default"));
            assert_eq!(
                x.targets.get("default").unwrap().to_string(),
                "foo()".to_string()
            );
            assert_eq!(x.targets_order, vec!["default".to_string()]);
        });
    }
}
