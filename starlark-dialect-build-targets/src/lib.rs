// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    anyhow::{anyhow, Result},
    starlark::values::{Mutable, TypedValue, Value},
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
