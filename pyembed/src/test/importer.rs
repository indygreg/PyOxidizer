// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{MainPythonInterpreter, OxidizedPythonInterpreterConfig},
    anyhow::Result,
    cpython::{ObjectProtocol, PyObject},
};

fn new_interpreter<'python, 'interpreter, 'resources>(
) -> Result<MainPythonInterpreter<'python, 'interpreter, 'resources>> {
    let mut config = OxidizedPythonInterpreterConfig::default();
    config.oxidized_importer = true;
    let interp = MainPythonInterpreter::new(config)?;

    Ok(interp)
}

fn get_importer(interp: &mut MainPythonInterpreter) -> Result<PyObject> {
    let py = interp.acquire_gil().unwrap();

    let sys = py.import("sys").unwrap();
    let meta_path = sys.get(py, "meta_path").unwrap();
    assert_eq!(meta_path.len(py).unwrap(), 2);

    let importer = meta_path.get_item(py, 0).unwrap();
    assert_eq!(importer.get_type(py).name(py), "PyOxidizerFinder");

    Ok(importer)
}

/// We can load our oxidized importer with no resources.
#[test]
fn no_resources() -> Result<()> {
    let mut config = OxidizedPythonInterpreterConfig::default();
    config.oxidized_importer = true;
    let mut interp = MainPythonInterpreter::new(config)?;

    let py = interp.acquire_gil().unwrap();
    let sys = py.import("sys").unwrap();
    let meta_path = sys.get(py, "meta_path").unwrap();
    assert_eq!(meta_path.len(py).unwrap(), 2);

    let importer = meta_path.get_item(py, 0).unwrap();
    assert_eq!(importer.get_type(py).name(py), "PyOxidizerFinder");

    let errno = py.import("errno").unwrap();
    let loader = errno.get(py, "__loader__").unwrap();
    // It isn't PyOxidizerFinder because PyOxidizerFinder is just a proxy.
    assert!(loader
        .to_string()
        .contains("_frozen_importlib.BuiltinImporter"));

    Ok(())
}

/// find_spec() returns None on missing module.
#[test]
fn find_spec_missing() -> Result<()> {
    let mut interp = new_interpreter()?;
    let importer = get_importer(&mut interp)?;
    let py = interp.acquire_gil().unwrap();

    assert_eq!(
        importer
            .call_method(py, "find_spec", ("missing_package", py.None()), None)
            .unwrap(),
        py.None()
    );
    assert_eq!(
        importer
            .call_method(py, "find_spec", ("foo.bar", py.None()), None)
            .unwrap(),
        py.None()
    );

    Ok(())
}

/// find_spec() returns something reasonable for built-in extensions.
#[test]
fn find_spec_builtin() -> Result<()> {
    let mut interp = new_interpreter()?;
    let importer = get_importer(&mut interp)?;
    let py = interp.acquire_gil().unwrap();

    let spec = importer
        .call_method(py, "find_spec", ("_io", py.None()), None)
        .unwrap();

    assert_eq!(spec.get_type(py).name(py), "ModuleSpec");
    assert_eq!(
        spec.getattr(py, "name")
            .unwrap()
            .extract::<String>(py)
            .unwrap(),
        "_io"
    );
    assert!(spec
        .getattr(py, "loader")
        .unwrap()
        .to_string()
        .contains("BuiltinImporter"));
    assert_eq!(
        spec.getattr(py, "origin")
            .unwrap()
            .extract::<String>(py)
            .unwrap(),
        "built-in"
    );
    assert_eq!(spec.getattr(py, "loader_state").unwrap(), py.None());
    assert_eq!(
        spec.getattr(py, "submodule_search_locations").unwrap(),
        py.None()
    );

    Ok(())
}

/// find_module() returns something reasonable for built-in extensions.
#[test]
fn find_module_builtin() -> Result<()> {
    let mut interp = new_interpreter()?;
    let importer = get_importer(&mut interp)?;
    let py = interp.acquire_gil().unwrap();

    let loader = importer
        .call_method(py, "find_module", ("_io", py.None()), None)
        .unwrap();

    assert!(loader.to_string().contains("BuiltinImporter"));

    Ok(())
}

/// get_code() returns None for a built-in.
#[test]
fn get_code_builtin() -> Result<()> {
    let mut interp = new_interpreter()?;
    let importer = get_importer(&mut interp)?;
    let py = interp.acquire_gil().unwrap();

    assert_eq!(
        importer
            .call_method(py, "get_code", ("_io",), None)
            .unwrap(),
        py.None()
    );

    Ok(())
}

/// get_source() returns None for a built-in.
#[test]
fn get_source_builtin() -> Result<()> {
    let mut interp = new_interpreter()?;
    let importer = get_importer(&mut interp)?;
    let py = interp.acquire_gil().unwrap();

    assert_eq!(
        importer
            .call_method(py, "get_source", ("_io",), None)
            .unwrap(),
        py.None()
    );

    Ok(())
}

/// get_filename() raises ImportError for a built-in.
#[test]
fn get_filename_builtin() -> Result<()> {
    let mut interp = new_interpreter()?;
    let importer = get_importer(&mut interp)?;
    let py = interp.acquire_gil().unwrap();

    let res = importer.call_method(py, "get_filename", ("_io",), None);

    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(err.ptype.to_string().contains("ImportError"));

    Ok(())
}
