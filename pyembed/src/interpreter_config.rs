// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Utilities for configuring a Python interpreter.

use {
    super::config::{
        CheckHashPYCsMode, OxidizedPythonInterpreterConfig, PythonInterpreterConfig,
        PythonInterpreterProfile, PythonRunMode,
    },
    libc::{c_int, size_t, wchar_t},
    python3_sys as pyffi,
    std::convert::TryInto,
    std::ffi::{CStr, CString, OsStr},
    std::path::Path,
};

#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;

#[cfg(target_family = "windows")]
use std::os::windows::prelude::OsStrExt;

fn py_status_to_string(status: &pyffi::PyStatus, context: &str) -> String {
    if !status.func.is_null() && !status.err_msg.is_null() {
        let func = unsafe { CStr::from_ptr(status.func) };
        let msg = unsafe { CStr::from_ptr(status.err_msg) };

        format!(
            "during {}: {}: {}",
            context,
            func.to_string_lossy(),
            msg.to_string_lossy()
        )
    } else if !status.err_msg.is_null() {
        let msg = unsafe { CStr::from_ptr(status.err_msg) };

        format!("during {}: {}", context, msg.to_string_lossy())
    } else {
        format!("during {}: could not format PyStatus", context)
    }
}

/// Set a PyConfig string value from a str.
fn set_config_string_from_str(
    config: &pyffi::PyConfig,
    dest: &*mut wchar_t,
    value: &str,
    context: &str,
) -> Result<(), String> {
    match CString::new(value) {
        Ok(value) => unsafe {
            let status = pyffi::PyConfig_SetBytesString(
                config as *const _ as *mut _,
                dest as *const *mut _ as *mut *mut _,
                value.as_ptr(),
            );
            if pyffi::PyStatus_Exception(status) != 0 {
                Err(py_status_to_string(&status, context))
            } else {
                Ok(())
            }
        },
        Err(_) => Err(format!(
            "during {}: unable to convert {} to C string",
            context, value
        )),
    }
}

#[cfg(unix)]
fn set_config_string_from_path(
    config: &pyffi::PyConfig,
    dest: &*mut wchar_t,
    path: &Path,
    context: &str,
) -> Result<(), String> {
    let value = CString::new(path.as_os_str().as_bytes())
        .or_else(|_| Err("cannot convert path to C string".to_string()))?;

    let status = unsafe {
        pyffi::PyConfig_SetBytesString(
            config as *const _ as *mut _,
            dest as *const *mut _ as *mut *mut _,
            value.as_ptr() as *const _,
        )
    };

    if unsafe { pyffi::PyStatus_Exception(status) } != 0 {
        Err(py_status_to_string(&status, context))
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn set_config_string_from_path(
    config: &pyffi::PyConfig,
    dest: &*mut wchar_t,
    path: &Path,
    context: &str,
) -> Result<(), String> {
    let status = unsafe {
        let value: Vec<wchar_t> = path.as_os_str().encode_wide().collect();

        pyffi::PyConfig_SetString(
            config as *const _ as *mut _,
            dest as *const *mut _ as *mut *mut _,
            value.as_ptr() as *const _,
        )
    };

    if unsafe { pyffi::PyStatus_Exception(status) } != 0 {
        Err(py_status_to_string(&status, context))
    } else {
        Ok(())
    }
}

/// Appends a value to a PyWideStringList from an 8-bit char* like source.
fn append_wide_string_list_from_str(
    dest: &mut pyffi::PyWideStringList,
    value: &str,
    context: &str,
) -> Result<(), String> {
    let value =
        CString::new(value).or_else(|_| Err("unable to convert value to C string".to_string()))?;

    let mut len: size_t = 0;

    let decoded = unsafe { pyffi::Py_DecodeLocale(value.as_ptr() as *const _, &mut len) };

    if decoded.is_null() {
        Err(format!("during {}: unable to decode value", context))
    } else {
        let status = unsafe { pyffi::PyWideStringList_Append(dest as *mut _, decoded) };
        unsafe {
            pyffi::PyMem_RawFree(decoded as *mut _);
        }

        if unsafe { pyffi::PyStatus_Exception(status) } != 0 {
            Err(py_status_to_string(&status, context))
        } else {
            Ok(())
        }
    }
}

#[cfg(unix)]
fn append_wide_string_list_from_path(
    dest: &mut pyffi::PyWideStringList,
    path: &Path,
    context: &str,
) -> Result<(), String> {
    let value = path
        .as_os_str()
        .to_str()
        .ok_or_else(|| "unable to convert value to str".to_string())?;

    append_wide_string_list_from_str(dest, value, context)
}

#[cfg(windows)]
fn append_wide_string_list_from_path(
    dest: &mut pyffi::PyWideStringList,
    path: &Path,
    context: &str,
) -> Result<(), String> {
    let status = unsafe {
        let value: Vec<wchar_t> = path.as_os_str().encode_wide().collect();

        pyffi::PyWideStringList_Append(dest as *mut _, value.as_ptr() as *const _)
    };

    if unsafe { pyffi::PyStatus_Exception(status) } != 0 {
        Err(py_status_to_string(&status, context))
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn append_wide_string_list_from_osstr(
    dest: &mut pyffi::PyWideStringList,
    value: &OsStr,
    context: &str,
) -> Result<(), String> {
    let value = String::from_utf8(value.as_bytes().into())
        .or_else(|_| Err("unable to convert value to str".to_string()))?;
    append_wide_string_list_from_str(dest, &value, context)
}

#[cfg(windows)]
fn append_wide_string_list_from_osstr(
    dest: &mut pyffi::PyWideStringList,
    value: &OsStr,
    context: &str,
) -> Result<(), String> {
    let status = unsafe {
        let value: Vec<wchar_t> = value.encode_wide().collect();

        pyffi::PyWideStringList_Append(dest as *mut _, value.as_ptr() as *const _)
    };

    if unsafe { pyffi::PyStatus_Exception(status) } != 0 {
        Err(py_status_to_string(&status, context))
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn set_windows_fs_encoding(_pre_config: &mut pyffi::PyPreConfig, _value: bool) {}

#[cfg(windows)]
fn set_windows_fs_encoding(pre_config: &mut pyffi::PyPreConfig, value: bool) {
    pre_config.legacy_windows_fs_encoding = if value { 1 } else { 0 };
}

#[cfg(unix)]
fn set_legacy_windows_stdio(_config: &mut pyffi::PyConfig, _value: bool) {}

#[cfg(windows)]
fn set_legacy_windows_stdio(config: &mut pyffi::PyConfig, value: bool) {
    config.legacy_windows_stdio = if value { 1 } else { 0 };
}

impl<'a> OxidizedPythonInterpreterConfig<'a> {
    /// Whether the run configuration should execute via Py_RunMain().
    pub(crate) fn uses_py_runmain(&self) -> bool {
        if self.interpreter_config.run_command.is_some()
            || self.interpreter_config.run_filename.is_some()
            || self.interpreter_config.run_module.is_some()
        {
            true
        } else {
            match &self.run {
                PythonRunMode::Eval { .. } => true,
                PythonRunMode::File { .. } => true,
                PythonRunMode::Module { .. } => true,
                PythonRunMode::Repl => true,
                PythonRunMode::None => false,
            }
        }
    }
}

impl TryInto<pyffi::PyPreConfig> for &PythonInterpreterConfig {
    type Error = String;

    fn try_into(self) -> Result<pyffi::PyPreConfig, Self::Error> {
        let mut pre_config = pyffi::PyPreConfig::default();
        unsafe {
            match self.profile {
                PythonInterpreterProfile::Python => {
                    pyffi::PyPreConfig_InitPythonConfig(&mut pre_config)
                }
                PythonInterpreterProfile::Isolated => {
                    pyffi::PyPreConfig_InitIsolatedConfig(&mut pre_config)
                }
            }
        }

        if let Some(parse_argv) = self.parse_argv {
            pre_config.parse_argv = if parse_argv { 1 } else { 0 };
        }
        if let Some(isolated) = self.isolated {
            pre_config.isolated = if isolated { 1 } else { 0 };
        }
        if let Some(use_environment) = self.use_environment {
            pre_config.use_environment = if use_environment { 1 } else { 0 };
        }
        if let Some(configure_locale) = self.configure_locale {
            pre_config.configure_locale = if configure_locale { 1 } else { 0 };
        }
        if let Some(coerce_c_locale) = self.coerce_c_locale {
            pre_config.coerce_c_locale = coerce_c_locale as c_int;
        }
        if let Some(coerce_c_locale_warn) = self.coerce_c_locale_warn {
            pre_config.coerce_c_locale_warn = if coerce_c_locale_warn { 1 } else { 0 };
        }
        if let Some(legacy_windows_fs_encoding) = self.legacy_windows_fs_encoding {
            set_windows_fs_encoding(&mut pre_config, legacy_windows_fs_encoding);
        }
        if let Some(utf8_mode) = self.utf8_mode {
            pre_config.utf8_mode = if utf8_mode { 1 } else { 0 };
        }
        if let Some(dev_mode) = self.development_mode {
            pre_config.dev_mode = if dev_mode { 1 } else { 0 };
        }
        if let Some(allocator) = self.allocator {
            pre_config.allocator = allocator as c_int;
        }

        Ok(pre_config)
    }
}

impl TryInto<pyffi::PyConfig> for &PythonInterpreterConfig {
    type Error = String;

    fn try_into(self) -> Result<pyffi::PyConfig, Self::Error> {
        let mut config = pyffi::PyConfig::default();
        unsafe {
            match self.profile {
                PythonInterpreterProfile::Isolated => {
                    pyffi::PyConfig_InitIsolatedConfig(&mut config)
                }
                PythonInterpreterProfile::Python => pyffi::PyConfig_InitPythonConfig(&mut config),
            }
        }

        if let Some(isolated) = self.isolated {
            config.isolated = if isolated { 1 } else { 0 };
        }
        if let Some(use_environment) = self.use_environment {
            config.use_environment = if use_environment { 1 } else { 0 };
        }
        if let Some(dev_mode) = self.development_mode {
            config.dev_mode = if dev_mode { 1 } else { 0 };
        }
        if let Some(install_signal_handlers) = self.install_signal_handlers {
            config.install_signal_handlers = if install_signal_handlers { 1 } else { 0 };
        }
        if let Some(hash_seed) = self.hash_seed {
            config.hash_seed = hash_seed;
            config.use_hash_seed = 1;
        }
        if let Some(fault_handler) = self.fault_handler {
            config.faulthandler = if fault_handler { 1 } else { 0 };
        }
        if let Some(tracemalloc) = self.tracemalloc {
            config.tracemalloc = if tracemalloc { 1 } else { 0 };
        }
        if let Some(import_time) = self.import_time {
            config.import_time = if import_time { 1 } else { 0 };
        }
        if let Some(show_ref_count) = self.show_ref_count {
            config.show_ref_count = if show_ref_count { 1 } else { 0 };
        }
        if let Some(show_alloc_coun) = self.show_alloc_count {
            config.show_alloc_count = if show_alloc_coun { 1 } else { 0 };
        }
        if let Some(dump_refs) = self.dump_refs {
            config.dump_refs = if dump_refs { 1 } else { 0 };
        }
        if let Some(malloc_stats) = self.malloc_stats {
            config.malloc_stats = if malloc_stats { 1 } else { 0 };
        }
        if let Some(filesystem_encoding) = &self.filesystem_encoding {
            set_config_string_from_str(
                &config,
                &config.filesystem_encoding,
                filesystem_encoding,
                "setting filesystem_encoding",
            )?;
        }
        if let Some(filesystem_errors) = &self.filesystem_errors {
            set_config_string_from_str(
                &config,
                &config.filesystem_errors,
                filesystem_errors,
                "setting filesystem_errors",
            )?;
        }
        if let Some(pycache_prefix) = &self.pycache_prefix {
            set_config_string_from_path(
                &config,
                &config.pycache_prefix,
                pycache_prefix,
                "setting pycache_prefix",
            )?;
        }
        if let Some(parse_argv) = self.parse_argv {
            config.parse_argv = if parse_argv { 1 } else { 0 };
        }
        if let Some(argv) = &self.argv {
            for value in argv {
                append_wide_string_list_from_osstr(&mut config.argv, value, "setting argv")?;
            }
        }
        if let Some(program_name) = &self.program_name {
            set_config_string_from_path(
                &config,
                &config.program_name,
                program_name,
                "setting program_name",
            )?;
        }
        if let Some(x_options) = &self.x_options {
            for value in x_options {
                append_wide_string_list_from_str(&mut config.xoptions, value, "setting xoption")?;
            }
        }
        if let Some(warn_options) = &self.warn_options {
            for value in warn_options {
                append_wide_string_list_from_str(
                    &mut config.warnoptions,
                    value,
                    "setting warn_option",
                )?;
            }
        }
        if let Some(site_import) = self.site_import {
            config.site_import = if site_import { 1 } else { 0 };
        }
        if let Some(bytes_warning) = self.bytes_warning {
            config.bytes_warning = bytes_warning as i32;
        }
        if let Some(inspect) = self.inspect {
            config.inspect = if inspect { 1 } else { 0 };
        }
        if let Some(interactive) = self.interactive {
            config.interactive = if interactive { 1 } else { 0 };
        }
        if let Some(optimization_level) = self.optimization_level {
            config.optimization_level = optimization_level as c_int;
        }
        if let Some(parser_debug) = self.parser_debug {
            config.parser_debug = if parser_debug { 1 } else { 0 };
        }
        if let Some(write_bytecode) = self.write_bytecode {
            config.write_bytecode = if write_bytecode { 1 } else { 0 };
        }
        if let Some(verbose) = self.verbose {
            config.verbose = if verbose { 1 } else { 0 };
        }
        if let Some(quiet) = self.quiet {
            config.quiet = if quiet { 1 } else { 0 };
        }
        if let Some(user_site_directory) = self.user_site_directory {
            config.user_site_directory = if user_site_directory { 1 } else { 0 };
        }
        if let Some(configure_c_stdio) = self.configure_c_stdio {
            config.configure_c_stdio = if configure_c_stdio { 1 } else { 0 };
        }
        if let Some(buffered_stdio) = self.buffered_stdio {
            config.buffered_stdio = if buffered_stdio { 1 } else { 0 };
        }
        if let Some(stdio_encoding) = &self.stdio_encoding {
            set_config_string_from_str(
                &config,
                &config.stdio_encoding,
                stdio_encoding,
                "setting stdio_encoding",
            )?;
        }
        if let Some(stdio_errors) = &self.stdio_errors {
            set_config_string_from_str(
                &config,
                &config.stdio_errors,
                stdio_errors,
                "setting stdio_errors",
            )?;
        }
        if let Some(legacy_windows_stdio) = self.legacy_windows_stdio {
            set_legacy_windows_stdio(&mut config, legacy_windows_stdio);
        }

        if let Some(check_hash_pycs_mode) = self.check_hash_pycs_mode {
            set_config_string_from_str(
                &config,
                &config.check_hash_pycs_mode,
                match check_hash_pycs_mode {
                    CheckHashPYCsMode::Always => "always",
                    CheckHashPYCsMode::Never => "never",
                    CheckHashPYCsMode::Default => "default",
                },
                "setting check_hash_pycs_mode",
            )?;
        }
        if let Some(pathconfig_warnings) = self.pathconfig_warnings {
            config.pathconfig_warnings = if pathconfig_warnings { 1 } else { 0 };
        }
        if let Some(python_path_env) = &self.python_path_env {
            set_config_string_from_str(
                &config,
                &config.pythonpath_env,
                python_path_env,
                "setting pythonpath_env",
            )?;
        }

        if let Some(home) = &self.home {
            set_config_string_from_path(&config, &config.home, home, "setting home")?;
        }
        if let Some(module_search_paths) = &self.module_search_paths {
            config.module_search_paths_set = 1;

            for path in module_search_paths {
                append_wide_string_list_from_path(
                    &mut config.module_search_paths,
                    path,
                    "setting module_search_paths",
                )?;
            }
        }
        if let Some(executable) = &self.executable {
            set_config_string_from_path(
                &config,
                &config.executable,
                executable,
                "setting executable",
            )?;
        }
        if let Some(base_executable) = &self.base_executable {
            set_config_string_from_path(
                &config,
                &config.base_executable,
                base_executable,
                "setting base_executable",
            )?;
        }
        if let Some(prefix) = &self.prefix {
            set_config_string_from_path(&config, &config.prefix, prefix, "setting prefix")?;
        }
        if let Some(base_prefix) = &self.base_prefix {
            set_config_string_from_path(
                &config,
                &config.base_prefix,
                base_prefix,
                "setting base_prefix",
            )?;
        }
        if let Some(exec_prefix) = &self.exec_prefix {
            set_config_string_from_path(
                &config,
                &config.exec_prefix,
                exec_prefix,
                "setting exec_prefix",
            )?;
        }
        if let Some(base_exec_prefix) = &self.base_exec_prefix {
            set_config_string_from_path(
                &config,
                &config.base_exec_prefix,
                base_exec_prefix,
                "setting base_exec_prefix",
            )?;
        }
        if let Some(skip_source_first_line) = self.skip_first_source_line {
            config.skip_source_first_line = if skip_source_first_line { 1 } else { 0 };
        }
        if let Some(run_command) = &self.run_command {
            set_config_string_from_str(
                &config,
                &config.run_command,
                run_command,
                "setting run_command",
            )?;
        }
        if let Some(run_module) = &self.run_module {
            set_config_string_from_str(
                &config,
                &config.run_module,
                run_module,
                "setting run_module",
            )?;
        }
        if let Some(run_filename) = &self.run_filename {
            set_config_string_from_path(
                &config,
                &config.run_filename,
                run_filename,
                "setting run_filename",
            )?;
        }

        Ok(config)
    }
}

impl<'a> TryInto<pyffi::PyConfig> for &'a OxidizedPythonInterpreterConfig<'a> {
    type Error = String;

    fn try_into(self) -> Result<pyffi::PyConfig, Self::Error> {
        // We use the raw configuration as a base then we apply any adjustments,
        // as needed.
        let config: pyffi::PyConfig = (&self.interpreter_config).try_into()?;

        // We define a configuration "profile" to dictate overall interpreter
        // behavior. In "python" mode, we behave like a `python` executable.
        // In "isolated" mode, we behave like an embedded application.
        //
        // In "python" mode, we don't set any config fields unless they were
        // explicitly defined in the `PythonInterpreterConfig`.
        //
        // In "isolated" mode, we automatically fill in various fields as
        // derived from the environment. But we never overwrite values that
        // are explicitly set in the config.
        if self.interpreter_config.profile == PythonInterpreterProfile::Isolated {
            let exe = std::env::current_exe()
                .or_else(|err| Err(format!("unable to obtain current executable: {}", err)))?;
            let origin = exe
                .parent()
                .ok_or_else(|| "unable to get current executable directory".to_string())?;

            // program_name set to path of current executable.
            if self.interpreter_config.program_name.is_none() {
                set_config_string_from_path(
                    &config,
                    &config.program_name,
                    &exe,
                    "setting program_name",
                )?;
            }

            // PYTHONHOME is set to directory of current executable.
            if self.interpreter_config.home.is_none() {
                set_config_string_from_path(&config, &config.home, origin, "setting home")?;
            }
        }

        match &self.run {
            PythonRunMode::None => {}
            PythonRunMode::Repl => {}
            PythonRunMode::Eval { code } => {
                if self.interpreter_config.run_command.is_none() {
                    set_config_string_from_str(
                        &config,
                        &config.run_command,
                        code,
                        "setting run_command",
                    )?;
                }
            }
            PythonRunMode::File { path } => {
                if self.interpreter_config.run_filename.is_none() {
                    set_config_string_from_path(
                        &config,
                        &config.run_filename,
                        path,
                        "setting run_filename",
                    )?;
                }
            }
            PythonRunMode::Module { module } => {
                if self.interpreter_config.run_module.is_none() {
                    set_config_string_from_str(
                        &config,
                        &config.run_module,
                        module,
                        "setting run_module",
                    )?;
                }
            }
        }

        Ok(config)
    }
}
