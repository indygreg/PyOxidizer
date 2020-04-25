// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Manage an embedded Python interpreter.

use {
    super::config::{PythonConfig, PythonRawAllocator, PythonRunMode, TerminfoResolution},
    super::importer::{initialize_importer, PyInit__pyoxidizer_importer},
    super::osutils::resolve_terminfo_dirs,
    super::pyalloc::{make_raw_rust_memory_allocator, RawAllocator},
    super::pystr::{osstr_to_pyobject, osstring_to_bytes},
    cpython::exc::{SystemExit, ValueError},
    cpython::{
        GILGuard, NoArgs, ObjectProtocol, PyClone, PyDict, PyErr, PyList, PyModule, PyObject,
        PyResult, PyString, Python, PythonObject, ToPyObject,
    },
    libc::{c_char, wchar_t},
    python3_sys as pyffi,
    std::collections::BTreeSet,
    std::env,
    std::ffi::{CStr, CString},
    std::fmt::{Display, Formatter},
    std::fs,
    std::io::Write,
    std::path::{Path, PathBuf},
};

#[cfg(unix)]
use {libc::size_t, std::os::unix::ffi::OsStrExt};

#[cfg(target_family = "windows")]
use std::os::windows::prelude::OsStrExt;

#[cfg(feature = "jemalloc-sys")]
use super::pyalloc::make_raw_jemalloc_allocator;

const PYOXIDIZER_IMPORTER_NAME_STR: &str = "_pyoxidizer_importer";
pub const PYOXIDIZER_IMPORTER_NAME: &[u8] = b"_pyoxidizer_importer\0";

/// Represents the results of executing Python code with exception handling.
#[derive(Debug)]
pub enum PythonRunResult {
    /// Code executed without raising an exception.
    Ok {},
    /// Code executed and raised an exception.
    Err {},
    /// Code executed and raised SystemExit with the specified exit code.
    Exit { code: i32 },
}

#[cfg(windows)]
extern "C" {
    pub fn __acrt_iob_func(x: u32) -> *mut libc::FILE;
}

#[cfg(windows)]
fn stdin_to_file() -> *mut libc::FILE {
    // The stdin symbol is made available by importing <stdio.h>. On Windows,
    // stdin is defined in corecrt_wstdio.h as a `#define` that calls this
    // internal CRT function. There's no exported symbol to use. So we
    // emulate the behavior of the C code.
    //
    // Relying on an internal CRT symbol is probably wrong. But Microsoft
    // typically keeps backwards compatibility for undocumented functions
    // like this because people use them in the wild.
    //
    // An attempt was made to use fdopen(0) like we do on POSIX. However,
    // this causes a crash. The Microsoft C Runtime is already bending over
    // backwards to coerce its native HANDLEs into POSIX file descriptors.
    // Even if there are other ways to coerce a FILE* from a HANDLE
    // (_open_osfhandle() + _fdopen() might work), using the same function
    // that <stdio.h> uses to obtain a FILE* seems like the least risky thing
    // to do.
    unsafe { __acrt_iob_func(0) }
}

#[cfg(unix)]
fn stdin_to_file() -> *mut libc::FILE {
    unsafe { libc::fdopen(libc::STDIN_FILENO, &('r' as libc::c_char)) }
}

#[cfg(windows)]
fn stderr_to_file() -> *mut libc::FILE {
    unsafe { __acrt_iob_func(2) }
}

#[cfg(unix)]
fn stderr_to_file() -> *mut libc::FILE {
    unsafe { libc::fdopen(libc::STDERR_FILENO, &('w' as libc::c_char)) }
}

#[cfg(unix)]
fn set_windows_fs_encoding(_config: &PythonConfig, _pre_config: &mut pyffi::PyPreConfig) {}

#[cfg(windows)]
fn set_windows_fs_encoding(config: &PythonConfig, pre_config: &mut pyffi::PyPreConfig) {
    pre_config.legacy_windows_fs_encoding = if config.legacy_windows_fs_encoding {
        1
    } else {
        0
    };
}

#[cfg(feature = "jemalloc-sys")]
fn raw_jemallocator() -> pyffi::PyMemAllocatorEx {
    make_raw_jemalloc_allocator()
}

#[cfg(not(feature = "jemalloc-sys"))]
fn raw_jemallocator() -> pyffi::PyMemAllocatorEx {
    panic!("jemalloc is not available in this build configuration");
}

#[cfg(unix)]
fn set_config_string_from_path(
    config: &pyffi::PyConfig,
    dest: &*mut wchar_t,
    path: &Path,
) -> pyffi::PyStatus {
    unsafe {
        pyffi::PyConfig_SetBytesString(
            config as *const _ as *mut _,
            dest as *const *mut _ as *mut *mut _,
            path.as_os_str().as_bytes().as_ptr() as *const _,
        )
    }
}

#[cfg(windows)]
fn set_config_string_from_path(
    config: &pyffi::PyConfig,
    dest: &*mut wchar_t,
    path: &Path,
) -> pyffi::PyStatus {
    unsafe {
        let value: Vec<wchar_t> = path.as_os_str().encode_wide().collect();

        pyffi::PyConfig_SetString(
            config as *const _ as *mut _,
            dest as *const *mut _ as *mut *mut _,
            value.as_ptr() as *const _,
        )
    }
}

#[cfg(unix)]
fn append_wide_string_list_from_path(
    dest: &mut pyffi::PyWideStringList,
    path: &Path,
) -> pyffi::PyStatus {
    let mut len: size_t = 0;

    let decoded = unsafe {
        pyffi::Py_DecodeLocale(path.as_os_str().as_bytes().as_ptr() as *const _, &mut len)
    };

    if decoded.is_null() {
        unsafe {
            pyffi::PyStatus_Error(
                CStr::from_bytes_with_nul_unchecked(b"unable to decode path\0").as_ptr(),
            )
        }
    } else {
        let res = unsafe { pyffi::PyWideStringList_Append(dest as *mut _, decoded) };
        unsafe {
            pyffi::PyMem_RawFree(decoded as *mut _);
        }
        res
    }
}

#[cfg(windows)]
fn append_wide_string_list_from_path(
    dest: &mut pyffi::PyWideStringList,
    path: &Path,
) -> pyffi::PyStatus {
    unsafe {
        let value: Vec<wchar_t> = path.as_os_str().encode_wide().collect();

        pyffi::PyWideStringList_Append(dest as *mut _, value.as_ptr() as *const _)
    }
}

#[cfg(unix)]
fn set_windows_flags(_config: &PythonConfig, _py_config: &mut pyffi::PyConfig) {}

#[cfg(windows)]
fn set_windows_flags(config: &PythonConfig, py_config: &mut pyffi::PyConfig) {
    py_config.legacy_windows_stdio = if config.legacy_windows_stdio { 1 } else { 0 };
}

/// Format a PyErr in a crude manner.
///
/// This is meant to be called during interpreter initialization. We can't
/// call PyErr_Print() because sys.stdout may not be available yet.
fn format_pyerr(py: Python, err: PyErr) -> Result<String, &'static str> {
    let type_repr = err
        .ptype
        .repr(py)
        .or_else(|_| Err("unable to get repr of error type"))?;

    if let Some(value) = &err.pvalue {
        let value_repr = value
            .repr(py)
            .or_else(|_| Err("unable to get repr of error value"))?;

        let value = format!(
            "{}: {}",
            type_repr.to_string_lossy(py),
            value_repr.to_string_lossy(py)
        );

        Ok(value)
    } else {
        Ok(type_repr.to_string_lossy(py).to_string())
    }
}

/// Represents an error encountered when creating an embedded Python interpreter.
pub enum NewInterpreterError {
    Simple(&'static str),
    Dynamic(String),
}

impl From<&'static str> for NewInterpreterError {
    fn from(v: &'static str) -> Self {
        Self::Simple(v)
    }
}

impl Display for NewInterpreterError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match &self {
            Self::Simple(value) => value.fmt(f),
            Self::Dynamic(value) => value.fmt(f),
        }
    }
}

impl NewInterpreterError {
    pub fn new_from_pyerr(py: Python, err: PyErr, context: &str) -> Self {
        match format_pyerr(py, err) {
            Ok(value) => Self::Dynamic(format!("during {}: {}", context, value)),
            Err(msg) => Self::Dynamic(format!("during {}: {}", context, msg)),
        }
    }

    pub fn new_from_pystatus(status: &pyffi::PyStatus, context: &str) -> Self {
        if !status.func.is_null() && !status.err_msg.is_null() {
            let func = unsafe { CStr::from_ptr(status.func) };
            let msg = unsafe { CStr::from_ptr(status.err_msg) };

            Self::Dynamic(format!(
                "during {}: {}: {}",
                context,
                func.to_string_lossy(),
                msg.to_string_lossy()
            ))
        } else if !status.err_msg.is_null() {
            let msg = unsafe { CStr::from_ptr(status.err_msg) };

            Self::Dynamic(format!("during {}: {}", context, msg.to_string_lossy()))
        } else {
            Self::Dynamic(format!("during {}: could not format PyStatus", context))
        }
    }
}

/// Manages an embedded Python interpreter.
///
/// **Warning: Python interpreters have global state. There should only be a
/// single instance of this type per process.**
///
/// Instances must only be constructed through [`MainPythonInterpreter::new()`](#method.new).
///
/// This type and its various functionality is a glorified wrapper around the
/// Python C API. But there's a lot of added functionality on top of what the C
/// API provides.
///
/// Both the low-level `python3-sys` and higher-level `cpython` crates are used.
pub struct MainPythonInterpreter<'a> {
    pub config: PythonConfig,
    init_run: bool,
    raw_allocator: Option<pyffi::PyMemAllocatorEx>,
    raw_rust_allocator: Option<RawAllocator>,
    gil: Option<GILGuard>,
    py: Option<Python<'a>>,
}

impl<'a> MainPythonInterpreter<'a> {
    /// Construct a Python interpreter from a configuration.
    ///
    /// The Python interpreter is initialized as a side-effect. The GIL is held.
    pub fn new(config: PythonConfig) -> Result<MainPythonInterpreter<'a>, NewInterpreterError> {
        match config.terminfo_resolution {
            TerminfoResolution::Dynamic => {
                if let Some(v) = resolve_terminfo_dirs() {
                    env::set_var("TERMINFO_DIRS", &v);
                }
            }
            TerminfoResolution::Static(ref v) => {
                env::set_var("TERMINFO_DIRS", v);
            }
            TerminfoResolution::None => {}
        }

        let (raw_allocator, raw_rust_allocator) = match config.raw_allocator {
            PythonRawAllocator::Jemalloc => (Some(raw_jemallocator()), None),
            PythonRawAllocator::Rust => (None, Some(make_raw_rust_memory_allocator())),
            PythonRawAllocator::System => (None, None),
        };

        let mut res = MainPythonInterpreter {
            config,
            init_run: false,
            raw_allocator,
            raw_rust_allocator,
            gil: None,
            py: None,
        };

        res.init()?;

        Ok(res)
    }

    /// Initialize the interpreter.
    ///
    /// This mutates global state in the Python interpreter according to the
    /// bound config and initializes the Python interpreter.
    ///
    /// After this is called, the embedded Python interpreter is ready to
    /// execute custom code.
    ///
    /// If called more than once, the function is a no-op from the perspective
    /// of interpreter initialization.
    ///
    /// Returns a Python instance which has the GIL acquired.
    fn init(&mut self) -> Result<Python, NewInterpreterError> {
        if self.init_run {
            return Ok(self.acquire_gil());
        }

        let config = &self.config;

        let exe = env::current_exe()
            .or_else(|_| Err(NewInterpreterError::Simple("could not obtain current exe")))?;
        let origin = exe
            .parent()
            .ok_or_else(|| NewInterpreterError::Simple("unable to get exe parent"))?
            .to_path_buf();
        let origin_string = origin.display().to_string();

        // Pre-configure Python.

        let mut pre_config = pyffi::PyPreConfig::default();
        unsafe {
            pyffi::PyPreConfig_InitIsolatedConfig(&mut pre_config);
        }

        // TODO add dedicated config field for argument parsing.
        pre_config.parse_argv = if config.ignore_python_env { 0 } else { 1 };
        // Side-effects:
        //
        // * Disables sys.path magic
        // * Python REPL doesn’t import readline nor enable default readline configuration
        //   on interactive prompts.
        // * Set use_environment and user_site_directory to 0.
        pre_config.isolated = 1;
        pre_config.use_environment = if config.ignore_python_env { 0 } else { 1 };
        // TODO handle configure_locale, coerce_c_locale.
        set_windows_fs_encoding(config, &mut pre_config);
        // TODO set utf8_mode
        // TODO set dev_mode

        // Default.
        pre_config.allocator = 0;

        unsafe {
            let status = pyffi::Py_PreInitialize(&pre_config);

            if pyffi::PyStatus_Exception(status) != 0 {
                return Err(NewInterpreterError::new_from_pystatus(
                    &status,
                    "Python pre-initialization",
                ));
            }
        };

        // TODO should we call PyMem::SetupDebugHooks() if enabled?
        if let Some(raw_allocator) = &self.raw_allocator {
            unsafe {
                let ptr = raw_allocator as *const _;
                pyffi::PyMem_SetAllocator(
                    pyffi::PyMemAllocatorDomain::PYMEM_DOMAIN_RAW,
                    ptr as *mut _,
                );
            }
        } else if let Some(raw_rust_allocator) = &self.raw_rust_allocator {
            unsafe {
                let ptr = &raw_rust_allocator.allocator as *const _;
                pyffi::PyMem_SetAllocator(
                    pyffi::PyMemAllocatorDomain::PYMEM_DOMAIN_RAW,
                    ptr as *mut _,
                );
            }
        }

        let mut py_config = pyffi::PyConfig::default();
        unsafe {
            pyffi::PyConfig_InitIsolatedConfig(&mut py_config);
        }

        py_config.bytes_warning = config.bytes_warning;
        py_config.parser_debug = if config.parser_debug { 1 } else { 0 };
        py_config.write_bytecode = if config.write_bytecode { 1 } else { 0 };
        py_config.use_environment = if config.ignore_python_env { 0 } else { 1 };
        py_config.interactive = if config.interactive { 1 } else { 0 };
        py_config.inspect = if config.inspect { 1 } else { 0 };
        py_config.isolated = if config.isolated { 1 } else { 0 };
        py_config.site_import = if config.import_site { 1 } else { 0 };
        py_config.user_site_directory = if config.import_user_site { 1 } else { 0 };
        py_config.optimization_level = config.opt_level;
        py_config.quiet = if config.quiet { 1 } else { 0 };
        py_config.buffered_stdio = if config.unbuffered_stdio { 0 } else { 1 };
        py_config.verbose = config.verbose;

        // We control the paths. Don't let Python initialize it.
        py_config.module_search_paths_set = 1;

        for path in &config.sys_paths {
            let path = PathBuf::from(path.replace("$ORIGIN", &origin_string));
            let status =
                append_wide_string_list_from_path(&mut py_config.module_search_paths, &path);
            if unsafe { pyffi::PyStatus_Exception(status) } != 0 {
                return Err(NewInterpreterError::new_from_pystatus(
                    &status,
                    "setting module search path",
                ));
            }
        }

        // Set PYTHONHOME to directory of current executable.
        let status = set_config_string_from_path(&py_config, &py_config.home, &origin);
        if unsafe { pyffi::PyStatus_Exception(status) } != 0 {
            return Err(NewInterpreterError::new_from_pystatus(
                &status,
                "setting Python home directory",
            ));
        }

        // Set program name to path of current executable.
        let status = set_config_string_from_path(&py_config, &py_config.program_name, &exe);
        if unsafe { pyffi::PyStatus_Exception(status) } != 0 {
            return Err(NewInterpreterError::new_from_pystatus(
                &status,
                "setting program name",
            ));
        }

        // Set stdio encoding and error handling.
        if let (Some(ref encoding), Some(ref errors)) =
            (&config.standard_io_encoding, &config.standard_io_errors)
        {
            let cencoding = CString::new(encoding.clone()).or_else(|_| {
                Err(NewInterpreterError::Simple(
                    "unable to convert encoding to C string",
                ))
            })?;
            let cerrors = CString::new(errors.clone()).or_else(|_| {
                Err(NewInterpreterError::Simple(
                    "unable to convert encoding error mode to C string",
                ))
            })?;

            unsafe {
                let status = pyffi::PyConfig_SetBytesString(
                    &mut py_config,
                    &mut py_config.stdio_encoding,
                    cencoding.as_ptr(),
                );
                if pyffi::PyStatus_Exception(status) != 0 {
                    return Err(NewInterpreterError::new_from_pystatus(
                        &status,
                        "setting stdio encoding",
                    ));
                }

                let status = pyffi::PyConfig_SetBytesString(
                    &mut py_config,
                    &mut py_config.stdio_errors,
                    cerrors.as_ptr(),
                );
                if pyffi::PyStatus_Exception(status) != 0 {
                    return Err(NewInterpreterError::new_from_pystatus(
                        &status,
                        "setting stdio error handler",
                    ));
                }
            }
        }

        unsafe {
            let encoding = CString::new("utf-8").unwrap();
            pyffi::PyConfig_SetBytesString(
                &mut py_config,
                &mut py_config.filesystem_encoding,
                encoding.as_ptr(),
            );
            let errors = CString::new("strict").unwrap();
            pyffi::PyConfig_SetBytesString(
                &mut py_config,
                &mut py_config.filesystem_errors,
                errors.as_ptr(),
            );
        }

        set_windows_flags(config, &mut py_config);

        // Enable multi-phase initialization. This allows us to initialize
        // our custom importer before Python attempts any imports.
        py_config._init_main = 0;

        if config.use_custom_importlib {
            // Register our _pyoxidizer_importer extension which provides importing functionality.
            unsafe {
                // name char* needs to live as long as the interpreter is active.
                pyffi::PyImport_AppendInittab(
                    PYOXIDIZER_IMPORTER_NAME.as_ptr() as *const i8,
                    Some(PyInit__pyoxidizer_importer),
                );
            }
        }

        // TODO call PyImport_ExtendInitTab to avoid O(n) overhead.
        for e in &config.extra_extension_modules {
            let res = unsafe {
                pyffi::PyImport_AppendInittab(e.name.as_ptr() as *const i8, Some(e.init_func))
            };

            if res != 0 {
                return Err(NewInterpreterError::Simple(
                    "unable to register extension module",
                ));
            }
        }

        let status = unsafe { pyffi::Py_InitializeFromConfig(&py_config) };
        if unsafe { pyffi::PyStatus_Exception(status) } != 0 {
            return Err(NewInterpreterError::new_from_pystatus(
                &status,
                "initializing Python core",
            ));
        }

        // At this point, the core of Python is initialized.
        // importlib._bootstrap has been loaded. But not
        // importlib._bootstrap_external. This is where we work our magic to
        // inject our custom importer.

        let py = unsafe { Python::assume_gil_acquired() };

        if config.use_custom_importlib {
            let oxidized_importer = py.import(PYOXIDIZER_IMPORTER_NAME_STR).or_else(|err| {
                Err(NewInterpreterError::new_from_pyerr(
                    py,
                    err,
                    "import of oxidized importer module",
                ))
            })?;

            initialize_importer(py, &oxidized_importer, exe, origin, config.packed_resources)
                .or_else(|err| {
                    Err(NewInterpreterError::new_from_pyerr(
                        py,
                        err,
                        "initialization of oxidized importer",
                    ))
                })?;
        }

        // Now proceed with the Python main initialization. This will initialize
        // importlib. And if the custom importlib bytecode was registered above,
        // our extension module will get imported and initialized.
        let status = unsafe { pyffi::_Py_InitializeMain() };

        if unsafe { pyffi::PyStatus_Exception(status) } != 0 {
            return Err(NewInterpreterError::new_from_pystatus(
                &status,
                "initializing Python main",
            ));
        }

        // When the main initialization ran, it initialized the "external"
        // importer (importlib._bootstrap_external). Our meta path importer
        // should have been registered first and would have been used for
        // all imports, if configured for such.
        //
        // Here, we remove the filesystem importer if we aren't configured
        // to use it. Ideally there would be a field on PyConfig to disable
        // just the external importer. But there isn't. The only field
        // controls both internal and external bootstrap modules and when
        // set it will disable a lot of "main" initialization.
        if !config.filesystem_importer {
            let sys_module = py.import("sys").or_else(|err| {
                Err(NewInterpreterError::new_from_pyerr(
                    py,
                    err,
                    "obtaining sys module",
                ))
            })?;
            let meta_path = sys_module.get(py, "meta_path").or_else(|err| {
                Err(NewInterpreterError::new_from_pyerr(
                    py,
                    err,
                    "obtaining sys.meta_path",
                ))
            })?;
            meta_path
                .call_method(py, "pop", NoArgs, None)
                .or_else(|err| {
                    Err(NewInterpreterError::new_from_pyerr(
                        py,
                        err,
                        "sys.meta_path.pop()",
                    ))
                })?;
        }

        unsafe {
            // TODO we could potentially have the config be an Option<i32> so we can control
            // the hash seed explicitly. But the APIs in Python 3.7 aren't great here, as we'd
            // need to set an environment variable. Once we support the new initialization
            // API in Python 3.8, things will be easier to implement.
            pyffi::Py_HashRandomizationFlag = if config.use_hash_seed { 1 } else { 0 };
        }

        /* Pre-initialization functions we could support:
         *
         * PyObject_SetArenaAllocator()
         * PySys_AddWarnOption()
         * PySys_AddXOption()
         * PySys_ResetWarnOptions()
         */

        self.py = Some(py);
        self.init_run = true;

        // env::args() panics if arguments aren't valid Unicode. But invalid
        // Unicode arguments are possible and some applications may want to
        // support them.
        //
        // env::args_os() provides access to the raw OsString instances, which
        // will be derived from wchar_t on Windows and char* on POSIX. We can
        // convert these to Python str instances using a platform-specific
        // mechanism.
        let args_objs = env::args_os()
            .map(|os_arg| osstr_to_pyobject(py, &os_arg, None))
            .collect::<Result<Vec<PyObject>, &'static str>>()?;

        // This will steal the pointer to the elements and mem::forget them.
        let args = PyList::new(py, &args_objs);
        let argv = b"argv\0";

        let res = args.with_borrowed_ptr(py, |args_ptr| unsafe {
            pyffi::PySys_SetObject(argv.as_ptr() as *const i8, args_ptr)
        });

        match res {
            0 => (),
            _ => return Err(NewInterpreterError::Simple("unable to set sys.argv")),
        }

        if config.argvb {
            let args_objs: Vec<PyObject> = env::args_os()
                .map(|os_arg| osstring_to_bytes(py, os_arg))
                .collect();

            let args = PyList::new(py, &args_objs);
            let argvb = b"argvb\0";

            let res = args.with_borrowed_ptr(py, |args_ptr| unsafe {
                pyffi::PySys_SetObject(argvb.as_ptr() as *const i8, args_ptr)
            });

            match res {
                0 => (),
                _ => return Err(NewInterpreterError::Simple("unable to set sys.argvb")),
            }
        }

        // As a convention, sys.oxidized is set to indicate we are running from
        // a self-contained application.
        let oxidized = b"oxidized\0";

        let res = py.True().with_borrowed_ptr(py, |py_true| unsafe {
            pyffi::PySys_SetObject(oxidized.as_ptr() as *const i8, py_true)
        });

        match res {
            0 => (),
            _ => return Err(NewInterpreterError::Simple("unable to set sys.oxidized")),
        }

        if config.sys_frozen {
            let frozen = b"frozen\0";

            match py.True().with_borrowed_ptr(py, |py_true| unsafe {
                pyffi::PySys_SetObject(frozen.as_ptr() as *const i8, py_true)
            }) {
                0 => (),
                _ => return Err(NewInterpreterError::Simple("unable to set sys.frozen")),
            }
        }

        if config.sys_meipass {
            let meipass = b"_MEIPASS\0";
            let value = PyString::new(py, &origin_string);

            match value.with_borrowed_ptr(py, |py_value| unsafe {
                pyffi::PySys_SetObject(meipass.as_ptr() as *const i8, py_value)
            }) {
                0 => (),
                _ => return Err(NewInterpreterError::Simple("unable to set sys._MEIPASS")),
            }
        }

        Ok(py)
    }

    /// Ensure the Python GIL is released.
    pub fn release_gil(&mut self) {
        if self.py.is_some() {
            self.py = None;
            self.gil = None;
        }
    }

    /// Ensure the Python GIL is acquired, returning a handle on the interpreter.
    pub fn acquire_gil(&mut self) -> Python<'a> {
        match self.py {
            Some(py) => py,
            None => {
                let gil = GILGuard::acquire();
                let py = unsafe { Python::assume_gil_acquired() };

                self.gil = Some(gil);
                self.py = Some(py);

                py
            }
        }
    }

    /// Runs the interpreter with the default code execution settings.
    ///
    /// The crate was built with settings that configure what should be
    /// executed by default. Those settings will be loaded and executed.
    pub fn run(&mut self) -> PyResult<PyObject> {
        // clone() to avoid issues mixing mutable and immutable borrows of self.
        let run = self.config.run.clone();

        let py = self.acquire_gil();

        match run {
            PythonRunMode::None => Ok(py.None()),
            PythonRunMode::Repl => self.run_repl(),
            PythonRunMode::Module { module } => self.run_module_as_main(&module),
            PythonRunMode::Eval { code } => self.run_code(&code),
            PythonRunMode::File { path } => self.run_file(&path),
        }
    }

    /// Handle a raised SystemExit exception.
    ///
    /// This emulates the behavior in pythonrun.c:handle_system_exit() and
    /// _Py_HandleSystemExit() but without the call to exit(), which we don't want.
    fn handle_system_exit(&mut self, py: Python, err: PyErr) -> Result<i32, &'static str> {
        std::io::stdout()
            .flush()
            .or_else(|_| Err("failed to flush stdout"))?;

        let mut value = match err.pvalue {
            Some(ref instance) => {
                if instance.as_ptr() == py.None().as_ptr() {
                    return Ok(0);
                }

                instance.clone_ref(py)
            }
            None => {
                return Ok(0);
            }
        };

        if unsafe { pyffi::PyExceptionInstance_Check(value.as_ptr()) } != 0 {
            // The error code should be in the "code" attribute.
            if let Ok(code) = value.getattr(py, "code") {
                if code == py.None() {
                    return Ok(0);
                }

                // Else pretend exc_value.code is the new exception value to use
                // and fall through to below.
                value = code;
            }
        }

        if unsafe { pyffi::PyLong_Check(value.as_ptr()) } != 0 {
            return Ok(unsafe { pyffi::PyLong_AsLong(value.as_ptr()) as i32 });
        }

        let sys_module = py
            .import("sys")
            .or_else(|_| Err("unable to obtain sys module"))?;
        let stderr = sys_module.get(py, "stderr");

        // This is a cargo cult from the canonical implementation.
        unsafe { pyffi::PyErr_Clear() }

        match stderr {
            Ok(o) => unsafe {
                pyffi::PyFile_WriteObject(value.as_ptr(), o.as_ptr(), pyffi::Py_PRINT_RAW);
            },
            Err(_) => {
                unsafe {
                    pyffi::PyObject_Print(value.as_ptr(), stderr_to_file(), pyffi::Py_PRINT_RAW);
                }
                std::io::stderr()
                    .flush()
                    .or_else(|_| Err("failure to flush stderr"))?;
            }
        }

        unsafe {
            pyffi::PySys_WriteStderr(b"\n\0".as_ptr() as *const i8);
        }

        // This frees references to this exception, which may be necessary to avoid
        // badness.
        err.restore(py);
        unsafe {
            pyffi::PyErr_Clear();
        }

        Ok(1)
    }

    /// Runs the interpreter and handles any exception that was raised.
    pub fn run_and_handle_error(&mut self) -> PythonRunResult {
        // There are underdefined lifetime bugs at play here. There is no
        // explicit lifetime for the PyObject's returned. If we don't have
        // the local variable in scope, we can get into a situation where
        // drop() on self is called before the PyObject's drop(). This is
        // problematic because PyObject's drop() attempts to acquire the GIL.
        // If the interpreter is shut down, there is no GIL to acquire, and
        // we may segfault.
        // TODO look into setting lifetimes properly so the compiler can
        // prevent some issues.
        let res = self.run();
        let py = self.acquire_gil();

        match res {
            Ok(_) => PythonRunResult::Ok {},
            Err(err) => {
                // SystemExit is special in that PyErr_PrintEx() will call
                // exit() if it is seen. So, we handle it manually so we can
                // return an exit code instead of exiting.

                // TODO surely the cpython crate offers a better way to do this...
                err.restore(py);
                let matches =
                    unsafe { pyffi::PyErr_ExceptionMatches(pyffi::PyExc_SystemExit) } != 0;
                let err = cpython::PyErr::fetch(py);

                if matches {
                    return PythonRunResult::Exit {
                        code: match self.handle_system_exit(py, err) {
                            Ok(code) => code,
                            Err(msg) => {
                                eprintln!("{}", msg);
                                1
                            }
                        },
                    };
                }

                self.print_err(err);

                PythonRunResult::Err {}
            }
        }
    }

    /// Calls run() and resolves a suitable exit code.
    pub fn run_as_main(&mut self) -> i32 {
        match self.run_and_handle_error() {
            PythonRunResult::Ok {} => 0,
            PythonRunResult::Err {} => 1,
            PythonRunResult::Exit { code } => code,
        }
    }

    /// Runs a Python module as the __main__ module.
    ///
    /// Returns the execution result of the module code.
    ///
    /// The interpreter is automatically initialized if needed.
    pub fn run_module_as_main(&mut self, name: &str) -> PyResult<PyObject> {
        let py = self.acquire_gil();

        // This is modeled after runpy.py:_run_module_as_main().
        let main: PyModule = unsafe {
            PyObject::from_borrowed_ptr(
                py,
                pyffi::PyImport_AddModule("__main__\0".as_ptr() as *const c_char),
            )
            .cast_into(py)?
        };

        let main_dict = main.dict(py);

        let importlib_util = py.import("importlib.util")?;
        let spec = importlib_util.call(py, "find_spec", (name,), None)?;
        let loader = spec.getattr(py, "loader")?;
        let code = loader.call_method(py, "get_code", (name,), None)?;

        let origin = spec.getattr(py, "origin")?;
        let cached = spec.getattr(py, "cached")?;

        // TODO handle __package__.
        main_dict.set_item(py, "__name__", "__main__")?;
        main_dict.set_item(py, "__file__", origin)?;
        main_dict.set_item(py, "__cached__", cached)?;
        main_dict.set_item(py, "__doc__", py.None())?;
        main_dict.set_item(py, "__loader__", loader)?;
        main_dict.set_item(py, "__spec__", spec)?;

        unsafe {
            let globals = main_dict.as_object().as_ptr();
            let res = pyffi::PyEval_EvalCode(code.as_ptr(), globals, globals);

            if res.is_null() {
                let err = PyErr::fetch(py);
                err.print(py);
                Err(PyErr::fetch(py))
            } else {
                Ok(PyObject::from_owned_ptr(py, res))
            }
        }
    }

    /// Start and run a Python REPL.
    ///
    /// This emulates what CPython's main.c does.
    ///
    /// The interpreter is automatically initialized if needed.
    pub fn run_repl(&mut self) -> PyResult<PyObject> {
        let py = self.acquire_gil();

        unsafe {
            pyffi::Py_InspectFlag = 0;
        }

        // readline is optional. We don't care if it fails.
        if py.import("readline").is_ok() {}

        let sys = py.import("sys")?;

        if let Ok(hook) = sys.get(py, "__interactivehook__") {
            hook.call(py, NoArgs, None)?;
        }

        let stdin_filename = "<stdin>";
        let filename = CString::new(stdin_filename)
            .or_else(|_| Err(PyErr::new::<ValueError, _>(py, "could not create CString")))?;
        let mut cf = pyffi::PyCompilerFlags {
            cf_flags: 0,
            cf_feature_version: 0,
        };

        unsafe {
            let stdin = stdin_to_file();
            let res =
                pyffi::PyRun_AnyFileExFlags(stdin, filename.as_ptr() as *const c_char, 0, &mut cf);

            if res == 0 {
                Ok(py.None())
            } else {
                Err(PyErr::new::<SystemExit, _>(py, 1))
            }
        }
    }

    /// Runs Python code provided by a string.
    ///
    /// This is similar to what ``python -c <code>`` would do.
    ///
    /// The interpreter is automatically initialized if needed.
    pub fn run_code(&mut self, code: &str) -> PyResult<PyObject> {
        let py = self.acquire_gil();

        let code = CString::new(code).or_else(|_| {
            Err(PyErr::new::<ValueError, _>(
                py,
                "source code is not a valid C string",
            ))
        })?;

        unsafe {
            let main = pyffi::PyImport_AddModule("__main__\0".as_ptr() as *const _);

            if main.is_null() {
                return Err(PyErr::fetch(py));
            }

            let main_dict = pyffi::PyModule_GetDict(main);

            let res = pyffi::PyRun_StringFlags(
                code.as_ptr() as *const _,
                pyffi::Py_file_input,
                main_dict,
                main_dict,
                std::ptr::null_mut(),
            );

            if res.is_null() {
                Err(PyErr::fetch(py))
            } else {
                Ok(PyObject::from_owned_ptr(py, res))
            }
        }
    }

    /// Runs Python code in a filesystem path.
    pub fn run_file(&mut self, filename: &CStr) -> PyResult<PyObject> {
        let py = self.acquire_gil();

        let res = unsafe {
            let fp = libc::fopen(filename.as_ptr(), "rb\0".as_ptr() as *const _);
            let mut cf = pyffi::PyCompilerFlags {
                cf_flags: 0,
                cf_feature_version: 0,
            };

            pyffi::PyRun_AnyFileExFlags(fp, filename.as_ptr(), 1, &mut cf)
        };

        if res == 0 {
            Ok(py.None())
        } else {
            Err(PyErr::new::<SystemExit, _>(py, 1))
        }
    }

    /// Print a Python error.
    ///
    /// Under the hood this calls ``PyErr_PrintEx()``, which may call
    /// ``Py_Exit()`` and may write to stderr.
    pub fn print_err(&mut self, err: PyErr) {
        let py = self.acquire_gil();
        err.print(py);
    }
}

/// Write loaded Python modules to a directory.
///
/// Given a Python interpreter and a path to a directory, this will create a
/// file in that directory named ``modules-<UUID>`` and write a ``\n`` delimited
/// list of loaded names from ``sys.modules`` into that file.
fn write_modules_to_directory(py: Python, path: &PathBuf) -> Result<(), &'static str> {
    // TODO this needs better error handling all over.

    fs::create_dir_all(path).or_else(|_| Err("could not create directory for modules"))?;

    let rand = uuid::Uuid::new_v4();

    let path = path.join(format!("modules-{}", rand.to_string()));

    let sys = py
        .import("sys")
        .or_else(|_| Err("could not obtain sys module"))?;
    let modules = sys
        .get(py, "modules")
        .or_else(|_| Err("could not obtain sys.modules"))?;

    let modules = modules
        .cast_as::<PyDict>(py)
        .or_else(|_| Err("sys.modules is not a dict"))?;

    let mut names = BTreeSet::new();
    for (key, _value) in modules.items(py) {
        names.insert(
            key.extract::<String>(py)
                .or_else(|_| Err("module name is not a str"))?,
        );
    }

    let mut f = fs::File::create(path).or_else(|_| Err("could not open file for writing"))?;

    for name in names {
        f.write_fmt(format_args!("{}\n", name))
            .or_else(|_| Err("could not write"))?;
    }

    Ok(())
}

impl<'a> Drop for MainPythonInterpreter<'a> {
    fn drop(&mut self) {
        if let Some(key) = &self.config.write_modules_directory_env {
            if let Ok(path) = env::var(key) {
                let path = PathBuf::from(path);
                let py = self.acquire_gil();

                if let Err(msg) = write_modules_to_directory(py, &path) {
                    eprintln!("error writing modules file: {}", msg);
                }
            }
        }

        let _ = unsafe { pyffi::Py_FinalizeEx() };
    }
}
