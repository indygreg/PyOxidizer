// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    super::util::{optional_bool_arg, optional_int_arg, optional_list_arg, optional_str_arg},
    crate::py_packaging::config::{default_raw_allocator, EmbeddedPythonConfig},
    python_packaging::interpreter::{MemoryAllocatorBackend, PythonRunMode, TerminfoResolution},
    starlark::{
        values::{
            error::{RuntimeError, ValueError, INCORRECT_PARAMETER_TYPE_ERROR_CODE},
            none::NoneType,
            {Immutable, TypedValue, Value, ValueResult},
        },
        {
            starlark_fun, starlark_module, starlark_parse_param_type, starlark_signature,
            starlark_signature_extraction, starlark_signatures,
        },
    },
    std::path::PathBuf,
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
    /// Obtain a default instance from the context of Starlark.
    pub fn default_starlark() -> Self {
        Self {
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
            // TODO this should come from the build context.
            raw_allocator: default_raw_allocator(crate::project_building::HOST),
            run_mode: PythonRunMode::Repl,
            terminfo_resolution: TerminfoResolution::Dynamic,
            user_site_directory: false,
            write_bytecode: false,
            write_modules_directory_env: None,
        }
    }

    /// PythonInterpreterConfig(...)
    #[allow(clippy::too_many_arguments)]
    pub fn starlark_new(
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
        let default = EmbeddedPythonConfig::default_starlark();

        let bytes_warning = optional_int_arg("bytes_warning", &bytes_warning)?
            .unwrap_or(default.bytes_warning as i64) as i32;
        let ignore_environment = optional_bool_arg("ignore_environment", &ignore_environment)?
            .unwrap_or(default.ignore_environment);
        let inspect = optional_bool_arg("inspect", &inspect)?.unwrap_or(default.inspect);
        let interactive =
            optional_bool_arg("interactive", &interactive)?.unwrap_or(default.interactive);
        let isolated = optional_bool_arg("isolated", &isolated)?.unwrap_or(default.isolated);
        let legacy_windows_fs_encoding =
            optional_bool_arg("legacy_windows_fs_encoding", &legacy_windows_fs_encoding)?
                .unwrap_or(default.legacy_windows_fs_encoding);
        let legacy_windows_stdio =
            optional_bool_arg("legacy_windows_stdio", &legacy_windows_stdio)?
                .unwrap_or(default.legacy_windows_stdio);
        let optimize_level =
            optional_int_arg("optimize_level", &optimize_level)?.unwrap_or(default.optimize_level);
        let parser_debug =
            optional_bool_arg("parser_debug", &parser_debug)?.unwrap_or(default.parser_debug);
        let stdio_encoding = optional_str_arg("stdio_encoding", &stdio_encoding)?;
        let unbuffered_stdio = optional_bool_arg("unbuffered_stdio", &unbuffered_stdio)?
            .unwrap_or(default.unbuffered_stdio);
        let filesystem_importer = optional_bool_arg("filesystem_importer", &filesystem_importer)?
            .unwrap_or(default.filesystem_importer);
        let quiet = optional_bool_arg("quiet", &quiet)?.unwrap_or(default.quiet);
        let run_eval = optional_str_arg("run_eval", &run_eval)?;
        let run_file = optional_str_arg("run_file", &run_file)?;
        let run_module = optional_str_arg("run_module", &run_module)?;
        let run_noop = optional_bool_arg("run_noop", &run_noop)?.unwrap_or(false);
        let run_repl = optional_bool_arg("run_repl", &run_repl)?.unwrap_or(false);
        let site_import =
            optional_bool_arg("site_importer", &site_import)?.unwrap_or(default.site_import);
        let sys_frozen =
            optional_bool_arg("sys_frozen", &sys_frozen)?.unwrap_or(default.sys_frozen);
        let sys_meipass =
            optional_bool_arg("sys_meipass", &sys_meipass)?.unwrap_or(default.sys_meipass);
        optional_list_arg("sys_paths", "string", &sys_paths)?;
        let raw_allocator = optional_str_arg("raw_allocator", &raw_allocator)?;
        let terminfo_resolution = optional_str_arg("terminfo_resolution", &terminfo_resolution)?;
        let terminfo_dirs = optional_str_arg("terminfo_dirs", &terminfo_dirs)?;
        let use_hash_seed =
            optional_bool_arg("use_hash_seed", &use_hash_seed)?.unwrap_or(default.use_hash_seed);
        let user_site_directory = optional_bool_arg("user_site_directory", &user_site_directory)?
            .unwrap_or(default.user_site_directory);
        let verbose =
            optional_int_arg("verbose", &verbose)?.unwrap_or(default.verbose as i64) as i32;
        let write_bytecode =
            optional_bool_arg("write_bytecode", &write_bytecode)?.unwrap_or(default.write_bytecode);
        let write_modules_directory_env =
            optional_str_arg("write_modules_directory_env", &write_modules_directory_env)?;

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
            PythonRunMode::Eval { code }
        } else if let Some(path) = run_file {
            PythonRunMode::File {
                path: PathBuf::from(path),
            }
        } else if let Some(module) = run_module {
            PythonRunMode::Module { module }
        } else if run_noop {
            PythonRunMode::None
        } else {
            default.run_mode
        };

        let (stdio_encoding_name, stdio_encoding_errors) = if let Some(ref v) = stdio_encoding {
            let values: Vec<&str> = v.split(':').collect();
            (Some(values[0].to_string()), Some(values[1].to_string()))
        } else {
            (None, None)
        };

        let raw_allocator = match raw_allocator {
            Some(x) => match x.as_ref() {
                "jemalloc" => MemoryAllocatorBackend::Jemalloc,
                "rust" => MemoryAllocatorBackend::Rust,
                "system" => MemoryAllocatorBackend::System,
                _ => {
                    return Err(ValueError::from(RuntimeError {
                        code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                        message: "invalid value for raw_allocator".to_string(),
                        label: "PythonInterpreterConfig()".to_string(),
                    }));
                }
            },
            None => default.raw_allocator,
        };

        let terminfo_resolution = match terminfo_resolution {
            Some(x) => match x.as_ref() {
                "dynamic" => TerminfoResolution::Dynamic,
                "none" => TerminfoResolution::None,
                "static" => TerminfoResolution::Static(if let Some(dirs) = terminfo_dirs {
                    dirs
                } else {
                    return Err(ValueError::from(RuntimeError {
                        code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                        message: "terminfo_dirs must be set when using static resolution"
                            .to_string(),
                        label: "PythonInterpreterConfig()".to_string(),
                    }));
                }),
                _ => {
                    return Err(ValueError::from(RuntimeError {
                        code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                        message: "terminfo_resolution must be 'dynamic', 'none', or 'static'"
                            .to_string(),
                        label: "PythonInterpreterConfig()".to_string(),
                    }));
                }
            },
            None => default.terminfo_resolution,
        };

        let sys_paths = match sys_paths.get_type() {
            "list" => sys_paths.iter()?.iter().map(|x| x.to_string()).collect(),
            _ => Vec::new(),
        };

        // Automatically enable the filesystem importer if sys.paths are defined.
        let filesystem_importer = filesystem_importer || !sys_paths.is_empty();

        Ok(Value::new(EmbeddedPythonConfig {
            bytes_warning,
            ignore_environment,
            inspect,
            interactive,
            isolated,
            legacy_windows_fs_encoding,
            legacy_windows_stdio,
            optimize_level,
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
            verbose,
            write_bytecode,
            write_modules_directory_env,
        }))
    }
}

starlark_module! { embedded_python_config_module =>
    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonInterpreterConfig(
        bytes_warning=NoneType::None,
        ignore_environment=NoneType::None,
        inspect=NoneType::None,
        interactive=NoneType::None,
        isolated=NoneType::None,
        legacy_windows_fs_encoding=NoneType::None,
        legacy_windows_stdio=NoneType::None,
        optimize_level=NoneType::None,
        parser_debug=NoneType::None,
        stdio_encoding=NoneType::None,
        unbuffered_stdio=NoneType::None,
        filesystem_importer=NoneType::None,
        quiet=NoneType::None,
        run_eval=NoneType::None,
        run_file=NoneType::None,
        run_module=NoneType::None,
        run_noop=NoneType::None,
        run_repl=NoneType::None,
        site_import=NoneType::None,
        sys_frozen=NoneType::None,
        sys_meipass=NoneType::None,
        sys_paths=NoneType::None,
        raw_allocator=NoneType::None,
        terminfo_resolution=NoneType::None,
        terminfo_dirs=NoneType::None,
        use_hash_seed=NoneType::None,
        user_site_directory=NoneType::None,
        verbose=NoneType::None,
        write_bytecode=NoneType::None,
        write_modules_directory_env=NoneType::None
    ) {
        EmbeddedPythonConfig::starlark_new(
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
    fn test_constructor() {
        let c = starlark_ok("PythonInterpreterConfig()");
        assert_eq!(c.get_type(), "PythonInterpreterConfig");

        let wanted = EmbeddedPythonConfig::default_starlark();

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
        assert_eq!(x.raw_allocator, MemoryAllocatorBackend::System);

        let c = starlark_ok("PythonInterpreterConfig(raw_allocator='jemalloc')");
        let x = c.downcast_ref::<EmbeddedPythonConfig>().unwrap();
        assert_eq!(x.raw_allocator, MemoryAllocatorBackend::Jemalloc);
        let c = starlark_ok("PythonInterpreterConfig(raw_allocator='rust')");
        let x = c.downcast_ref::<EmbeddedPythonConfig>().unwrap();
        assert_eq!(x.raw_allocator, MemoryAllocatorBackend::Rust);
    }

    #[test]
    fn test_run_eval() {
        let c = starlark_ok("PythonInterpreterConfig(run_eval='1')");
        let x = c.downcast_ref::<EmbeddedPythonConfig>().unwrap();
        assert_eq!(
            x.run_mode,
            PythonRunMode::Eval {
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
            PythonRunMode::File {
                path: PathBuf::from("hello.py"),
            }
        );
    }

    #[test]
    fn test_run_module() {
        let c = starlark_ok("PythonInterpreterConfig(run_module='main')");
        let x = c.downcast_ref::<EmbeddedPythonConfig>().unwrap();
        assert_eq!(
            x.run_mode,
            PythonRunMode::Module {
                module: "main".to_string()
            }
        );
    }

    #[test]
    fn test_run_noop() {
        let c = starlark_ok("PythonInterpreterConfig(run_noop=True)");
        let x = c.downcast_ref::<EmbeddedPythonConfig>().unwrap();
        assert_eq!(x.run_mode, PythonRunMode::None);
    }

    #[test]
    fn test_run_repl() {
        let c = starlark_ok("PythonInterpreterConfig(run_repl=True)");
        let x = c.downcast_ref::<EmbeddedPythonConfig>().unwrap();
        assert_eq!(x.run_mode, PythonRunMode::Repl);
    }

    #[test]
    fn test_terminfo_resolution() {
        let c = starlark_ok("PythonInterpreterConfig(terminfo_resolution=None)");
        let x = c.downcast_ref::<EmbeddedPythonConfig>().unwrap();
        assert_eq!(x.terminfo_resolution, TerminfoResolution::Dynamic);

        let c = starlark_ok("PythonInterpreterConfig(terminfo_resolution='dynamic')");
        let x = c.downcast_ref::<EmbeddedPythonConfig>().unwrap();
        assert_eq!(x.terminfo_resolution, TerminfoResolution::Dynamic);

        let c = starlark_ok("PythonInterpreterConfig(terminfo_resolution='none')");
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
