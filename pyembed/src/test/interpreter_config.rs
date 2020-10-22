// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{MainPythonInterpreter, OxidizedPythonInterpreterConfig},
    cpython::{ObjectProtocol, PyBytes, PyList, PyObject, PyString, PyStringData},
    python3_sys as pyffi,
    python_packaging::interpreter::PythonInterpreterProfile,
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

rusty_fork_test! {
    #[test]
    fn test_default_interpreter() {
        let mut config = OxidizedPythonInterpreterConfig::default();
        // Otherwise the Rust arguments are interpreted as Python arguments.
        config.interpreter_config.parse_argv = Some(false);
        config.set_missing_path_configuration = false;
        let mut interp = MainPythonInterpreter::new(config).unwrap();

        let py = interp.acquire_gil().unwrap();
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
    fn test_isolated_interpreter() {
        let mut config = OxidizedPythonInterpreterConfig::default();
        config.interpreter_config.profile = PythonInterpreterProfile::Isolated;
        // This allows us to pick up the default paths from the Python install
        // detected by python3_sys. Without this, sys.path and other paths reference
        // directories next to the Rust test executable, and there is no Python stdlib
        // there.
        config.set_missing_path_configuration = false;

        let mut interp = MainPythonInterpreter::new(config).unwrap();

        let py = interp.acquire_gil().unwrap();
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
    fn test_sys_paths_origin() {
        let mut config = OxidizedPythonInterpreterConfig::default();
        // Otherwise the Rust arguments are interpreted as Python arguments.
        config.interpreter_config.parse_argv = Some(false);
        config.interpreter_config.module_search_paths = Some(vec![PathBuf::from("$ORIGIN/lib")]);

        let origin = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();

        let paths = config.resolve_module_search_paths().unwrap();
        assert_eq!(
            paths,
            &Some(vec![PathBuf::from(format!("{}/lib", origin.display()))])
        );

        let py_config: pyffi::PyConfig = (&config).try_into().unwrap();

        assert_eq!(py_config.module_search_paths_set, 1);
        assert_eq!(py_config.module_search_paths.length, 1);
    }

    /// sys.argv is initialized using the Rust process's arguments by default.
    #[test]
    fn test_argv_default() {
        let mut config = OxidizedPythonInterpreterConfig::default();
        // Otherwise the Rust arguments are interpreted as Python arguments.
        config.interpreter_config.parse_argv = Some(false);
        config.set_missing_path_configuration = false;

        let mut interp = MainPythonInterpreter::new(config).unwrap();

        let py = interp.acquire_gil().unwrap();
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
        let mut config = OxidizedPythonInterpreterConfig::default();
        // Otherwise the arguments are interpreted as Python arguments.
        config.interpreter_config.parse_argv = Some(false);
        config.set_missing_path_configuration = false;

        // .argv expands to current process args by default. But setting
        // .interpreter_config.argv overrides this behavior.
        config.argv = None;
        config.interpreter_config.argv = Some(vec![
            OsString::from("prog"),
            OsString::from("arg0"),
            OsString::from("arg1"),
        ]);

        let mut interp = MainPythonInterpreter::new(config).unwrap();

        let py = interp.acquire_gil().unwrap();
        let sys = py.import("sys").unwrap();

        let argv = sys
            .get(py, "argv")
            .unwrap()
            .extract::<Vec<String>>(py)
            .unwrap();
        assert_eq!(argv, vec!["prog", "arg0", "arg1"]);
    }

    /// `OxidizedPythonInterpreterConfig.argv` can be used to define `sys.argv`.
    #[test]
    fn test_argv_override() {
        let mut config = OxidizedPythonInterpreterConfig::default();
        // Otherwise the arguments are interpreted as Python arguments.
        config.interpreter_config.parse_argv = Some(false);
        config.set_missing_path_configuration = false;
        config.argv = Some(vec![
            OsString::from("prog"),
            OsString::from("foo"),
            OsString::from("bar"),
        ]);

        let mut interp = MainPythonInterpreter::new(config).unwrap();

        let py = interp.acquire_gil().unwrap();
        let sys = py.import("sys").unwrap();

        let argv = sys
            .get(py, "argv")
            .unwrap()
            .extract::<Vec<String>>(py)
            .unwrap();
        assert_eq!(argv, vec!["prog", "foo", "bar"]);
    }

    #[test]
    fn test_argvb_utf8() {
        let mut config = OxidizedPythonInterpreterConfig::default();

        // Otherwise the Rust arguments are interpreted as Python arguments.
        config.interpreter_config.parse_argv = Some(false);
        config.set_missing_path_configuration = false;
        config.argv = Some(vec![get_unicode_argument()]);
        config.argvb = true;

        let mut interp = MainPythonInterpreter::new(config).unwrap();

        let py = interp.acquire_gil().unwrap();
        let sys = py.import("sys").unwrap();

        let argvb_raw = sys.get(py, "argvb").unwrap();
        let argvb = argvb_raw.cast_as::<PyList>(py).unwrap();
        assert_eq!(argvb.len(py), 1);

        let value_raw = argvb.get_item(py, 0);
        let value_bytes = value_raw.cast_as::<PyBytes>(py).unwrap();
        assert_eq!(
            value_bytes.data(py),
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
        let mut config = OxidizedPythonInterpreterConfig::default();

        // Otherwise the Rust arguments are interpreted as Python arguments.
        config.interpreter_config.parse_argv = Some(false);
        config.set_missing_path_configuration = false;
        config.argv = Some(vec![get_unicode_argument()]);

        let mut interp = MainPythonInterpreter::new(config).unwrap();

        let py = interp.acquire_gil().unwrap();
        let sys = py.import("sys").unwrap();

        let argv_raw = sys.get(py, "argv").unwrap();
        let argv = argv_raw.cast_as::<PyList>(py).unwrap();
        assert_eq!(argv.len(py), 1);

        let value_raw = argv.get_item(py, 0);
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
        let mut config = OxidizedPythonInterpreterConfig::default();
        config.interpreter_config.profile = PythonInterpreterProfile::Isolated;
        // Otherwise the Rust arguments are interpreted as Python arguments.
        config.interpreter_config.parse_argv = Some(false);
        config.set_missing_path_configuration = false;
        config.set_missing_path_configuration = false;
        config.argv = Some(vec![get_unicode_argument()]);

        let mut interp = MainPythonInterpreter::new(config).unwrap();

        let py = interp.acquire_gil().unwrap();
        let sys = py.import("sys").unwrap();

        let argv_raw = sys.get(py, "argv").unwrap();
        let argv = argv_raw.cast_as::<PyList>(py).unwrap();
        assert_eq!(argv.len(py), 1);

        let value_raw = argv.get_item(py, 0);

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
        let mut config = OxidizedPythonInterpreterConfig::default();
        config.interpreter_config.profile = PythonInterpreterProfile::Isolated;
        config.interpreter_config.configure_locale = Some(true);
        // Otherwise the Rust arguments are interpreted as Python arguments.
        config.interpreter_config.parse_argv = Some(false);
        config.set_missing_path_configuration = false;
        config.argv = Some(vec![get_unicode_argument()]);

        let mut interp = MainPythonInterpreter::new(config).unwrap();

        let py = interp.acquire_gil().unwrap();
        let sys = py.import("sys").unwrap();

        let argv_raw = sys.get(py, "argv").unwrap();
        let argv = argv_raw.cast_as::<PyList>(py).unwrap();
        assert_eq!(argv.len(py), 1);

        let value_raw = argv.get_item(py, 0);

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
        let mut config = OxidizedPythonInterpreterConfig::default();
        // Otherwise the Rust arguments are interpreted as Python arguments.
        config.interpreter_config.parse_argv = Some(false);
        config.tcl_library = Some(PathBuf::from("$ORIGIN/lib/tcl8.6"));

        let origin = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();

        let tcl_library = config.resolve_tcl_library().unwrap();
        assert_eq!(tcl_library, Some(origin.join("lib").join("tcl8.6").into_os_string()));
    }
}
