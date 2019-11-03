// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::env::{optional_str_arg, required_str_arg};
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

#[derive(Debug, Clone)]
pub struct BuildConfig {
    pub config: super::super::pyrepackager::config::BuildConfig,
}

impl TypedValue for BuildConfig {
    immutable!();
    any!();
    not_supported!(binop);
    not_supported!(container);
    not_supported!(function);
    not_supported!(get_hash);
    not_supported!(to_int);

    fn to_str(&self) -> String {
        format!("BuildConfig<{:#?}>", self.config)
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_type(&self) -> &'static str {
        "BuildConfig"
    }

    fn to_bool(&self) -> bool {
        true
    }

    fn compare(&self, other: &dyn TypedValue, _recursion: u32) -> Result<Ordering, ValueError> {
        default_compare(self, other)
    }
}

starlark_module! { build_config_env =>
    #[allow(non_snake_case)]
    BuildConfig(env env, application_name, build_path=None) {
        let application_name = required_str_arg("application_name", &application_name)?;
        let build_path = optional_str_arg("build_path", &build_path)?;

        let cwd = env.get("CWD").expect("CWD not set").to_string();

        let build_path = match build_path {
            Some(p) => PathBuf::from(p.replace("$ORIGIN", &cwd)),
            None => PathBuf::from(&cwd).join("build"),
        };

        let config = super::super::pyrepackager::config::BuildConfig {
            application_name,
            build_path,
        };

        Ok(Value::new(BuildConfig { config }))
    }
}

#[cfg(test)]
mod tests {
    use super::super::testutil::*;
    use super::*;

    #[test]
    fn test_default() {
        let e = starlark_nok("BuildConfig()");
        assert!(e.message.starts_with("Missing parameter application_name"));
    }

    #[test]
    fn test_application_path() {
        let v = starlark_ok("BuildConfig('foo')");
        let wanted = super::super::super::pyrepackager::config::BuildConfig {
            application_name: "foo".to_string(),
            build_path: std::env::current_dir().unwrap().join("build"),
        };

        v.downcast_apply(|x: &BuildConfig| assert_eq!(x.config, wanted));
    }

    #[test]
    fn test_build_path_simple() {
        let v = starlark_ok("BuildConfig('foo', build_path='/some/path')");
        v.downcast_apply(|x: &BuildConfig| {
            assert_eq!(x.config.build_path, PathBuf::from("/some/path"))
        });
    }

    #[test]
    fn test_build_path_origin() {
        let v = starlark_ok("BuildConfig('foo', build_path='$ORIGIN/custom')");
        v.downcast_apply(|x: &BuildConfig| {
            assert_eq!(
                x.config.build_path,
                std::env::current_dir().unwrap().join("custom")
            );
        });
    }
}
