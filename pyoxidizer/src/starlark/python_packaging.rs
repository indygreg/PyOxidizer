// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

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

use super::env::{
    optional_dict_arg, optional_list_arg, optional_str_arg, required_bool_arg, required_list_arg,
    required_str_arg, required_type_arg,
};
use crate::app_packaging::config::{
    resolve_install_location, PackagingFilterInclude, PackagingPackageRoot,
    PackagingPipInstallSimple, PackagingPipRequirementsFile, PackagingSetupPyInstall,
    PackagingStdlib, PackagingStdlibExtensionVariant, PackagingStdlibExtensionsExplicitExcludes,
    PackagingStdlibExtensionsExplicitIncludes, PackagingStdlibExtensionsPolicy,
    PackagingVirtualenv, PackagingWriteLicenseFiles,
};
use crate::py_packaging::distribution::ExtensionModuleFilter;

#[derive(Debug, Clone)]
pub struct FilterInclude {
    pub rule: PackagingFilterInclude,
}

impl TypedValue for FilterInclude {
    immutable!();
    any!();
    not_supported!(binop);
    not_supported!(container);
    not_supported!(function);
    not_supported!(get_hash);
    not_supported!(to_int);

    fn to_str(&self) -> String {
        format!("FilterInclude<{:#?}>", self.rule)
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_type(&self) -> &'static str {
        "FilterInclude"
    }

    fn to_bool(&self) -> bool {
        true
    }

    fn compare(&self, other: &dyn TypedValue, _recursion: u32) -> Result<Ordering, ValueError> {
        default_compare(self, other)
    }
}

#[derive(Debug, Clone)]
pub struct PackageRoot {
    pub rule: PackagingPackageRoot,
}

impl TypedValue for PackageRoot {
    immutable!();
    any!();
    not_supported!(binop);
    not_supported!(container);
    not_supported!(function);
    not_supported!(get_hash);
    not_supported!(to_int);

    fn to_str(&self) -> String {
        format!("PackageRoot<{:#?}>", self.rule)
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_type(&self) -> &'static str {
        "PackageRoot"
    }

    fn to_bool(&self) -> bool {
        true
    }

    fn compare(&self, other: &dyn TypedValue, _recursion: u32) -> Result<Ordering, ValueError> {
        default_compare(self, other)
    }
}

#[derive(Debug, Clone)]
pub struct PipInstallSimple {
    pub rule: PackagingPipInstallSimple,
}

impl TypedValue for PipInstallSimple {
    immutable!();
    any!();
    not_supported!(binop);
    not_supported!(container);
    not_supported!(function);
    not_supported!(get_hash);
    not_supported!(to_int);

    fn to_str(&self) -> String {
        format!("PipInstallSimple<{:#?}>", self.rule)
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_type(&self) -> &'static str {
        "PipInstallSimple"
    }

    fn to_bool(&self) -> bool {
        true
    }

    fn compare(&self, other: &dyn TypedValue, _recursion: u32) -> Result<Ordering, ValueError> {
        default_compare(self, other)
    }
}

#[derive(Debug, Clone)]
pub struct PipRequirementsFile {
    pub rule: PackagingPipRequirementsFile,
}

impl TypedValue for PipRequirementsFile {
    immutable!();
    any!();
    not_supported!(binop);
    not_supported!(container);
    not_supported!(function);
    not_supported!(get_hash);
    not_supported!(to_int);

    fn to_str(&self) -> String {
        format!("PipRequirementsFile<{:#?}>", self.rule)
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_type(&self) -> &'static str {
        "PipRequirementsFile"
    }

    fn to_bool(&self) -> bool {
        true
    }

    fn compare(&self, other: &dyn TypedValue, _recursion: u32) -> Result<Ordering, ValueError> {
        default_compare(self, other)
    }
}

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

#[derive(Debug, Clone)]
pub struct StdlibExtensionsExplicitExcludes {
    pub rule: PackagingStdlibExtensionsExplicitExcludes,
}

impl TypedValue for StdlibExtensionsExplicitExcludes {
    immutable!();
    any!();
    not_supported!(binop);
    not_supported!(container);
    not_supported!(function);
    not_supported!(get_hash);
    not_supported!(to_int);

    fn to_str(&self) -> String {
        format!("StdlibExtensionsExplicitExcludes<{:#?}>", self.rule)
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_type(&self) -> &'static str {
        "StdlibExtensionsExplicitExcludes"
    }

    fn to_bool(&self) -> bool {
        true
    }

    fn compare(&self, other: &dyn TypedValue, _recursion: u32) -> Result<Ordering, ValueError> {
        default_compare(self, other)
    }
}

#[derive(Debug, Clone)]
pub struct StdlibExtensionVariant {
    pub rule: PackagingStdlibExtensionVariant,
}

impl TypedValue for StdlibExtensionVariant {
    immutable!();
    any!();
    not_supported!(binop);
    not_supported!(container);
    not_supported!(function);
    not_supported!(get_hash);
    not_supported!(to_int);

    fn to_str(&self) -> String {
        format!("StdlibExtensionVariant<{:#?}>", self.rule)
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_type(&self) -> &'static str {
        "StdlibExtensionVariant"
    }

    fn to_bool(&self) -> bool {
        true
    }

    fn compare(&self, other: &dyn TypedValue, _recursion: u32) -> Result<Ordering, ValueError> {
        default_compare(self, other)
    }
}

#[derive(Debug, Clone)]
pub struct Stdlib {
    pub rule: PackagingStdlib,
}

impl TypedValue for Stdlib {
    immutable!();
    any!();
    not_supported!(binop);
    not_supported!(container);
    not_supported!(function);
    not_supported!(get_hash);
    not_supported!(to_int);

    fn to_str(&self) -> String {
        format!("Stdlib<{:#?}>", self.rule)
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_type(&self) -> &'static str {
        "Stdlib"
    }

    fn to_bool(&self) -> bool {
        true
    }

    fn compare(&self, other: &dyn TypedValue, _recursion: u32) -> Result<Ordering, ValueError> {
        default_compare(self, other)
    }
}

#[derive(Debug, Clone)]
pub struct Virtualenv {
    pub rule: PackagingVirtualenv,
}

impl TypedValue for Virtualenv {
    immutable!();
    any!();
    not_supported!(binop);
    not_supported!(container);
    not_supported!(function);
    not_supported!(get_hash);
    not_supported!(to_int);

    fn to_str(&self) -> String {
        format!("Virtualenv<{:#?}>", self.rule)
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_type(&self) -> &'static str {
        "Virtualenv"
    }

    fn to_bool(&self) -> bool {
        true
    }

    fn compare(&self, other: &dyn TypedValue, _recursion: u32) -> Result<Ordering, ValueError> {
        default_compare(self, other)
    }
}

#[derive(Debug, Clone)]
pub struct WriteLicenseFiles {
    pub rule: PackagingWriteLicenseFiles,
}

impl TypedValue for WriteLicenseFiles {
    immutable!();
    any!();
    not_supported!(binop);
    not_supported!(container);
    not_supported!(function);
    not_supported!(get_hash);
    not_supported!(to_int);

    fn to_str(&self) -> String {
        format!("WriteLicenseFiles<{:#?}>", self.rule)
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_type(&self) -> &'static str {
        "WriteLicenseFiles"
    }

    fn to_bool(&self) -> bool {
        true
    }

    fn compare(&self, other: &dyn TypedValue, _recursion: u32) -> Result<Ordering, ValueError> {
        default_compare(self, other)
    }
}

starlark_module! { python_packaging_env =>
    #[allow(non_snake_case, clippy::ptr_arg)]
    FilterInclude(files=None, glob_files=None) {
        optional_list_arg("files", "string", &files)?;
        optional_list_arg("glob_files", "string", &glob_files)?;

        let files = match files.get_type() {
            "list" => files.into_iter()?.map(|x| x.to_string()).collect(),
            "NoneType" => Vec::new(),
            _ => panic!("should have validated type above"),
        };
        let glob_files = match glob_files.get_type() {
            "list" => glob_files.into_iter()?.map(|x| x.to_string()).collect(),
            "NoneType" => Vec::new(),
            _ => panic!("should have validated type above"),
        };

        let rule = PackagingFilterInclude {
            files,
            glob_files,
        };

        Ok(Value::new(FilterInclude { rule }))
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PackageRoot(
        path,
        packages=None,
        optimize_level=0,
        excludes=None,
        include_source=true,
        install_location="embedded"
    ) {
        let path = required_str_arg("path", &path)?;
        required_list_arg("packages", "string", &packages)?;
        required_type_arg("optimize_level", "int", &optimize_level)?;
        optional_list_arg("excludes", "string", &excludes)?;
        let include_source = required_bool_arg("include_source", &include_source)?;
        let install_location = required_str_arg("install_location", &install_location)?;

        let packages = packages.into_iter()?.map(|x| x.to_string()).collect();
        let optimize_level = optimize_level.to_int()?;
        let excludes = match excludes.get_type() {
            "list" => excludes.into_iter()?.map(|x| x.to_string()).collect(),
            "NoneType" => Vec::new(),
            _ => panic!("type should have been validated above"),
        };
        let install_location = resolve_install_location(&install_location).or_else(|e| {
            Err(RuntimeError {
                code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                message: e.to_string(),
                label: e.to_string(),
            }.into())
        })?;

        let rule = PackagingPackageRoot {
            path,
            packages,
            optimize_level,
            excludes,
            include_source,
            install_location,
        };

        Ok(Value::new(PackageRoot { rule }))
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PipInstallSimple(
        package,
        venv_path=None,
        extra_env=None,
        optimize_level=0,
        excludes=None,
        include_source=true,
        install_location="embedded",
        extra_args=None
    ) {
        let package = required_str_arg("package", &package)?;
        let venv_path = optional_str_arg("venv_path", &venv_path)?;
        optional_dict_arg("extra_env", "string", "string", &extra_env)?;
        required_type_arg("optimize_level", "int", &optimize_level)?;
        optional_list_arg("excludes", "string", &excludes)?;
        let include_source = required_bool_arg("include_source", &include_source)?;
        let install_location = required_str_arg("install_location", &install_location)?;
        optional_list_arg("extra_args", "string", &extra_args)?;

        let extra_env = match extra_env.get_type() {
            "dict" => extra_env.into_iter()?.map(|key| {
                let k = key.to_string();
                let v = extra_env.at(key.clone()).unwrap().to_string();
                (k, v)
            }).collect(),
            "NoneType" => HashMap::new(),
            _ => panic!("should have validated type above"),
        };

        let optimize_level = optimize_level.to_int()?;
        let excludes = match excludes.get_type() {
            "list" => excludes.into_iter()?.map(|x| x.to_string()).collect(),
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
        let extra_args = match extra_args.get_type() {
            "list" => Some(extra_args.into_iter()?.map(|x| x.to_string()).collect()),
            "NoneType" => None,
            _ => panic!("should have validated type above"),
        };

        let rule = PackagingPipInstallSimple {
            package,
            venv_path,
            extra_env,
            optimize_level,
            excludes,
            include_source,
            install_location,
            extra_args,
        };

        Ok(Value::new(PipInstallSimple { rule }))
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PipRequirementsFile(
        requirements_path,
        venv_path=None,
        extra_env=None,
        optimize_level=0,
        include_source=true,
        install_location="embedded",
        extra_args=None
    ) {
        let requirements_path = required_str_arg("path", &requirements_path)?;
        let venv_path = optional_str_arg("venv_path", &venv_path)?;
        optional_dict_arg("extra_env", "string", "string", &extra_env)?;
        required_type_arg("optimize_level", "int", &optimize_level)?;
        let include_source = required_bool_arg("include_source", &include_source)?;
        let install_location = required_str_arg("install_location", &install_location)?;
        optional_list_arg("extra_args", "string", &extra_args)?;

        let extra_env = match extra_env.get_type() {
            "dict" => extra_env.into_iter()?.map(|key| {
                let k = key.to_string();
                let v = extra_env.at(key.clone()).unwrap().to_string();
                (k, v)
            }).collect(),
            "NoneType" => HashMap::new(),
            _ => panic!("should have validated type above"),
        };

        let optimize_level = optimize_level.to_int()?;
         let install_location = resolve_install_location(&install_location).or_else(|e| {
            Err(RuntimeError {
                code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                message: e.to_string(),
                label: e.to_string(),
            }.into())
        })?;

        let extra_args = match extra_args.get_type() {
            "list" => Some(extra_args.into_iter()?.map(|x| x.to_string()).collect()),
            "NoneType" => None,
            _ => panic!("should have validated type above"),
        };

        let rule = PackagingPipRequirementsFile {
            venv_path,
            requirements_path,
            extra_env,
            optimize_level,
            include_source,
            install_location,
            extra_args,
        };

        Ok(Value::new(PipRequirementsFile { rule }))
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    SetupPyInstall(
        package_path,
        venv_path=None,
        extra_env=None,
        extra_global_arguments=None,
        optimize_level=0,
        include_source=true,
        install_location="embedded",
        excludes=None
    ) {
        let package_path = required_str_arg("package_path", &package_path)?;
        let venv_path = optional_str_arg("venv_path", &venv_path)?;
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
            venv_path,
            extra_env,
            extra_global_arguments,
            optimize_level: optimize_level.to_int().unwrap(),
            include_source,
            install_location,
            excludes,
        };

        Ok(Value::new(SetupPyInstall { rule }))
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    StdlibExtensionsPolicy(policy) {
        let policy = required_str_arg("policy", &policy)?;

        let filter = match policy.as_str() {
            "minimal" => ExtensionModuleFilter::Minimal,
            "all" => ExtensionModuleFilter::All,
            "no-libraries" => ExtensionModuleFilter::NoLibraries,
            "no-gpl" => ExtensionModuleFilter::NoGPL,
            _ => return Err(RuntimeError {
                code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                message: "policy must be one of {minimal, all, no-libraries, no-gpl}".to_string(),
                label: "invalid policy value".to_string(),
            }.into())
        };

        let rule = PackagingStdlibExtensionsPolicy {
            filter,
        };

        Ok(Value::new(StdlibExtensionsPolicy { rule }))
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    StdlibExtensionsExplicitIncludes(includes=None) {
        required_list_arg("includes", "string", &includes)?;

        let includes = includes.into_iter()?.map(|x| x.to_string()).collect();

        let rule = PackagingStdlibExtensionsExplicitIncludes {
            includes,
        };

        Ok(Value::new(StdlibExtensionsExplicitIncludes { rule }))
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    StdlibExtensionsExplicitExcludes(excludes=None) {
        required_list_arg("excludes", "string", &excludes)?;

        let excludes = excludes.into_iter()?.map(|x| x.to_string()).collect();

        let rule = PackagingStdlibExtensionsExplicitExcludes {
            excludes,
        };

        Ok(Value::new(StdlibExtensionsExplicitExcludes { rule }))
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    StdlibExtensionVariant(extension, variant) {
        let extension = required_str_arg("extension", &extension)?;
        let variant = required_str_arg("variant", &variant)?;

        let rule = PackagingStdlibExtensionVariant {
            extension,
            variant,
        };

        Ok(Value::new(StdlibExtensionVariant { rule }))
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    Stdlib(
        optimize_level=0,
        exclude_test_modules=true,
        excludes=None,
        include_source=true,
        include_resources=true,
        install_location="embedded"
    ) {
        required_type_arg("optimize_level", "int", &optimize_level)?;
        optional_list_arg("excludes", "string", &excludes)?;
        let exclude_test_modules = required_bool_arg("exclude_test_modules", &exclude_test_modules)?;
        let include_source = required_bool_arg("include_source", &include_source)?;
        let include_resources = required_bool_arg("include_resources", &include_resources)?;
        let install_location = required_str_arg("install_location", &install_location)?;

        let excludes = match excludes.get_type() {
            "list" => excludes.into_iter()?.map(|x| x.to_string()).collect(),
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

        let rule = PackagingStdlib {
            optimize_level: optimize_level.to_int()?,
            exclude_test_modules,
            excludes,
            include_source,
            include_resources,
            install_location,
        };

        Ok(Value::new(Stdlib { rule }))
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    Virtualenv(
        path,
        optimize_level=0,
        excludes=None,
        include_source=true,
        install_location="embedded"
    ) {
        let path = required_str_arg("path", &path)?;
        required_type_arg("optimize_level", "int", &optimize_level)?;
        optional_list_arg("excludes", "string", &excludes)?;
        let include_source = required_bool_arg("include_source", &include_source)?;
        let install_location = required_str_arg("include_location", &install_location)?;

        let optimize_level = optimize_level.to_int()?;
        let excludes = match excludes.get_type() {
            "list" => excludes.into_iter()?.map(|x| x.to_string()).collect(),
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

        let rule = PackagingVirtualenv {
            path,
            optimize_level,
            excludes,
            include_source,
            install_location,
        };

        Ok(Value::new(Virtualenv { rule }))
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    WriteLicenseFiles(path) {
        let path = required_str_arg("path", &path)?;

        let rule = PackagingWriteLicenseFiles {
            path,
        };

        Ok(Value::new(WriteLicenseFiles { rule }))
    }
}

#[cfg(test)]
mod tests {
    use super::super::testutil::*;
    use super::*;
    use crate::app_packaging::config::InstallLocation;

    #[test]
    fn test_filter_include_default() {
        let v = starlark_ok("FilterInclude()");
        let wanted = PackagingFilterInclude {
            files: Vec::new(),
            glob_files: Vec::new(),
        };

        v.downcast_apply(|x: &FilterInclude| assert_eq!(x.rule, wanted));
    }

    #[test]
    fn test_package_root_default() {
        let err = starlark_nok("PackageRoot()");
        assert!(err.message.starts_with("Missing parameter path"));
    }

    #[test]
    fn test_package_root_basic() {
        let v = starlark_ok("PackageRoot('path', ['foo', 'bar'])");
        let wanted = PackagingPackageRoot {
            path: "path".to_string(),
            packages: vec!["foo".to_string(), "bar".to_string()],
            optimize_level: 0,
            excludes: Vec::new(),
            include_source: true,
            install_location: InstallLocation::Embedded,
        };

        v.downcast_apply(|x: &PackageRoot| assert_eq!(x.rule, wanted));
    }

    #[test]
    fn test_pip_install_simple_default() {
        let err = starlark_nok("PipInstallSimple()");
        assert!(err.message.starts_with("Missing parameter package"));
    }

    #[test]
    fn test_pip_install_simple_basic() {
        let v = starlark_ok("PipInstallSimple('foo')");
        let wanted = PackagingPipInstallSimple {
            package: "foo".to_string(),
            venv_path: None,
            extra_env: HashMap::new(),
            optimize_level: 0,
            excludes: Vec::new(),
            include_source: true,
            install_location: InstallLocation::Embedded,
            extra_args: None,
        };

        v.downcast_apply(|x: &PipInstallSimple| assert_eq!(x.rule, wanted));
    }

    #[test]
    fn test_pip_requirements_file_default() {
        let err = starlark_nok("PipRequirementsFile()");
        assert!(err
            .message
            .starts_with("Missing parameter requirements_path"));
    }

    #[test]
    fn test_pip_requirements_file_basic() {
        let v = starlark_ok("PipRequirementsFile('path')");
        let wanted = PackagingPipRequirementsFile {
            requirements_path: "path".to_string(),
            venv_path: None,
            extra_env: HashMap::new(),
            optimize_level: 0,
            include_source: true,
            install_location: InstallLocation::Embedded,
            extra_args: None,
        };

        v.downcast_apply(|x: &PipRequirementsFile| assert_eq!(x.rule, wanted));
    }

    #[test]
    fn test_pip_requirements_file_extra_args() {
        let v = starlark_ok("PipRequirementsFile('path', extra_args=['foo'])");
        let wanted = PackagingPipRequirementsFile {
            venv_path: None,
            requirements_path: "path".to_string(),
            extra_env: HashMap::new(),
            optimize_level: 0,
            include_source: true,
            install_location: InstallLocation::Embedded,
            extra_args: Some(vec!["foo".to_string()]),
        };

        v.downcast_apply(|x: &PipRequirementsFile| assert_eq!(x.rule, wanted));
    }

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
            venv_path: None,
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
        let v = starlark_ok("StdlibExtensionsPolicy('all')");
        let wanted = PackagingStdlibExtensionsPolicy {
            filter: ExtensionModuleFilter::All,
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

    #[test]
    fn test_stdlib_extensions_explicit_excludes_default() {
        let err = starlark_nok("StdlibExtensionsExplicitExcludes()");
        assert_eq!(
            err.message,
            "function expects a list for excludes; got type NoneType"
        );
    }

    #[test]
    fn test_stdlib_extensions_explicit_excludes_excludes() {
        let v = starlark_ok("StdlibExtensionsExplicitExcludes(['foo', 'bar'])");
        let wanted = PackagingStdlibExtensionsExplicitExcludes {
            excludes: vec!["foo".to_string(), "bar".to_string()],
        };
        v.downcast_apply(|x: &StdlibExtensionsExplicitExcludes| assert_eq!(x.rule, wanted));
    }

    #[test]
    fn test_stdlib_extension_variant_default() {
        let err = starlark_nok("StdlibExtensionVariant()");
        assert!(err.message.starts_with("Missing parameter extension"));
    }

    #[test]
    fn test_stdlib_extension_variant_basic() {
        let v = starlark_ok("StdlibExtensionVariant('foo', 'bar')");
        let wanted = PackagingStdlibExtensionVariant {
            extension: "foo".to_string(),
            variant: "bar".to_string(),
        };
        v.downcast_apply(|x: &StdlibExtensionVariant| assert_eq!(x.rule, wanted));
    }

    #[test]
    fn test_stdlib_default() {
        let v = starlark_ok("Stdlib()");
        let wanted = PackagingStdlib {
            optimize_level: 0,
            exclude_test_modules: true,
            excludes: Vec::new(),
            include_source: true,
            include_resources: true,
            install_location: InstallLocation::Embedded,
        };
        v.downcast_apply(|x: &Stdlib| assert_eq!(x.rule, wanted));
    }

    #[test]
    fn test_virtualenv_default() {
        let err = starlark_nok("Virtualenv()");
        assert!(err.message.starts_with("Missing parameter path"));
    }

    #[test]
    fn test_virtualenv_basic() {
        let v = starlark_ok("Virtualenv('path')");
        let wanted = PackagingVirtualenv {
            path: "path".to_string(),
            optimize_level: 0,
            excludes: Vec::new(),
            include_source: true,
            install_location: InstallLocation::Embedded,
        };
        v.downcast_apply(|x: &Virtualenv| assert_eq!(x.rule, wanted));
    }

    #[test]
    fn test_write_license_files_default() {
        let err = starlark_nok("WriteLicenseFiles()");
        assert!(err.message.starts_with("Missing parameter path"));
    }

    #[test]
    fn test_write_license_files_basic() {
        let v = starlark_ok("WriteLicenseFiles('path')");
        let wanted = PackagingWriteLicenseFiles {
            path: "path".to_string(),
        };
        v.downcast_apply(|x: &WriteLicenseFiles| assert_eq!(x.rule, wanted));
    }
}
