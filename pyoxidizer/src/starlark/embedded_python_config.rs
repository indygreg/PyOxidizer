// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::super::pyrepackager::config::{default_raw_allocator, RawAllocator, TerminfoResolution};
use super::env::{optional_list_arg, optional_str_arg, required_bool_arg, required_type_arg};
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
pub struct EmbeddedPythonConfig {
    pub config: super::super::pyrepackager::config::EmbeddedPythonConfig,
}

impl TypedValue for EmbeddedPythonConfig {
    immutable!();
    any!();
    not_supported!(binop);
    not_supported!(container);
    not_supported!(function);
    not_supported!(get_hash);
    not_supported!(to_int);

    fn to_str(&self) -> String {
        format!("EmbeddedPythonConfig<{:#?}>", self.config)
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_type(&self) -> &'static str {
        "EmbeddedPythonConfig"
    }

    fn to_bool(&self) -> bool {
        true
    }

    fn compare(&self, other: &dyn TypedValue, _recursion: u32) -> Result<Ordering, ValueError> {
        default_compare(self, other)
    }
}

starlark_module! { embedded_python_config_module =>
    #[allow(non_snake_case)]
    EmbeddedPythonConfig(
        env env,
        bytes_warning=0,
        dont_write_bytecode=true,
        ignore_environment=true,
        inspect=false,
        interactive=false,
        isolated=false,
        legacy_windows_fs_encoding=false,
        legacy_windows_stdio=false,
        no_site=true,
        no_user_site_directory=true,
        optimize_level=0,
        parser_debug=false,
        stdio_encoding=None,
        unbuffered_stdio=false,
        filesystem_importer=false,
        quiet=false,
        sys_frozen=false,
        sys_meipass=false,
        sys_paths=None,
        raw_allocator=None,
        terminfo_resolution="dynamic",
        terminfo_dirs=None,
        use_hash_seed=false,
        verbose=0,
        write_modules_directory_env=None
    ) {
        required_type_arg("bytes_warning", "int", &bytes_warning)?;
        let dont_write_bytecode = required_bool_arg("dont_write_bytecode", &dont_write_bytecode)?;
        let ignore_environment = required_bool_arg("ignore_environment", &ignore_environment)?;
        let inspect = required_bool_arg("inspect", &inspect)?;
        let interactive = required_bool_arg("interactive", &interactive)?;
        let isolated = required_bool_arg("isolated", &isolated)?;
        let legacy_windows_fs_encoding = required_bool_arg("legacy_windows_fs_encoding", &legacy_windows_fs_encoding)?;
        let legacy_windows_stdio = required_bool_arg("legacy_windows_stdio", &legacy_windows_stdio)?;
        let no_site = required_bool_arg("no_site", &no_site)?;
        let no_user_site_directory = required_bool_arg("no_user_site_directory", &no_user_site_directory)?;
        required_type_arg("optimize_level", "int", &optimize_level)?;
        let parser_debug = required_bool_arg("parser_debug", &parser_debug)?;
        let stdio_encoding = optional_str_arg("stdio_encoding", &stdio_encoding)?;
        let unbuffered_stdio = required_bool_arg("unbuffered_stdio", &unbuffered_stdio)?;
        let filesystem_importer = required_bool_arg("filesystem_importer", &filesystem_importer)?;
        let quiet = required_bool_arg("quiet", &quiet)?;
        let sys_frozen = required_bool_arg("sys_frozen", &sys_frozen)?;
        let sys_meipass = required_bool_arg("sys_meipass", &sys_meipass)?;
        optional_list_arg("sys_paths", "string", &sys_paths)?;
        let raw_allocator = optional_str_arg("raw_allocator", &raw_allocator)?;
        let terminfo_resolution = optional_str_arg("terminfo_resolution", &terminfo_resolution)?;
        let terminfo_dirs = optional_str_arg("terminfo_dirs", &terminfo_dirs)?;
        let use_hash_seed = required_bool_arg("use_hash_seed", &use_hash_seed)?;
        required_type_arg("verbose", "int", &verbose)?;
        let write_modules_directory_env = optional_str_arg("write_modules_directory_env", &write_modules_directory_env)?;

        let build_target = env.get("BUILD_TARGET").unwrap().to_str();

        let (stdio_encoding_name, stdio_encoding_errors) = if let Some(ref v) = stdio_encoding {
            let values: Vec<&str> = v.split(':').collect();
            (Some(values[0].to_string()), Some(values[1].to_string()))
        } else {
            (None, None)
        };

        let raw_allocator = match raw_allocator {
            Some(x) => match x.as_ref() {
                "jemalloc" => RawAllocator::Jemalloc,
                "rust" => RawAllocator::Rust,
                "system" => RawAllocator::System,
                _ => {
                    return Err(RuntimeError {
                        code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                        message: "invalid value for raw_allocator".to_string(),
                        label: "invalid value for raw_allocator".to_string(),
                    }.into());
                }
            },
            None => default_raw_allocator(&build_target),
        };

        let terminfo_resolution = match terminfo_resolution {
            Some(x) => match x.as_ref() {
                "dynamic" => TerminfoResolution::Dynamic,
                "static" => {
                    TerminfoResolution::Static(if let Some(dirs) = terminfo_dirs {
                        dirs.clone()
                    } else {
                        return Err(RuntimeError {
                            code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                            message: "terminfo_dirs must be set when using static resolution".to_string(),
                            label: "terminfo_dirs must be set when using static resolution".to_string(),
                        }.into());
                    })
                },
                _ => {
                    return Err(RuntimeError {
                        code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                        message: "terminfo_resolution must be 'dynamic' or 'static'".to_string(),
                        label: "terminfo_resolution must be 'dynamic' or 'static'".to_string()
                    }.into());
                }
            },
            None => TerminfoResolution::None,
        };

        let sys_paths = match sys_paths.get_type() {
            "list" => sys_paths.into_iter().unwrap().map(|x| x.to_string()).collect(),
            _ => Vec::new(),
        };

        let config = super::super::pyrepackager::config::EmbeddedPythonConfig {
            bytes_warning: bytes_warning.to_int().unwrap() as i32,
            dont_write_bytecode,
            ignore_environment,
            inspect,
            interactive,
            isolated,
            legacy_windows_fs_encoding,
            legacy_windows_stdio,
            no_site,
            no_user_site_directory,
            optimize_level: optimize_level.to_int().unwrap(),
            parser_debug,
            quiet,
            stdio_encoding_name,
            stdio_encoding_errors,
            unbuffered_stdio,
            filesystem_importer,
            sys_frozen,
            sys_meipass,
            sys_paths,
            raw_allocator,
            terminfo_resolution,
            use_hash_seed,
            verbose: verbose.to_int().unwrap() as i32,
            write_modules_directory_env,
        };

        Ok(Value::new(EmbeddedPythonConfig { config }))
    }
}

#[cfg(test)]
mod tests {
    use super::super::testutil::*;
    use super::*;

    #[test]
    fn test_default() {
        let c = starlark_ok("EmbeddedPythonConfig()");
        assert_eq!(c.get_type(), "EmbeddedPythonConfig");

        let wanted = super::super::super::pyrepackager::config::EmbeddedPythonConfig {
            bytes_warning: 0,
            dont_write_bytecode: true,
            ignore_environment: true,
            inspect: false,
            interactive: false,
            isolated: false,
            legacy_windows_fs_encoding: false,
            legacy_windows_stdio: false,
            no_site: true,
            no_user_site_directory: true,
            optimize_level: 0,
            parser_debug: false,
            quiet: false,
            use_hash_seed: false,
            verbose: 0,
            stdio_encoding_name: None,
            stdio_encoding_errors: None,
            unbuffered_stdio: false,
            filesystem_importer: false,
            sys_frozen: false,
            sys_meipass: false,
            sys_paths: Vec::new(),
            raw_allocator: default_raw_allocator(
                super::super::super::pyrepackager::repackage::HOST,
            ),
            terminfo_resolution: TerminfoResolution::Dynamic,
            write_modules_directory_env: None,
        };

        c.downcast_apply(|x: &EmbeddedPythonConfig| assert_eq!(x.config, wanted));
    }

    #[test]
    fn test_bytes_warning() {
        let c = starlark_ok("EmbeddedPythonConfig(bytes_warning=2)");
        c.downcast_apply(|x: &EmbeddedPythonConfig| assert_eq!(x.config.bytes_warning, 2));
    }

    #[test]
    fn test_optimize_level() {
        let c = starlark_ok("EmbeddedPythonConfig(optimize_level=1)");
        c.downcast_apply(|x: &EmbeddedPythonConfig| assert_eq!(x.config.optimize_level, 1));
    }

    #[test]
    fn test_sys_paths() {
        let c = starlark_ok("EmbeddedPythonConfig(sys_paths=['foo', 'bar'])");
        c.downcast_apply(|x: &EmbeddedPythonConfig| {
            assert_eq!(x.config.sys_paths, ["foo", "bar"]);
        });
    }

    #[test]
    fn test_stdio_encoding() {
        let c = starlark_ok("EmbeddedPythonConfig(stdio_encoding='foo:strict')");
        c.downcast_apply(|x: &EmbeddedPythonConfig| {
            assert_eq!(x.config.stdio_encoding_name, Some("foo".to_string()));
            assert_eq!(x.config.stdio_encoding_errors, Some("strict".to_string()));
        })
    }

    #[test]
    fn test_raw_allocator() {
        let c = starlark_ok("EmbeddedPythonConfig(raw_allocator='system')");
        c.downcast_apply(|x: &EmbeddedPythonConfig| {
            assert_eq!(x.config.raw_allocator, RawAllocator::System);
        });
        let c = starlark_ok("EmbeddedPythonConfig(raw_allocator='jemalloc')");
        c.downcast_apply(|x: &EmbeddedPythonConfig| {
            assert_eq!(x.config.raw_allocator, RawAllocator::Jemalloc);
        });
        let c = starlark_ok("EmbeddedPythonConfig(raw_allocator='rust')");
        c.downcast_apply(|x: &EmbeddedPythonConfig| {
            assert_eq!(x.config.raw_allocator, RawAllocator::Rust);
        });
    }

    #[test]
    fn test_terminfo_resolution() {
        let c = starlark_ok("EmbeddedPythonConfig(terminfo_resolution=None)");
        c.downcast_apply(|x: &EmbeddedPythonConfig| {
            assert_eq!(x.config.terminfo_resolution, TerminfoResolution::None);
        });

        let c =
            starlark_ok("EmbeddedPythonConfig(terminfo_resolution='static', terminfo_dirs='foo')");
        c.downcast_apply(|x: &EmbeddedPythonConfig| {
            assert_eq!(
                x.config.terminfo_resolution,
                TerminfoResolution::Static("foo".to_string())
            );
        });
    }
}
