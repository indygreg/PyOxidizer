// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    super::{
        file_resource::FileManifestValue,
        python_embedded_resources::PythonEmbeddedResources,
        python_executable::PythonExecutable,
        target::{BuildContext, BuildTarget, ResolvedTarget},
        util::{optional_list_arg, required_bool_arg, required_str_arg, required_type_arg},
    },
    crate::py_packaging::distribution::DistributionCache,
    anyhow::{anyhow, Context, Result},
    linked_hash_map::LinkedHashMap,
    path_dedot::ParseDot,
    slog::warn,
    starlark::{
        environment::{Environment, EnvironmentError, TypeValues},
        eval::call_stack::CallStack,
        values::{
            error::{RuntimeError, ValueError},
            none::NoneType,
            {Mutable, TypedValue, Value, ValueResult},
        },
        {
            starlark_fun, starlark_module, starlark_parse_param_type, starlark_signature,
            starlark_signature_extraction, starlark_signatures,
        },
    },
    std::{
        collections::BTreeMap,
        path::{Path, PathBuf},
        sync::Arc,
    },
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

    /// Cache of ready-to-clone Python distribution objects.
    ///
    /// This exists because constructing a new instance can take a
    /// few seconds in debug builds. And this adds up, especially in tests!
    pub distribution_cache: Arc<DistributionCache>,

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
        distribution_cache: Option<Arc<DistributionCache>>,
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

        let python_distributions_path = build_path.join("python_distributions");
        let distribution_cache = distribution_cache
            .unwrap_or_else(|| Arc::new(DistributionCache::new(Some(&python_distributions_path))));

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
            python_distributions_path: python_distributions_path.clone(),
            distribution_cache,
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
        .parse_dot()?
        .to_path_buf();

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

        // TODO surely this can use dynamic dispatch.
        let resolved_target: ResolvedTarget = match resolved_value.get_type() {
            "FileManifest" => resolved_value
                .downcast_mut::<FileManifestValue>()
                .map_err(|_| anyhow!("object isn't mutable"))?
                .ok_or_else(|| anyhow!("invalid cast"))?
                .build(&context),
            "PythonExecutable" => resolved_value
                .downcast_mut::<PythonExecutable>()
                .map_err(|_| anyhow!("object isn't mutable"))?
                .ok_or_else(|| anyhow!("invalid cast"))?
                .build(&context),
            "PythonEmbeddedResources" => resolved_value
                .downcast_mut::<PythonEmbeddedResources>()
                .map_err(|_| anyhow!("object isn't mutable"))?
                .ok_or_else(|| anyhow!("invalid cast"))?
                .build(&context),
            _ => Err(anyhow!("could not determine type of target")),
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
    type Holder = Mutable<EnvironmentContext>;
    const TYPE: &'static str = "EnvironmentContext";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }
}

/// Starlark type holding context for PyOxidizer.
pub struct PyOxidizerContext {}

impl Default for PyOxidizerContext {
    fn default() -> Self {
        PyOxidizerContext {}
    }
}

impl TypedValue for PyOxidizerContext {
    type Holder = Mutable<PyOxidizerContext>;
    const TYPE: &'static str = "PyOxidizer";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }
}

/// Obtain the EnvironmentContext for the Starlark execution environment.
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

/// print(*args)
fn starlark_print(type_values: &TypeValues, args: &Vec<Value>) -> ValueResult {
    let raw_context = get_context(type_values)?;
    let context = raw_context
        .downcast_ref::<EnvironmentContext>()
        .ok_or(ValueError::IncorrectParameterType)?;

    let mut parts = Vec::new();
    let mut first = true;
    for arg in args {
        if !first {
            parts.push(" ".to_string());
        }
        first = false;
        parts.push(arg.to_string());
    }

    warn!(&context.logger, "{}", parts.join(""));

    Ok(Value::new(NoneType::None))
}

/// register_target(target, callable, depends=None, default=false)
fn starlark_register_target(
    type_values: &TypeValues,
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
        "list" => depends.iter()?.iter().map(|x| x.to_string()).collect(),
        _ => Vec::new(),
    };

    let raw_context = get_context(type_values)?;
    let mut context = raw_context
        .downcast_mut::<EnvironmentContext>()?
        .ok_or(ValueError::IncorrectParameterType)?;

    context.register_target(
        target.clone(),
        callable.clone(),
        depends.clone(),
        default,
        default_build_script,
    );

    Ok(Value::new(NoneType::None))
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
/// a `.downcast_mut()` (which most of them do), we would have nested mutable
/// borrows and Rust would panic at runtime.
#[allow(clippy::ptr_arg)]
fn starlark_resolve_target(
    type_values: &TypeValues,
    call_stack: &mut CallStack,
    target: &Value,
) -> ValueResult {
    let target = required_str_arg("target", &target)?;

    // We need the EnvironmentContext borrow to get dropped before calling
    // into Starlark or we can get double borrows. Hence the block here.
    let target_entry = {
        let raw_context = get_context(type_values)?;
        let context = raw_context
            .downcast_ref::<EnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        // If we have a resolved value for this target, return it.
        if let Some(v) = if let Some(t) = context.targets.get(&target) {
            if let Some(v) = &t.resolved_value {
                Some(v.clone())
            } else {
                None
            }
        } else {
            None
        } {
            return Ok(v);
        }

        warn!(&context.logger, "resolving target {}", target);

        match &context.targets.get(&target) {
            Some(v) => Ok((*v).clone()),
            None => Err(ValueError::from(RuntimeError {
                code: "PYOXIDIZER_BUILD",
                message: format!("target {} does not exist", target),
                label: "resolve_target()".to_string(),
            })),
        }?
    };

    // Resolve target dependencies.
    let mut args = Vec::new();

    for depend_target in target_entry.depends {
        let depend_target = Value::new(depend_target);
        args.push(starlark_resolve_target(
            type_values,
            call_stack,
            &depend_target,
        )?);
    }

    let res = target_entry.callable.call(
        call_stack,
        type_values,
        args,
        LinkedHashMap::new(),
        None,
        None,
    )?;

    // TODO consider replacing the target's callable with a new function that returns the
    // resolved value. This will ensure a target function is only ever called once.

    // We can't obtain a mutable reference to the context above because it
    // would create multiple borrows.
    let raw_context = get_context(type_values)?;
    let mut context = raw_context
        .downcast_mut::<EnvironmentContext>()?
        .ok_or(ValueError::IncorrectParameterType)?;

    if let Some(target_entry) = context.targets.get_mut(&target) {
        target_entry.resolved_value = Some(res.clone());
    }

    Ok(res)
}

/// resolve_targets()
#[allow(clippy::ptr_arg)]
fn starlark_resolve_targets(type_values: &TypeValues, call_stack: &mut CallStack) -> ValueResult {
    let resolve_target_fn = type_values
        .get_type_value(&Value::new(PyOxidizerContext::default()), "resolve_target")
        .ok_or_else(|| {
            ValueError::from(RuntimeError {
                code: "PYOXIDIZER_BUILD",
                message: "could not find resolve_target() function (this should never happen)"
                    .to_string(),
                label: "resolve_targets()".to_string(),
            })
        })?;

    // Limit lifetime of EnvironmentContext borrow to prevent double borrows
    // due to Starlark calls below.
    let targets = {
        let raw_context = get_context(type_values)?;
        let context = raw_context
            .downcast_ref::<EnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        context.targets_to_resolve()
    };

    println!("resolving {} targets", targets.len());
    for target in targets {
        resolve_target_fn.call(
            call_stack,
            type_values,
            vec![Value::new(target)],
            LinkedHashMap::new(),
            None,
            None,
        )?;
    }

    Ok(Value::new(NoneType::None))
}

/// set_build_path(path)
fn starlark_set_build_path(type_values: &TypeValues, path: &Value) -> ValueResult {
    let path = required_str_arg("path", &path)?;

    let raw_context = get_context(type_values)?;
    let mut context = raw_context
        .downcast_mut::<EnvironmentContext>()?
        .ok_or(ValueError::IncorrectParameterType)?;

    context.set_build_path(&PathBuf::from(&path)).map_err(|e| {
        ValueError::from(RuntimeError {
            code: "PYOXIDIZER_BUILD",
            message: e.to_string(),
            label: "set_build_path()".to_string(),
        })
    })?;

    Ok(Value::new(NoneType::None))
}

starlark_module! { global_module =>
    print(env env, *args) {
        starlark_print(&env, &args)
    }

    #[allow(clippy::ptr_arg)]
    register_target(
        env env,
        target,
        callable,
        depends=NoneType::None,
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
        starlark_resolve_target(&env, cs, &target)
    }

    #[allow(clippy::ptr_arg)]
    resolve_targets(env env, call_stack cs) {
        starlark_resolve_targets(&env, cs)
    }

    #[allow(clippy::ptr_arg)]
    set_build_path(env env, path) {
        starlark_set_build_path(&env, &path)
    }
}

/// Obtain a Starlark environment for evaluating PyOxidizer configurations.
pub fn global_environment(
    context: &EnvironmentContext,
) -> Result<(Environment, TypeValues), EnvironmentError> {
    let (mut env, mut type_values) = starlark::stdlib::global_environment();
    global_module(&mut env, &mut type_values);
    super::file_resource::file_resource_env(&mut env, &mut type_values);
    super::python_distribution::python_distribution_module(&mut env, &mut type_values);
    super::python_executable::python_executable_env(&mut env, &mut type_values);
    super::python_packaging_policy::python_packaging_policy_module(&mut env, &mut type_values);

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

    // We alias various globals as PyOxidizer.* attributes so they are
    // available via the type object API. This is a bit hacky. But it allows
    // Rust code with only access to the TypeValues dictionary to retrieve
    // these globals.
    for f in &[
        "register_target",
        "resolve_target",
        "resolve_targets",
        "set_build_path",
        "CONTEXT",
        "CWD",
        "CONFIG_PATH",
        "BUILD_TARGET_TRIPLE",
    ] {
        type_values.add_type_value(PyOxidizerContext::TYPE, f, env.get(f)?);
    }

    Ok((env, type_values))
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
    fn test_print() {
        starlark_ok("print('hello, world')");
    }

    #[test]
    fn test_register_target() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;
        env.eval("def foo(): pass")?;
        env.eval("register_target('default', foo)")?;

        let raw_context = env.eval("CONTEXT")?;
        let context = raw_context
            .downcast_ref::<EnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)
            .unwrap();

        assert_eq!(context.targets.len(), 1);
        assert!(context.targets.contains_key("default"));
        assert_eq!(
            context.targets.get("default").unwrap().callable.to_string(),
            "foo()".to_string()
        );
        assert_eq!(context.targets_order, vec!["default".to_string()]);
        assert_eq!(context.default_target, Some("default".to_string()));

        Ok(())
    }

    #[test]
    fn test_register_target_multiple() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;
        env.eval("def foo(): pass")?;
        env.eval("def bar(): pass")?;
        env.eval("register_target('foo', foo)")?;
        env.eval("register_target('bar', bar, depends=['foo'], default=True)")?;
        let raw_context = env.eval("CONTEXT")?;
        let context = raw_context
            .downcast_ref::<EnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)
            .unwrap();

        assert_eq!(context.targets.len(), 2);
        assert_eq!(context.default_target, Some("bar".to_string()));
        assert_eq!(
            &context.targets.get("bar").unwrap().depends,
            &vec!["foo".to_string()],
        );

        Ok(())
    }
}
