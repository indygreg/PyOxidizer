// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Data structures for configuring a Python interpreter.

use {
    libc::c_ulong,
    python3_sys as pyffi,
    python_packaging::interpreter::{PythonRawAllocator, TerminfoResolution},
    std::ffi::{CString, OsString},
    std::path::PathBuf,
};

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

/// Defines an extra extension module to load.
#[derive(Clone, Debug)]
pub struct ExtensionModule {
    /// Name of the extension module.
    pub name: CString,

    /// Extension module initialization function.
    pub init_func: unsafe extern "C" fn() -> *mut pyffi::PyObject,
}

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

/// See https://docs.python.org/3/c-api/init_config.html#c.PyPreConfig.allocator.
#[derive(Clone, Copy, Debug)]
pub enum Allocator {
    NotSet = 0,
    Default = 1,
    Debug = 2,
    Malloc = 3,
    MallocDebug = 4,
    PyMalloc = 5,
    PyMallocDebug = 6,
}

/// Holds values for coerce_c_locale.
///
/// See https://docs.python.org/3/c-api/init_config.html#c.PyPreConfig.coerce_c_locale.
#[derive(Clone, Copy, Debug)]
pub enum CoerceCLocale {
    LCCtype = 1,
    C = 2,
}

/// Defines what to do when comparing bytes with str.
///
/// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.bytes_warning.
#[derive(Clone, Copy, Debug)]
pub enum BytesWarning {
    None = 0,
    Warn = 1,
    Raise = 2,
}

/// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.check_hash_pycs_mode.
#[derive(Clone, Copy, Debug)]
pub enum CheckHashPYCsMode {
    Always,
    Never,
    Default,
}

/// Optimization level for bytecode.
///
/// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.optimization_level.
#[derive(Clone, Copy, Debug)]
pub enum OptimizationLevel {
    Zero = 0,
    One = 1,
    Two = 2,
}

/// Holds configuration of a Python interpreter.
///
/// This struct holds fields that are exposed by `PyPreConfig` and
/// `PyConfig` in the CPython API.
///
/// Other than the profile (which is used to initialize instances of
/// `PyPreConfig` and `PyConfig`), all fields are optional. Only fields
/// with `Some(T)` will be updated from the defaults.
#[derive(Clone, Debug, Default)]
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

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.pythonpath_env.
    pub python_path_env: Option<String>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.module_search_paths.
    pub module_search_paths: Option<Vec<PathBuf>>,

    /// See https://docs.python.org/3/c-api/init_config.html#c.PyConfig.optimization_level.
    pub optimization_level: Option<OptimizationLevel>,

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

/// Configure a Python interpreter.
///
/// This type defines the configuration of a Python interpreter. It is used
/// to initialize a Python interpreter embedded in the current process.
///
/// The type contains a reference to a `PythonInterpreterConfig` instance,
/// which is an abstraction over the low-level C structs that Python uses during
/// interpreter initialization.
///
/// The `PythonInterpreterConfig` has a single non-optional field: `profile`.
/// This defines the defaults for various fields of the `PyPreConfig` and
/// `PyConfig` instances that are initialized as part of interpreter
/// initialization. See
/// https://docs.python.org/3/c-api/init_config.html#isolated-configuration for
/// more.
///
/// During interpreter initialization, we produce a `PyPreConfig` and
/// `PyConfig` derived from this type. Config settings are applied in
/// layers. First, we use the `PythonInterpreterConfig.profile` to derive
/// a default instance given a profile. Next, we override fields if the
/// `PythonInterpreterConfig` has `Some(T)` value set. Finally, we populate
/// some fields if they are missing but required for the given configuration.
/// For example, when in *isolated* mode, we set `program_name` and `home`
/// unless an explicit value was provided in the `PythonInterpreterConfig`.
///
/// Generally speaking, the `PythonInterpreterConfig` exists to hold
/// configuration that is defined in the CPython initialization and
/// configuration API and `OxidizedPythonInterpreterConfig` exists to
/// hold higher-level configuration for features specific to this crate.
#[derive(Clone, Debug)]
pub struct OxidizedPythonInterpreterConfig<'a> {
    /// Low-level configuration of Python interpreter.
    pub interpreter_config: PythonInterpreterConfig,

    /// Allocator to use for Python's raw allocator.
    pub raw_allocator: Option<PythonRawAllocator>,

    /// Whether to install our custom meta path importer on interpreter init.
    pub oxidized_importer: bool,

    /// Whether to install the default `PathFinder` meta path finder.
    pub filesystem_importer: bool,

    /// Reference to packed resources data.
    ///
    /// The referenced data contains Python module data. It likely comes from an
    /// `include_bytes!(...)` of a file generated by PyOxidizer.
    ///
    /// The format of the data is defined by the ``python-packed-resources``
    /// crate. The data will be parsed as part of initializing the custom
    /// meta path importer during interpreter initialization.
    pub packed_resources: Option<&'a [u8]>,

    /// Extra extension modules to make available to the interpreter.
    ///
    /// The values will effectively be passed to ``PyImport_ExtendInitTab()``.
    pub extra_extension_modules: Option<Vec<ExtensionModule>>,

    /// Whether to set sys.argvb with bytes versions of process arguments.
    ///
    /// On Windows, bytes will be UTF-16. On POSIX, bytes will be raw char*
    /// values passed to `int main()`.
    pub argvb: bool,

    /// Whether to set sys.frozen=True.
    ///
    /// Setting this will enable Python to emulate "frozen" binaries, such as
    /// those used by PyInstaller.
    pub sys_frozen: bool,

    /// Whether to set sys._MEIPASS to the directory of the executable.
    ///
    /// Setting this will enable Python to emulate PyInstaller's behavior
    /// of setting this attribute.
    pub sys_meipass: bool,

    /// How to resolve the `terminfo` database.
    pub terminfo_resolution: TerminfoResolution,

    /// Environment variable holding the directory to write a loaded modules file.
    ///
    /// If this value is set and the environment it refers to is set,
    /// on interpreter shutdown, we will write a ``modules-<random>`` file to
    /// the directory specified containing a ``\n`` delimited list of modules
    /// loaded in ``sys.modules``.
    pub write_modules_directory_env: Option<String>,

    /// Defines what code to run by default.
    ///
    pub run: PythonRunMode,
}

impl<'a> Default for OxidizedPythonInterpreterConfig<'a> {
    fn default() -> Self {
        Self {
            interpreter_config: PythonInterpreterConfig {
                profile: PythonInterpreterProfile::Python,
                ..PythonInterpreterConfig::default()
            },
            raw_allocator: None,
            oxidized_importer: false,
            filesystem_importer: true,
            packed_resources: None,
            extra_extension_modules: None,
            argvb: false,
            sys_frozen: false,
            sys_meipass: false,
            terminfo_resolution: TerminfoResolution::Dynamic,
            write_modules_directory_env: None,
            run: PythonRunMode::Repl,
        }
    }
}
