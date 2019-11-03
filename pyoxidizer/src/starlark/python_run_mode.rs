// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::super::pyrepackager::config::RunMode;
use super::env::required_str_arg;
use starlark::environment::Environment;
use starlark::values::{default_compare, TypedValue, Value, ValueError, ValueResult};
use starlark::{
    any, immutable, not_supported, starlark_fun, starlark_module, starlark_signature,
    starlark_signature_extraction, starlark_signatures,
};
use std::any::Any;
use std::cmp::Ordering;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct PythonRunMode {
    pub run_mode: RunMode,
}

impl TypedValue for PythonRunMode {
    immutable!();
    any!();
    not_supported!(binop);
    not_supported!(container);
    not_supported!(function);
    not_supported!(get_hash);
    not_supported!(to_int);

    fn to_str(&self) -> String {
        format!("PythonRunMode<{:#?}>", self.run_mode)
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_type(&self) -> &'static str {
        "PythonRunMode"
    }

    fn to_bool(&self) -> bool {
        true
    }

    fn compare(&self, other: &dyn TypedValue, _recursion: u32) -> Result<Ordering, ValueError> {
        default_compare(self, other)
    }
}

starlark_module! { python_run_mode_env =>
    python_run_mode_noop(call_stack _stack) {
        Ok(Value::new(PythonRunMode { run_mode: RunMode::Noop }))
    }

    python_run_mode_repl(call_stack _stack) {
        Ok(Value::new(PythonRunMode { run_mode: RunMode::Repl }))
    }

    python_run_mode_module(module) {
        let module = required_str_arg("module", &module)?;

        Ok(Value::new(PythonRunMode { run_mode: RunMode::Module { module }}))
    }

    python_run_mode_eval(code) {
        let code = required_str_arg("code", &code)?;

        Ok(Value::new(PythonRunMode { run_mode: RunMode::Eval { code }}))
    }
}

#[cfg(test)]
mod tests {
    use super::super::testutil::*;
    use super::*;

    #[test]
    fn test_run_mode_noop() {
        let v = starlark_ok("python_run_mode_noop()");
        v.downcast_apply(|x: &PythonRunMode| assert_eq!(x.run_mode, RunMode::Noop));
    }

    #[test]
    fn test_run_mode_repl() {
        let v = starlark_ok("python_run_mode_repl()");
        v.downcast_apply(|x: &PythonRunMode| assert_eq!(x.run_mode, RunMode::Repl));
    }

    #[test]
    fn test_run_mode_module() {
        let v = starlark_ok("python_run_mode_module('mod')");
        v.downcast_apply(|x: &PythonRunMode| {
            assert_eq!(
                x.run_mode,
                RunMode::Module {
                    module: "mod".to_string()
                }
            );
        })
    }

    #[test]
    fn test_run_mode_eval() {
        let v = starlark_ok("python_run_mode_eval('code')");
        v.downcast_apply(|x: &PythonRunMode| {
            assert_eq!(
                x.run_mode,
                RunMode::Eval {
                    code: "code".to_string()
                }
            );
        });
    }
}
