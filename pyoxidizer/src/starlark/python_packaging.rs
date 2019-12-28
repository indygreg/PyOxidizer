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

use super::env::{optional_list_arg, required_bool_arg, required_str_arg, required_type_arg};
use crate::app_packaging::config::{
    resolve_install_location, PackagingFilterInclude, PackagingStdlib, PackagingWriteLicenseFiles,
};

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
