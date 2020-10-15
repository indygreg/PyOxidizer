// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{MainPythonInterpreter, OxidizedPythonInterpreterConfig},
    anyhow::Result,
    cpython::{ObjectProtocol, PyBytes, PyList, PyString, PyStringData},
    python3_sys as pyffi,
    python_packaging::interpreter::PythonInterpreterProfile,
    std::{convert::TryInto, ffi::OsString, path::PathBuf},
};

#[cfg(target_family = "unix")]
use std::os::unix::ffi::OsStringExt;

#[cfg(target_family = "windows")]
use std::os::windows::ffi::OsStringExt;

#[test]
fn test_default_interpreter() -> Result<()> {
    let mut config = OxidizedPythonInterpreterConfig::default();
    // Otherwise the Rust arguments are interpreted as Python arguments.
    config.interpreter_config.parse_argv = Some(false);
    let mut interp = MainPythonInterpreter::new(config)?;

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

    Ok(())
}

#[test]
fn test_isolated_interpreter() -> Result<()> {
    let mut config = OxidizedPythonInterpreterConfig::default();
    config.interpreter_config.profile = PythonInterpreterProfile::Isolated;
    // This allows us to pick up the default paths from the Python install
    // detected by python3_sys. Without this, sys.path and other paths reference
    // directories next to the Rust test executable, and there is no Python stdlib
    // there.
    config.isolated_auto_set_path_configuration = false;

    let mut interp = MainPythonInterpreter::new(config)?;

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

    Ok(())
}

#[test]
fn test_sys_paths_origin() -> Result<()> {
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

    Ok(())
}

/// sys.argv is initialized using the Rust process's arguments by default.
#[test]
fn test_argv_default() -> Result<()> {
    let mut config = OxidizedPythonInterpreterConfig::default();
    // Otherwise the Rust arguments are interpreted as Python arguments.
    config.interpreter_config.parse_argv = Some(false);

    let mut interp = MainPythonInterpreter::new(config)?;

    let py = interp.acquire_gil().unwrap();
    let sys = py.import("sys").unwrap();

    let argv = sys
        .get(py, "argv")
        .unwrap()
        .extract::<Vec<String>>(py)
        .unwrap();
    let rust_args = std::env::args().collect::<Vec<_>>();
    assert_eq!(argv, rust_args);

    Ok(())
}

/// `OxidizedPythonInterpreterConfig.interpreter_config.argv` is respected.
#[test]
fn test_argv_respect_interpreter_config() -> Result<()> {
    let mut config = OxidizedPythonInterpreterConfig::default();

    // .argv expands to current process args by default. But setting
    // .interpreter_config.argv overrides this behavior.
    config.argv = None;
    config.interpreter_config.argv = Some(vec![
        OsString::from("prog"),
        OsString::from("arg0"),
        OsString::from("arg1"),
    ]);

    let mut interp = MainPythonInterpreter::new(config)?;

    let py = interp.acquire_gil().unwrap();
    let sys = py.import("sys").unwrap();

    let argv = sys
        .get(py, "argv")
        .unwrap()
        .extract::<Vec<String>>(py)
        .unwrap();
    assert_eq!(argv, vec!["arg0", "arg1"]);

    Ok(())
}

/// `OxidizedPythonInterpreterConfig.argv` can be used to define `sys.argv`.
#[test]
fn test_argv_override() -> Result<()> {
    let mut config = OxidizedPythonInterpreterConfig::default();
    config.argv = Some(vec![
        OsString::from("prog"),
        OsString::from("foo"),
        OsString::from("bar"),
    ]);

    let mut interp = MainPythonInterpreter::new(config)?;

    let py = interp.acquire_gil().unwrap();
    let sys = py.import("sys").unwrap();

    let argv = sys
        .get(py, "argv")
        .unwrap()
        .extract::<Vec<String>>(py)
        .unwrap();
    assert_eq!(argv, vec!["foo", "bar"]);

    Ok(())
}

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

#[test]
fn test_argv_utf8() -> Result<()> {
    let mut config = OxidizedPythonInterpreterConfig::default();

    config.argv = Some(vec![OsString::from("ignored"), get_unicode_argument()]);
    config.argvb = true;

    let mut interp = MainPythonInterpreter::new(config)?;

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

    let argvb_raw = sys.get(py, "argvb").unwrap();
    let argvb = argvb_raw.cast_as::<PyList>(py).unwrap();
    // TODO should be same length as sys.argv
    assert_eq!(argvb.len(py), 2);

    let value_raw = argvb.get_item(py, 1);
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

    Ok(())
}
