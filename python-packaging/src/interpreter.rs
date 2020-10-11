// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Functionality related to running Python interpreters. */

use {
    crate::resource::BytecodeOptimizationLevel,
    std::{ffi::OsString, os::raw::c_ulong, path::PathBuf},
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

/// Defines Python code to run.
#[derive(Clone, Debug, PartialEq)]
pub enum PythonRunMode {
    /// No-op.
    None,
    /// Run a Python REPL.
    Repl,
    /// Run a Python module as the main module.
    Module { module: String },
    /// Evaluate Python code from a string.
    Eval { code: String },
    /// Execute Python code in a file.
    ///
    /// We define this as a CString because the underlying API wants
    /// a char* and we want the constructor of this type to worry about
    /// the type coercion.
    File { path: PathBuf },
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

/// Defines a backend for a memory allocator.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MemoryAllocatorBackend {
    /// The default system allocator.
    System,
    /// Use jemalloc.
    Jemalloc,
    /// Use Rust's global allocator.
    Rust,
}

/// Defines configuration for Python's raw allocator.
///
/// This allocator is what Python uses for all memory allocations.
///
/// See https://docs.python.org/3/c-api/memory.html for more.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PythonRawAllocator {
    /// Which allocator backend to use.
    pub backend: MemoryAllocatorBackend,
    /// Whether memory debugging should be enabled.
    pub debug: bool,
}

impl PythonRawAllocator {
    pub fn system() -> Self {
        Self {
            backend: MemoryAllocatorBackend::System,
            ..PythonRawAllocator::default()
        }
    }

    pub fn jemalloc() -> Self {
        Self {
            backend: MemoryAllocatorBackend::Jemalloc,
            ..PythonRawAllocator::default()
        }
    }

    pub fn rust() -> Self {
        Self {
            backend: MemoryAllocatorBackend::Rust,
            ..PythonRawAllocator::default()
        }
    }
}

impl Default for PythonRawAllocator {
    fn default() -> Self {
        Self {
            backend: if cfg!(windows) {
                MemoryAllocatorBackend::System
            } else {
                MemoryAllocatorBackend::Jemalloc
            },
            debug: false,
        }
    }
}

/// Holds values for coerce_c_locale.
///
/// See https://docs.python.org/3/c-api/init_config.html#c.PyPreConfig.coerce_c_locale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CoerceCLocale {
    LCCtype = 1,
    C = 2,
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
pub enum CheckHashPYCsMode {
    Always,
    Never,
    Default,
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
    pub check_hash_pycs_mode: Option<CheckHashPYCsMode>,

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

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.show_alloc_count.
    pub show_alloc_count: Option<bool>,

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
