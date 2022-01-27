// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    super::{default_interpreter_config, set_sys_paths, PYTHON_INTERPRETER_PATH},
    crate::{MainPythonInterpreter, OxidizedPythonInterpreterConfig},
    pyo3::{
        ffi as pyffi,
        prelude::*,
        types::{PyBytes, PyList, PyString, PyStringData},
    },
    python_packaging::{
        interpreter::{BytesWarning, MemoryAllocatorBackend, PythonInterpreterProfile},
        resource::BytecodeOptimizationLevel,
    },
    rusty_fork::rusty_fork_test,
    std::{ffi::OsString, path::PathBuf},
};

#[cfg(target_family = "unix")]
use std::os::unix::ffi::OsStringExt;

#[cfg(target_family = "windows")]
use std::os::windows::ffi::OsStringExt;

#[cfg(target_family = "unix")]
fn get_unicode_argument() -> OsString {
    // 中文 = U+4e2d / 20013 + U+6587 / 25991
    OsString::from_vec([0xe4, 0xb8, 0xad, 0xe6, 0x96, 0x87].to_vec())
}

#[cfg(target_family = "windows")]
fn get_unicode_argument() -> OsString {
    // 中文
    OsString::from_wide(&[20013, 25991])
}

fn reprs(container: &PyAny) -> PyResult<Vec<String>> {
    let mut names = Vec::new();
    for x in container.iter()? {
        names.push(x?.to_string());
    }
    Ok(names)
}

fn assert_importer(oxidized: bool, filesystem: bool) {
    let mut config = default_interpreter_config();

    config.oxidized_importer = oxidized;
    config.filesystem_importer = filesystem;
    let interp = MainPythonInterpreter::new(config).unwrap();

    interp.with_gil(|py| {
        let sys = py.import("sys").unwrap();
        let meta_path_reprs = reprs(sys.getattr("meta_path").unwrap()).unwrap();
        let path_hook_reprs = reprs(sys.getattr("path_hooks").unwrap()).unwrap();
        const PATH_HOOK_REPR: &str =
            "built-in method path_hook of oxidized_importer.OxidizedFinder object";

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
    });
}

rusty_fork_test! {
    #[test]
    fn test_default_interpreter() {
        let config = default_interpreter_config();
        let interp = MainPythonInterpreter::new(config).unwrap();

        interp.with_gil(|py| {
            let sys = py.import("sys").unwrap();
            let meta_path = sys.getattr("meta_path").unwrap();
            assert_eq!(meta_path.len().unwrap(), 3);

            let importer = meta_path.get_item(0).unwrap();
            assert!(importer
                .to_string()
                .contains("_frozen_importlib.BuiltinImporter"));
            let importer = meta_path.get_item(1).unwrap();
            assert!(importer
                .to_string()
                .contains("_frozen_importlib.FrozenImporter"));
            let importer = meta_path.get_item(2).unwrap();
            assert!(importer
                .to_string()
                .contains("_frozen_importlib_external.PathFinder"));
        });
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
        set_sys_paths(&mut config);

        let interp = MainPythonInterpreter::new(config).unwrap();

        interp.with_gil(|py| {
            let sys = py.import("sys").unwrap();
            let flags = sys.getattr("flags").unwrap();

            assert_eq!(
                flags
                    .getattr("isolated")
                    .unwrap()
                    .extract::<i32>()
                    .unwrap(),
                1
            );
        });
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

    #[cfg(feature = "jemalloc-sys")]
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

    #[cfg(feature = "jemalloc-sys")]
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

    #[cfg(feature = "libmimalloc-sys")]
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

    #[cfg(feature = "libmimalloc-sys")]
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

    #[cfg(feature = "snmalloc-sys")]
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

    #[cfg(feature = "snmalloc-sys")]
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

        let interp = MainPythonInterpreter::new(config).unwrap();

        interp.with_gil(|py| {
            let sys = py.import("sys").unwrap();

            let argv = sys
                .getattr("argv")
                .unwrap()
                .extract::<Vec<String>>()
                .unwrap();
            let rust_args = std::env::args().collect::<Vec<_>>();
            assert_eq!(argv, rust_args);
        });
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

        let interp = MainPythonInterpreter::new(config).unwrap();

        interp.with_gil(|py| {
            let sys = py.import("sys").unwrap();

            let argv = sys
                .getattr("argv")
                .unwrap()
                .extract::<Vec<String>>()
                .unwrap();
            assert_eq!(argv, vec![PYTHON_INTERPRETER_PATH, "arg0", "arg1"]);
        });
    }

    /// `OxidizedPythonInterpreterConfig.argv` can be used to define `sys.argv`.
    #[test]
    fn test_argv_override() {
        let mut config = default_interpreter_config();
        // keep argv[0] the same because it is used for path calculation.
        config.argv.as_mut().unwrap().push(OsString::from("foo"));
        config.argv.as_mut().unwrap().push(OsString::from("bar"));
        config.interpreter_config.argv = Some(vec![OsString::from("shoud-be-ignored")]);

        let interp = MainPythonInterpreter::new(config).unwrap();

        interp.with_gil(|py| {
            let sys = py.import("sys").unwrap();

            let argv = sys
                .getattr("argv")
                .unwrap()
                .extract::<Vec<String>>()
                .unwrap();
            assert_eq!(argv, vec![PYTHON_INTERPRETER_PATH, "foo", "bar"]);
        });
    }

    #[test]
    fn test_argvb_utf8() {
        let mut config = default_interpreter_config();
        config.argv.as_mut().unwrap().push(get_unicode_argument());
        config.argvb = true;

        let interp = MainPythonInterpreter::new(config).unwrap();

        interp.with_gil(|py| {
            let sys = py.import("sys").unwrap();

            let argvb_raw = sys.getattr("argvb").unwrap();
            let argvb = argvb_raw.cast_as::<PyList>().unwrap();
            assert_eq!(argvb.len(), 2);

            let value_raw = argvb.get_item(1).unwrap();
            let value_bytes = value_raw.cast_as::<PyBytes>().unwrap();
            assert_eq!(
                value_bytes.as_bytes().to_vec(),
                if cfg!(windows) {
                    // UTF-16-LE.
                    b"\x2d\x4e\x87\x65".to_vec()
                } else {
                    // UTF-8.
                    b"\xe4\xb8\xad\xe6\x96\x87".to_vec()
                }
            );
        });
    }

    #[test]
    fn test_argv_utf8() {
        let mut config = default_interpreter_config();
        config.argv.as_mut().unwrap().push(get_unicode_argument());

        let interp = MainPythonInterpreter::new(config).unwrap();

        interp.with_gil(|py| {
            let sys = py.import("sys").unwrap();

            let argv_raw = sys.getattr("argv").unwrap();
            let argv = argv_raw.cast_as::<PyList>().unwrap();
            assert_eq!(argv.len(), 2);

            let value_raw = argv.get_item(1).unwrap();
            let value_string = value_raw.cast_as::<PyString>().unwrap();

            match unsafe { value_string.data().unwrap() } {
                PyStringData::Ucs2(&[20013, 25991]) => {},
                value => panic!("{:?}", value),
            }
        });
    }

    #[test]
    fn test_argv_utf8_isolated() {
        let mut config = default_interpreter_config();
        config.interpreter_config.profile = PythonInterpreterProfile::Isolated;
        config.argv.as_mut().unwrap().push(get_unicode_argument());
        set_sys_paths(&mut config);

        let interp = MainPythonInterpreter::new(config).unwrap();

        interp.with_gil(|py| {
            let sys = py.import("sys").unwrap();

            let argv_raw = sys.getattr("argv").unwrap();
            let argv = argv_raw.cast_as::<PyList>().unwrap();
            assert_eq!(argv.len(), 2);

            let value_raw = argv.get_item(1).unwrap();
            let value_string = value_raw.cast_as::<PyString>().unwrap();

            // The result in isolated mode without configure_locale is kinda wonky.
            match unsafe { value_string.data().unwrap() } {
                // This is the correct value.
                PyStringData::Ucs2(&[20013, 25991]) => {
                    if !cfg!(any(target_family = "windows", target_os = "macos")) {
                        panic!("Unexpected result");
                    }
                }
                // This is some abomination.
                PyStringData::Ucs2(&[56548, 56504, 56493, 56550, 56470, 56455]) => {
                    if !cfg!(target_family = "unix") {
                        panic!("Unexpected result");
                    }
                }
                value => panic!("unexpected string data: {:?}", value),
            }
        });
    }

    #[test]
    fn test_argv_utf8_isolated_configure_locale() {
        let mut config = default_interpreter_config();
        config.interpreter_config.profile = PythonInterpreterProfile::Isolated;
        config.interpreter_config.configure_locale = Some(true);
        config.argv.as_mut().unwrap().push(get_unicode_argument());
        set_sys_paths(&mut config);

        let interp = MainPythonInterpreter::new(config).unwrap();

        interp.with_gil(|py| {
            let sys = py.import("sys").unwrap();

            let argv_raw = sys.getattr("argv").unwrap();
            let argv = argv_raw.cast_as::<PyList>().unwrap();
            assert_eq!(argv.len(), 2);

            let value_raw = argv.get_item(1).unwrap();
            let value_string = value_raw.cast_as::<PyString>().unwrap();

            match unsafe { value_string.data().unwrap() } {
                PyStringData::Ucs2(&[20013, 25991]) => {},
                value => panic!("unexpected string data: {:?}", value),
            }
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

        let interp = MainPythonInterpreter::new(config).unwrap();

        interp.with_gil(|py| {
            let sys = py.import("sys").unwrap();

            let flags = sys.getattr("flags").unwrap();
            assert!(flags.getattr("dev_mode").unwrap().extract::<bool>().unwrap());
        });
    }

    #[test]
    fn test_use_environment() {
        let mut config = default_interpreter_config();
        config.interpreter_config.use_environment = Some(false);
        set_sys_paths(&mut config);

        let interp = MainPythonInterpreter::new(config).unwrap();

        interp.with_gil(|py| {
            let sys = py.import("sys").unwrap();

            let flags = sys.getattr("flags").unwrap();
            assert_eq!(flags.getattr("ignore_environment").unwrap().extract::<i64>().unwrap(), 1);
        });
    }

    #[test]
    fn test_utf8_mode() {
        let mut config = default_interpreter_config();
        config.interpreter_config.utf8_mode = Some(true);

        let interp = MainPythonInterpreter::new(config).unwrap();

        interp.with_gil(|py| {
            let sys = py.import("sys").unwrap();

            let flags = sys.getattr("flags").unwrap();
            assert_eq!(flags.getattr("utf8_mode").unwrap().extract::<i64>().unwrap(), 1);
        });
    }

    #[test]
    fn test_bytes_warning_warn() {
        let mut config = default_interpreter_config();
        config.interpreter_config.bytes_warning = Some(BytesWarning::Warn);

        let interp = MainPythonInterpreter::new(config).unwrap();

        interp.with_gil(|py| {
            let sys = py.import("sys").unwrap();

            let flags = sys.getattr("flags").unwrap();
            assert_eq!(flags.getattr("bytes_warning").unwrap().extract::<i64>().unwrap(), 1);
        });
    }

    #[test]
    fn test_bytes_warning_raise() {
        let mut config = default_interpreter_config();
        config.interpreter_config.bytes_warning = Some(BytesWarning::Raise);

        let interp = MainPythonInterpreter::new(config).unwrap();

        interp.with_gil(|py| {
            let sys = py.import("sys").unwrap();

            let flags = sys.getattr("flags").unwrap();
            assert_eq!(flags.getattr("bytes_warning").unwrap().extract::<i64>().unwrap(), 2);
        });
    }

    #[test]
    fn test_optimization_level_one() {
        let mut config = default_interpreter_config();
        config.interpreter_config.optimization_level = Some(BytecodeOptimizationLevel::One);

        let interp = MainPythonInterpreter::new(config).unwrap();

        interp.with_gil(|py| {
            let sys = py.import("sys").unwrap();

            let flags = sys.getattr("flags").unwrap();
            assert_eq!(flags.getattr("optimize").unwrap().extract::<i64>().unwrap(), 1);
        });
    }

    #[test]
    fn test_optimization_level_two() {
        let mut config = default_interpreter_config();
        config.interpreter_config.optimization_level = Some(BytecodeOptimizationLevel::Two);

        let interp = MainPythonInterpreter::new(config).unwrap();

        interp.with_gil(|py| {
            let sys = py.import("sys").unwrap();

            let flags = sys.getattr("flags").unwrap();
            assert_eq!(flags.getattr("optimize").unwrap().extract::<i64>().unwrap(), 2);
        });
    }

    #[test]
    fn test_inspect() {
        let mut config = default_interpreter_config();
        config.interpreter_config.inspect = Some(true);

        let interp = MainPythonInterpreter::new(config).unwrap();

        interp.with_gil(|py| {
            let sys = py.import("sys").unwrap();

            let flags = sys.getattr("flags").unwrap();
            assert_eq!(flags.getattr("inspect").unwrap().extract::<i64>().unwrap(), 1);
        });
    }

    #[test]
    fn test_interactive() {
        let mut config = default_interpreter_config();
        config.interpreter_config.interactive = Some(true);

        let interp = MainPythonInterpreter::new(config).unwrap();

        interp.with_gil(|py| {
            let sys = py.import("sys").unwrap();

            let flags = sys.getattr("flags").unwrap();
            assert_eq!(flags.getattr("interactive").unwrap().extract::<i64>().unwrap(), 1);
        });
    }

    #[test]
    fn test_quiet() {
        let mut config = default_interpreter_config();
        config.interpreter_config.quiet = Some(true);

        let interp = MainPythonInterpreter::new(config).unwrap();

        interp.with_gil(|py| {
            let sys = py.import("sys").unwrap();

            let flags = sys.getattr("flags").unwrap();
            assert_eq!(flags.getattr("quiet").unwrap().extract::<i64>().unwrap(), 1);
        });
    }

    #[test]
    fn test_site_import_false() {
        let mut config = default_interpreter_config();
        config.interpreter_config.site_import = Some(false);

        let interp = MainPythonInterpreter::new(config).unwrap();

        interp.with_gil(|py| {
            let sys = py.import("sys").unwrap();

            let flags = sys.getattr("flags").unwrap();
            assert_eq!(flags.getattr("no_site").unwrap().extract::<i64>().unwrap(), 1);
        });
    }

    #[test]
    fn test_site_import_true() {
        let mut config = default_interpreter_config();
        config.interpreter_config.site_import = Some(true);
        let interp = MainPythonInterpreter::new(config).unwrap();

        interp.with_gil(|py| {
            let sys = py.import("sys").unwrap();

            let flags = sys.getattr("flags").unwrap();
            assert_eq!(flags.getattr("no_site").unwrap().extract::<i64>().unwrap(), 0);
        });
    }

    #[test]
    fn test_user_site_directory_false() {
        let mut config = default_interpreter_config();
        config.interpreter_config.user_site_directory = Some(false);

        let interp = MainPythonInterpreter::new(config).unwrap();

        interp.with_gil(|py| {
            let sys = py.import("sys").unwrap();

            let flags = sys.getattr("flags").unwrap();
            assert_eq!(flags.getattr("no_user_site").unwrap().extract::<i64>().unwrap(), 1);
        });
    }

    #[test]
    fn test_user_site_directory_true() {
        let mut config = default_interpreter_config();
        config.interpreter_config.user_site_directory = Some(true);

        let interp = MainPythonInterpreter::new(config).unwrap();

        interp.with_gil(|py| {
            let sys = py.import("sys").unwrap();

            let flags = sys.getattr("flags").unwrap();
            assert_eq!(flags.getattr("no_user_site").unwrap().extract::<i64>().unwrap(), 0);
        });
    }

    #[test]
    fn test_write_bytecode() {
        let mut config = default_interpreter_config();
        config.interpreter_config.write_bytecode = Some(false);

        let interp = MainPythonInterpreter::new(config).unwrap();

        interp.with_gil(|py| {
            let sys = py.import("sys").unwrap();

            let flags = sys.getattr("flags").unwrap();
            assert_eq!(flags.getattr("dont_write_bytecode").unwrap().extract::<i64>().unwrap(), 1);
        });
    }
}
