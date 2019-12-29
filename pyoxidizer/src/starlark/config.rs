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
use std::path::PathBuf;

use super::embedded_python_config::EmbeddedPythonConfig;
use super::env::{required_str_arg, required_type_arg};
use super::python_distribution::PythonDistribution;
use super::python_run_mode::PythonRunMode;
use crate::app_packaging::config::{BuildConfig as ConfigBuildConfig, Config as ConfigConfig};
use crate::app_packaging::environment::EnvironmentContext;
use crate::py_packaging::config::{EmbeddedPythonConfig as ConfigEmbeddedPythonConfig, RunMode};
use crate::py_packaging::distribution::PythonDistributionLocation;

#[derive(Debug, Clone)]
pub struct Config {
    pub config: ConfigConfig,
}

impl TypedValue for Config {
    immutable!();
    any!();
    not_supported!(binop);
    not_supported!(container);
    not_supported!(function);
    not_supported!(get_hash);
    not_supported!(to_int);

    fn to_str(&self) -> String {
        format!("Config<{:#?}>", self.config)
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_type(&self) -> &'static str {
        "Config"
    }

    fn to_bool(&self) -> bool {
        true
    }

    fn compare(&self, other: &dyn TypedValue, _recursion: u32) -> Result<Ordering, ValueError> {
        default_compare(self, other)
    }
}

starlark_module! { config_env =>
    #[allow(non_snake_case, clippy::ptr_arg)]
    Config(
        env env,
        application_name,
        embedded_python_config=None,
        python_distribution=None,
        python_run_mode=None
    ) {
        let application_name = required_str_arg("application_name", &application_name)?;
        required_type_arg("embedded_python_config", "EmbeddedPythonConfig", &embedded_python_config)?;
        required_type_arg("python_distribution", "PythonDistribution", &python_distribution)?;
        required_type_arg("python_run_mode", "PythonRunMode", &python_run_mode)?;

        let context = env.get("CONTEXT").expect("CONTEXT not set");

        let build_path = context.downcast_apply(|x: &EnvironmentContext| x.build_path.clone());

        let build_config = ConfigBuildConfig {
            application_name,
            build_path,
        };

        let embedded_python_config = embedded_python_config.downcast_apply(|x: &EmbeddedPythonConfig| -> ConfigEmbeddedPythonConfig {
            x.config.clone()
        });
        let python_distribution = python_distribution.downcast_apply(|x: &PythonDistribution| -> PythonDistributionLocation {
            x.source.clone()
        });

        let run = python_run_mode.downcast_apply(|x: &PythonRunMode| -> RunMode {
            x.run_mode.clone()
        });

        let config_path = env.get("CONFIG_PATH").expect("CONFIG_PATH should always be available").to_string();

        let config = ConfigConfig {
            config_path: PathBuf::from(config_path),
            build_config,
            embedded_python_config,
            python_distribution,
            run,
            distributions: Vec::new(),
        };

        let v = Value::new(Config { config });

        env.get_parent().unwrap().set("CONFIG", v.clone()).unwrap();

        Ok(v)
    }
}

#[cfg(test)]
mod tests {
    use super::super::testutil::*;
    use indoc::indoc;

    #[test]
    fn test_config_default() {
        let err = starlark_nok("Config()");
        assert!(err
            .message
            .starts_with("Missing parameter application_name"));
    }

    #[test]
    fn test_config_basic() {
        let content = indoc!(
            r#"
            Config(
                application_name='myapp',
                embedded_python_config=EmbeddedPythonConfig(),
                python_distribution=default_python_distribution(),
                python_run_mode=python_run_mode_repl(),
            )
        "#
        );

        let v = starlark_ok(content);
        assert_eq!(v.get_type(), "Config");
    }
}
