// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Functionality related to running Python interpreters. */

use {
    crate::resource::BytecodeOptimizationLevel,
    std::{convert::TryFrom, ffi::OsString, os::raw::c_ulong, path::PathBuf},
};

/// Defines the profile to use to configure a Python interpreter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PythonInterpreterProfile {
    /// Python is isolated from the system.
    ///
    /// See https://docs.python.org/3/c-api/init_config.html#isolated-configuration.
    Isolated,

    /// Python interpreter behaves like `python`.
    ///
    /// See https://docs.python.org/3/c-api/init_config.html#python-configuration.
    Python,
}

impl Default for PythonInterpreterProfile {
    fn default() -> Self {
        PythonInterpreterProfile::Isolated
    }
}

impl ToString for PythonInterpreterProfile {
    fn to_string(&self) -> String {
        match self {
            Self::Isolated => "isolated",
            Self::Python => "python",
        }
        .to_string()
    }
}

impl TryFrom<&str> for PythonInterpreterProfile {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "isolated" => Ok(Self::Isolated),
            "python" => Ok(Self::Python),
            _ => Err(format!(
                "{} is not a valid profile; use 'isolated' or 'python'",
                value
            )),
        }
    }
}

/// Defines `terminfo`` database resolution semantics.
#[derive(Clone, Debug, PartialEq)]
pub enum TerminfoResolution {
    /// Resolve `terminfo` database using appropriate behavior for current OS.
    Dynamic,
    /// Do not attempt to resolve the `terminfo` database. Basically a no-op.
    None,
    /// Use a specified string as the `TERMINFO_DIRS` value.
    Static(String),
}

impl ToString for TerminfoResolution {
    fn to_string(&self) -> String {
        match self {
            Self::Dynamic => "dynamic".to_string(),
            Self::None => "none".to_string(),
            Self::Static(value) => format!("static:{}", value),
        }
    }
}

impl TryFrom<&str> for TerminfoResolution {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        if value == "dynamic" {
            Ok(Self::Dynamic)
        } else if value == "none" {
            Ok(Self::None)
        } else if let Some(suffix) = value.strip_prefix("static:") {
            Ok(Self::Static(suffix.to_string()))
        } else {
            Err(format!(
                "{} is not a valid terminfo resolution value",
                value
            ))
        }
    }
}

/// Defines a backend for a memory allocator.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MemoryAllocatorBackend {
    /// The default allocator as configured by Python.
    Default,
    /// Use jemalloc.
    Jemalloc,
    /// Use Mimalloc.
    Mimalloc,
    /// Use Snmalloc.
    Snmalloc,
    /// Use Rust's global allocator.
    Rust,
}

impl Default for MemoryAllocatorBackend {
    fn default() -> Self {
        if cfg!(windows) {
            Self::Default
        } else {
            Self::Jemalloc
        }
    }
}

impl ToString for MemoryAllocatorBackend {
    fn to_string(&self) -> String {
        match self {
            Self::Default => "default",
            Self::Jemalloc => "jemalloc",
            Self::Mimalloc => "mimalloc",
            Self::Snmalloc => "snmalloc",
            Self::Rust => "rust",
        }
        .to_string()
    }
}

impl TryFrom<&str> for MemoryAllocatorBackend {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "default" => Ok(Self::Default),
            "jemalloc" => Ok(Self::Jemalloc),
            "mimalloc" => Ok(Self::Mimalloc),
            "snmalloc" => Ok(Self::Snmalloc),
            "rust" => Ok(Self::Rust),
            _ => Err(format!("{} is not a valid memory allocator backend", value)),
        }
    }
}

/// Holds values for coerce_c_locale.
///
/// See https://docs.python.org/3/c-api/init_config.html#c.PyPreConfig.coerce_c_locale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CoerceCLocale {
    #[allow(clippy::upper_case_acronyms)]
    LCCtype = 1,
    C = 2,
}

impl ToString for CoerceCLocale {
    fn to_string(&self) -> String {
        match self {
            Self::LCCtype => "LC_CTYPE",
            Self::C => "C",
        }
        .to_string()
    }
}

impl TryFrom<&str> for CoerceCLocale {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "LC_CTYPE" => Ok(Self::LCCtype),
            "C" => Ok(Self::C),
            _ => Err(format!("{} is not a valid C locale coercion value", value)),
        }
    }
}

/// Defines what to do when comparing bytes with str.
///
/// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.bytes_warning.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BytesWarning {
    None = 0,
    Warn = 1,
    Raise = 2,
}

impl ToString for BytesWarning {
    fn to_string(&self) -> String {
        match self {
            Self::None => "none",
            Self::Warn => "warn",
            Self::Raise => "raise",
        }
        .to_string()
    }
}

impl TryFrom<&str> for BytesWarning {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "none" => Ok(Self::None),
            "warn" => Ok(Self::Warn),
            "raise" => Ok(Self::Raise),
            _ => Err(format!("{} is not a valid bytes warning value", value)),
        }
    }
}

impl From<i32> for BytesWarning {
    fn from(value: i32) -> BytesWarning {
        match value {
            0 => Self::None,
            1 => Self::Warn,
            _ => Self::Raise,
        }
    }
}

/// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.check_hash_pycs_mode.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CheckHashPycsMode {
    Always,
    Never,
    Default,
}

impl ToString for CheckHashPycsMode {
    fn to_string(&self) -> String {
        match self {
            Self::Always => "always",
            Self::Never => "never",
            Self::Default => "default",
        }
        .to_string()
    }
}

impl TryFrom<&str> for CheckHashPycsMode {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "always" => Ok(Self::Always),
            "never" => Ok(Self::Never),
            "default" => Ok(Self::Default),
            _ => Err(format!(
                "{} is not a valid check hash pycs mode value",
                value
            )),
        }
    }
}

/// See https://docs.python.org/3/c-api/init_config.html#c.PyPreConfig.allocator.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Allocator {
    NotSet = 0,
    Default = 1,
    Debug = 2,
    Malloc = 3,
    MallocDebug = 4,
    PyMalloc = 5,
    PyMallocDebug = 6,
}

impl ToString for Allocator {
    fn to_string(&self) -> String {
        match self {
            Self::NotSet => "not-set",
            Self::Default => "default",
            Self::Debug => "debug",
            Self::Malloc => "malloc",
            Self::MallocDebug => "malloc-debug",
            Self::PyMalloc => "py-malloc",
            Self::PyMallocDebug => "py-malloc-debug",
        }
        .to_string()
    }
}

impl TryFrom<&str> for Allocator {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "not-set" => Ok(Self::NotSet),
            "default" => Ok(Self::Default),
            "debug" => Ok(Self::Debug),
            "malloc" => Ok(Self::Malloc),
            "malloc-debug" => Ok(Self::MallocDebug),
            "py-malloc" => Ok(Self::PyMalloc),
            "py-malloc-debug" => Ok(Self::PyMallocDebug),
            _ => Err(format!("{} is not a valid allocator value", value)),
        }
    }
}

/// Holds configuration of a Python interpreter.
///
/// This struct holds fields that are exposed by `PyPreConfig` and
/// `PyConfig` in the CPython API.
///
/// Other than the profile (which is used to initialize instances of
/// `PyPreConfig` and `PyConfig`), all fields are optional. Only fields
/// with `Some(T)` will be updated from the defaults.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PythonInterpreterConfig {
    /// Profile to use to initialize pre-config and config state of interpreter.
    pub profile: PythonInterpreterProfile,

    // The following fields are from PyPreConfig or are shared with PyConfig.
    /// See https://docs.python.org/3/c-api/init_config.html#c.PyPreConfig.allocator.
    pub allocator: Option<Allocator>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyPreConfig.configure_locale.
    pub configure_locale: Option<bool>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyPreConfig.coerce_c_locale.
    pub coerce_c_locale: Option<CoerceCLocale>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyPreConfig.coerce_c_locale_warn.
    pub coerce_c_locale_warn: Option<bool>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.dev_mode.
    pub development_mode: Option<bool>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyPreConfig.isolated.
    pub isolated: Option<bool>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyPreConfig.legacy_windows_fs_encoding.
    pub legacy_windows_fs_encoding: Option<bool>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyPreConfig.parse_argv.
    pub parse_argv: Option<bool>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.use_environment.
    pub use_environment: Option<bool>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyPreConfig.utf8_mode.
    pub utf8_mode: Option<bool>,
    // The following fields are from PyConfig.
    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.argv.
    pub argv: Option<Vec<OsString>>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.base_exec_prefix.
    pub base_exec_prefix: Option<PathBuf>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.base_executable.
    pub base_executable: Option<PathBuf>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.base_prefix.
    pub base_prefix: Option<PathBuf>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.buffered_stdio.
    pub buffered_stdio: Option<bool>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.bytes_warning.
    pub bytes_warning: Option<BytesWarning>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.check_hash_pycs_mode.
    pub check_hash_pycs_mode: Option<CheckHashPycsMode>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.configure_c_stdio.
    pub configure_c_stdio: Option<bool>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.dump_refs.
    pub dump_refs: Option<bool>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.exec_prefix.
    pub exec_prefix: Option<PathBuf>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.executable.
    pub executable: Option<PathBuf>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.faulthandler.
    pub fault_handler: Option<bool>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.filesystem_encoding.
    pub filesystem_encoding: Option<String>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.filesystem_errors.
    pub filesystem_errors: Option<String>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.hash_seed.
    pub hash_seed: Option<c_ulong>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.home.
    pub home: Option<PathBuf>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.import_time.
    pub import_time: Option<bool>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.inspect.
    pub inspect: Option<bool>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.install_signal_handlers.
    pub install_signal_handlers: Option<bool>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.interactive.
    pub interactive: Option<bool>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.legacy_windows_stdio.
    pub legacy_windows_stdio: Option<bool>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.malloc_stats.
    pub malloc_stats: Option<bool>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.module_search_paths.
    pub module_search_paths: Option<Vec<PathBuf>>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.optimization_level.
    pub optimization_level: Option<BytecodeOptimizationLevel>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.parser_debug.
    pub parser_debug: Option<bool>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.pathconfig_warnings.
    pub pathconfig_warnings: Option<bool>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.prefix.
    pub prefix: Option<PathBuf>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.program_name.
    pub program_name: Option<PathBuf>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.pycache_prefix.
    pub pycache_prefix: Option<PathBuf>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.pythonpath_env.
    pub python_path_env: Option<String>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.quiet.
    pub quiet: Option<bool>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.run_command.
    pub run_command: Option<String>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.run_filename.
    pub run_filename: Option<PathBuf>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.run_module.
    pub run_module: Option<String>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.show_ref_count.
    pub show_ref_count: Option<bool>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.site_import.
    pub site_import: Option<bool>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.skip_source_first_line.
    pub skip_first_source_line: Option<bool>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.stdio_encoding.
    pub stdio_encoding: Option<String>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.stdio_errors.
    pub stdio_errors: Option<String>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.tracemalloc.
    pub tracemalloc: Option<bool>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.user_site_directory.
    pub user_site_directory: Option<bool>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.verbose.
    pub verbose: Option<bool>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.warnoptions.
    pub warn_options: Option<Vec<String>>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.write_bytecode.
    pub write_bytecode: Option<bool>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.xoptions.
    pub x_options: Option<Vec<String>>,
}
