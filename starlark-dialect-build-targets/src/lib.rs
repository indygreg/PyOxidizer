// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    anyhow::{anyhow, Result},
    starlark::values::Value,
    std::{
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
