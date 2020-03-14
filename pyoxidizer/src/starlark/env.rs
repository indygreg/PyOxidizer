// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    super::file_resource::FileManifest,
    super::python_embedded_resources::PythonEmbeddedResources,
    super::python_executable::PythonExecutable,
    super::target::{BuildContext, BuildTarget, ResolvedTarget},
    super::util::{optional_list_arg, required_bool_arg, required_str_arg, required_type_arg},
    anyhow::{anyhow, Context, Result},
    path_dedot::ParseDot,
    slog::warn,
    starlark::environment::{Environment, EnvironmentError},
    starlark::values::{default_compare, RuntimeError, TypedValue, Value, ValueError, ValueResult},
    starlark::{
        any, immutable, not_supported, starlark_fun, starlark_module, starlark_signature,
        starlark_signature_extraction, starlark_signatures,
    },
    std::any::Any,
    std::cmp::Ordering,
    std::collections::{BTreeMap, HashMap},
    std::path::{Path, PathBuf},
};

/// Represents a registered target in the Starlark environment.
#[derive(Debug, Clone)]
pub struct Target {
    /// The Starlark callable registered to this target.
    pub callable: Value,

    /// Other targets this one depends on.
    pub depends: Vec<String>,

    /// What calling callable returned, if it has been called.
    pub resolved_value: Option<Value>,

    /// The `ResolvedTarget` instance this target's build() returned.
    ///
    /// TODO consider making this an Arc<T> so we don't have to clone it.
    pub built_target: Option<ResolvedTarget>,
}

/// Holds state for evaluating a Starlark config file.
#[derive(Debug, Clone)]
pub struct EnvironmentContext {
    pub logger: slog::Logger,

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

    /// Base directory to use for build state.
    pub build_path: PathBuf,

    /// Path where Python distributions are written.
    pub python_distributions_path: PathBuf,

    /// Registered build targets.
    ///
    /// A target consists of a name and a Starlark callable.
    pub targets: BTreeMap<String, Target>,

    /// Order targets are registered in.
    pub targets_order: Vec<String>,

    /// Name of default target.
    pub default_target: Option<String>,

    /// Name of default target to resolve in build script mode.
    pub default_build_script_target: Option<String>,

    /// List of targets to resolve.
    pub resolve_targets: Option<Vec<String>>,

    /// Whether we are operating in Rust build script mode.
    ///
    /// This will change the default target to resolve.
    pub build_script_mode: bool,
}

impl EnvironmentContext {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        logger: &slog::Logger,
        verbose: bool,
        config_path: &Path,
        build_host_triple: &str,
        build_target_triple: &str,
        build_release: bool,
        build_opt_level: &str,
        resolve_targets: Option<Vec<String>>,
        build_script_mode: bool,
    ) -> Result<EnvironmentContext> {
        let parent = config_path
            .parent()
            .with_context(|| "resolving parent directory of config".to_string())?;

        let parent = if parent.is_relative() {
            std::env::current_dir()?.join(parent)
        } else {
            parent.to_path_buf()
        };

        let build_path = parent.join("build");

        Ok(EnvironmentContext {
            logger: logger.clone(),
            verbose,
            cwd: parent,
            config_path: config_path.to_path_buf(),
            build_host_triple: build_host_triple.to_string(),
            build_target_triple: build_target_triple.to_string(),
            build_release,
            build_opt_level: build_opt_level.to_string(),
            build_path: build_path.clone(),
            python_distributions_path: build_path.join("python_distributions"),
            targets: BTreeMap::new(),
            targets_order: Vec::new(),
            default_target: None,
            default_build_script_target: None,
            resolve_targets,
            build_script_mode,
        })
    }

    pub fn set_build_path(&mut self, path: &Path) -> Result<()> {
        let path = if path.is_relative() {
            self.cwd.join(path)
        } else {
            path.to_path_buf()
        }
        .parse_dot()?;

        self.build_path = path.clone();
        self.python_distributions_path = path.join("python_distributions");

        Ok(())
    }

    /// Register a named target.
    pub fn register_target(
        &mut self,
        target: String,
        callable: Value,
        depends: Vec<String>,
        default: bool,
        default_build_script: bool,
    ) {
        if !self.targets.contains_key(&target) {
            self.targets_order.push(target.clone());
        }

        self.targets.insert(
            target.clone(),
            Target {
                callable,
                depends,
                resolved_value: None,
                built_target: None,
            },
        );

        if default || self.default_target.is_none() {
            self.default_target = Some(target.clone());
        }

        if default_build_script || self.default_build_script_target.is_none() {
            self.default_build_script_target = Some(target);
        }
    }

    /// Determine what targets should be resolved.
    ///
    /// This isn't the full list of targets that will be resolved, only the main
    /// targets that we will instruct the resolver to resolve.
    pub fn targets_to_resolve(&self) -> Vec<String> {
        if let Some(targets) = &self.resolve_targets {
            targets.clone()
        } else if self.build_script_mode && self.default_build_script_target.is_some() {
            vec![self.default_build_script_target.clone().unwrap()]
        } else if let Some(target) = &self.default_target {
            vec![target.to_string()]
        } else {
            Vec::new()
        }
    }

    /// Build a resolved target.
    pub fn build_resolved_target(&mut self, target: &str) -> Result<ResolvedTarget> {
        let resolved_value = if let Some(t) = self.targets.get(target) {
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

        let mut raw_value = resolved_value.0.borrow_mut();
        let raw_any = raw_value.as_any_mut();

        let output_path = self
            .build_path
            .join(&self.build_target_triple)
            .join(if self.build_release {
                "release"
            } else {
                "debug"
            })
            .join(target);

        std::fs::create_dir_all(&output_path).context("creating output path")?;

        let context = BuildContext {
            logger: self.logger.clone(),
            host_triple: self.build_host_triple.clone(),
            target_triple: self.build_target_triple.clone(),
            release: self.build_release,
            opt_level: self.build_opt_level.clone(),
            output_path,
        };

        let resolved_target: ResolvedTarget = if raw_any.is::<FileManifest>() {
            raw_any
                .downcast_mut::<FileManifest>()
                .unwrap()
                .build(&context)
        } else if raw_any.is::<PythonExecutable>() {
            raw_any
                .downcast_mut::<PythonExecutable>()
                .unwrap()
                .build(&context)
        } else if raw_any.is::<PythonEmbeddedResources>() {
            raw_any
                .downcast_mut::<PythonEmbeddedResources>()
                .unwrap()
                .build(&context)
        } else {
            Err(anyhow!("could not determine type of target"))
        }?;

        self.targets.get_mut(target).unwrap().built_target = Some(resolved_target.clone());

        Ok(resolved_target)
    }

    /// Build a target, defined optionally.
    ///
    /// This will build the default target if `target` is `None`.
    pub fn build_target(&mut self, target: Option<&str>) -> Result<ResolvedTarget> {
        let build_target = if let Some(t) = target {
            t.to_string()
        } else if let Some(t) = &self.default_target {
            t.to_string()
        } else {
            return Err(anyhow!("unable to determine target to build"));
        };

        self.build_resolved_target(&build_target)
    }

    /// Evaluate a target and run it, if possible.
    pub fn run_resolved_target(&mut self, target: &str) -> Result<()> {
        let resolved_target = self.build_resolved_target(target)?;

        resolved_target.run()
    }

    pub fn run_target(&mut self, target: Option<&str>) -> Result<()> {
        let target = if let Some(t) = target {
            t.to_string()
        } else if let Some(t) = &self.default_target {
            t.to_string()
        } else {
            return Err(anyhow!("unable to determine target to run"));
        };

        self.run_resolved_target(&target)
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

/// register_target(target, callable, depends=None, default=false)
fn starlark_register_target(
    env: &Environment,
    target: &Value,
    callable: &Value,
    depends: &Value,
    default: &Value,
    default_build_script: &Value,
) -> ValueResult {
    let target = required_str_arg("target", &target)?;
    required_type_arg("callable", "function", &callable)?;
    optional_list_arg("depends", "string", &depends)?;
    let default = required_bool_arg("default", &default)?;
    let default_build_script = required_bool_arg("default_build_script", &default_build_script)?;

    let depends = match depends.get_type() {
        "list" => depends
            .into_iter()
            .unwrap()
            .map(|x| x.to_string())
            .collect(),
        _ => Vec::new(),
    };

    let mut context = env.get("CONTEXT").expect("CONTEXT not set");

    context.downcast_apply_mut(|x: &mut EnvironmentContext| {
        x.register_target(
            target.clone(),
            callable.clone(),
            depends.clone(),
            default,
            default_build_script,
        )
    });

    Ok(Value::new(None))
}

/// resolve_target(target)
///
/// This will return a Value returned from the called function.
///
/// If the target is already resolved, its cached return value is returned
/// immediately.
///
/// If the target depends on other targets, those targets will be resolved
/// recursively before calling the target's function.
///
/// This exists as a standalone function and operates against the raw Starlark
/// `Environment` and has wonky handling of `EnvironmentContext` instances in
/// order to avoid nested mutable borrows. If we passed an
/// `&mut EnvironmentContext` around then called a Starlark function that performed
/// a `.downcast_apply_mut()` (which most of them do), we would have nested mutable
/// borrows and Rust would panic at runtime.
#[allow(clippy::ptr_arg)]
fn starlark_resolve_target(
    env: &Environment,
    call_stack: &Vec<(String, String)>,
    target: &Value,
) -> ValueResult {
    let target = required_str_arg("target", &target)?;

    let mut context = env.get("CONTEXT").expect("CONTEXT not set");

    // If we have a resolved value for this target, return it.
    if let Some(v) = context.downcast_apply(|x: &EnvironmentContext| {
        if let Some(t) = x.targets.get(&target) {
            if let Some(v) = &t.resolved_value {
                Some(v.clone())
            } else {
                None
            }
        } else {
            None
        }
    }) {
        return Ok(v);
    }

    let target_entry = context.downcast_apply(|x: &EnvironmentContext| {
        warn!(&x.logger, "resolving target {}", target);

        match &x.targets.get(&target) {
            Some(v) => Ok((*v).clone()),
            None => Err(RuntimeError {
                code: "PYOXIDIZER_BUILD",
                message: format!("target {} does not exist", target),
                label: "resolve_target()".to_string(),
            }
            .into()),
        }
    })?;

    // Resolve target dependencies.
    let mut args = Vec::new();

    for depend_target in target_entry.depends {
        let depend_target = Value::new(depend_target);
        args.push(starlark_resolve_target(env, call_stack, &depend_target)?);
    }

    let res =
        target_entry
            .callable
            .call(call_stack, env.clone(), args, HashMap::new(), None, None)?;

    // TODO consider replacing the target's callable with a new function that returns the
    // resolved value. This will ensure a target function is only ever called once.

    context.downcast_apply_mut(|x: &mut EnvironmentContext| {
        if let Some(target_entry) = x.targets.get_mut(&target) {
            target_entry.resolved_value = Some(res.clone());
        }
    });

    Ok(res)
}

/// resolve_targets()
#[allow(clippy::ptr_arg)]
fn starlark_resolve_targets(env: &Environment, call_stack: &Vec<(String, String)>) -> ValueResult {
    let context = env.get("CONTEXT").expect("CONTEXT not set");

    let targets =
        context.downcast_apply(|context: &EnvironmentContext| context.targets_to_resolve());

    println!("resolving {} targets", targets.len());
    for target in targets {
        let resolve = env.get("resolve_target").unwrap();

        resolve.call(
            call_stack,
            env.clone(),
            vec![Value::new(target)],
            HashMap::new(),
            None,
            None,
        )?;
    }

    Ok(Value::new(None))
}

/// set_build_path(path)
fn starlark_set_build_path(env: &Environment, path: &Value) -> ValueResult {
    let path = required_str_arg("path", &path)?;
    let mut context = env.get("CONTEXT").expect("CONTEXT not set");

    context
        .downcast_apply_mut(|x: &mut EnvironmentContext| x.set_build_path(&PathBuf::from(&path)))
        .or_else(|e| {
            Err(RuntimeError {
                code: "PYOXIDIZER_BUILD",
                message: e.to_string(),
                label: "set_build_path()".to_string(),
            }
            .into())
        })?;

    Ok(Value::new(None))
}

starlark_module! { global_module =>
    #[allow(clippy::ptr_arg)]
    register_target(
        env env,
        target,
        callable,
        depends=None,
        default=false,
        default_build_script=false
    ) {
        starlark_register_target(
            &env,
            &target,
            &callable,
            &depends,
            &default,
            &default_build_script,
        )
    }

    #[allow(clippy::ptr_arg)]
    resolve_target(env env, call_stack cs, target) {
        starlark_resolve_target(&env, &cs, &target)
    }

    #[allow(clippy::ptr_arg)]
    resolve_targets(env env, call_stack cs) {
        starlark_resolve_targets(&env, &cs)
    }

    #[allow(clippy::ptr_arg)]
    set_build_path(env env, path) {
        starlark_set_build_path(&env, &path)
    }
}

/// Obtain a Starlark environment for evaluating PyOxidizer configurations.
pub fn global_environment(context: &EnvironmentContext) -> Result<Environment, EnvironmentError> {
    let env = starlark::stdlib::global_environment();
    let env = global_module(env);
    let env = super::file_resource::file_resource_env(env);
    let env = super::python_distribution::python_distribution_module(env);
    let env = super::python_executable::python_executable_env(env);
    let env = super::python_interpreter_config::embedded_python_config_module(env);

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
        assert_eq!(target.to_str(), crate::project_building::HOST);
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
                x.targets.get("default").unwrap().callable.to_string(),
                "foo()".to_string()
            );
            assert_eq!(x.targets_order, vec!["default".to_string()]);
            assert_eq!(x.default_target, Some("default".to_string()));
        });
    }

    #[test]
    fn test_register_target_multiple() {
        let mut env = starlark_env();
        starlark_eval_in_env(&mut env, "def foo(): pass").unwrap();
        starlark_eval_in_env(&mut env, "def bar(): pass").unwrap();
        starlark_eval_in_env(&mut env, "register_target('foo', foo)").unwrap();
        starlark_eval_in_env(
            &mut env,
            "register_target('bar', bar, depends=['foo'], default=True)",
        )
        .unwrap();

        let context = env.get("CONTEXT").unwrap();

        context.downcast_apply(|x: &EnvironmentContext| {
            assert_eq!(x.targets.len(), 2);
            assert_eq!(x.default_target, Some("bar".to_string()));
            assert_eq!(
                &x.targets.get("bar").unwrap().depends,
                &vec!["foo".to_string()],
            );
        });
    }
}
