// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    super::util::{optional_bool_arg, optional_int_arg, optional_list_arg, optional_str_arg},
    crate::py_packaging::config::{default_raw_allocator, EmbeddedPythonConfig},
    python_packaging::{
        interpreter::{
            BytesWarning, MemoryAllocatorBackend, PythonInterpreterConfig,
            PythonInterpreterProfile, PythonRunMode, TerminfoResolution,
        },
        resource::BytecodeOptimizationLevel,
    },
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
    std::{convert::TryFrom, path::PathBuf},
};

#[derive(Debug, Clone)]
pub struct PythonInterpreterConfigValue {
    pub inner: EmbeddedPythonConfig,
}

impl PythonInterpreterConfigValue {
    pub fn new(inner: EmbeddedPythonConfig) -> Self {
        Self { inner }
    }
}

impl TypedValue for PythonInterpreterConfigValue {
    type Holder = Immutable<PythonInterpreterConfigValue>;
    const TYPE: &'static str = "PythonInterpreterConfig";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }

    fn to_str(&self) -> String {
        format!("PythonInterpreterConfig<{:#?}>", self.inner)
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }
}

// Starlark functions.
impl PythonInterpreterConfigValue {
    /// Obtain a default instance from the context of Starlark.
    pub fn default_starlark() -> Self {
        Self::new(EmbeddedPythonConfig {
            config: PythonInterpreterConfig {
                profile: PythonInterpreterProfile::Isolated,
                ..PythonInterpreterConfig::default()
            },
            // TODO this should come from the build context.
            raw_allocator: default_raw_allocator(crate::project_building::HOST),
            oxidized_importer: true,
            filesystem_importer: false,
            argvb: false,
            sys_frozen: false,
            sys_meipass: false,
            terminfo_resolution: TerminfoResolution::Dynamic,
            write_modules_directory_env: None,
            run_mode: PythonRunMode::Repl,
        })
    }

    /// PythonInterpreterConfig(...)
    #[allow(clippy::too_many_arguments)]
    pub fn starlark_new(
        bytes_warning: &Value,
        ignore_environment: &Value,
        inspect: &Value,
        interactive: &Value,
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
        user_site_directory: &Value,
        verbose: &Value,
        write_bytecode: &Value,
        write_modules_directory_env: &Value,
    ) -> ValueResult {
        let default = PythonInterpreterConfigValue::default_starlark().inner;

        let bytes_warning = optional_int_arg("bytes_warning", &bytes_warning)?
            .map(|x| BytesWarning::from(x as i32));
        let use_environment =
            optional_bool_arg("ignore_environment", &ignore_environment)?.map(|v| !v);
        let inspect = optional_bool_arg("inspect", &inspect)?;
        let interactive = optional_bool_arg("interactive", &interactive)?;
        let legacy_windows_fs_encoding =
            optional_bool_arg("legacy_windows_fs_encoding", &legacy_windows_fs_encoding)?;
        let legacy_windows_stdio =
            optional_bool_arg("legacy_windows_stdio", &legacy_windows_stdio)?;
        let optimize_level = optional_int_arg("optimize_level", &optimize_level)?;
        let parser_debug = optional_bool_arg("parser_debug", &parser_debug)?;
        let stdio_encoding = optional_str_arg("stdio_encoding", &stdio_encoding)?;
        let buffered_stdio = optional_bool_arg("unbuffered_stdio", &unbuffered_stdio)?.map(|v| !v);
        let filesystem_importer = optional_bool_arg("filesystem_importer", &filesystem_importer)?
            .unwrap_or(default.filesystem_importer);
        let quiet = optional_bool_arg("quiet", &quiet)?;
        let run_eval = optional_str_arg("run_eval", &run_eval)?;
        let run_file = optional_str_arg("run_file", &run_file)?;
        let run_module = optional_str_arg("run_module", &run_module)?;
        let run_noop = optional_bool_arg("run_noop", &run_noop)?;
        let run_repl = optional_bool_arg("run_repl", &run_repl)?;
        let site_import = optional_bool_arg("site_importer", &site_import)?;
        let sys_frozen =
            optional_bool_arg("sys_frozen", &sys_frozen)?.unwrap_or(default.sys_frozen);
        let sys_meipass =
            optional_bool_arg("sys_meipass", &sys_meipass)?.unwrap_or(default.sys_meipass);
        optional_list_arg("sys_paths", "string", &sys_paths)?;
        let raw_allocator = optional_str_arg("raw_allocator", &raw_allocator)?;
        let terminfo_resolution = optional_str_arg("terminfo_resolution", &terminfo_resolution)?;
        let terminfo_dirs = optional_str_arg("terminfo_dirs", &terminfo_dirs)?;
        let user_site_directory = optional_bool_arg("user_site_directory", &user_site_directory)?;
        let verbose = optional_bool_arg("verbose", &verbose)?;
        let write_bytecode = optional_bool_arg("write_bytecode", &write_bytecode)?;
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
        if run_noop.is_some() {
            run_count += 1;
        }
        if run_repl.is_some() {
            run_count += 1;
        }

        if run_count > 1 {
            return Err(ValueError::from(RuntimeError {
                code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                message: "multiple run_* arguments specified; use at most 1".to_string(),
                label: "PythonInterpreterConfig()".to_string(),
            }));
        }

        let optimization_level = if let Some(value) = optimize_level {
            Some(
                BytecodeOptimizationLevel::try_from(value as i32).map_err(|e| {
                    ValueError::from(RuntimeError {
                        code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                        message: format!("invalid bytecode optimization level: {}", e),
                        label: "PythonInterpreterConfig()".to_string(),
                    })
                })?,
            )
        } else {
            None
        };

        let run_mode = if let Some(code) = run_eval {
            PythonRunMode::Eval { code }
        } else if let Some(path) = run_file {
            PythonRunMode::File {
                path: PathBuf::from(path),
            }
        } else if let Some(module) = run_module {
            PythonRunMode::Module { module }
        } else if run_noop == Some(true) {
            PythonRunMode::None
        } else {
            default.run_mode.clone()
        };

        let (stdio_encoding, stdio_errors) = if let Some(ref v) = stdio_encoding {
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
            None => default.raw_allocator.clone(),
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
            None => default.terminfo_resolution.clone(),
        };

        let module_search_paths = match sys_paths.get_type() {
            "list" => Some(
                sys_paths
                    .iter()?
                    .iter()
                    .map(|x| PathBuf::from(x.to_string()))
                    .collect(),
            ),
            _ => None,
        };

        // Automatically enable the filesystem importer if sys.paths are defined.
        let filesystem_importer = filesystem_importer || module_search_paths.is_some();

        Ok(Value::new(PythonInterpreterConfigValue::new(
            EmbeddedPythonConfig {
                config: PythonInterpreterConfig {
                    profile: PythonInterpreterProfile::Isolated,
                    buffered_stdio,
                    bytes_warning,
                    inspect,
                    interactive,
                    legacy_windows_fs_encoding,
                    legacy_windows_stdio,
                    parser_debug,
                    quiet,
                    module_search_paths,
                    optimization_level,
                    site_import,
                    stdio_encoding,
                    stdio_errors,
                    use_environment,
                    user_site_directory,
                    verbose,
                    write_bytecode,
                    ..PythonInterpreterConfig::default()
                },
                raw_allocator,
                oxidized_importer: true,
                filesystem_importer,
                argvb: false,
                sys_frozen,
                sys_meipass,
                terminfo_resolution,
                write_modules_directory_env,
                run_mode,
            },
        )))
    }
}

starlark_module! { embedded_python_config_module =>
    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonInterpreterConfig(
        bytes_warning=NoneType::None,
        ignore_environment=NoneType::None,
        inspect=NoneType::None,
        interactive=NoneType::None,
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
        user_site_directory=NoneType::None,
        verbose=NoneType::None,
        write_bytecode=NoneType::None,
        write_modules_directory_env=NoneType::None
    ) {
        PythonInterpreterConfigValue::starlark_new(
            &bytes_warning,
            &ignore_environment,
            &inspect,
            &interactive,
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

        let wanted = PythonInterpreterConfigValue::default_starlark();

        let x = c.downcast_ref::<PythonInterpreterConfigValue>().unwrap();
        assert_eq!(x.inner, wanted.inner);
    }

    #[test]
    fn test_bytes_warning() {
        let c = starlark_ok("PythonInterpreterConfig(bytes_warning=2)");
        let x = c.downcast_ref::<PythonInterpreterConfigValue>().unwrap();
        assert_eq!(x.inner.config.bytes_warning, Some(BytesWarning::Raise));
    }

    #[test]
    fn test_optimize_level() {
        let c = starlark_ok("PythonInterpreterConfig(optimize_level=1)");
        let x = c.downcast_ref::<PythonInterpreterConfigValue>().unwrap();
        assert_eq!(
            x.inner.config.optimization_level,
            Some(BytecodeOptimizationLevel::One)
        );
    }

    #[test]
    fn test_sys_paths() {
        let c = starlark_ok("PythonInterpreterConfig(sys_paths=['foo', 'bar'])");
        let x = c.downcast_ref::<PythonInterpreterConfigValue>().unwrap();
        assert_eq!(
            x.inner.config.module_search_paths,
            Some(vec![PathBuf::from("foo"), PathBuf::from("bar")])
        );
        // Setting sys_paths enables filesystem importer.
        assert!(x.inner.filesystem_importer);
    }

    #[test]
    fn test_stdio_encoding() {
        let c = starlark_ok("PythonInterpreterConfig(stdio_encoding='foo:strict')");
        let x = c.downcast_ref::<PythonInterpreterConfigValue>().unwrap();
        assert_eq!(x.inner.config.stdio_encoding, Some("foo".to_string()));
        assert_eq!(x.inner.config.stdio_errors, Some("strict".to_string()));
    }

    #[test]
    fn test_raw_allocator() {
        let c = starlark_ok("PythonInterpreterConfig(raw_allocator='system')");
        let x = c.downcast_ref::<PythonInterpreterConfigValue>().unwrap();
        assert_eq!(x.inner.raw_allocator, MemoryAllocatorBackend::System);

        let c = starlark_ok("PythonInterpreterConfig(raw_allocator='jemalloc')");
        let x = c.downcast_ref::<PythonInterpreterConfigValue>().unwrap();
        assert_eq!(x.inner.raw_allocator, MemoryAllocatorBackend::Jemalloc);
        let c = starlark_ok("PythonInterpreterConfig(raw_allocator='rust')");
        let x = c.downcast_ref::<PythonInterpreterConfigValue>().unwrap();
        assert_eq!(x.inner.raw_allocator, MemoryAllocatorBackend::Rust);
    }

    #[test]
    fn test_run_eval() {
        let c = starlark_ok("PythonInterpreterConfig(run_eval='1')");
        let x = c.downcast_ref::<PythonInterpreterConfigValue>().unwrap();
        assert_eq!(
            x.inner.run_mode,
            PythonRunMode::Eval {
                code: "1".to_string()
            }
        );
    }

    #[test]
    fn test_run_file() {
        let c = starlark_ok("PythonInterpreterConfig(run_file='hello.py')");
        let x = c.downcast_ref::<PythonInterpreterConfigValue>().unwrap();

        assert_eq!(
            x.inner.run_mode,
            PythonRunMode::File {
                path: PathBuf::from("hello.py"),
            }
        );
    }

    #[test]
    fn test_run_module() {
        let c = starlark_ok("PythonInterpreterConfig(run_module='main')");
        let x = c.downcast_ref::<PythonInterpreterConfigValue>().unwrap();
        assert_eq!(
            x.inner.run_mode,
            PythonRunMode::Module {
                module: "main".to_string()
            }
        );
    }

    #[test]
    fn test_run_noop() {
        let c = starlark_ok("PythonInterpreterConfig(run_noop=True)");
        let x = c.downcast_ref::<PythonInterpreterConfigValue>().unwrap();
        assert_eq!(x.inner.run_mode, PythonRunMode::None);
    }

    #[test]
    fn test_run_repl() {
        let c = starlark_ok("PythonInterpreterConfig(run_repl=True)");
        let x = c.downcast_ref::<PythonInterpreterConfigValue>().unwrap();
        assert_eq!(x.inner.run_mode, PythonRunMode::Repl);
    }

    #[test]
    fn test_terminfo_resolution() {
        let c = starlark_ok("PythonInterpreterConfig(terminfo_resolution=None)");
        let x = c.downcast_ref::<PythonInterpreterConfigValue>().unwrap();
        assert_eq!(x.inner.terminfo_resolution, TerminfoResolution::Dynamic);

        let c = starlark_ok("PythonInterpreterConfig(terminfo_resolution='dynamic')");
        let x = c.downcast_ref::<PythonInterpreterConfigValue>().unwrap();
        assert_eq!(x.inner.terminfo_resolution, TerminfoResolution::Dynamic);

        let c = starlark_ok("PythonInterpreterConfig(terminfo_resolution='none')");
        let x = c.downcast_ref::<PythonInterpreterConfigValue>().unwrap();
        assert_eq!(x.inner.terminfo_resolution, TerminfoResolution::None);

        let c = starlark_ok(
            "PythonInterpreterConfig(terminfo_resolution='static', terminfo_dirs='foo')",
        );
        let x = c.downcast_ref::<PythonInterpreterConfigValue>().unwrap();
        assert_eq!(
            x.inner.terminfo_resolution,
            TerminfoResolution::Static("foo".to_string())
        );
    }
}
