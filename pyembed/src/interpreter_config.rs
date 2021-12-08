// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Utilities for configuring a Python interpreter.

use {
    crate::{config::ResolvedOxidizedPythonInterpreterConfig, NewInterpreterError},
    pyo3::ffi as pyffi,
    python_packaging::{
        interpreter::{CheckHashPycsMode, PythonInterpreterConfig, PythonInterpreterProfile},
        resource::BytecodeOptimizationLevel,
    },
    std::os::raw::c_int,
    std::{
        ffi::{CString, OsString},
        path::Path,
    },
};

#[cfg(target_family = "unix")]
use std::{ffi::NulError, os::unix::ffi::OsStrExt};

#[allow(non_camel_case_types)]
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
type wchar_t = u32;

#[allow(non_camel_case_types)]
#[cfg(all(
    target_family = "unix",
    not(all(target_arch = "aarch64", target_os = "linux"))
))]
type wchar_t = i32;

#[cfg(target_family = "windows")]
use std::os::windows::prelude::OsStrExt;

#[allow(non_camel_case_types)]
#[cfg(target_family = "windows")]
type wchar_t = u16;

/// Set a PyConfig string value from a str.
fn set_config_string_from_str(
    config: &pyffi::PyConfig,
    dest: &*mut wchar_t,
    value: &str,
    context: &str,
) -> Result<(), NewInterpreterError> {
    match CString::new(value) {
        Ok(value) => unsafe {
            let status = pyffi::PyConfig_SetBytesString(
                config as *const _ as *mut _,
                dest as *const *mut _ as *mut *mut _,
                value.as_ptr(),
            );
            if pyffi::PyStatus_Exception(status) != 0 {
                Err(NewInterpreterError::new_from_pystatus(&status, context))
            } else {
                Ok(())
            }
        },
        Err(_) => Err(NewInterpreterError::Dynamic(format!(
            "during {}: unable to convert {} to C string",
            context, value
        ))),
    }
}

#[cfg(unix)]
fn set_config_string_from_path(
    config: &pyffi::PyConfig,
    dest: &*mut wchar_t,
    path: &Path,
    context: &str,
) -> Result<(), NewInterpreterError> {
    let value = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| NewInterpreterError::Simple("cannot convert path to C string"))?;

    let status = unsafe {
        pyffi::PyConfig_SetBytesString(
            config as *const _ as *mut _,
            dest as *const *mut _ as *mut *mut _,
            value.as_ptr() as *const _,
        )
    };

    if unsafe { pyffi::PyStatus_Exception(status) } != 0 {
        Err(NewInterpreterError::new_from_pystatus(&status, context))
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
) -> Result<(), NewInterpreterError> {
    let status = unsafe {
        let mut value: Vec<wchar_t> = path.as_os_str().encode_wide().collect();
        // NULL terminate.
        value.push(0);

        pyffi::PyConfig_SetString(
            config as *const _ as *mut _,
            dest as *const *mut _ as *mut *mut _,
            value.as_ptr() as *const _,
        )
    };

    if unsafe { pyffi::PyStatus_Exception(status) } != 0 {
        Err(NewInterpreterError::new_from_pystatus(&status, context))
    } else {
        Ok(())
    }
}

/// Appends a value to a PyWideStringList from an 8-bit char* like source.
fn append_wide_string_list_from_str(
    dest: &mut pyffi::PyWideStringList,
    value: &str,
    context: &str,
) -> Result<(), NewInterpreterError> {
    let value = CString::new(value)
        .map_err(|_| NewInterpreterError::Simple("unable to convert value to C string"))?;

    let mut len: pyffi::Py_ssize_t = 0;

    let decoded = unsafe { pyffi::Py_DecodeLocale(value.as_ptr() as *const _, &mut len) };

    if decoded.is_null() {
        Err(NewInterpreterError::Dynamic(format!(
            "during {}: unable to decode value",
            context
        )))
    } else {
        let status = unsafe { pyffi::PyWideStringList_Append(dest as *mut _, decoded) };
        unsafe {
            pyffi::PyMem_RawFree(decoded as *mut _);
        }

        if unsafe { pyffi::PyStatus_Exception(status) } != 0 {
            Err(NewInterpreterError::new_from_pystatus(&status, context))
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
) -> Result<(), NewInterpreterError> {
    let value = path
        .as_os_str()
        .to_str()
        .ok_or(NewInterpreterError::Simple(
            "unable to convert value to str",
        ))?;

    append_wide_string_list_from_str(dest, value, context)
}

#[cfg(windows)]
fn append_wide_string_list_from_path(
    dest: &mut pyffi::PyWideStringList,
    path: &Path,
    context: &str,
) -> Result<(), NewInterpreterError> {
    let mut value: Vec<wchar_t> = path.as_os_str().encode_wide().collect();
    // NULL terminate.
    value.push(0);

    let status =
        unsafe { pyffi::PyWideStringList_Append(dest as *mut _, value.as_ptr() as *const _) };

    if unsafe { pyffi::PyStatus_Exception(status) } != 0 {
        Err(NewInterpreterError::new_from_pystatus(&status, context))
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

#[cfg(target_family = "unix")]
pub fn set_argv(
    config: &mut pyffi::PyConfig,
    args: &[OsString],
) -> Result<(), NewInterpreterError> {
    let argc = args.len() as isize;
    let argv = args
        .iter()
        .map(|x| CString::new(x.as_bytes()))
        .collect::<Result<Vec<_>, NulError>>()
        .map_err(|_| NewInterpreterError::Simple("unable to construct C string from OsString"))?;
    let argvp = argv
        .iter()
        .map(|x| x.as_ptr() as *mut i8)
        .collect::<Vec<_>>();

    let status =
        unsafe { pyffi::PyConfig_SetBytesArgv(config as *mut _, argc, argvp.as_ptr() as *mut _) };

    if unsafe { pyffi::PyStatus_Exception(status) } != 0 {
        Err(NewInterpreterError::new_from_pystatus(
            &status,
            "setting argv",
        ))
    } else {
        Ok(())
    }
}

#[cfg(target_family = "windows")]
pub fn set_argv(
    config: &mut pyffi::PyConfig,
    args: &[OsString],
) -> Result<(), NewInterpreterError> {
    let argc = args.len() as isize;
    let argv = args
        .iter()
        .map(|x| {
            let mut buffer = x.encode_wide().collect::<Vec<u16>>();
            buffer.push(0);

            buffer
        })
        .collect::<Vec<_>>();
    let argvp = argv
        .iter()
        .map(|x| x.as_ptr() as *mut u16)
        .collect::<Vec<_>>();

    let status =
        unsafe { pyffi::PyConfig_SetArgv(config as *mut _, argc, argvp.as_ptr() as *mut _) };

    if unsafe { pyffi::PyStatus_Exception(status) } != 0 {
        Err(NewInterpreterError::new_from_pystatus(
            &status,
            "setting argv",
        ))
    } else {
        Ok(())
    }
}

impl<'a> TryFrom<&ResolvedOxidizedPythonInterpreterConfig<'a>> for pyffi::PyPreConfig {
    type Error = NewInterpreterError;

    fn try_from(config: &ResolvedOxidizedPythonInterpreterConfig<'a>) -> Result<Self, Self::Error> {
        let value = &config.interpreter_config;

        let mut pre_config: pyffi::PyPreConfig = unsafe { core::mem::zeroed() };
        unsafe {
            match value.profile {
                PythonInterpreterProfile::Python => {
                    pyffi::PyPreConfig_InitPythonConfig(&mut pre_config)
                }
                PythonInterpreterProfile::Isolated => {
                    pyffi::PyPreConfig_InitIsolatedConfig(&mut pre_config)
                }
            }
        }

        if let Some(parse_argv) = value.parse_argv {
            pre_config.parse_argv = if parse_argv { 1 } else { 0 };
        }
        if let Some(isolated) = value.isolated {
            pre_config.isolated = if isolated { 1 } else { 0 };
        }
        if let Some(use_environment) = value.use_environment {
            pre_config.use_environment = if use_environment { 1 } else { 0 };
        }
        if let Some(configure_locale) = value.configure_locale {
            pre_config.configure_locale = if configure_locale { 1 } else { 0 };
        }
        if let Some(coerce_c_locale) = value.coerce_c_locale {
            pre_config.coerce_c_locale = coerce_c_locale as c_int;
        }
        if let Some(coerce_c_locale_warn) = value.coerce_c_locale_warn {
            pre_config.coerce_c_locale_warn = if coerce_c_locale_warn { 1 } else { 0 };
        }
        if let Some(legacy_windows_fs_encoding) = value.legacy_windows_fs_encoding {
            set_windows_fs_encoding(&mut pre_config, legacy_windows_fs_encoding);
        }
        if let Some(utf8_mode) = value.utf8_mode {
            pre_config.utf8_mode = if utf8_mode { 1 } else { 0 };
        }
        if let Some(dev_mode) = value.development_mode {
            pre_config.dev_mode = if dev_mode { 1 } else { 0 };
        }
        if let Some(allocator) = value.allocator {
            pre_config.allocator = allocator as c_int;
        }

        Ok(pre_config)
    }
}

pub fn python_interpreter_config_to_py_config(
    value: &PythonInterpreterConfig,
) -> Result<pyffi::PyConfig, NewInterpreterError> {
    let mut config: pyffi::PyConfig = unsafe { std::mem::zeroed() };
    unsafe {
        match value.profile {
            PythonInterpreterProfile::Isolated => pyffi::PyConfig_InitIsolatedConfig(&mut config),
            PythonInterpreterProfile::Python => pyffi::PyConfig_InitPythonConfig(&mut config),
        }
    }

    if let Some(isolated) = value.isolated {
        config.isolated = if isolated { 1 } else { 0 };
    }
    if let Some(use_environment) = value.use_environment {
        config.use_environment = if use_environment { 1 } else { 0 };
    }
    if let Some(dev_mode) = value.development_mode {
        config.dev_mode = if dev_mode { 1 } else { 0 };
    }
    if let Some(install_signal_handlers) = value.install_signal_handlers {
        config.install_signal_handlers = if install_signal_handlers { 1 } else { 0 };
    }
    if let Some(hash_seed) = value.hash_seed {
        config.hash_seed = hash_seed;
        config.use_hash_seed = 1;
    }
    if let Some(fault_handler) = value.fault_handler {
        config.faulthandler = if fault_handler { 1 } else { 0 };
    }
    if let Some(tracemalloc) = value.tracemalloc {
        config.tracemalloc = if tracemalloc { 1 } else { 0 };
    }
    if let Some(import_time) = value.import_time {
        config.import_time = if import_time { 1 } else { 0 };
    }
    if let Some(show_ref_count) = value.show_ref_count {
        config.show_ref_count = if show_ref_count { 1 } else { 0 };
    }
    if let Some(dump_refs) = value.dump_refs {
        config.dump_refs = if dump_refs { 1 } else { 0 };
    }
    if let Some(malloc_stats) = value.malloc_stats {
        config.malloc_stats = if malloc_stats { 1 } else { 0 };
    }
    if let Some(filesystem_encoding) = &value.filesystem_encoding {
        set_config_string_from_str(
            &config,
            &config.filesystem_encoding,
            filesystem_encoding,
            "setting filesystem_encoding",
        )?;
    }
    if let Some(filesystem_errors) = &value.filesystem_errors {
        set_config_string_from_str(
            &config,
            &config.filesystem_errors,
            filesystem_errors,
            "setting filesystem_errors",
        )?;
    }
    if let Some(pycache_prefix) = &value.pycache_prefix {
        set_config_string_from_path(
            &config,
            &config.pycache_prefix,
            pycache_prefix,
            "setting pycache_prefix",
        )?;
    }
    if let Some(parse_argv) = value.parse_argv {
        config.parse_argv = if parse_argv { 1 } else { 0 };
    }
    if let Some(argv) = &value.argv {
        set_argv(&mut config, argv)?;
    }
    if let Some(program_name) = &value.program_name {
        set_config_string_from_path(
            &config,
            &config.program_name,
            program_name,
            "setting program_name",
        )?;
    }
    if let Some(x_options) = &value.x_options {
        for value in x_options {
            append_wide_string_list_from_str(&mut config.xoptions, value, "setting xoption")?;
        }
    }
    if let Some(warn_options) = &value.warn_options {
        for value in warn_options {
            append_wide_string_list_from_str(
                &mut config.warnoptions,
                value,
                "setting warn_option",
            )?;
        }
    }
    if let Some(site_import) = value.site_import {
        config.site_import = if site_import { 1 } else { 0 };
    }
    if let Some(bytes_warning) = value.bytes_warning {
        config.bytes_warning = bytes_warning as i32;
    }
    if let Some(inspect) = value.inspect {
        config.inspect = if inspect { 1 } else { 0 };
    }
    if let Some(interactive) = value.interactive {
        config.interactive = if interactive { 1 } else { 0 };
    }
    if let Some(optimization_level) = value.optimization_level {
        config.optimization_level = match optimization_level {
            BytecodeOptimizationLevel::Zero => 0,
            BytecodeOptimizationLevel::One => 1,
            BytecodeOptimizationLevel::Two => 2,
        };
    }
    if let Some(parser_debug) = value.parser_debug {
        config.parser_debug = if parser_debug { 1 } else { 0 };
    }
    if let Some(write_bytecode) = value.write_bytecode {
        config.write_bytecode = if write_bytecode { 1 } else { 0 };
    }
    if let Some(verbose) = value.verbose {
        config.verbose = if verbose { 1 } else { 0 };
    }
    if let Some(quiet) = value.quiet {
        config.quiet = if quiet { 1 } else { 0 };
    }
    if let Some(user_site_directory) = value.user_site_directory {
        config.user_site_directory = if user_site_directory { 1 } else { 0 };
    }
    if let Some(configure_c_stdio) = value.configure_c_stdio {
        config.configure_c_stdio = if configure_c_stdio { 1 } else { 0 };
    }
    if let Some(buffered_stdio) = value.buffered_stdio {
        config.buffered_stdio = if buffered_stdio { 1 } else { 0 };
    }
    if let Some(stdio_encoding) = &value.stdio_encoding {
        set_config_string_from_str(
            &config,
            &config.stdio_encoding,
            stdio_encoding,
            "setting stdio_encoding",
        )?;
    }
    if let Some(stdio_errors) = &value.stdio_errors {
        set_config_string_from_str(
            &config,
            &config.stdio_errors,
            stdio_errors,
            "setting stdio_errors",
        )?;
    }
    if let Some(legacy_windows_stdio) = value.legacy_windows_stdio {
        set_legacy_windows_stdio(&mut config, legacy_windows_stdio);
    }

    if let Some(check_hash_pycs_mode) = value.check_hash_pycs_mode {
        set_config_string_from_str(
            &config,
            &config.check_hash_pycs_mode,
            match check_hash_pycs_mode {
                CheckHashPycsMode::Always => "always",
                CheckHashPycsMode::Never => "never",
                CheckHashPycsMode::Default => "default",
            },
            "setting check_hash_pycs_mode",
        )?;
    }
    if let Some(pathconfig_warnings) = value.pathconfig_warnings {
        config.pathconfig_warnings = if pathconfig_warnings { 1 } else { 0 };
    }
    if let Some(python_path_env) = &value.python_path_env {
        set_config_string_from_str(
            &config,
            &config.pythonpath_env,
            python_path_env,
            "setting pythonpath_env",
        )?;
    }

    if let Some(home) = &value.home {
        set_config_string_from_path(&config, &config.home, home, "setting home")?;
    }
    if let Some(module_search_paths) = &value.module_search_paths {
        config.module_search_paths_set = 1;

        for path in module_search_paths {
            append_wide_string_list_from_path(
                &mut config.module_search_paths,
                path,
                "setting module_search_paths",
            )?;
        }
    }
    if let Some(executable) = &value.executable {
        set_config_string_from_path(
            &config,
            &config.executable,
            executable,
            "setting executable",
        )?;
    }
    if let Some(base_executable) = &value.base_executable {
        set_config_string_from_path(
            &config,
            &config.base_executable,
            base_executable,
            "setting base_executable",
        )?;
    }
    if let Some(prefix) = &value.prefix {
        set_config_string_from_path(&config, &config.prefix, prefix, "setting prefix")?;
    }
    if let Some(base_prefix) = &value.base_prefix {
        set_config_string_from_path(
            &config,
            &config.base_prefix,
            base_prefix,
            "setting base_prefix",
        )?;
    }
    if let Some(exec_prefix) = &value.exec_prefix {
        set_config_string_from_path(
            &config,
            &config.exec_prefix,
            exec_prefix,
            "setting exec_prefix",
        )?;
    }
    if let Some(base_exec_prefix) = &value.base_exec_prefix {
        set_config_string_from_path(
            &config,
            &config.base_exec_prefix,
            base_exec_prefix,
            "setting base_exec_prefix",
        )?;
    }
    if let Some(skip_source_first_line) = value.skip_first_source_line {
        config.skip_source_first_line = if skip_source_first_line { 1 } else { 0 };
    }
    if let Some(run_command) = &value.run_command {
        set_config_string_from_str(
            &config,
            &config.run_command,
            run_command,
            "setting run_command",
        )?;
    }
    if let Some(run_module) = &value.run_module {
        set_config_string_from_str(
            &config,
            &config.run_module,
            run_module,
            "setting run_module",
        )?;
    }
    if let Some(run_filename) = &value.run_filename {
        set_config_string_from_path(
            &config,
            &config.run_filename,
            run_filename,
            "setting run_filename",
        )?;
    }

    Ok(config)
}

impl<'a> TryInto<pyffi::PyConfig> for &'a ResolvedOxidizedPythonInterpreterConfig<'a> {
    type Error = NewInterpreterError;

    fn try_into(self) -> Result<pyffi::PyConfig, Self::Error> {
        // We use the raw configuration as a base then we apply any adjustments,
        // as needed.
        let mut config: pyffi::PyConfig =
            python_interpreter_config_to_py_config(&self.interpreter_config)?;

        if let Some(argv) = &self.argv {
            set_argv(&mut config, argv)?;
        }

        if self.exe.is_none() {
            return Err(NewInterpreterError::Simple(
                "current executable not set; must call ensure_origin() 1st",
            ));
        }
        if self.origin.is_none() {
            return Err(NewInterpreterError::Simple(
                "origin not set; must call ensure_origin() 1st",
            ));
        }
        let exe = self.exe.as_ref().unwrap();
        let origin = self.origin.as_ref().unwrap();

        if self.set_missing_path_configuration {
            // program_name set to path of current executable.
            if self.interpreter_config.program_name.is_none() {
                set_config_string_from_path(
                    &config,
                    &config.program_name,
                    exe,
                    "setting program_name",
                )?;
            }

            // PYTHONHOME is set to directory of current executable.
            if self.interpreter_config.home.is_none() {
                set_config_string_from_path(&config, &config.home, origin, "setting home")?;
            }
        }

        Ok(config)
    }
}
