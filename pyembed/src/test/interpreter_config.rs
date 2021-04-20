// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    super::{default_interpreter_config, PYTHON_INTERPRETER_PATH},
    crate::{MainPythonInterpreter, OxidizedPythonInterpreterConfig},
    cpython::{ObjectProtocol, PyBytes, PyList, PyObject, PyString, PyStringData},
    python3_sys as pyffi,
    python_packaging::{
        interpreter::{BytesWarning, MemoryAllocatorBackend, PythonInterpreterProfile},
        resource::BytecodeOptimizationLevel,
    },
    rusty_fork::rusty_fork_test,
    std::{convert::TryInto, ffi::OsString, path::PathBuf},
};

#[cfg(target_family = "unix")]
use std::os::unix::ffi::OsStringExt;

#[cfg(target_family = "windows")]
use std::os::windows::ffi::OsStringExt;

#[cfg(target_family = "unix")]
fn get_unicode_argument() -> OsString {
    // 中文
    OsString::from_vec([0xe4, 0xb8, 0xad, 0xe6, 0x96, 0x87].to_vec())
}

#[cfg(target_family = "windows")]
fn get_unicode_argument() -> OsString {
    // 中文
    OsString::from_wide(&[0x2d4e, 0x8765])
}

fn reprs(py: cpython::Python, container: &PyObject) -> cpython::PyResult<Vec<String>> {
    let mut names = Vec::new();
    for x in container.iter(py)? {
        names.push(x?.to_string());
    }
    Ok(names)
}

fn assert_importer(oxidized: bool, filesystem: bool) {
    let mut config = default_interpreter_config();

    config.oxidized_importer = oxidized;
    config.filesystem_importer = filesystem;
    let mut interp = MainPythonInterpreter::new(config).unwrap();

    let py = interp.acquire_gil();
    let sys = py.import("sys").unwrap();
    let meta_path_reprs = reprs(py, &sys.get(py, "meta_path").unwrap()).unwrap();
    let path_hook_reprs = reprs(py, &sys.get(py, "path_hooks").unwrap()).unwrap();
    const PATH_HOOK_REPR: &str = "built-in method path_hook of OxidizedFinder object";

    // OxidizedFinder should be installed sys.meta_path and sys.path_hooks as first
    // element when enabled. Its presence also replaced BuiltinImporter and
    // FrozenImporter.
    if oxidized {
        assert!(meta_path_reprs[0].contains("OxidizedFinder"));
        assert!(path_hook_reprs[0].contains(PATH_HOOK_REPR));

        assert!(meta_path_reprs
            .iter()
            .all(|s| !s.contains("_frozen_importlib.BuiltinImporter")));
        assert!(meta_path_reprs
            .iter()
            .all(|s| !s.contains("_frozen_importlib.FrozenImporter")));
    } else {
        assert!(meta_path_reprs
            .iter()
            .all(|s| !s.contains("OxidizedFinder")));
        assert!(path_hook_reprs.iter().all(|s| !s.contains(PATH_HOOK_REPR)));

        assert!(meta_path_reprs
            .iter()
            .any(|s| s.contains("_frozen_importlib.BuiltinImporter")));
        assert!(meta_path_reprs
            .iter()
            .any(|s| s.contains("_frozen_importlib.FrozenImporter")));
    }

    // PathFinder should only be present when filesystem importer is enabled.
    if filesystem {
        assert!(meta_path_reprs
            .last()
            .unwrap()
            .contains("_frozen_importlib_external.PathFinder"));
    } else {
        assert!(meta_path_reprs
            .iter()
            .all(|s| !s.contains("_frozen_importlib_external.PathFinder")));
    }
}

rusty_fork_test! {
    #[test]
    fn test_default_interpreter() {
        let config = default_interpreter_config();
        let mut interp = MainPythonInterpreter::new(config).unwrap();

        let py = interp.acquire_gil();
        let sys = py.import("sys").unwrap();
        let meta_path = sys.get(py, "meta_path").unwrap();
        assert_eq!(meta_path.len(py).unwrap(), 3);

        let importer = meta_path.get_item(py, 0).unwrap();
        assert!(importer
            .to_string()
            .contains("_frozen_importlib.BuiltinImporter"));
        let importer = meta_path.get_item(py, 1).unwrap();
        assert!(importer
            .to_string()
            .contains("_frozen_importlib.FrozenImporter"));
        let importer = meta_path.get_item(py, 2).unwrap();
        assert!(importer
            .to_string()
            .contains("_frozen_importlib_external.PathFinder"));
    }

    #[test]
    fn test_importer_oxidized() {
        assert_importer(true, false);
    }

    #[test]
    fn test_importer_oxidized_filesystem() {
        assert_importer(true, true);
    }

    #[test]
    fn test_importer_filesystem() {
        assert_importer(false, true);
    }

    #[test]
    fn test_importer_neither() {
        assert_importer(false, false);
    }

    #[test]
    fn test_isolated_interpreter() {
        let mut config = default_interpreter_config();
        config.interpreter_config.profile = PythonInterpreterProfile::Isolated;

        let mut interp = MainPythonInterpreter::new(config).unwrap();

        let py = interp.acquire_gil();
        let sys = py.import("sys").unwrap();
        let flags = sys.get(py, "flags").unwrap();

        assert_eq!(
            flags
                .getattr(py, "isolated")
                .unwrap()
                .extract::<i32>(py)
                .unwrap(),
            1
        );
    }

    #[test]
    fn test_allocator_default() {
        let mut config = default_interpreter_config();

        config.allocator_backend = MemoryAllocatorBackend::Default;

        let interp = MainPythonInterpreter::new(config).unwrap();

        assert!(interp.allocator.is_none());
    }

    #[test]
    fn test_allocator_rust() {
        let mut config = default_interpreter_config();

        config.allocator_backend = MemoryAllocatorBackend::Rust;
        config.allocator_raw = true;
        config.allocator_mem = true;
        config.allocator_obj = true;

        let interp = MainPythonInterpreter::new(config).unwrap();

        assert!(interp.allocator.is_some());
        assert_eq!(interp.allocator.as_ref().unwrap().backend(), MemoryAllocatorBackend::Rust);
    }

    #[test]
    fn test_allocator_rust_pymalloc_arena() {
        let mut config = default_interpreter_config();

        config.allocator_backend = MemoryAllocatorBackend::Rust;
        config.allocator_raw = true;
        config.allocator_pymalloc_arena = true;

        let interp = MainPythonInterpreter::new(config).unwrap();

        assert!(interp.allocator.is_some());
        assert_eq!(interp.allocator.as_ref().unwrap().backend(), MemoryAllocatorBackend::Rust);
    }

    #[cfg(feature = "jemalloc")]
    #[test]
    fn test_allocator_jemalloc() {
        let mut config = default_interpreter_config();

        config.allocator_backend = MemoryAllocatorBackend::Jemalloc;
        config.allocator_raw = true;
        config.allocator_mem = true;
        config.allocator_obj = true;

        let interp = MainPythonInterpreter::new(config).unwrap();

        assert!(interp.allocator.is_some());
        assert_eq!(interp.allocator.as_ref().unwrap().backend(), MemoryAllocatorBackend::Jemalloc);
    }

    #[cfg(feature = "jemalloc")]
    #[test]
    fn test_allocator_jemalloc_pymalloc_arena() {
        let mut config = default_interpreter_config();

        config.allocator_backend = MemoryAllocatorBackend::Jemalloc;
        config.allocator_raw = true;
        config.allocator_pymalloc_arena = true;

        let interp = MainPythonInterpreter::new(config).unwrap();

        assert!(interp.allocator.is_some());
        assert_eq!(interp.allocator.as_ref().unwrap().backend(), MemoryAllocatorBackend::Jemalloc);
    }

    #[cfg(feature = "mimalloc")]
    #[test]
    fn test_allocator_mimalloc() {
        let mut config = default_interpreter_config();

        config.allocator_backend = MemoryAllocatorBackend::Mimalloc;
        config.allocator_raw = true;
        config.allocator_mem = true;
        config.allocator_obj = true;

        let interp = MainPythonInterpreter::new(config).unwrap();

        assert!(interp.allocator.is_some());
        assert_eq!(interp.allocator.as_ref().unwrap().backend(), MemoryAllocatorBackend::Mimalloc);
    }

    #[cfg(feature = "mimalloc")]
    #[test]
    fn test_allocator_mimalloc_pymalloc_arena() {
        let mut config = default_interpreter_config();

        config.allocator_backend = MemoryAllocatorBackend::Mimalloc;
        config.allocator_raw = true;
        config.allocator_pymalloc_arena = true;

        let interp = MainPythonInterpreter::new(config).unwrap();

        assert!(interp.allocator.is_some());
        assert_eq!(interp.allocator.as_ref().unwrap().backend(), MemoryAllocatorBackend::Mimalloc);
    }

    #[cfg(feature = "snmalloc")]
    #[test]
    fn test_allocator_snmalloc() {
        let mut config = default_interpreter_config();

        config.allocator_backend = MemoryAllocatorBackend::Snmalloc;
        config.allocator_raw = true;
        config.allocator_mem = true;
        config.allocator_obj = true;

        let interp = MainPythonInterpreter::new(config).unwrap();

        assert!(interp.allocator.is_some());
        assert_eq!(interp.allocator.as_ref().unwrap().backend(), MemoryAllocatorBackend::Snmalloc);
    }

    #[cfg(feature = "snmalloc")]
    #[test]
    fn test_allocator_snmalloc_pymalloc_arena() {
        let mut config = default_interpreter_config();

        config.allocator_backend = MemoryAllocatorBackend::Snmalloc;
        config.allocator_raw = true;
        config.allocator_pymalloc_arena = true;

        let interp = MainPythonInterpreter::new(config).unwrap();

        assert!(interp.allocator.is_some());
        assert_eq!(interp.allocator.as_ref().unwrap().backend(), MemoryAllocatorBackend::Snmalloc);
    }

    #[test]
    fn test_allocator_debug() {
        let mut config = default_interpreter_config();

        config.allocator_debug = true;

        MainPythonInterpreter::new(config).unwrap();
    }

    #[test]
    fn test_allocator_debug_custom_backend() {
        let mut config = default_interpreter_config();

        config.allocator_backend = MemoryAllocatorBackend::Rust;
        config.allocator_raw = true;
        config.allocator_debug = true;

        MainPythonInterpreter::new(config).unwrap();
    }

    #[test]
    fn test_sys_paths_origin() {
        let mut config = OxidizedPythonInterpreterConfig::default();
        // Otherwise the Rust arguments are interpreted as Python arguments.
        config.interpreter_config.parse_argv = Some(false);
        config.interpreter_config.module_search_paths = Some(vec![PathBuf::from("$ORIGIN/lib")]);

        let config = config.resolve().unwrap();

        let origin = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();

        assert_eq!(
            &config.interpreter_config.module_search_paths,
            &Some(vec![PathBuf::from(format!("{}/lib", origin.display()))])
        );

        let py_config: pyffi::PyConfig = (&config).try_into().unwrap();

        assert_eq!(py_config.module_search_paths_set, 1);
        assert_eq!(py_config.module_search_paths.length, 1);
    }

    /// sys.argv is initialized using the Rust process's arguments by default.
    #[test]
    fn test_argv_default() {
        let mut config = default_interpreter_config();
        // Undo defaults from default_interpreter_config().
        config.argv = None;

        let mut interp = MainPythonInterpreter::new(config).unwrap();

        let py = interp.acquire_gil();
        let sys = py.import("sys").unwrap();

        let argv = sys
            .get(py, "argv")
            .unwrap()
            .extract::<Vec<String>>(py)
            .unwrap();
        let rust_args = std::env::args().collect::<Vec<_>>();
        assert_eq!(argv, rust_args);
    }

    /// `OxidizedPythonInterpreterConfig.interpreter_config.argv` is respected.
    #[test]
    fn test_argv_respect_interpreter_config() {
        let mut config = default_interpreter_config();
        // .argv expands to current process args by default. But setting
        // .interpreter_config.argv overrides this behavior.
        config.interpreter_config.argv = Some(vec![
            config.argv.as_ref().unwrap()[0].clone(),
            OsString::from("arg0"),
            OsString::from("arg1"),
        ]);
        config.argv = None;

        let mut interp = MainPythonInterpreter::new(config).unwrap();

        let py = interp.acquire_gil();
        let sys = py.import("sys").unwrap();

        let argv = sys
            .get(py, "argv")
            .unwrap()
            .extract::<Vec<String>>(py)
            .unwrap();
        assert_eq!(argv, vec![PYTHON_INTERPRETER_PATH, "arg0", "arg1"]);
    }

    /// `OxidizedPythonInterpreterConfig.argv` can be used to define `sys.argv`.
    #[test]
    fn test_argv_override() {
        let mut config = default_interpreter_config();
        // keep argv[0] the same because it is used for path calculation.
        config.argv.as_mut().unwrap().push(OsString::from("foo"));
        config.argv.as_mut().unwrap().push(OsString::from("bar"));
        config.interpreter_config.argv = Some(vec![OsString::from("shoud-be-ignored")]);

        let mut interp = MainPythonInterpreter::new(config).unwrap();

        let py = interp.acquire_gil();
        let sys = py.import("sys").unwrap();

        let argv = sys
            .get(py, "argv")
            .unwrap()
            .extract::<Vec<String>>(py)
            .unwrap();
        assert_eq!(argv, vec![PYTHON_INTERPRETER_PATH, "foo", "bar"]);
    }

    #[test]
    fn test_argvb_utf8() {
        let mut config = default_interpreter_config();
        config.argv.as_mut().unwrap().push(get_unicode_argument());
        config.argvb = true;

        let mut interp = MainPythonInterpreter::new(config).unwrap();

        let py = interp.acquire_gil();
        let sys = py.import("sys").unwrap();

        let argvb_raw = sys.get(py, "argvb").unwrap();
        let argvb = argvb_raw.cast_as::<PyList>(py).unwrap();
        assert_eq!(argvb.len(py), 2);

        let value_raw = argvb.get_item(py, 1);
        let value_bytes = value_raw.cast_as::<PyBytes>(py).unwrap();
        assert_eq!(
            value_bytes.data(py).to_vec(),
            if cfg!(windows) {
                // UTF-16.
                b"\x4e\x2d\x65\x87".to_vec()
            } else {
                // UTF-8.
                b"\xe4\xb8\xad\xe6\x96\x87".to_vec()
            }
        );
    }

    #[test]
    fn test_argv_utf8() {
        let mut config = default_interpreter_config();
        config.argv.as_mut().unwrap().push(get_unicode_argument());

        let mut interp = MainPythonInterpreter::new(config).unwrap();

        let py = interp.acquire_gil();
        let sys = py.import("sys").unwrap();

        let argv_raw = sys.get(py, "argv").unwrap();
        let argv = argv_raw.cast_as::<PyList>(py).unwrap();
        assert_eq!(argv.len(py), 2);

        let value_raw = argv.get_item(py, 1);
        let value_string = value_raw.cast_as::<PyString>(py).unwrap();
        match value_string.data(py) {
            PyStringData::Utf8(b"\xe4\xb8\xad\xe6\x96\x87") => {
                if cfg!(target_family = "unix") {
                    assert!(true)
                } else {
                    assert!(false)
                }
            }
            PyStringData::Utf8(b"\xe2\xb5\x8e\xe8\x9d\xa5") => {
                if cfg!(target_family = "windows") {
                    assert!(true)
                } else {
                    assert!(false)
                }
            }
            value => assert!(false, "{:?}", value),
        }
    }

    #[test]
    fn test_argv_utf8_isolated() {
        let mut config = default_interpreter_config();
        config.interpreter_config.profile = PythonInterpreterProfile::Isolated;
        config.argv.as_mut().unwrap().push(get_unicode_argument());

        let mut interp = MainPythonInterpreter::new(config).unwrap();

        let py = interp.acquire_gil();
        let sys = py.import("sys").unwrap();

        let argv_raw = sys.get(py, "argv").unwrap();
        let argv = argv_raw.cast_as::<PyList>(py).unwrap();
        assert_eq!(argv.len(py), 2);

        let value_raw = argv.get_item(py, 1);

        // The cpython crate only converts to PyStringData if the internal
        // representation is UTF-8 and will panic otherwise. Since we're using
        // surrogates for the test string, `.data()` will panic. So let's poke
        // at the CPython APIs to get what we need.
        //
        // Unfortunately, python3_sys doesn't expose `PyUnicode_Type` to us,
        // since `PyUnicode_KIND` is a macro. Neither do we have bindings to
        // `PyUnicode_DATA`.
        assert_eq!(unsafe {
            pyffi::PyUnicode_GetLength(value_raw.as_ptr())
        }, if cfg!(target_family = "windows") {
            2
        } else {
            6
        });

        let encoded_raw = unsafe {
            PyObject::from_owned_ptr(
                py,
                pyffi::PyUnicode_AsEncodedString(
                    value_raw.as_ptr(),
                    b"utf-8\0".as_ptr() as *const _,
                    b"surrogatepass\0".as_ptr() as *const _,
                )
            )
        };
        let encoded_bytes = encoded_raw.cast_as::<PyBytes>(py).unwrap();
        let encoded_data = encoded_bytes.data(py);

        if cfg!(target_family = "windows") {
            assert_eq!(encoded_data.len(), 6);
            assert_eq!(encoded_data, b"\xe2\xb5\x8e\xe8\x9d\xa5");
        } else {
            // This is very wrong.
            assert_eq!(encoded_data.len(), 18);
            assert_eq!(
                String::from_utf8(encoded_data.to_owned()).unwrap_err().to_string(),
                "invalid utf-8 sequence of 1 bytes from index 0",
            );
        }
    }

    #[test]
    fn test_argv_utf8_isolated_configure_locale() {
        let mut config = default_interpreter_config();
        config.interpreter_config.profile = PythonInterpreterProfile::Isolated;
        config.interpreter_config.configure_locale = Some(true);
        config.argv.as_mut().unwrap().push(get_unicode_argument());

        let mut interp = MainPythonInterpreter::new(config).unwrap();

        let py = interp.acquire_gil();
        let sys = py.import("sys").unwrap();

        let argv_raw = sys.get(py, "argv").unwrap();
        let argv = argv_raw.cast_as::<PyList>(py).unwrap();
        assert_eq!(argv.len(py), 2);

        let value_raw = argv.get_item(py, 1);

        assert_eq!(unsafe {
            pyffi::PyUnicode_GetLength(value_raw.as_ptr())
        }, 2);

        let encoded_raw = unsafe {
            PyObject::from_owned_ptr(
                py,
                pyffi::PyUnicode_AsEncodedString(
                    value_raw.as_ptr(),
                    b"utf-8\0".as_ptr() as *const _,
                    b"strict\0".as_ptr() as *const _,
                )
            )
        };
        let encoded_bytes = encoded_raw.cast_as::<PyBytes>(py).unwrap();
        let encoded_data = encoded_bytes.data(py);
        assert_eq!(encoded_data, if cfg!(target_family = "windows") {
            b"\xe2\xb5\x8e\xe8\x9d\xa5"
        } else {
            b"\xe4\xb8\xad\xe6\x96\x87"
        });
    }

    #[test]
    fn test_tcl_library_origin() {
        let mut config = default_interpreter_config();
        config.tcl_library = Some(PathBuf::from("$ORIGIN").join("lib").join("tcl8.6"));

        let config = config.resolve().unwrap();

        let origin = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();


        assert_eq!(config.tcl_library, Some(origin.join("lib").join("tcl8.6")));
    }

    #[test]
    fn test_dev_mode() {
        let mut config = default_interpreter_config();
        config.interpreter_config.development_mode = Some(true);

        let mut interp = MainPythonInterpreter::new(config).unwrap();

        let py = interp.acquire_gil();
        let sys = py.import("sys").unwrap();

        let flags = sys.get(py, "flags").unwrap();
        assert!(flags.getattr(py, "dev_mode").unwrap().extract::<bool>(py).unwrap());
    }

    #[test]
    fn test_use_environment() {
        let mut config = default_interpreter_config();
        config.interpreter_config.use_environment = Some(false);

        let mut interp = MainPythonInterpreter::new(config).unwrap();

        let py = interp.acquire_gil();
        let sys = py.import("sys").unwrap();

        let flags = sys.get(py, "flags").unwrap();
        assert_eq!(flags.getattr(py, "ignore_environment").unwrap().extract::<i64>(py).unwrap(), 1);
    }

    #[test]
    fn test_utf8_mode() {
        let mut config = default_interpreter_config();
        config.interpreter_config.utf8_mode = Some(true);

        let mut interp = MainPythonInterpreter::new(config).unwrap();

        let py = interp.acquire_gil();
        let sys = py.import("sys").unwrap();

        let flags = sys.get(py, "flags").unwrap();
        assert_eq!(flags.getattr(py, "utf8_mode").unwrap().extract::<i64>(py).unwrap(), 1);
    }

    #[test]
    fn test_bytes_warning_warn() {
        let mut config = default_interpreter_config();
        config.interpreter_config.bytes_warning = Some(BytesWarning::Warn);

        let mut interp = MainPythonInterpreter::new(config).unwrap();

        let py = interp.acquire_gil();
        let sys = py.import("sys").unwrap();

        let flags = sys.get(py, "flags").unwrap();
        assert_eq!(flags.getattr(py, "bytes_warning").unwrap().extract::<i64>(py).unwrap(), 1);
    }

    #[test]
    fn test_bytes_warning_raise() {
        let mut config = default_interpreter_config();
        config.interpreter_config.bytes_warning = Some(BytesWarning::Raise);

        let mut interp = MainPythonInterpreter::new(config).unwrap();

        let py = interp.acquire_gil();
        let sys = py.import("sys").unwrap();

        let flags = sys.get(py, "flags").unwrap();
        assert_eq!(flags.getattr(py, "bytes_warning").unwrap().extract::<i64>(py).unwrap(), 2);
    }

    #[test]
    fn test_optimization_level_one() {
        let mut config = default_interpreter_config();
        config.interpreter_config.optimization_level = Some(BytecodeOptimizationLevel::One);

        let mut interp = MainPythonInterpreter::new(config).unwrap();

        let py = interp.acquire_gil();
        let sys = py.import("sys").unwrap();

        let flags = sys.get(py, "flags").unwrap();
        assert_eq!(flags.getattr(py, "optimize").unwrap().extract::<i64>(py).unwrap(), 1);
    }

    #[test]
    fn test_optimization_level_two() {
        let mut config = default_interpreter_config();
        config.interpreter_config.optimization_level = Some(BytecodeOptimizationLevel::Two);

        let mut interp = MainPythonInterpreter::new(config).unwrap();

        let py = interp.acquire_gil();
        let sys = py.import("sys").unwrap();

        let flags = sys.get(py, "flags").unwrap();
        assert_eq!(flags.getattr(py, "optimize").unwrap().extract::<i64>(py).unwrap(), 2);
    }

    #[test]
    fn test_inspect() {
        let mut config = default_interpreter_config();
        config.interpreter_config.inspect = Some(true);

        let mut interp = MainPythonInterpreter::new(config).unwrap();

        let py = interp.acquire_gil();
        let sys = py.import("sys").unwrap();

        let flags = sys.get(py, "flags").unwrap();
        assert_eq!(flags.getattr(py, "inspect").unwrap().extract::<i64>(py).unwrap(), 1);
    }

    #[test]
    fn test_interactive() {
        let mut config = default_interpreter_config();
        config.interpreter_config.interactive = Some(true);

        let mut interp = MainPythonInterpreter::new(config).unwrap();

        let py = interp.acquire_gil();
        let sys = py.import("sys").unwrap();

        let flags = sys.get(py, "flags").unwrap();
        assert_eq!(flags.getattr(py, "interactive").unwrap().extract::<i64>(py).unwrap(), 1);
    }

    #[test]
    fn test_quiet() {
        let mut config = default_interpreter_config();
        config.interpreter_config.quiet = Some(true);

        let mut interp = MainPythonInterpreter::new(config).unwrap();

        let py = interp.acquire_gil();
        let sys = py.import("sys").unwrap();

        let flags = sys.get(py, "flags").unwrap();
        assert_eq!(flags.getattr(py, "quiet").unwrap().extract::<i64>(py).unwrap(), 1);
    }

    #[test]
    fn test_site_import() {
        let mut config = default_interpreter_config();
        config.interpreter_config.site_import = Some(false);

        let mut interp = MainPythonInterpreter::new(config).unwrap();

        let py = interp.acquire_gil();
        let sys = py.import("sys").unwrap();

        let flags = sys.get(py, "flags").unwrap();
        assert_eq!(flags.getattr(py, "no_site").unwrap().extract::<i64>(py).unwrap(), 1);
    }

    #[test]
    fn test_user_site_directory() {
        let mut config = default_interpreter_config();
        config.interpreter_config.user_site_directory = Some(false);

        let mut interp = MainPythonInterpreter::new(config).unwrap();

        let py = interp.acquire_gil();
        let sys = py.import("sys").unwrap();

        let flags = sys.get(py, "flags").unwrap();
        assert_eq!(flags.getattr(py, "no_user_site").unwrap().extract::<i64>(py).unwrap(), 1);
    }

    #[test]
    fn test_write_bytecode() {
        let mut config = default_interpreter_config();
        config.interpreter_config.write_bytecode = Some(false);

        let mut interp = MainPythonInterpreter::new(config).unwrap();

        let py = interp.acquire_gil();
        let sys = py.import("sys").unwrap();

        let flags = sys.get(py, "flags").unwrap();
        assert_eq!(flags.getattr(py, "dont_write_bytecode").unwrap().extract::<i64>(py).unwrap(), 1);
    }
}
