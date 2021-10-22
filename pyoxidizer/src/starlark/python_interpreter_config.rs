// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    super::util::ToValue,
    crate::py_packaging::config::PyembedPythonInterpreterConfig,
    python_packaging::{
        interpreter::{
            Allocator, BytesWarning, CheckHashPycsMode, CoerceCLocale, MemoryAllocatorBackend,
            MultiprocessingStartMethod, PythonInterpreterProfile, TerminfoResolution,
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
    std::{
        str::FromStr,
        sync::{Arc, Mutex, MutexGuard},
    },
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
    pub inner: Arc<Mutex<PyembedPythonInterpreterConfig>>,
}

impl PythonInterpreterConfigValue {
    pub fn new(inner: PyembedPythonInterpreterConfig) -> Self {
        Self {
            inner: Arc::new(Mutex::new(inner)),
        }
    }

    pub fn inner(
        &self,
        label: &str,
    ) -> Result<MutexGuard<PyembedPythonInterpreterConfig>, ValueError> {
        self.inner.try_lock().map_err(|e| {
            ValueError::Runtime(RuntimeError {
                code: "PYTHON_INTERPRETER_CONFIG",
                message: format!("error obtaining lock: {}", e),
                label: label.to_string(),
            })
        })
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
        let inner = self.inner(&format!("PythonInterpreterConfig.{}", attribute))?;

        let v = match attribute {
            "config_profile" => inner.config.profile.to_value(),
            "allocator" => inner.config.allocator.to_value(),
            "configure_locale" => inner.config.configure_locale.to_value(),
            "coerce_c_locale" => inner.config.coerce_c_locale.to_value(),
            "coerce_c_locale_warn" => inner.config.coerce_c_locale_warn.to_value(),
            "development_mode" => inner.config.development_mode.to_value(),
            "isolated" => inner.config.isolated.to_value(),
            "legacy_windows_fs_encoding" => inner.config.legacy_windows_fs_encoding.to_value(),
            "parse_argv" => inner.config.parse_argv.to_value(),
            "use_environment" => inner.config.use_environment.to_value(),
            "utf8_mode" => inner.config.utf8_mode.to_value(),
            "base_exec_prefix" => inner.config.base_exec_prefix.to_value(),
            "base_executable" => inner.config.base_executable.to_value(),
            "base_prefix" => inner.config.base_prefix.to_value(),
            "buffered_stdio" => inner.config.buffered_stdio.to_value(),
            "bytes_warning" => inner.config.bytes_warning.to_value(),
            "check_hash_pycs_mode" => inner.config.check_hash_pycs_mode.to_value(),
            "configure_c_stdio" => inner.config.configure_c_stdio.to_value(),
            "dump_refs" => inner.config.dump_refs.to_value(),
            "exec_prefix" => inner.config.exec_prefix.to_value(),
            "executable" => inner.config.executable.to_value(),
            "fault_handler" => inner.config.fault_handler.to_value(),
            "filesystem_encoding" => inner.config.filesystem_encoding.to_value(),
            "filesystem_errors" => inner.config.filesystem_errors.to_value(),
            "hash_seed" => inner.config.hash_seed.to_value(),
            "home" => inner.config.home.to_value(),
            "import_time" => inner.config.import_time.to_value(),
            "inspect" => inner.config.inspect.to_value(),
            "install_signal_handlers" => inner.config.install_signal_handlers.to_value(),
            "interactive" => inner.config.interactive.to_value(),
            "legacy_windows_stdio" => inner.config.legacy_windows_stdio.to_value(),
            "malloc_stats" => inner.config.malloc_stats.to_value(),
            "module_search_paths" => inner.config.module_search_paths.to_value(),
            "optimization_level" => inner.config.optimization_level.to_value(),
            "parser_debug" => inner.config.parser_debug.to_value(),
            "pathconfig_warnings" => inner.config.pathconfig_warnings.to_value(),
            "prefix" => inner.config.prefix.to_value(),
            "program_name" => inner.config.program_name.to_value(),
            "pycache_prefix" => inner.config.pycache_prefix.to_value(),
            "python_path_env" => inner.config.python_path_env.to_value(),
            "quiet" => inner.config.quiet.to_value(),
            "run_command" => inner.config.run_command.to_value(),
            "run_filename" => inner.config.run_filename.to_value(),
            "run_module" => inner.config.run_module.to_value(),
            "show_ref_count" => inner.config.show_ref_count.to_value(),
            "site_import" => inner.config.site_import.to_value(),
            "skip_first_source_line" => inner.config.skip_first_source_line.to_value(),
            "stdio_encoding" => inner.config.stdio_encoding.to_value(),
            "stdio_errors" => inner.config.stdio_errors.to_value(),
            "tracemalloc" => inner.config.tracemalloc.to_value(),
            "user_site_directory" => inner.config.user_site_directory.to_value(),
            "verbose" => inner.config.verbose.to_value(),
            "warn_options" => inner.config.warn_options.to_value(),
            "write_bytecode" => inner.config.write_bytecode.to_value(),
            "x_options" => inner.config.x_options.to_value(),
            "allocator_backend" => inner.allocator_backend.to_value(),
            "allocator_raw" => Value::from(inner.allocator_raw),
            "allocator_mem" => Value::from(inner.allocator_mem),
            "allocator_obj" => Value::from(inner.allocator_obj),
            "allocator_pymalloc_arena" => Value::from(inner.allocator_pymalloc_arena),
            "allocator_debug" => Value::from(inner.allocator_debug),
            "oxidized_importer" => Value::from(inner.oxidized_importer),
            "filesystem_importer" => Value::from(inner.filesystem_importer),
            "argvb" => Value::from(inner.argvb),
            "multiprocessing_auto_dispatch" => Value::from(inner.multiprocessing_auto_dispatch),
            "multiprocessing_start_method" => {
                Value::from(inner.multiprocessing_start_method.to_string())
            }
            "sys_frozen" => Value::from(inner.sys_frozen),
            "sys_meipass" => Value::from(inner.sys_meipass),
            "terminfo_resolution" => inner.terminfo_resolution.to_value(),
            "write_modules_directory_env" => inner.write_modules_directory_env.to_value(),
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
                | "multiprocessing_auto_dispatch"
                | "multiprocessing_start_method"
                | "sys_frozen"
                | "sys_meipass"
                | "terminfo_resolution"
                | "write_modules_directory_env"
        ))
    }

    fn set_attr(&mut self, attribute: &str, value: Value) -> Result<(), ValueError> {
        let mut inner = self.inner(&format!("PythonInterpreterConfig.{}", attribute))?;

        match attribute {
            "config_profile" => {
                inner.config.profile = PythonInterpreterProfile::try_from(
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
                inner.config.allocator = if value.get_type() == "NoneType" {
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
                inner.config.configure_locale = value.to_optional();
            }
            "coerce_c_locale" => {
                inner.config.coerce_c_locale = if value.get_type() == "NoneType" {
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
                inner.config.coerce_c_locale_warn = value.to_optional();
            }
            "development_mode" => {
                inner.config.development_mode = value.to_optional();
            }
            "isolated" => {
                inner.config.isolated = value.to_optional();
            }
            "legacy_windows_fs_encoding" => {
                inner.config.legacy_windows_fs_encoding = value.to_optional();
            }
            "parse_argv" => {
                inner.config.parse_argv = value.to_optional();
            }
            "use_environment" => {
                inner.config.use_environment = value.to_optional();
            }
            "utf8_mode" => {
                inner.config.utf8_mode = value.to_optional();
            }
            "base_exec_prefix" => {
                inner.config.base_exec_prefix = value.to_optional();
            }
            "base_executable" => {
                inner.config.base_executable = value.to_optional();
            }
            "base_prefix" => {
                inner.config.base_prefix = value.to_optional();
            }
            "buffered_stdio" => {
                inner.config.buffered_stdio = value.to_optional();
            }
            "bytes_warning" => {
                inner.config.bytes_warning = if value.get_type() == "NoneType" {
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
                inner.config.check_hash_pycs_mode = if value.get_type() == "NoneType" {
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
                inner.config.configure_c_stdio = value.to_optional();
            }
            "dump_refs" => {
                inner.config.dump_refs = value.to_optional();
            }
            "exec_prefix" => {
                inner.config.exec_prefix = value.to_optional();
            }
            "executable" => {
                inner.config.executable = value.to_optional();
            }
            "fault_handler" => {
                inner.config.fault_handler = value.to_optional();
            }
            "filesystem_encoding" => {
                inner.config.filesystem_encoding = value.to_optional();
            }
            "filesystem_errors" => {
                inner.config.filesystem_errors = value.to_optional();
            }
            "hash_seed" => {
                inner.config.hash_seed = value.try_to_optional()?;
            }
            "home" => {
                inner.config.home = value.to_optional();
            }
            "import_time" => {
                inner.config.import_time = value.to_optional();
            }
            "inspect" => {
                inner.config.inspect = value.to_optional();
            }
            "install_signal_handlers" => {
                inner.config.install_signal_handlers = value.to_optional();
            }
            "interactive" => {
                inner.config.interactive = value.to_optional();
            }
            "legacy_windows_stdio" => {
                inner.config.legacy_windows_stdio = value.to_optional();
            }
            "malloc_stats" => {
                inner.config.malloc_stats = value.to_optional();
            }
            "module_search_paths" => {
                inner.config.module_search_paths = value.try_to_optional()?;

                // Automatically enable filesystem importer if module search paths
                // are registered.
                if let Some(paths) = &inner.config.module_search_paths {
                    if !paths.is_empty() {
                        inner.filesystem_importer = true;
                    }
                }
            }
            "optimization_level" => {
                inner.config.optimization_level =
                    bytecode_optimization_level_try_to_optional(value)?;
            }
            "parser_debug" => {
                inner.config.parser_debug = value.to_optional();
            }
            "pathconfig_warnings" => {
                inner.config.pathconfig_warnings = value.to_optional();
            }
            "prefix" => {
                inner.config.prefix = value.to_optional();
            }
            "program_name" => {
                inner.config.program_name = value.to_optional();
            }
            "pycache_prefix" => {
                inner.config.pycache_prefix = value.to_optional();
            }
            "python_path_env" => {
                inner.config.python_path_env = value.to_optional();
            }
            "quiet" => {
                inner.config.quiet = value.to_optional();
            }
            "run_command" => {
                inner.config.run_command = value.to_optional();
            }
            "run_filename" => {
                inner.config.run_filename = value.to_optional();
            }
            "run_module" => {
                inner.config.run_module = value.to_optional();
            }
            "show_ref_count" => {
                inner.config.show_ref_count = value.to_optional();
            }
            "site_import" => {
                inner.config.site_import = value.to_optional();
            }
            "skip_first_source_line" => {
                inner.config.skip_first_source_line = value.to_optional();
            }
            "stdio_encoding" => {
                inner.config.stdio_encoding = value.to_optional();
            }
            "stdio_errors" => {
                inner.config.stdio_errors = value.to_optional();
            }
            "tracemalloc" => {
                inner.config.tracemalloc = value.to_optional();
            }
            "user_site_directory" => {
                inner.config.user_site_directory = value.to_optional();
            }
            "verbose" => {
                inner.config.configure_locale = value.to_optional();
            }
            "warn_options" => {
                inner.config.warn_options = value.try_to_optional()?;
            }
            "write_bytecode" => {
                inner.config.write_bytecode = value.to_optional();
            }
            "x_options" => {
                inner.config.x_options = value.try_to_optional()?;
            }
            "allocator_backend" => {
                inner.allocator_backend =
                    MemoryAllocatorBackend::try_from(value.to_string().as_str()).map_err(|e| {
                        ValueError::from(RuntimeError {
                            code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                            message: e,
                            label: format!("{}.{}", Self::TYPE, attribute),
                        })
                    })?;
            }
            "allocator_raw" => {
                inner.allocator_raw = value.to_bool();
            }
            "allocator_mem" => {
                inner.allocator_mem = value.to_bool();
            }
            "allocator_obj" => {
                inner.allocator_obj = value.to_bool();
            }
            "allocator_pymalloc_arena" => {
                inner.allocator_pymalloc_arena = value.to_bool();
            }
            "allocator_debug" => {
                inner.allocator_debug = value.to_bool();
            }
            "oxidized_importer" => {
                inner.oxidized_importer = value.to_bool();
            }
            "filesystem_importer" => {
                inner.filesystem_importer = value.to_bool();
            }
            "argvb" => {
                inner.argvb = value.to_bool();
            }
            "multiprocessing_auto_dispatch" => {
                inner.multiprocessing_auto_dispatch = value.to_bool();
            }
            "multiprocessing_start_method" => {
                inner.multiprocessing_start_method = MultiprocessingStartMethod::from_str(
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
            "sys_frozen" => {
                inner.sys_frozen = value.to_bool();
            }
            "sys_meipass" => {
                inner.sys_meipass = value.to_bool();
            }
            "terminfo_resolution" => {
                inner.terminfo_resolution =
                    TerminfoResolution::try_from(value.to_string().as_str()).map_err(|e| {
                        ValueError::from(RuntimeError {
                            code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                            message: e,
                            label: format!("{}.{}", Self::TYPE, attribute),
                        })
                    })?;
            }
            "write_modules_directory_env" => {
                inner.write_modules_directory_env = value.to_optional();
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
        env.eval("config.user_site_directory = True")?;
        let v = env.eval("config.user_site_directory")?;
        assert!(v.to_bool());
        env.eval("config.user_site_directory = False")?;
        let v = env.eval("config.user_site_directory")?;
        assert!(!v.to_bool());

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
    fn test_multiprocessing_auto_dispatch() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.multiprocessing_auto_dispatch == True")?;

        env.eval("config.multiprocessing_auto_dispatch = False")?;
        eval_assert(&mut env, "config.multiprocessing_auto_dispatch == False")?;

        Ok(())
    }

    #[test]
    fn test_multiprocessing_start_method() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.multiprocessing_start_method == 'auto'")?;

        env.eval("config.multiprocessing_start_method = 'none'")?;
        eval_assert(&mut env, "config.multiprocessing_start_method == 'none'")?;

        env.eval("config.multiprocessing_start_method = 'fork'")?;
        eval_assert(&mut env, "config.multiprocessing_start_method == 'fork'")?;

        env.eval("config.multiprocessing_start_method = 'forkserver'")?;
        eval_assert(
            &mut env,
            "config.multiprocessing_start_method == 'forkserver'",
        )?;

        env.eval("config.multiprocessing_start_method = 'spawn'")?;
        eval_assert(&mut env, "config.multiprocessing_start_method == 'spawn'")?;

        env.eval("config.multiprocessing_start_method = 'auto'")?;
        eval_assert(&mut env, "config.multiprocessing_start_method == 'auto'")?;

        Ok(())
    }

    #[test]
    fn test_sys_frozen() -> Result<()> {
        let mut env = get_env()?;

        eval_assert(&mut env, "config.sys_frozen == True")?;

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
