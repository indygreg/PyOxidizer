// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::super::pyrepackager::config::{
    resolve_install_location, PackagingSetupPyInstall, PackagingStdlibExtensionsExplicitIncludes,
    PackagingStdlibExtensionsPolicy,
};
use super::env::{
    optional_dict_arg, optional_list_arg, required_bool_arg, required_list_arg, required_str_arg,
    required_type_arg,
};
use starlark::environment::Environment;
use starlark::values::{
    default_compare, RuntimeError, TypedValue, Value, ValueError, ValueResult,
    INCORRECT_PARAMETER_TYPE_ERROR_CODE,
};
use starlark::{
    any, immutable, not_supported, starlark_fun, starlark_module, starlark_signature,
    starlark_signature_extraction, starlark_signatures,
};
use std::any::Any;
use std::cmp::Ordering;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct SetupPyInstall {
    pub rule: PackagingSetupPyInstall,
}

impl TypedValue for SetupPyInstall {
    immutable!();
    any!();
    not_supported!(binop);
    not_supported!(container);
    not_supported!(function);
    not_supported!(get_hash);
    not_supported!(to_int);

    fn to_str(&self) -> String {
        format!("SetupPyInstall<{:#?}>", self.rule)
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_type(&self) -> &'static str {
        "SetupPyInstall"
    }

    fn to_bool(&self) -> bool {
        true
    }

    fn compare(&self, other: &dyn TypedValue, _recursion: u32) -> Result<Ordering, ValueError> {
        default_compare(self, other)
    }
}

#[derive(Debug, Clone)]
pub struct StdlibExtensionsPolicy {
    pub rule: PackagingStdlibExtensionsPolicy,
}

impl TypedValue for StdlibExtensionsPolicy {
    immutable!();
    any!();
    not_supported!(binop);
    not_supported!(container);
    not_supported!(function);
    not_supported!(get_hash);
    not_supported!(to_int);

    fn to_str(&self) -> String {
        format!("StdlibExtensionsPolicy<{:#?}>", self.rule)
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_type(&self) -> &'static str {
        "StdlibExtensionsPolicy"
    }

    fn to_bool(&self) -> bool {
        true
    }

    fn compare(&self, other: &dyn TypedValue, _recursion: u32) -> Result<Ordering, ValueError> {
        default_compare(self, other)
    }
}

#[derive(Debug, Clone)]
pub struct StdlibExtensionsExplicitIncludes {
    pub rule: PackagingStdlibExtensionsExplicitIncludes,
}

impl TypedValue for StdlibExtensionsExplicitIncludes {
    immutable!();
    any!();
    not_supported!(binop);
    not_supported!(container);
    not_supported!(function);
    not_supported!(get_hash);
    not_supported!(to_int);

    fn to_str(&self) -> String {
        format!("StdlibExtensionsExplicitIncludes<{:#?}>", self.rule)
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_type(&self) -> &'static str {
        "StdlibExtensionsExplicitIncludes"
    }

    fn to_bool(&self) -> bool {
        true
    }

    fn compare(&self, other: &dyn TypedValue, _recursion: u32) -> Result<Ordering, ValueError> {
        default_compare(self, other)
    }
}

starlark_module! { python_packaging_env =>
    #[allow(non_snake_case)]
    SetupPyInstall(
        package_path,
        extra_env=None,
        extra_global_arguments=None,
        optimize_level=0,
        include_source=true,
        install_location="embedded",
        excludes=None
    ) {
        let package_path = required_str_arg("package_path", &package_path)?;
        optional_dict_arg("extra_env", "string", "string", &extra_env)?;
        optional_list_arg("extra_global_arguments", "string", &extra_global_arguments)?;
        required_type_arg("optimize_level", "int", &optimize_level)?;
        let include_source = required_bool_arg("include_source", &include_source)?;
        let install_location = required_str_arg("install_location", &install_location)?;
        optional_list_arg("excludes", "string", &excludes)?;

        let extra_env = match extra_env.get_type() {
            "dict" => extra_env.into_iter()?.map(|key| {
                let k = key.to_string();
                let v = extra_env.at(key.clone()).unwrap().to_string();
                (k, v)
            }).collect(),
            "NoneType" => HashMap::new(),
            _ => panic!("should have validated type above"),
        };
        let extra_global_arguments = match extra_global_arguments.get_type() {
            "list" => extra_global_arguments.into_iter()?.map(|x| x.to_string()).collect(),
            "NoneType" => Vec::new(),
            _ => panic!("should have validated type above"),
        };
        let install_location = resolve_install_location(&install_location).or_else(|e| {
            Err(RuntimeError {
                code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                message: e.to_string(),
                label: e.to_string(),
            }.into())
        })?;
        let excludes = match excludes.get_type() {
            "list" => excludes.into_iter()?.map(|x| x.to_string()).collect(),
            "NoneType" => Vec::new(),
            _ => panic!("should have validated type above"),
        };

        let rule = PackagingSetupPyInstall {
            path: package_path,
            extra_env,
            extra_global_arguments,
            optimize_level: optimize_level.to_int().unwrap(),
            include_source,
            install_location,
            excludes,
        };

        Ok(Value::new(SetupPyInstall { rule }))
    }

    #[allow(non_snake_case)]
    StdlibExtensionsPolicy(policy) {
        let policy = required_str_arg("policy", &policy)?;

        let rule = PackagingStdlibExtensionsPolicy {
            policy,
        };

        Ok(Value::new(StdlibExtensionsPolicy { rule }))
    }

    #[allow(non_snake_case)]
    StdlibExtensionsExplicitIncludes(includes=None) {
        required_list_arg("includes", "string", &includes)?;

        let includes = includes.into_iter()?.map(|x| x.to_string()).collect();

        let rule = PackagingStdlibExtensionsExplicitIncludes {
            includes,
        };

        Ok(Value::new(StdlibExtensionsExplicitIncludes { rule }))
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::pyrepackager::config::InstallLocation;
    use super::super::testutil::*;
    use super::*;

    #[test]
    fn test_setup_py_install_default() {
        let err = starlark_nok("SetupPyInstall()");
        assert!(err.message.starts_with("Missing parameter package_path"));
    }

    #[test]
    fn test_setup_py_install_minimal() {
        let v = starlark_ok("SetupPyInstall('foo')");
        let wanted = PackagingSetupPyInstall {
            path: "foo".to_string(),
            extra_env: HashMap::new(),
            extra_global_arguments: Vec::new(),
            optimize_level: 0,
            include_source: true,
            install_location: InstallLocation::Embedded,
            excludes: Vec::new(),
        };

        v.downcast_apply(|x: &SetupPyInstall| assert_eq!(x.rule, wanted));
    }

    #[test]
    fn test_setup_py_install_extra_global_arguments() {
        let v = starlark_ok("SetupPyInstall('foo', extra_global_arguments=['arg1', 'arg2'])");
        v.downcast_apply(|x: &SetupPyInstall| {
            assert_eq!(x.rule.extra_global_arguments, vec!["arg1", "arg2"])
        });
    }

    #[test]
    fn test_stdlib_extensions_policy_default() {
        let err = starlark_nok("StdlibExtensionsPolicy()");
        assert!(err.message.starts_with("Missing parameter policy"));
    }

    #[test]
    fn test_stdlib_extensions_policy_policy() {
        let v = starlark_ok("StdlibExtensionsPolicy('foo')");
        let wanted = PackagingStdlibExtensionsPolicy {
            policy: "foo".to_string(),
        };
        v.downcast_apply(|x: &StdlibExtensionsPolicy| assert_eq!(x.rule, wanted));
    }

    #[test]
    fn test_stdlib_extensions_explicit_includes_default() {
        let err = starlark_nok("StdlibExtensionsExplicitIncludes()");
        assert_eq!(
            err.message,
            "function expects a list for includes; got type NoneType"
        );
    }

    #[test]
    fn test_stdlib_extensions_explicit_includes_includes() {
        let v = starlark_ok("StdlibExtensionsExplicitIncludes(['foo', 'bar'])");
        let wanted = PackagingStdlibExtensionsExplicitIncludes {
            includes: vec!["foo".to_string(), "bar".to_string()],
        };
        v.downcast_apply(|x: &StdlibExtensionsExplicitIncludes| assert_eq!(x.rule, wanted));
    }
}
