// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    anyhow::{anyhow, Result},
    starlark::{
        environment::TypeValues,
        values::{
            error::{RuntimeError, ValueError, INCORRECT_PARAMETER_TYPE_ERROR_CODE},
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
        fmt::Formatter,
        path::{Path, PathBuf},
    },
};

/// How a resolved target can be run.
#[derive(Debug, Clone)]
pub enum RunMode {
    /// Target cannot be run.
    None,
    /// Target is run by executing a path.
    Path { path: PathBuf },
}

/// Represents a resolved target.
#[derive(Debug, Clone)]
pub struct ResolvedTarget {
    /// How the built target can be run.
    pub run_mode: RunMode,

    /// Where build artifacts are stored on the filesystem.
    pub output_path: PathBuf,
}

impl ResolvedTarget {
    pub fn run(&self) -> Result<()> {
        match &self.run_mode {
            RunMode::None => Ok(()),
            RunMode::Path { path } => {
                let status = std::process::Command::new(&path)
                    .current_dir(&path.parent().unwrap())
                    .status()?;

                if status.success() {
                    Ok(())
                } else {
                    Err(anyhow!("cargo run failed"))
                }
            }
        }
    }
}

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

#[derive(Debug)]
pub enum GetStateError {
    /// The requested key is not valid for this context.
    InvalidKey(String),
    /// The type of the config key being requested is wrong.
    WrongType(String),
    /// There was an error resolving this config key.
    Resolve((String, String)),
}

impl std::fmt::Display for GetStateError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidKey(key) => write!(f, "the requested key '{}' is not valid", key),
            Self::WrongType(key) => write!(f, "requested the wrong type of key '{}'", key),
            Self::Resolve((key, message)) => {
                write!(f, "failed resolving key '{}': {}", key, message)
            }
        }
    }
}

impl std::error::Error for GetStateError {}

/// Describes a generic context used when a specific target is built.
pub trait BuildContext {
    /// Obtain a logger that can be used to log events.
    fn logger(&self) -> &slog::Logger;

    /// Obtain the string value of a state key.
    fn get_state_string(&self, key: &str) -> Result<&str, GetStateError>;

    /// Obtain the bool value of a state key.
    fn get_state_bool(&self, key: &str) -> Result<bool, GetStateError>;

    /// Obtain the path value of a state key.
    fn get_state_path(&self, key: &str) -> Result<&Path, GetStateError>;
}

/// Trait that indicates a type can be resolved as a target.
pub trait BuildTarget {
    /// Build the target, resolving it
    fn build(&mut self, context: &dyn BuildContext) -> Result<ResolvedTarget>;
}

/// Holds execution context for a Starlark environment.
#[derive(Clone, Debug)]
pub struct EnvironmentContext {
    logger: slog::Logger,

    /// Registered targets.
    ///
    /// A target is a name and a Starlark callable.
    targets: BTreeMap<String, Target>,

    /// Order targets are registered in.
    targets_order: Vec<String>,

    /// Name of the default target.
    default_target: Option<String>,

    /// List of targets to resolve.
    resolve_targets: Option<Vec<String>>,

    // TODO figure out a generic way to express build script mode.
    /// Name of default target to resolve in build script mode.
    pub default_build_script_target: Option<String>,

    /// Whether we are operating in Rust build script mode.
    ///
    /// This will change the default target to resolve.
    pub build_script_mode: bool,
}

impl EnvironmentContext {
    pub fn new(logger: &slog::Logger) -> Self {
        Self {
            logger: logger.clone(),
            targets: BTreeMap::new(),
            targets_order: vec![],
            default_target: None,
            resolve_targets: None,
            default_build_script_target: None,
            build_script_mode: false,
        }
    }

    /// Obtain a logger for this instance.
    pub fn logger(&self) -> &slog::Logger {
        &self.logger
    }

    /// Obtain all registered targets.
    pub fn targets(&self) -> &BTreeMap<String, Target> {
        &self.targets
    }

    /// Obtain the default target to resolve.
    pub fn default_target(&self) -> Option<&str> {
        self.default_target.as_deref()
    }

    /// Obtain a named target.
    pub fn get_target(&self, target: &str) -> Option<&Target> {
        self.targets.get(target)
    }

    /// Obtain a mutable named target.
    pub fn get_target_mut(&mut self, target: &str) -> Option<&mut Target> {
        self.targets.get_mut(target)
    }

    /// Set the list of targets to resolve.
    pub fn set_resolve_targets(&mut self, targets: Vec<String>) {
        self.resolve_targets = Some(targets);
    }

    /// Obtain the order that targets were registered in.
    pub fn targets_order(&self) -> &Vec<String> {
        &self.targets_order
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
}

impl TypedValue for EnvironmentContext {
    type Holder = Mutable<EnvironmentContext>;
    const TYPE: &'static str = "EnvironmentContext";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }
}

#[derive(Default)]
struct PlaceholderContext {}

impl TypedValue for PlaceholderContext {
    type Holder = Mutable<PlaceholderContext>;
    const TYPE: &'static str = "PlaceholderContext";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }
}

pub fn required_type_arg(arg_name: &str, arg_type: &str, value: &Value) -> Result<(), ValueError> {
    let t = value.get_type();
    if t == arg_type {
        Ok(())
    } else {
        Err(ValueError::from(RuntimeError {
            code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
            message: format!(
                "function expects a {} for {}; got type {}",
                arg_type, arg_name, t
            ),
            label: format!("expect type {}; got {}", arg_type, t),
        }))
    }
}

pub fn optional_type_arg(arg_name: &str, arg_type: &str, value: &Value) -> Result<(), ValueError> {
    match value.get_type() {
        "NoneType" => Ok(()),
        _ => required_type_arg(arg_name, arg_type, value),
    }
}

pub fn required_str_arg(name: &str, value: &Value) -> Result<String, ValueError> {
    match value.get_type() {
        "string" => Ok(value.to_str()),
        t => Err(ValueError::from(RuntimeError {
            code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
            message: format!("function expects a string for {}; got type {}", name, t),
            label: format!("expected type string; got {}", t),
        })),
    }
}

pub fn optional_str_arg(name: &str, value: &Value) -> Result<Option<String>, ValueError> {
    match value.get_type() {
        "NoneType" => Ok(None),
        "string" => Ok(Some(value.to_str())),
        t => Err(ValueError::from(RuntimeError {
            code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
            message: format!(
                "function expects an optional string for {}; got type {}",
                name, t
            ),
            label: format!("expected type string; got {}", t),
        })),
    }
}

pub fn required_bool_arg(name: &str, value: &Value) -> Result<bool, ValueError> {
    match value.get_type() {
        "bool" => Ok(value.to_bool()),
        t => Err(ValueError::from(RuntimeError {
            code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
            message: format!(
                "function expects an required bool for {}; got type {}",
                name, t
            ),
            label: format!("expected type bool; got {}", t),
        })),
    }
}

pub fn optional_bool_arg(name: &str, value: &Value) -> Result<Option<bool>, ValueError> {
    match value.get_type() {
        "NoneType" => Ok(None),
        "bool" => Ok(Some(value.to_bool())),
        t => Err(ValueError::from(RuntimeError {
            code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
            message: format!(
                "function expects an optional bool for {}; got type {}",
                name, t
            ),
            label: format!("expected type bool; got {}", t),
        })),
    }
}

pub fn optional_int_arg(name: &str, value: &Value) -> Result<Option<i64>, ValueError> {
    match value.get_type() {
        "NoneType" => Ok(None),
        "int" => Ok(Some(value.to_int()?)),
        t => Err(ValueError::from(RuntimeError {
            code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
            message: format!(
                "function expected an optional int for {}; got type {}",
                name, t
            ),
            label: format!("expected type int; got {}", t),
        })),
    }
}

pub fn required_list_arg(
    arg_name: &str,
    value_type: &str,
    value: &Value,
) -> Result<(), ValueError> {
    match value.get_type() {
        "list" => {
            for v in &value.iter()? {
                if v.get_type() == value_type {
                    Ok(())
                } else {
                    Err(ValueError::from(RuntimeError {
                        code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                        message: format!(
                            "list {} expects values of type {}; got {}",
                            arg_name,
                            value_type,
                            v.get_type()
                        ),
                        label: format!("expected type {}; got {}", value_type, v.get_type()),
                    }))
                }?;
            }
            Ok(())
        }
        t => Err(ValueError::from(RuntimeError {
            code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
            message: format!("function expects a list for {}; got type {}", arg_name, t),
            label: format!("expected type list; got {}", t),
        })),
    }
}

pub fn optional_list_arg(
    arg_name: &str,
    value_type: &str,
    value: &Value,
) -> Result<(), ValueError> {
    if value.get_type() == "NoneType" {
        return Ok(());
    }

    required_list_arg(arg_name, value_type, value)
}

pub fn required_dict_arg(
    arg_name: &str,
    key_type: &str,
    value_type: &str,
    value: &Value,
) -> Result<(), ValueError> {
    match value.get_type() {
        "dict" => {
            for k in &value.iter()? {
                if k.get_type() == key_type {
                    Ok(())
                } else {
                    Err(ValueError::from(RuntimeError {
                        code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                        message: format!(
                            "dict {} expects keys of type {}; got {}",
                            arg_name,
                            key_type,
                            k.get_type()
                        ),
                        label: format!("expected type {}; got {}", key_type, k.get_type()),
                    }))
                }?;

                let v = value.at(k.clone())?;

                if v.get_type() == value_type {
                    Ok(())
                } else {
                    Err(ValueError::from(RuntimeError {
                        code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                        message: format!(
                            "dict {} expects values of type {}; got {}",
                            arg_name,
                            value_type,
                            v.get_type(),
                        ),
                        label: format!("expected type {}; got {}", value_type, v.get_type()),
                    }))
                }?;
            }
            Ok(())
        }
        t => Err(ValueError::from(RuntimeError {
            code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
            message: format!("function expects a dict for {}; got type {}", arg_name, t),
            label: format!("expected type dict; got {}", t),
        })),
    }
}

pub fn optional_dict_arg(
    arg_name: &str,
    key_type: &str,
    value_type: &str,
    value: &Value,
) -> Result<(), ValueError> {
    if value.get_type() == "NoneType" {
        return Ok(());
    }

    required_dict_arg(arg_name, key_type, value_type, value)
}

const ENVIRONMENT_CONTEXT_SYMBOL: &str = "BUILD_CONTEXT";

/// Obtain the `Value` holding the `EnvironmentContext` for a Starlark environment.
///
/// This is a helper function. The returned `Value` needs to be casted
/// to have much value.
pub fn get_context_value(type_values: &TypeValues) -> ValueResult {
    type_values
        .get_type_value(
            &Value::new(PlaceholderContext::default()),
            ENVIRONMENT_CONTEXT_SYMBOL,
        )
        .ok_or_else(|| {
            ValueError::from(RuntimeError {
                code: "STARLARK_BUILD_CONTEXT",
                message: "Unable to resolve context (this should never happen)".to_string(),
                label: "".to_string(),
            })
        })
}

/// register_target(target, callable, depends=None, default=false)
fn starlark_register_target(
    type_values: &TypeValues,
    target: String,
    callable: Value,
    depends: Value,
    default: bool,
    default_build_script: bool,
) -> ValueResult {
    required_type_arg("callable", "function", &callable)?;
    optional_list_arg("depends", "string", &depends)?;

    let depends = match depends.get_type() {
        "list" => depends.iter()?.iter().map(|x| x.to_string()).collect(),
        _ => Vec::new(),
    };

    let raw_context = get_context_value(type_values)?;
    let mut context = raw_context
        .downcast_mut::<EnvironmentContext>()?
        .ok_or(ValueError::IncorrectParameterType)?;

    context.register_target(target, callable, depends, default, default_build_script);

    Ok(Value::new(NoneType::None))
}

starlark_module! { build_targets_module =>
    register_target(
        env env,
        target: String,
        callable,
        depends = NoneType::None,
        default: bool = false,
        default_build_script: bool = false
    ) {
        starlark_register_target(env, target, callable, depends, default, default_build_script)
    }
}
