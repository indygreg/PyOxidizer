// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    super::util::ToValue,
    crate::py_packaging::config::PyembedPythonInterpreterConfig,
    python_packaging::{
        interpreter::{
            Allocator, BytesWarning, CheckHashPycsMode, CoerceCLocale, MemoryAllocatorBackend,
            PythonInterpreterProfile, TerminfoResolution,
        },
        resource::BytecodeOptimizationLevel,
    },
    starlark::values::{
        error::{
            RuntimeError, UnsupportedOperation, ValueError, INCORRECT_PARAMETER_TYPE_ERROR_CODE,
        },
        none::NoneType,
        {Mutable, TypedValue, Value, ValueResult},
    },
    starlark_dialect_build_targets::{ToOptional, TryToOptional},
    std::convert::TryFrom,
};

impl ToValue for PythonInterpreterProfile {
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

impl ToValue for Option<CheckHashPycsMode> {
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

fn bytecode_optimization_level_try_to_optional(
    v: Value,
) -> Result<Option<BytecodeOptimizationLevel>, ValueError> {
    if v.get_type() == "NoneType" {
        Ok(None)
    } else {
        match v.to_int()? {
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

#[derive(Debug, Clone)]
pub struct PythonInterpreterConfigValue {
    pub inner: PyembedPythonInterpreterConfig,
}

impl PythonInterpreterConfigValue {
    pub fn new(inner: PyembedPythonInterpreterConfig) -> Self {
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
            "allocator_backend" => self.inner.allocator_backend.to_value(),
            "allocator_raw" => Value::from(self.inner.allocator_raw),
            "allocator_mem" => Value::from(self.inner.allocator_mem),
            "allocator_obj" => Value::from(self.inner.allocator_obj),
            "allocator_pymalloc_arena" => Value::from(self.inner.allocator_pymalloc_arena),
            "allocator_debug" => Value::from(self.inner.allocator_debug),
            "oxidized_importer" => Value::from(self.inner.oxidized_importer),
            "filesystem_importer" => Value::from(self.inner.filesystem_importer),
            "argvb" => Value::from(self.inner.argvb),
            "sys_frozen" => Value::from(self.inner.sys_frozen),
            "sys_meipass" => Value::from(self.inner.sys_meipass),
            "terminfo_resolution" => self.inner.terminfo_resolution.to_value(),
            "write_modules_directory_env" => self.inner.write_modules_directory_env.to_value(),
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
        Ok(matches!(
            attribute,
            "config_profile"
                | "allocator"
                | "configure_locale"
                | "coerce_c_locale"
                | "coerce_c_locale_warn"
                | "development_mode"
                | "isolated"
                | "legacy_windows_fs_encoding"
                | "parse_argv"
                | "use_environment"
                | "utf8_mode"
                | "base_exec_prefix"
                | "base_executable"
                | "base_prefix"
                | "buffered_stdio"
                | "bytes_warning"
                | "check_hash_pycs_mode"
                | "configure_c_stdio"
                | "dump_refs"
                | "exec_prefix"
                | "executable"
                | "fault_handler"
                | "filesystem_encoding"
                | "filesystem_errors"
                | "hash_seed"
                | "home"
                | "import_time"
                | "inspect"
                | "install_signal_handlers"
                | "interactive"
                | "legacy_windows_stdio"
                | "malloc_stats"
                | "module_search_paths"
                | "optimization_level"
                | "parser_debug"
                | "pathconfig_warnings"
                | "prefix"
                | "program_name"
                | "pycache_prefix"
                | "python_path_env"
                | "quiet"
                | "run_command"
                | "run_filename"
                | "run_module"
                | "show_ref_count"
                | "site_import"
                | "skip_first_source_line"
                | "stdio_encoding"
                | "stdio_errors"
                | "tracemalloc"
                | "user_site_directory"
                | "verbose"
                | "warn_options"
                | "write_bytecode"
                | "x_options"
                | "allocator_backend"
                | "allocator_raw"
                | "allocator_mem"
                | "allocator_obj"
                | "allocator_pymalloc_arena"
                | "allocator_debug"
                | "oxidized_importer"
                | "filesystem_importer"
                | "argvb"
                | "sys_frozen"
                | "sys_meipass"
                | "terminfo_resolution"
                | "write_modules_directory_env"
        ))
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
                        CheckHashPycsMode::try_from(value.to_string().as_str()).map_err(|e| {
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
                self.inner.config.optimization_level =
                    bytecode_optimization_level_try_to_optional(value)?;
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
            "allocator_backend" => {
                self.inner.allocator_backend =
                    MemoryAllocatorBackend::try_from(value.to_string().as_str()).map_err(|e| {
                        ValueError::from(RuntimeError {
                            code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                            message: e,
                            label: format!("{}.{}", Self::TYPE, attribute),
                        })
                    })?;
            }
            "allocator_raw" => {
                self.inner.allocator_raw = value.to_bool();
            }
            "allocator_mem" => {
                self.inner.allocator_mem = value.to_bool();
            }
            "allocator_obj" => {
                self.inner.allocator_obj = value.to_bool();
            }
            "allocator_pymalloc_arena" => {
                self.inner.allocator_pymalloc_arena = value.to_bool();
            }
            "allocator_debug" => {
                self.inner.allocator_debug = value.to_bool();
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

#[cfg(test)]
mod tests {
    use crate::starlark::eval::EvaluationContext;
    use {super::super::testutil::*, anyhow::Result};

    // TODO instantiating a new distribution every call is expensive. Can we cache this?
    fn get_env() -> Result<EvaluationContext> {
        let mut eval = test_evaluation_context_builder()?.into_context()?;
        eval.eval("dist = default_python_distribution()")?;
        eval.eval("config = dist.make_python_interpreter_config()")?;

        Ok(eval)
    }

    #[test]
    fn test_profile() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.config_profile == 'isolated'")?;

        env.eval("config.config_profile = 'python'")?;
        eval_assert(&mut env, "config.config_profile == 'python'")?;

        env.eval("config.config_profile = 'isolated'")?;
        eval_assert(&mut env, "config.config_profile == 'isolated'")?;

        Ok(())
    }

    #[test]
    fn test_allocator() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.allocator == None")?;

        env.eval("config.allocator = 'not-set'")?;
        eval_assert(&mut env, "config.allocator == 'not-set'")?;

        env.eval("config.allocator = 'default'")?;
        eval_assert(&mut env, "config.allocator == 'default'")?;

        env.eval("config.allocator = 'debug'")?;
        eval_assert(&mut env, "config.allocator == 'debug'")?;

        env.eval("config.allocator = 'malloc'")?;
        eval_assert(&mut env, "config.allocator == 'malloc'")?;

        env.eval("config.allocator = 'malloc-debug'")?;
        eval_assert(&mut env, "config.allocator == 'malloc-debug'")?;

        env.eval("config.allocator = 'py-malloc'")?;
        eval_assert(&mut env, "config.allocator == 'py-malloc'")?;

        env.eval("config.allocator = 'py-malloc-debug'")?;
        eval_assert(&mut env, "config.allocator == 'py-malloc-debug'")?;

        env.eval("config.allocator = None")?;
        eval_assert(&mut env, "config.allocator == None")?;

        Ok(())
    }

    #[test]
    fn test_configure_locale() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.configure_locale == True")?;

        Ok(())
    }

    #[test]
    fn test_coerce_c_locale() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.coerce_c_locale == None")?;

        Ok(())
    }

    #[test]
    fn test_development_mode() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.development_mode == None")?;

        Ok(())
    }

    #[test]
    fn test_isolated() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.isolated == None")?;

        Ok(())
    }

    #[test]
    fn test_legacy_windows_fs_encoding() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.legacy_windows_fs_encoding == None")?;

        Ok(())
    }

    #[test]
    fn test_parse_argv() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.parse_argv == None")?;

        Ok(())
    }

    #[test]
    fn test_use_environment() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.use_environment == None")?;

        Ok(())
    }

    #[test]
    fn test_utf8_mode() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.utf8_mode == None")?;

        Ok(())
    }

    #[test]
    fn test_base_exec_prefix() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.base_exec_prefix == None")?;

        Ok(())
    }

    #[test]
    fn test_base_executable() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.base_executable == None")?;

        Ok(())
    }

    #[test]
    fn test_base_prefix() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.base_prefix == None")?;

        Ok(())
    }

    #[test]
    fn test_buffered_stdio() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.buffered_stdio == None")?;

        Ok(())
    }

    #[test]
    fn test_bytes_warning() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.bytes_warning == None")?;

        env.eval("config.bytes_warning = 'warn'")?;
        eval_assert(&mut env, "config.bytes_warning == 'warn'")?;

        env.eval("config.bytes_warning = 'raise'")?;
        eval_assert(&mut env, "config.bytes_warning == 'raise'")?;

        env.eval("config.bytes_warning = None")?;
        eval_assert(&mut env, "config.bytes_warning == None")?;

        Ok(())
    }

    #[test]
    fn test_check_hash_pycs_mode() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.check_hash_pycs_mode == None")?;

        Ok(())
    }

    #[test]
    fn test_configure_c_stdio() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.configure_c_stdio == None")?;

        Ok(())
    }

    #[test]
    fn test_dump_refs() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.dump_refs == None")?;

        Ok(())
    }

    #[test]
    fn test_exec_prefix() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.exec_prefix == None")?;

        Ok(())
    }

    #[test]
    fn test_executable() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.executable == None")?;

        Ok(())
    }

    #[test]
    fn test_fault_handler() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.fault_handler == None")?;

        Ok(())
    }

    #[test]
    fn test_filesystem_encoding() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.filesystem_encoding == None")?;

        Ok(())
    }

    #[test]
    fn test_filesystem_errors() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.filesystem_errors == None")?;

        Ok(())
    }

    #[test]
    fn test_hash_seed() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.hash_seed == None")?;

        Ok(())
    }

    #[test]
    fn test_home() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.home == None")?;

        Ok(())
    }

    #[test]
    fn test_import_time() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.import_time == None")?;

        Ok(())
    }

    #[test]
    fn test_inspect() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.inspect == None")?;

        Ok(())
    }

    #[test]
    fn test_install_signal_handlers() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.install_signal_handlers == None")?;

        Ok(())
    }

    #[test]
    fn test_interactive() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.interactive == None")?;

        Ok(())
    }

    #[test]
    fn test_legacy_windows_stdio() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.legacy_windows_stdio == None")?;

        Ok(())
    }

    #[test]
    fn test_malloc_stats() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.malloc_stats == None")?;

        Ok(())
    }

    #[test]
    fn test_module_search_paths() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.module_search_paths == None")?;
        eval_assert(&mut env, "config.filesystem_importer == False")?;

        env.eval("config.module_search_paths = []")?;
        eval_assert(&mut env, "config.module_search_paths == []")?;
        eval_assert(&mut env, "config.filesystem_importer == False")?;

        env.eval("config.module_search_paths = ['foo']")?;
        eval_assert(&mut env, "config.module_search_paths == ['foo']")?;
        // filesystem_importer enabled when setting paths.
        eval_assert(&mut env, "config.filesystem_importer == True")?;

        Ok(())
    }

    #[test]
    fn test_optimization_level() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.optimization_level == None")?;

        env.eval("config.optimization_level = 0")?;
        eval_assert(&mut env, "config.optimization_level == 0")?;

        env.eval("config.optimization_level = 1")?;
        eval_assert(&mut env, "config.optimization_level == 1")?;

        env.eval("config.optimization_level = 2")?;
        eval_assert(&mut env, "config.optimization_level == 2")?;

        Ok(())
    }

    #[test]
    fn test_parser_debug() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.parser_debug == None")?;

        Ok(())
    }

    #[test]
    fn test_pathconfig_warnings() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.pathconfig_warnings == None")?;

        Ok(())
    }

    #[test]
    fn test_prefix() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.prefix == None")?;

        Ok(())
    }

    #[test]
    fn test_program_name() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.program_name == None")?;

        Ok(())
    }

    #[test]
    fn test_pycache_prefix() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.pycache_prefix == None")?;

        Ok(())
    }

    #[test]
    fn test_python_path_env() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.python_path_env == None")?;

        Ok(())
    }

    #[test]
    fn test_quiet() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.quiet == None")?;

        Ok(())
    }

    #[test]
    fn test_run_command() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.run_command == None")?;

        Ok(())
    }

    #[test]
    fn test_run_filename() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.run_filename == None")?;

        Ok(())
    }

    #[test]
    fn test_run_module() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.run_module == None")?;

        Ok(())
    }

    #[test]
    fn test_show_ref_count() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.show_ref_count == None")?;

        Ok(())
    }

    #[test]
    fn test_site_import() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.site_import == None")?;

        Ok(())
    }

    #[test]
    fn test_skip_first_source_line() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.skip_first_source_line == None")?;

        Ok(())
    }

    #[test]
    fn test_stdio_encoding() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.stdio_encoding == None")?;

        Ok(())
    }

    #[test]
    fn test_stdio_errors() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.stdio_errors == None")?;

        Ok(())
    }

    #[test]
    fn test_tracemalloc() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.tracemalloc == None")?;

        Ok(())
    }

    #[test]
    fn test_user_site_directory() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.user_site_directory == None")?;

        Ok(())
    }

    #[test]
    fn test_verbose() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.verbose == None")?;

        Ok(())
    }

    #[test]
    fn test_warn_options() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.warn_options == None")?;

        Ok(())
    }

    #[test]
    fn test_write_bytecode() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.write_bytecode == None")?;

        Ok(())
    }

    #[test]
    fn test_x_options() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.x_options == None")?;

        Ok(())
    }

    #[test]
    fn test_allocator_backend() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.allocator_backend == 'default'")?;

        env.eval("config.allocator_backend = 'jemalloc'")?;
        eval_assert(&mut env, "config.allocator_backend == 'jemalloc'")?;

        env.eval("config.allocator_backend = 'mimalloc'")?;
        eval_assert(&mut env, "config.allocator_backend == 'mimalloc'")?;

        env.eval("config.allocator_backend = 'rust'")?;
        eval_assert(&mut env, "config.allocator_backend == 'rust'")?;

        env.eval("config.allocator_backend = 'snmalloc'")?;
        eval_assert(&mut env, "config.allocator_backend == 'snmalloc'")?;

        env.eval("config.allocator_backend = 'default'")?;
        eval_assert(&mut env, "config.allocator_backend == 'default'")?;

        Ok(())
    }

    #[test]
    fn test_allocator_raw() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.allocator_raw == True")?;

        env.eval("config.allocator_raw = False")?;
        eval_assert(&mut env, "config.allocator_raw == False")?;

        Ok(())
    }

    #[test]
    fn test_allocator_mem() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.allocator_mem == False")?;

        env.eval("config.allocator_mem = True")?;
        eval_assert(&mut env, "config.allocator_mem == True")?;

        Ok(())
    }

    #[test]
    fn test_allocator_obj() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.allocator_obj == False")?;

        env.eval("config.allocator_obj = True")?;
        eval_assert(&mut env, "config.allocator_obj == True")?;

        Ok(())
    }

    #[test]
    fn test_allocator_pymalloc_arena() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.allocator_pymalloc_arena == False")?;

        env.eval("config.allocator_pymalloc_arena = True")?;
        eval_assert(&mut env, "config.allocator_pymalloc_arena == True")?;

        Ok(())
    }

    #[test]
    fn test_allocator_debug() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.allocator_debug == False")?;

        env.eval("config.allocator_debug = True")?;
        eval_assert(&mut env, "config.allocator_debug == True")?;

        Ok(())
    }

    #[test]
    fn test_oxidized_importer() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.oxidized_importer == True")?;

        Ok(())
    }

    #[test]
    fn test_filesystem_importer() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.filesystem_importer == False")?;

        Ok(())
    }

    #[test]
    fn test_argvb() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.argvb == False")?;

        Ok(())
    }

    #[test]
    fn test_sys_frozen() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.sys_frozen == False")?;

        Ok(())
    }

    #[test]
    fn test_sys_meipass() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.sys_meipass == False")?;

        Ok(())
    }

    #[test]
    fn test_terminfo_resolution() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.terminfo_resolution == 'dynamic'")?;

        env.eval("config.terminfo_resolution = 'none'")?;
        eval_assert(&mut env, "config.terminfo_resolution == 'none'")?;

        env.eval("config.terminfo_resolution = 'static:foo'")?;
        eval_assert(&mut env, "config.terminfo_resolution == 'static:foo'")?;

        Ok(())
    }

    #[test]
    fn test_write_modules_directory_env() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.write_modules_directory_env == None")?;

        Ok(())
    }
}
