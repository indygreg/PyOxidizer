// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    anyhow::{anyhow, Result},
    starlark::values::Value,
    std::path::PathBuf,
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

/// Describes context that a target is built in.
///
/// This is used to pass metadata to the `BuildTarget::build()` method.
pub struct BuildContext {
    /// Logger where messages can be written.
    pub logger: slog::Logger,

    /// Rust target triple for build host.
    pub host_triple: String,

    /// Rust target triple for build target.
    pub target_triple: String,

    /// Whether we are building in release mode.
    ///
    /// Debug if false.
    pub release: bool,

    /// Optimization level for Rust compiler.
    pub opt_level: String,

    /// Where generated files should be written.
    pub output_path: PathBuf,
}

/// Trait that indicates a type can be resolved as a target.
pub trait BuildTarget {
    /// Build the target, resolving it
    fn build(&mut self, context: &BuildContext) -> Result<ResolvedTarget>;
}
