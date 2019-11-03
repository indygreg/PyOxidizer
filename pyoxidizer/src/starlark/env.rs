// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use starlark::environment::{Environment, EnvironmentError};
use starlark::values::{default_compare, TypedValue, Value, ValueError, ValueResult};
use starlark::{any, immutable, not_supported};
use std::any::Any;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::PathBuf;

/// Holds state for evaluating a starlark environment.
#[derive(Debug, Clone)]
pub struct EnvironmentContext {
    /// Directory the environment should be evaluated from.
    ///
    /// Typically used to resolve filenames.
    pub cwd: PathBuf,
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

/// Obtain a Starlark environment for evaluating PyOxidizer configurations.
pub fn global_environment(context: &EnvironmentContext) -> Result<Environment, EnvironmentError> {
    let env = starlark::stdlib::global_environment();

    env.set("CONTEXT", Value::new(context.clone()))?;

    env.set("CWD", Value::from(context.cwd.display().to_string()))?;

    Ok(env)
}
