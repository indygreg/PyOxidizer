// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::py_packaging::config::RunMode;
use {
    super::env::{get_context, EnvironmentContext},
    super::util::{optional_list_arg, optional_str_arg, required_bool_arg, required_type_arg},
    crate::py_packaging::config::{
        default_raw_allocator, EmbeddedPythonConfig, RawAllocator, TerminfoResolution,
    },
    starlark::environment::Environment,
    starlark::values::error::{RuntimeError, ValueError, INCORRECT_PARAMETER_TYPE_ERROR_CODE},
    starlark::values::none::NoneType,
    starlark::values::{Immutable, TypedValue, Value, ValueResult},
    starlark::{
        starlark_fun, starlark_module, starlark_param_name, starlark_parse_param_type,
        starlark_signature, starlark_signature_extraction, starlark_signatures,
    },
};

impl TypedValue for EmbeddedPythonConfig {
    type Holder = Immutable<EmbeddedPythonConfig>;
    const TYPE: &'static str = "PythonInterpreterConfig";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }

    fn to_str(&self) -> String {
        format!("PythonInterpreterConfig<{:#?}>", self)
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }
}

// Starlark functions.
impl EmbeddedPythonConfig {
    /// PythonInterpreterConfig(...)
    #[allow(clippy::too_many_arguments)]
    pub fn starlark_new(
        env: &Environment,
        bytes_warning: &Value,
        ignore_environment: &Value,
        inspect: &Value,
        interactive: &Value,
        isolated: &Value,
        legacy_windows_fs_encoding: &Value,
        legacy_windows_stdio: &Value,
        optimize_level: &Value,
        parser_debug: &Value,
        stdio_encoding: &Value,
        unbuffered_stdio: &Value,
        filesystem_importer: &Value,
        quiet: &Value,
        run_eval: &Value,
        run_file: &Value,
        run_module: &Value,
        run_noop: &Value,
        run_repl: &Value,
        site_import: &Value,
        sys_frozen: &Value,
        sys_meipass: &Value,
        sys_paths: &Value,
        raw_allocator: &Value,
        terminfo_resolution: &Value,
        terminfo_dirs: &Value,
        use_hash_seed: &Value,
        user_site_directory: &Value,
        verbose: &Value,
        write_bytecode: &Value,
        write_modules_directory_env: &Value,
    ) -> ValueResult {
        required_type_arg("bytes_warning", "int", &bytes_warning)?;
        let ignore_environment = required_bool_arg("ignore_environment", &ignore_environment)?;
        let inspect = required_bool_arg("inspect", &inspect)?;
        let interactive = required_bool_arg("interactive", &interactive)?;
        let isolated = required_bool_arg("isolated", &isolated)?;
        let legacy_windows_fs_encoding =
            required_bool_arg("legacy_windows_fs_encoding", &legacy_windows_fs_encoding)?;
        let legacy_windows_stdio =
            required_bool_arg("legacy_windows_stdio", &legacy_windows_stdio)?;
        required_type_arg("optimize_level", "int", &optimize_level)?;
        let parser_debug = required_bool_arg("parser_debug", &parser_debug)?;
        let stdio_encoding = optional_str_arg("stdio_encoding", &stdio_encoding)?;
        let unbuffered_stdio = required_bool_arg("unbuffered_stdio", &unbuffered_stdio)?;
        let filesystem_importer = required_bool_arg("filesystem_importer", &filesystem_importer)?;
        let quiet = required_bool_arg("quiet", &quiet)?;
        let run_eval = optional_str_arg("run_eval", &run_eval)?;
        let run_file = optional_str_arg("run_file", &run_file)?;
        let run_module = optional_str_arg("run_module", &run_module)?;
        let run_noop = required_bool_arg("run_noop", &run_noop)?;
        let run_repl = required_bool_arg("run_repl", &run_repl)?;
        let sys_frozen = required_bool_arg("sys_frozen", &sys_frozen)?;
        let sys_meipass = required_bool_arg("sys_meipass", &sys_meipass)?;
        optional_list_arg("sys_paths", "string", &sys_paths)?;
        let raw_allocator = optional_str_arg("raw_allocator", &raw_allocator)?;
        let site_import = required_bool_arg("site_importer", &site_import)?;
        let terminfo_resolution = optional_str_arg("terminfo_resolution", &terminfo_resolution)?;
        let terminfo_dirs = optional_str_arg("terminfo_dirs", &terminfo_dirs)?;
        let use_hash_seed = required_bool_arg("use_hash_seed", &use_hash_seed)?;
        let user_site_directory = required_bool_arg("user_site_directory", &user_site_directory)?;
        required_type_arg("verbose", "int", &verbose)?;
        let write_bytecode = required_bool_arg("write_bytecode", &write_bytecode)?;
        let write_modules_directory_env =
            optional_str_arg("write_modules_directory_env", &write_modules_directory_env)?;

        let raw_context = get_context(env)?;
        let context = raw_context
            .downcast_ref::<EnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        let mut run_count = 0;
        if run_eval.is_some() {
            run_count += 1;
        }
        if run_file.is_some() {
            run_count += 1;
        }
        if run_module.is_some() {
            run_count += 1;
        }
        if run_noop {
            run_count += 1;
        }
        if run_repl {
            run_count += 1;
        }

        if run_count > 1 {
            return Err(ValueError::from(RuntimeError {
                code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                message: "multiple run_* arguments specified; use at most 1".to_string(),
                label: "PythonInterpreterConfig()".to_string(),
            }));
        }

        let run_mode = if let Some(code) = run_eval {
            RunMode::Eval { code }
        } else if let Some(path) = run_file {
            RunMode::File { path }
        } else if let Some(module) = run_module {
            RunMode::Module { module }
        } else if run_noop {
            RunMode::Noop
        } else {
            RunMode::Repl
        };

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
                    return Err(ValueError::from(RuntimeError {
                        code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                        message: "invalid value for raw_allocator".to_string(),
                        label: "invalid value for raw_allocator".to_string(),
                    }));
                }
            },
            None => default_raw_allocator(&context.build_target_triple),
        };

        let terminfo_resolution = match terminfo_resolution {
            Some(x) => match x.as_ref() {
                "dynamic" => TerminfoResolution::Dynamic,
                "static" => TerminfoResolution::Static(if let Some(dirs) = terminfo_dirs {
                    dirs
                } else {
                    return Err(ValueError::from(RuntimeError {
                        code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                        message: "terminfo_dirs must be set when using static resolution"
                            .to_string(),
                        label: "terminfo_dirs must be set when using static resolution".to_string(),
                    }));
                }),
                _ => {
                    return Err(ValueError::from(RuntimeError {
                        code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                        message: "terminfo_resolution must be 'dynamic' or 'static'".to_string(),
                        label: "terminfo_resolution must be 'dynamic' or 'static'".to_string(),
                    }));
                }
            },
            None => TerminfoResolution::None,
        };

        let sys_paths = match sys_paths.get_type() {
            "list" => sys_paths.iter()?.iter().map(|x| x.to_string()).collect(),
            _ => Vec::new(),
        };

        let filesystem_importer = filesystem_importer || !sys_paths.is_empty();

        Ok(Value::new(EmbeddedPythonConfig {
            bytes_warning: bytes_warning.to_int().unwrap() as i32,
            ignore_environment,
            inspect,
            interactive,
            isolated,
            legacy_windows_fs_encoding,
            legacy_windows_stdio,
            optimize_level: optimize_level.to_int().unwrap(),
            parser_debug,
            quiet,
            stdio_encoding_name,
            stdio_encoding_errors,
            unbuffered_stdio,
            filesystem_importer,
            site_import,
            sys_frozen,
            sys_meipass,
            sys_paths,
            raw_allocator,
            run_mode,
            terminfo_resolution,
            use_hash_seed,
            user_site_directory,
            verbose: verbose.to_int().unwrap() as i32,
            write_bytecode,
            write_modules_directory_env,
        }))
    }
}

starlark_module! { embedded_python_config_module =>
    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonInterpreterConfig(
        env env,
        bytes_warning=0,
        ignore_environment=true,
        inspect=false,
        interactive=false,
        isolated=true,
        legacy_windows_fs_encoding=false,
        legacy_windows_stdio=false,
        optimize_level=0,
        parser_debug=false,
        stdio_encoding=NoneType::None,
        unbuffered_stdio=false,
        filesystem_importer=false,
        quiet=false,
        run_eval=NoneType::None,
        run_file=NoneType::None,
        run_module=NoneType::None,
        run_noop=false,
        run_repl=false,
        site_import=false,
        sys_frozen=false,
        sys_meipass=false,
        sys_paths=NoneType::None,
        raw_allocator=NoneType::None,
        terminfo_resolution="dynamic",
        terminfo_dirs=NoneType::None,
        use_hash_seed=false,
        user_site_directory=false,
        verbose=0,
        write_bytecode=false,
        write_modules_directory_env=NoneType::None
    ) {
        EmbeddedPythonConfig::starlark_new(
            &env,
            &bytes_warning,
            &ignore_environment,
            &inspect,
            &interactive,
            &isolated,
            &legacy_windows_fs_encoding,
            &legacy_windows_stdio,
            &optimize_level,
            &parser_debug,
            &stdio_encoding,
            &unbuffered_stdio,
            &filesystem_importer,
            &quiet,
            &run_eval,
            &run_file,
            &run_module,
            &run_noop,
            &run_repl,
            &site_import,
            &sys_frozen,
            &sys_meipass,
            &sys_paths,
            &raw_allocator,
            &terminfo_resolution,
            &terminfo_dirs,
            &use_hash_seed,
            &user_site_directory,
            &verbose,
            &write_bytecode,
            &write_modules_directory_env
        )
    }
}

#[cfg(test)]
mod tests {
    use {super::super::testutil::*, super::*};

    #[test]
    fn test_default() {
        let c = starlark_ok("PythonInterpreterConfig()");
        assert_eq!(c.get_type(), "PythonInterpreterConfig");

        let wanted = crate::py_packaging::config::EmbeddedPythonConfig {
            bytes_warning: 0,
            ignore_environment: true,
            inspect: false,
            interactive: false,
            isolated: true,
            legacy_windows_fs_encoding: false,
            legacy_windows_stdio: false,
            optimize_level: 0,
            parser_debug: false,
            quiet: false,
            use_hash_seed: false,
            verbose: 0,
            stdio_encoding_name: None,
            stdio_encoding_errors: None,
            unbuffered_stdio: false,
            filesystem_importer: false,
            site_import: false,
            sys_frozen: false,
            sys_meipass: false,
            sys_paths: Vec::new(),
            raw_allocator: default_raw_allocator(crate::project_building::HOST),
            run_mode: RunMode::Repl,
            terminfo_resolution: TerminfoResolution::Dynamic,
            user_site_directory: false,
            write_bytecode: false,
            write_modules_directory_env: None,
        };

        let x = c.downcast_ref::<EmbeddedPythonConfig>().unwrap();
        assert_eq!(*x, wanted);
    }

    #[test]
    fn test_bytes_warning() {
        let c = starlark_ok("PythonInterpreterConfig(bytes_warning=2)");
        let x = c.downcast_ref::<EmbeddedPythonConfig>().unwrap();
        assert_eq!(x.bytes_warning, 2);
    }

    #[test]
    fn test_optimize_level() {
        let c = starlark_ok("PythonInterpreterConfig(optimize_level=1)");
        let x = c.downcast_ref::<EmbeddedPythonConfig>().unwrap();
        assert_eq!(x.optimize_level, 1);
    }

    #[test]
    fn test_sys_paths() {
        let c = starlark_ok("PythonInterpreterConfig(sys_paths=['foo', 'bar'])");
        let x = c.downcast_ref::<EmbeddedPythonConfig>().unwrap();
        assert_eq!(x.sys_paths, ["foo", "bar"]);
        // Setting sys_paths enables filesystem importer.
        assert!(x.filesystem_importer);
    }

    #[test]
    fn test_stdio_encoding() {
        let c = starlark_ok("PythonInterpreterConfig(stdio_encoding='foo:strict')");
        let x = c.downcast_ref::<EmbeddedPythonConfig>().unwrap();
        assert_eq!(x.stdio_encoding_name, Some("foo".to_string()));
        assert_eq!(x.stdio_encoding_errors, Some("strict".to_string()));
    }

    #[test]
    fn test_raw_allocator() {
        let c = starlark_ok("PythonInterpreterConfig(raw_allocator='system')");
        let x = c.downcast_ref::<EmbeddedPythonConfig>().unwrap();
        assert_eq!(x.raw_allocator, RawAllocator::System);

        let c = starlark_ok("PythonInterpreterConfig(raw_allocator='jemalloc')");
        let x = c.downcast_ref::<EmbeddedPythonConfig>().unwrap();
        assert_eq!(x.raw_allocator, RawAllocator::Jemalloc);
        let c = starlark_ok("PythonInterpreterConfig(raw_allocator='rust')");
        let x = c.downcast_ref::<EmbeddedPythonConfig>().unwrap();
        assert_eq!(x.raw_allocator, RawAllocator::Rust);
    }

    #[test]
    fn test_run_eval() {
        let c = starlark_ok("PythonInterpreterConfig(run_eval='1')");
        let x = c.downcast_ref::<EmbeddedPythonConfig>().unwrap();
        assert_eq!(
            x.run_mode,
            RunMode::Eval {
                code: "1".to_string()
            }
        );
    }

    #[test]
    fn test_run_file() {
        let c = starlark_ok("PythonInterpreterConfig(run_file='hello.py')");
        let x = c.downcast_ref::<EmbeddedPythonConfig>().unwrap();

        assert_eq!(
            x.run_mode,
            RunMode::File {
                path: "hello.py".to_string(),
            }
        );
    }

    #[test]
    fn test_run_module() {
        let c = starlark_ok("PythonInterpreterConfig(run_module='main')");
        let x = c.downcast_ref::<EmbeddedPythonConfig>().unwrap();
        assert_eq!(
            x.run_mode,
            RunMode::Module {
                module: "main".to_string()
            }
        );
    }

    #[test]
    fn test_run_noop() {
        let c = starlark_ok("PythonInterpreterConfig(run_noop=True)");
        let x = c.downcast_ref::<EmbeddedPythonConfig>().unwrap();
        assert_eq!(x.run_mode, RunMode::Noop);
    }

    #[test]
    fn test_run_repl() {
        let c = starlark_ok("PythonInterpreterConfig(run_repl=True)");
        let x = c.downcast_ref::<EmbeddedPythonConfig>().unwrap();
        assert_eq!(x.run_mode, RunMode::Repl);
    }

    #[test]
    fn test_terminfo_resolution() {
        let c = starlark_ok("PythonInterpreterConfig(terminfo_resolution=None)");
        let x = c.downcast_ref::<EmbeddedPythonConfig>().unwrap();
        assert_eq!(x.terminfo_resolution, TerminfoResolution::None);

        let c = starlark_ok(
            "PythonInterpreterConfig(terminfo_resolution='static', terminfo_dirs='foo')",
        );
        let x = c.downcast_ref::<EmbeddedPythonConfig>().unwrap();
        assert_eq!(
            x.terminfo_resolution,
            TerminfoResolution::Static("foo".to_string())
        );
    }
}
