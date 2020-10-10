// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{MainPythonInterpreter, OxidizedPythonInterpreterConfig},
    anyhow::Result,
    cpython::ObjectProtocol,
    python3_sys as pyffi,
    python_packaging::interpreter::PythonInterpreterProfile,
    std::{convert::TryInto, path::PathBuf},
};

#[test]
fn test_default_interpreter() -> Result<()> {
    let config = OxidizedPythonInterpreterConfig::default();
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
    config.interpreter_config.module_search_paths = Some(vec![PathBuf::from("$ORIGIN/lib")]);

    let paths = config.resolve_module_search_paths().unwrap();
    assert_eq!(paths, &Some(vec![PathBuf::from("$ORIGIN/lib")]));

    let py_config: pyffi::PyConfig = (&config).try_into().unwrap();

    assert_eq!(py_config.module_search_paths_set, 1);
    assert_eq!(py_config.module_search_paths.length, 1);

    Ok(())
}
