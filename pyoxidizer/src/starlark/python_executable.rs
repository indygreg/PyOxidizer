// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use starlark::environment::Environment;
use starlark::values::{default_compare, TypedValue, Value, ValueError, ValueResult};
use starlark::{
    any, immutable, not_supported, starlark_fun, starlark_module, starlark_signature,
    starlark_signature_extraction, starlark_signatures,
};
use std::any::Any;
use std::cmp::Ordering;
use std::collections::HashMap;

use super::embedded_python_config::EmbeddedPythonConfig;
use super::env::{required_str_arg, required_type_arg};
use super::python_distribution::PythonDistribution;
use super::python_resource::PythonEmbeddedResources;
use super::python_run_mode::PythonRunMode;
use crate::app_packaging::environment::EnvironmentContext;
use crate::py_packaging::binary::PreBuiltPythonExecutable;
use crate::py_packaging::distribution::ExtensionModuleFilter;

impl TypedValue for PreBuiltPythonExecutable {
    immutable!();
    any!();
    not_supported!(binop, container, function, get_hash, to_int);

    fn to_str(&self) -> String {
        "PythonExecutable<>".to_string()
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_type(&self) -> &'static str {
        "PythonExecutable"
    }

    fn to_bool(&self) -> bool {
        true
    }

    fn compare(&self, other: &dyn TypedValue, _recursion: u32) -> Result<Ordering, ValueError> {
        default_compare(self, other)
    }
}

starlark_module! { python_executable_env =>
    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable(env env, name, distribution, resources, config, run_mode) {
        let name = required_str_arg("name", &name)?;
        required_type_arg("distribution", "PythonDistribution", &distribution)?;
        required_type_arg("resources", "PythonEmbeddedResources", &resources)?;
        required_type_arg("config", "EmbeddedPythonConfig", &config)?;
        required_type_arg("run_mode", "PythonRunMode", &run_mode)?;

        let context = env.get("CONTEXT").expect("CONTEXT not defined");
        let logger = context.downcast_apply(|x: &EnvironmentContext| x.logger.clone());

        let mut distribution = distribution.clone();

        let distribution = distribution.downcast_apply_mut(|dist: &mut PythonDistribution| {
            dist.ensure_distribution_resolved(&logger);

            dist.distribution.clone().unwrap().clone()
        });

        let mut resources = resources.downcast_apply(|r: &PythonEmbeddedResources| r.embedded.clone());
        let config = config.downcast_apply(|c: &EmbeddedPythonConfig| c.config.clone());
        let run_mode = run_mode.downcast_apply(|m: &PythonRunMode| m.run_mode.clone());

        // Always ensure minimal extension modules are present, otherwise we get
        // missing symbol errors at link time.
        for ext in distribution.filter_extension_modules(&logger, &ExtensionModuleFilter::Minimal, None) {
            resources.add_extension_module(&ext);
        }

        Ok(Value::new(PreBuiltPythonExecutable {
            name,
            distribution,
            resources,
            config,
            run_mode
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::super::testutil::*;
    use super::*;

    #[test]
    fn test_no_args() {
        let err = starlark_nok("PythonExecutable()");
        assert!(err.message.starts_with("Missing parameter name"));
    }

    #[test]
    fn test_default_values() {
        let mut env = starlark_env();

        starlark_eval_in_env(&mut env, "dist = default_python_distribution()").unwrap();
        starlark_eval_in_env(&mut env, "resources = PythonEmbeddedResources()").unwrap();
        starlark_eval_in_env(&mut env, "run_mode = python_run_mode_noop()").unwrap();
        starlark_eval_in_env(&mut env, "config = EmbeddedPythonConfig()").unwrap();

        let exe = starlark_eval_in_env(
            &mut env,
            "PythonExecutable('testapp', dist, resources, config, run_mode)",
        )
        .unwrap();

        assert_eq!(exe.get_type(), "PythonExecutable");

        exe.downcast_apply(|exe: &PreBuiltPythonExecutable| {
            assert_eq!(exe.run_mode, crate::py_packaging::config::RunMode::Noop);
        });
    }
}
