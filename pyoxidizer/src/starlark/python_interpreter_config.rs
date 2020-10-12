// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    super::util::{
        optional_bool_arg, optional_int_arg, optional_list_arg, optional_str_arg, ToOptional,
        ToValue, TryToOptional,
    },
    crate::py_packaging::config::{default_raw_allocator, EmbeddedPythonConfig},
    python_packaging::{
        interpreter::{
            Allocator, BytesWarning, CheckHashPYCsMode, CoerceCLocale, MemoryAllocatorBackend,
            PythonInterpreterConfig, PythonInterpreterProfile, PythonRunMode, TerminfoResolution,
        },
        resource::BytecodeOptimizationLevel,
    },
    starlark::{
        values::{
            error::{
                RuntimeError, UnsupportedOperation, ValueError, INCORRECT_PARAMETER_TYPE_ERROR_CODE,
            },
            none::NoneType,
            {Mutable, TypedValue, Value, ValueResult},
        },
        {
            starlark_fun, starlark_module, starlark_parse_param_type, starlark_signature,
            starlark_signature_extraction, starlark_signatures,
        },
    },
    std::{convert::TryFrom, path::PathBuf},
};

impl ToValue for PythonInterpreterProfile {
    fn to_value(&self) -> Value {
        Value::from(self.to_string())
    }
}

impl ToValue for PythonRunMode {
    fn to_value(&self) -> Value {
        Value::from(self.to_string())
    }
}

impl ToValue for TerminfoResolution {
    fn to_value(&self) -> Value {
        Value::from(self.to_string())
    }
}

impl ToValue for Option<CoerceCLocale> {
    fn to_value(&self) -> Value {
        match self {
            Some(value) => Value::from(value.to_string()),
            None => Value::from(NoneType::None),
        }
    }
}

impl ToValue for Option<BytesWarning> {
    fn to_value(&self) -> Value {
        match self {
            Some(value) => Value::from(value.to_string()),
            None => Value::from(NoneType::None),
        }
    }
}

impl ToValue for Option<CheckHashPYCsMode> {
    fn to_value(&self) -> Value {
        match self {
            Some(value) => Value::from(value.to_string()),
            None => Value::from(NoneType::None),
        }
    }
}

impl ToValue for Option<Allocator> {
    fn to_value(&self) -> Value {
        match self {
            Some(value) => Value::from(value.to_string()),
            None => Value::from(NoneType::None),
        }
    }
}

impl ToValue for Option<BytecodeOptimizationLevel> {
    fn to_value(&self) -> Value {
        match self {
            Some(value) => Value::from(*value as i32),
            None => Value::from(NoneType::None),
        }
    }
}

impl ToValue for MemoryAllocatorBackend {
    fn to_value(&self) -> Value {
        Value::from(self.to_string())
    }
}

impl TryToOptional<BytecodeOptimizationLevel> for Value {
    fn try_to_optional(&self) -> Result<Option<BytecodeOptimizationLevel>, ValueError> {
        if self.get_type() == "NoneType" {
            Ok(None)
        } else {
            match self.to_int()? {
                0 => Ok(Some(BytecodeOptimizationLevel::Zero)),
                1 => Ok(Some(BytecodeOptimizationLevel::One)),
                2 => Ok(Some(BytecodeOptimizationLevel::Two)),
                _ => Err(ValueError::from(RuntimeError {
                    code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                    message: "invalid Python bytecode integer value".to_string(),
                    label: "PythonInterpreterConfig.optimization_level".to_string(),
                })),
            }
        }
    }
}

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
    type Holder = Mutable<PythonInterpreterConfigValue>;
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

    fn get_attr(&self, attribute: &str) -> ValueResult {
        let v = match attribute {
            "config_profile" => self.inner.config.profile.to_value(),
            "allocator" => self.inner.config.allocator.to_value(),
            "configure_locale" => self.inner.config.configure_locale.to_value(),
            "coerce_c_locale" => self.inner.config.coerce_c_locale.to_value(),
            "coerce_c_locale_warn" => self.inner.config.coerce_c_locale_warn.to_value(),
            "development_mode" => self.inner.config.development_mode.to_value(),
            "isolated" => self.inner.config.isolated.to_value(),
            "legacy_windows_fs_encoding" => self.inner.config.legacy_windows_fs_encoding.to_value(),
            "parse_argv" => self.inner.config.parse_argv.to_value(),
            "use_environment" => self.inner.config.use_environment.to_value(),
            "utf8_mode" => self.inner.config.utf8_mode.to_value(),
            "base_exec_prefix" => self.inner.config.base_exec_prefix.to_value(),
            "base_executable" => self.inner.config.base_executable.to_value(),
            "base_prefix" => self.inner.config.base_prefix.to_value(),
            "buffered_stdio" => self.inner.config.buffered_stdio.to_value(),
            "bytes_warning" => self.inner.config.bytes_warning.to_value(),
            "check_hash_pycs_mode" => self.inner.config.check_hash_pycs_mode.to_value(),
            "configure_c_stdio" => self.inner.config.configure_c_stdio.to_value(),
            "dump_refs" => self.inner.config.dump_refs.to_value(),
            "exec_prefix" => self.inner.config.exec_prefix.to_value(),
            "executable" => self.inner.config.executable.to_value(),
            "fault_handler" => self.inner.config.fault_handler.to_value(),
            "filesystem_encoding" => self.inner.config.filesystem_encoding.to_value(),
            "filesystem_errors" => self.inner.config.filesystem_errors.to_value(),
            "hash_seed" => self.inner.config.hash_seed.to_value(),
            "home" => self.inner.config.home.to_value(),
            "import_time" => self.inner.config.import_time.to_value(),
            "inspect" => self.inner.config.inspect.to_value(),
            "install_signal_handlers" => self.inner.config.install_signal_handlers.to_value(),
            "interactive" => self.inner.config.interactive.to_value(),
            "legacy_windows_stdio" => self.inner.config.legacy_windows_stdio.to_value(),
            "malloc_stats" => self.inner.config.malloc_stats.to_value(),
            "module_search_paths" => self.inner.config.module_search_paths.to_value(),
            "optimization_level" => self.inner.config.optimization_level.to_value(),
            "parser_debug" => self.inner.config.parser_debug.to_value(),
            "pathconfig_warnings" => self.inner.config.pathconfig_warnings.to_value(),
            "prefix" => self.inner.config.prefix.to_value(),
            "program_name" => self.inner.config.program_name.to_value(),
            "pycache_prefix" => self.inner.config.pycache_prefix.to_value(),
            "python_path_env" => self.inner.config.python_path_env.to_value(),
            "quiet" => self.inner.config.quiet.to_value(),
            "run_command" => self.inner.config.run_command.to_value(),
            "run_filename" => self.inner.config.run_filename.to_value(),
            "run_module" => self.inner.config.run_module.to_value(),
            "show_alloc_count" => self.inner.config.show_alloc_count.to_value(),
            "show_ref_count" => self.inner.config.show_ref_count.to_value(),
            "site_import" => self.inner.config.site_import.to_value(),
            "skip_first_source_line" => self.inner.config.skip_first_source_line.to_value(),
            "stdio_encoding" => self.inner.config.stdio_encoding.to_value(),
            "stdio_errors" => self.inner.config.stdio_errors.to_value(),
            "tracemalloc" => self.inner.config.tracemalloc.to_value(),
            "user_site_directory" => self.inner.config.user_site_directory.to_value(),
            "verbose" => self.inner.config.verbose.to_value(),
            "warn_options" => self.inner.config.warn_options.to_value(),
            "write_bytecode" => self.inner.config.write_bytecode.to_value(),
            "x_options" => self.inner.config.x_options.to_value(),
            "raw_allocator" => self.inner.raw_allocator.to_value(),
            "oxidized_importer" => Value::from(self.inner.oxidized_importer),
            "filesystem_importer" => Value::from(self.inner.filesystem_importer),
            "argvb" => Value::from(self.inner.argvb),
            "sys_frozen" => Value::from(self.inner.sys_frozen),
            "sys_meipass" => Value::from(self.inner.sys_meipass),
            "terminfo_resolution" => self.inner.terminfo_resolution.to_value(),
            "write_modules_directory_env" => self.inner.write_modules_directory_env.to_value(),
            "run_mode" => self.inner.run_mode.to_value(),
            attr => {
                return Err(ValueError::OperationNotSupported {
                    op: UnsupportedOperation::GetAttr(attr.to_string()),
                    left: Self::TYPE.to_owned(),
                    right: None,
                })
            }
        };

        Ok(v)
    }

    fn has_attr(&self, attribute: &str) -> Result<bool, ValueError> {
        Ok(match attribute {
            "config_profile" => true,
            "allocator" => true,
            "configure_locale" => true,
            "coerce_c_locale" => true,
            "coerce_c_locale_warn" => true,
            "development_mode" => true,
            "isolated" => true,
            "legacy_windows_fs_encoding" => true,
            "parse_argv" => true,
            "use_environment" => true,
            "utf8_mode" => true,
            "base_exec_prefix" => true,
            "base_executable" => true,
            "base_prefix" => true,
            "buffered_stdio" => true,
            "bytes_warning" => true,
            "check_hash_pycs_mode" => true,
            "configure_c_stdio" => true,
            "dump_refs" => true,
            "exec_prefix" => true,
            "executable" => true,
            "fault_handler" => true,
            "filesystem_encoding" => true,
            "filesystem_errors" => true,
            "hash_seed" => true,
            "home" => true,
            "import_time" => true,
            "inspect" => true,
            "install_signal_handlers" => true,
            "interactive" => true,
            "legacy_windows_stdio" => true,
            "malloc_stats" => true,
            "module_search_paths" => true,
            "optimization_level" => true,
            "parser_debug" => true,
            "pathconfig_warnings" => true,
            "prefix" => true,
            "program_name" => true,
            "pycache_prefix" => true,
            "python_path_env" => true,
            "quiet" => true,
            "run_command" => true,
            "run_filename" => true,
            "run_module" => true,
            "show_alloc_count" => true,
            "show_ref_count" => true,
            "site_import" => true,
            "skip_first_source_line" => true,
            "stdio_encoding" => true,
            "stdio_errors" => true,
            "tracemalloc" => true,
            "user_site_directory" => true,
            "verbose" => true,
            "warn_options" => true,
            "write_bytecode" => true,
            "x_options" => true,
            "raw_allocator" => true,
            "oxidized_importer" => true,
            "filesystem_importer" => true,
            "argvb" => true,
            "sys_frozen" => true,
            "sys_meipass" => true,
            "terminfo_resolution" => true,
            "write_modules_directory_env" => true,
            "run_mode" => true,
            _ => false,
        })
    }

    fn set_attr(&mut self, attribute: &str, value: Value) -> Result<(), ValueError> {
        match attribute {
            "config_profile" => {
                self.inner.config.profile = PythonInterpreterProfile::try_from(
                    value.to_string().as_str(),
                )
                .map_err(|e| {
                    ValueError::from(RuntimeError {
                        code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                        message: e,
                        label: format!("{}.{}", Self::TYPE, attribute),
                    })
                })?;
            }
            "allocator" => {
                self.inner.config.allocator = if value.get_type() == "NoneType" {
                    None
                } else {
                    Some(
                        Allocator::try_from(value.to_string().as_str()).map_err(|e| {
                            ValueError::from(RuntimeError {
                                code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                                message: e,
                                label: format!("{}.{}", Self::TYPE, attribute),
                            })
                        })?,
                    )
                };
            }
            "configure_locale" => {
                self.inner.config.configure_locale = value.to_optional();
            }
            "coerce_c_locale" => {
                self.inner.config.coerce_c_locale = if value.get_type() == "NoneType" {
                    None
                } else {
                    Some(
                        CoerceCLocale::try_from(value.to_string().as_str()).map_err(|e| {
                            ValueError::from(RuntimeError {
                                code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                                message: e,
                                label: format!("{}.{}", Self::TYPE, attribute),
                            })
                        })?,
                    )
                };
            }
            "coerce_c_locale_warn" => {
                self.inner.config.coerce_c_locale_warn = value.to_optional();
            }
            "development_mode" => {
                self.inner.config.development_mode = value.to_optional();
            }
            "isolated" => {
                self.inner.config.isolated = value.to_optional();
            }
            "legacy_windows_fs_encoding" => {
                self.inner.config.legacy_windows_fs_encoding = value.to_optional();
            }
            "parse_argv" => {
                self.inner.config.parse_argv = value.to_optional();
            }
            "use_environment" => {
                self.inner.config.use_environment = value.to_optional();
            }
            "utf8_mode" => {
                self.inner.config.utf8_mode = value.to_optional();
            }
            "base_exec_prefix" => {
                self.inner.config.base_exec_prefix = value.to_optional();
            }
            "base_executable" => {
                self.inner.config.base_executable = value.to_optional();
            }
            "base_prefix" => {
                self.inner.config.base_prefix = value.to_optional();
            }
            "buffered_stdio" => {
                self.inner.config.buffered_stdio = value.to_optional();
            }
            "bytes_warning" => {
                self.inner.config.bytes_warning = if value.get_type() == "NoneType" {
                    None
                } else {
                    Some(
                        BytesWarning::try_from(value.to_string().as_str()).map_err(|e| {
                            ValueError::from(RuntimeError {
                                code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                                message: e,
                                label: format!("{}.{}", Self::TYPE, attribute),
                            })
                        })?,
                    )
                };
            }
            "check_hash_pycs_mode" => {
                self.inner.config.check_hash_pycs_mode = if value.get_type() == "NoneType" {
                    None
                } else {
                    Some(
                        CheckHashPYCsMode::try_from(value.to_string().as_str()).map_err(|e| {
                            ValueError::from(RuntimeError {
                                code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                                message: e,
                                label: format!("{}.{}", Self::TYPE, attribute),
                            })
                        })?,
                    )
                };
            }
            "configure_c_stdio" => {
                self.inner.config.configure_c_stdio = value.to_optional();
            }
            "dump_refs" => {
                self.inner.config.dump_refs = value.to_optional();
            }
            "exec_prefix" => {
                self.inner.config.exec_prefix = value.to_optional();
            }
            "executable" => {
                self.inner.config.executable = value.to_optional();
            }
            "fault_handler" => {
                self.inner.config.fault_handler = value.to_optional();
            }
            "filesystem_encoding" => {
                self.inner.config.filesystem_encoding = value.to_optional();
            }
            "filesystem_errors" => {
                self.inner.config.filesystem_errors = value.to_optional();
            }
            "hash_seed" => {
                self.inner.config.hash_seed = value.try_to_optional()?;
            }
            "home" => {
                self.inner.config.home = value.to_optional();
            }
            "import_time" => {
                self.inner.config.import_time = value.to_optional();
            }
            "inspect" => {
                self.inner.config.inspect = value.to_optional();
            }
            "install_signal_handlers" => {
                self.inner.config.install_signal_handlers = value.to_optional();
            }
            "interactive" => {
                self.inner.config.interactive = value.to_optional();
            }
            "legacy_windows_stdio" => {
                self.inner.config.legacy_windows_stdio = value.to_optional();
            }
            "malloc_stats" => {
                self.inner.config.malloc_stats = value.to_optional();
            }
            "module_search_paths" => {
                self.inner.config.module_search_paths = value.try_to_optional()?;

                // Automatically enable filesystem importer if module search paths
                // are registered.
                if let Some(paths) = &self.inner.config.module_search_paths {
                    if !paths.is_empty() {
                        self.inner.filesystem_importer = true;
                    }
                }
            }
            "optimization_level" => {
                self.inner.config.optimization_level = value.try_to_optional()?;
            }
            "parser_debug" => {
                self.inner.config.parser_debug = value.to_optional();
            }
            "pathconfig_warnings" => {
                self.inner.config.pathconfig_warnings = value.to_optional();
            }
            "prefix" => {
                self.inner.config.prefix = value.to_optional();
            }
            "program_name" => {
                self.inner.config.program_name = value.to_optional();
            }
            "pycache_prefix" => {
                self.inner.config.pycache_prefix = value.to_optional();
            }
            "python_path_env" => {
                self.inner.config.python_path_env = value.to_optional();
            }
            "quiet" => {
                self.inner.config.quiet = value.to_optional();
            }
            "run_command" => {
                self.inner.config.run_command = value.to_optional();
            }
            "run_filename" => {
                self.inner.config.run_filename = value.to_optional();
            }
            "run_module" => {
                self.inner.config.run_module = value.to_optional();
            }
            "show_alloc_count" => {
                self.inner.config.show_alloc_count = value.to_optional();
            }
            "show_ref_count" => {
                self.inner.config.show_ref_count = value.to_optional();
            }
            "site_import" => {
                self.inner.config.site_import = value.to_optional();
            }
            "skip_first_source_line" => {
                self.inner.config.skip_first_source_line = value.to_optional();
            }
            "stdio_encoding" => {
                self.inner.config.stdio_encoding = value.to_optional();
            }
            "stdio_errors" => {
                self.inner.config.stdio_errors = value.to_optional();
            }
            "tracemalloc" => {
                self.inner.config.tracemalloc = value.to_optional();
            }
            "user_site_directory" => {
                self.inner.config.user_site_directory = value.to_optional();
            }
            "verbose" => {
                self.inner.config.configure_locale = value.to_optional();
            }
            "warn_options" => {
                self.inner.config.warn_options = value.try_to_optional()?;
            }
            "write_bytecode" => {
                self.inner.config.write_bytecode = value.to_optional();
            }
            "x_options" => {
                self.inner.config.x_options = value.try_to_optional()?;
            }
            "raw_allocator" => {
                self.inner.raw_allocator =
                    MemoryAllocatorBackend::try_from(value.to_string().as_str()).map_err(|e| {
                        ValueError::from(RuntimeError {
                            code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                            message: e,
                            label: format!("{}.{}", Self::TYPE, attribute),
                        })
                    })?;
            }
            "oxidized_importer" => {
                self.inner.oxidized_importer = value.to_bool();
            }
            "filesystem_importer" => {
                self.inner.filesystem_importer = value.to_bool();
            }
            "argvb" => {
                self.inner.argvb = value.to_bool();
            }
            "sys_frozen" => {
                self.inner.sys_frozen = value.to_bool();
            }
            "sys_meipass" => {
                self.inner.sys_meipass = value.to_bool();
            }
            "terminfo_resolution" => {
                self.inner.terminfo_resolution =
                    TerminfoResolution::try_from(value.to_string().as_str()).map_err(|e| {
                        ValueError::from(RuntimeError {
                            code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                            message: e,
                            label: format!("{}.{}", Self::TYPE, attribute),
                        })
                    })?;
            }
            "write_modules_directory_env" => {
                self.inner.write_modules_directory_env = value.to_optional();
            }
            "run_mode" => {
                self.inner.run_mode =
                    PythonRunMode::try_from(value.to_string().as_str()).map_err(|e| {
                        ValueError::from(RuntimeError {
                            code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                            message: e,
                            label: format!("{}.{}", Self::TYPE, attribute),
                        })
                    })?;
            }
            attr => {
                return Err(ValueError::OperationNotSupported {
                    op: UnsupportedOperation::SetAttr(attr.to_string()),
                    left: Self::TYPE.to_owned(),
                    right: None,
                })
            }
        }

        Ok(())
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
    use {super::super::testutil::*, super::*, anyhow::Result};

    // TODO instantiating a new distribution every call is expensive. Can we cache this?
    fn get_env() -> Result<StarlarkEnvironment> {
        let mut env = StarlarkEnvironment::new()?;

        env.eval("dist = default_python_distribution()")?;
        env.eval("config = dist.make_python_interpreter_config()")?;

        Ok(env)
    }

    #[test]
    fn test_constructor() {
        let c = starlark_ok("PythonInterpreterConfig()");
        assert_eq!(c.get_type(), "PythonInterpreterConfig");

        let wanted = PythonInterpreterConfigValue::default_starlark();

        let x = c.downcast_ref::<PythonInterpreterConfigValue>().unwrap();
        assert_eq!(x.inner, wanted.inner);
    }

    #[test]
    fn test_profile() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.config_profile == 'isolated'")?;

        env.eval("config.config_profile = 'python'")?;
        env.eval_assert("config.config_profile == 'python'")?;

        env.eval("config.config_profile = 'isolated'")?;
        env.eval_assert("config.config_profile == 'isolated'")?;

        Ok(())
    }

    #[test]
    fn test_allocator() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.allocator == None")?;

        env.eval("config.allocator = 'not-set'")?;
        env.eval_assert("config.allocator == 'not-set'")?;

        env.eval("config.allocator = 'default'")?;
        env.eval_assert("config.allocator == 'default'")?;

        env.eval("config.allocator = 'debug'")?;
        env.eval_assert("config.allocator == 'debug'")?;

        env.eval("config.allocator = 'malloc'")?;
        env.eval_assert("config.allocator == 'malloc'")?;

        env.eval("config.allocator = 'malloc-debug'")?;
        env.eval_assert("config.allocator == 'malloc-debug'")?;

        env.eval("config.allocator = 'py-malloc'")?;
        env.eval_assert("config.allocator == 'py-malloc'")?;

        env.eval("config.allocator = 'py-malloc-debug'")?;
        env.eval_assert("config.allocator == 'py-malloc-debug'")?;

        env.eval("config.allocator = None")?;
        env.eval_assert("config.allocator == None")?;

        Ok(())
    }

    #[test]
    fn test_configure_locale() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.configure_locale == None")?;

        Ok(())
    }

    #[test]
    fn test_coerce_c_locale() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.coerce_c_locale == None")?;

        Ok(())
    }

    #[test]
    fn test_development_mode() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.development_mode == None")?;

        Ok(())
    }

    #[test]
    fn test_isolated() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.isolated == None")?;

        Ok(())
    }

    #[test]
    fn test_legacy_windows_fs_encoding() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.legacy_windows_fs_encoding == None")?;

        Ok(())
    }

    #[test]
    fn test_parse_argv() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.parse_argv == None")?;

        Ok(())
    }

    #[test]
    fn test_use_environment() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.use_environment == None")?;

        Ok(())
    }

    #[test]
    fn test_utf8_mode() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.utf8_mode == None")?;

        Ok(())
    }

    #[test]
    fn test_base_exec_prefix() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.base_exec_prefix == None")?;

        Ok(())
    }

    #[test]
    fn test_base_executable() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.base_executable == None")?;

        Ok(())
    }

    #[test]
    fn test_base_prefix() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.base_prefix == None")?;

        Ok(())
    }

    #[test]
    fn test_buffered_stdio() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.buffered_stdio == None")?;

        Ok(())
    }

    #[test]
    fn test_bytes_warning() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.bytes_warning == None")?;

        env.eval("config.bytes_warning = 'warn'")?;
        env.eval_assert("config.bytes_warning == 'warn'")?;

        env.eval("config.bytes_warning = 'raise'")?;
        env.eval_assert("config.bytes_warning == 'raise'")?;

        env.eval("config.bytes_warning = None")?;
        env.eval_assert("config.bytes_warning == None")?;

        Ok(())
    }

    #[test]
    fn test_check_hash_pycs_mode() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.check_hash_pycs_mode == None")?;

        Ok(())
    }

    #[test]
    fn test_configure_c_stdio() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.configure_c_stdio == None")?;

        Ok(())
    }

    #[test]
    fn test_dump_refs() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.dump_refs == None")?;

        Ok(())
    }

    #[test]
    fn test_exec_prefix() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.exec_prefix == None")?;

        Ok(())
    }

    #[test]
    fn test_executable() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.executable == None")?;

        Ok(())
    }

    #[test]
    fn test_fault_handler() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.fault_handler == None")?;

        Ok(())
    }

    #[test]
    fn test_filesystem_encoding() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.filesystem_encoding == None")?;

        Ok(())
    }

    #[test]
    fn test_filesystem_errors() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.filesystem_errors == None")?;

        Ok(())
    }

    #[test]
    fn test_hash_seed() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.hash_seed == None")?;

        Ok(())
    }

    #[test]
    fn test_home() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.home == None")?;

        Ok(())
    }

    #[test]
    fn test_import_time() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.import_time == None")?;

        Ok(())
    }

    #[test]
    fn test_inspect() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.inspect == None")?;

        Ok(())
    }

    #[test]
    fn test_install_signal_handlers() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.install_signal_handlers == None")?;

        Ok(())
    }

    #[test]
    fn test_interactive() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.interactive == None")?;

        Ok(())
    }

    #[test]
    fn test_legacy_windows_stdio() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.legacy_windows_stdio == None")?;

        Ok(())
    }

    #[test]
    fn test_malloc_stats() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.malloc_stats == None")?;

        Ok(())
    }

    #[test]
    fn test_module_search_paths() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.module_search_paths == None")?;
        env.eval_assert("config.filesystem_importer == False")?;

        env.eval("config.module_search_paths = []")?;
        env.eval_assert("config.module_search_paths == []")?;
        env.eval_assert("config.filesystem_importer == False")?;

        env.eval("config.module_search_paths = ['foo']")?;
        env.eval_assert("config.module_search_paths == ['foo']")?;
        // filesystem_importer enabled when setting paths.
        env.eval_assert("config.filesystem_importer == True")?;

        Ok(())
    }

    #[test]
    fn test_optimization_level() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.optimization_level == None")?;

        env.eval("config.optimization_level = 0")?;
        env.eval_assert("config.optimization_level == 0")?;

        env.eval("config.optimization_level = 1")?;
        env.eval_assert("config.optimization_level == 1")?;

        env.eval("config.optimization_level = 2")?;
        env.eval_assert("config.optimization_level == 2")?;

        Ok(())
    }

    #[test]
    fn test_parser_debug() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.parser_debug == None")?;

        Ok(())
    }

    #[test]
    fn test_pathconfig_warnings() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.pathconfig_warnings == None")?;

        Ok(())
    }

    #[test]
    fn test_prefix() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.prefix == None")?;

        Ok(())
    }

    #[test]
    fn test_program_name() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.program_name == None")?;

        Ok(())
    }

    #[test]
    fn test_pycache_prefix() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.pycache_prefix == None")?;

        Ok(())
    }

    #[test]
    fn test_python_path_env() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.python_path_env == None")?;

        Ok(())
    }

    #[test]
    fn test_quiet() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.quiet == None")?;

        Ok(())
    }

    #[test]
    fn test_run_command() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.run_command == None")?;

        Ok(())
    }

    #[test]
    fn test_run_filename() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.run_filename == None")?;

        Ok(())
    }

    #[test]
    fn test_run_module() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.run_module == None")?;

        Ok(())
    }

    #[test]
    fn test_show_alloc_count() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.show_alloc_count == None")?;

        Ok(())
    }

    #[test]
    fn test_show_ref_count() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.show_ref_count == None")?;

        Ok(())
    }

    #[test]
    fn test_site_import() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.site_import == None")?;

        Ok(())
    }

    #[test]
    fn test_skip_first_source_line() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.skip_first_source_line == None")?;

        Ok(())
    }

    #[test]
    fn test_stdio_encoding() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.stdio_encoding == None")?;

        Ok(())
    }

    #[test]
    fn test_stdio_errors() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.stdio_errors == None")?;

        Ok(())
    }

    #[test]
    fn test_tracemalloc() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.tracemalloc == None")?;

        Ok(())
    }

    #[test]
    fn test_user_site_directory() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.user_site_directory == None")?;

        Ok(())
    }

    #[test]
    fn test_verbose() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.verbose == None")?;

        Ok(())
    }

    #[test]
    fn test_warn_options() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.warn_options == None")?;

        Ok(())
    }

    #[test]
    fn test_write_bytecode() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.write_bytecode == None")?;

        Ok(())
    }

    #[test]
    fn test_x_options() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.x_options == None")?;

        Ok(())
    }

    #[test]
    fn test_raw_allocator() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.raw_allocator == 'system'")?;

        env.eval("config.raw_allocator = 'jemalloc'")?;
        env.eval_assert("config.raw_allocator == 'jemalloc'")?;

        env.eval("config.raw_allocator = 'rust'")?;
        env.eval_assert("config.raw_allocator == 'rust'")?;

        Ok(())
    }

    #[test]
    fn test_oxidized_importer() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.oxidized_importer == True")?;

        Ok(())
    }

    #[test]
    fn test_filesystem_importer() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.filesystem_importer == False")?;

        Ok(())
    }

    #[test]
    fn test_argvb() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.argvb == False")?;

        Ok(())
    }

    #[test]
    fn test_sys_frozen() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.sys_frozen == False")?;

        Ok(())
    }

    #[test]
    fn test_sys_meipass() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.sys_meipass == False")?;

        Ok(())
    }

    #[test]
    fn test_terminfo_resolution() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.terminfo_resolution == 'dynamic'")?;

        env.eval("config.terminfo_resolution = 'none'")?;
        env.eval_assert("config.terminfo_resolution == 'none'")?;

        env.eval("config.terminfo_resolution = 'static:foo'")?;
        env.eval_assert("config.terminfo_resolution == 'static:foo'")?;

        Ok(())
    }

    #[test]
    fn test_write_modules_directory_env() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.write_modules_directory_env == None")?;

        Ok(())
    }

    #[test]
    fn test_run_mode() -> Result<()> {
        let mut env = get_env()?;

        env.eval_assert("config.run_mode == 'repl'")?;

        env.eval("config.run_mode = 'module:foo'")?;
        env.eval_assert("config.run_mode == 'module:foo'")?;

        env.eval("config.run_mode = 'none'")?;
        env.eval_assert("config.run_mode == 'none'")?;

        env.eval("config.run_mode = 'eval:code'")?;
        env.eval_assert("config.run_mode == 'eval:code'")?;

        env.eval("config.run_mode = 'file:path'")?;
        env.eval_assert("config.run_mode == 'file:path'")?;

        Ok(())
    }
}
