// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{MainPythonInterpreter, OxidizedPythonInterpreterConfig},
    anyhow::{anyhow, Result},
    cpython::{ObjectProtocol, PyObject},
    std::path::PathBuf,
};

fn new_interpreter<'python, 'interpreter, 'resources>(
) -> Result<MainPythonInterpreter<'python, 'interpreter, 'resources>> {
    let mut config = OxidizedPythonInterpreterConfig::default();
    config.oxidized_importer = true;
    let interp = MainPythonInterpreter::new(config)?;

    Ok(interp)
}

fn run_py_test(test_filename: &str) -> Result<()> {
    let test_dir = env!("PYEMBED_TESTS_DIR");
    let test_path = PathBuf::from(test_dir).join(test_filename);

    let mut config = OxidizedPythonInterpreterConfig::default();
    config.oxidized_importer = true;
    config.interpreter_config.run_filename = Some(test_path);
    config.interpreter_config.buffered_stdio = Some(false);
    let mut interp = MainPythonInterpreter::new(config)?;

    let exit_code = interp.run_as_main();
    if exit_code != 0 {
        Err(anyhow!("Python code did not exit successfully"))
    } else {
        Ok(())
    }
}

fn get_importer(interp: &mut MainPythonInterpreter) -> Result<PyObject> {
    let py = interp.acquire_gil().unwrap();

    let sys = py.import("sys").unwrap();
    let meta_path = sys.get(py, "meta_path").unwrap();
    assert_eq!(meta_path.len(py).unwrap(), 2);

    let importer = meta_path.get_item(py, 0).unwrap();
    assert_eq!(importer.get_type(py).name(py), "OxidizedFinder");

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
    assert_eq!(importer.get_type(py).name(py), "OxidizedFinder");

    let errno = py.import("errno").unwrap();
    let loader = errno.get(py, "__loader__").unwrap();
    // It isn't OxidizedFinder because OxidizedFinder is just a proxy.
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

/// Run test_importer_builtins.py.
#[test]
fn builtins_py() -> Result<()> {
    run_py_test("test_importer_builtins.py")
}

/// Run test_importer_module.py.
#[test]
fn importer_module_py() -> Result<()> {
    run_py_test("test_importer_module.py")
}

/// Run test_importer_construction.py.
#[test]
fn importer_construction_py() -> Result<()> {
    run_py_test("test_importer_construction.py")
}

/// Run test_importer_iter_modules.py.
#[test]
fn importer_iter_modules_py() -> Result<()> {
    run_py_test("test_importer_iter_modules.py")
}

/// Run test_importer_metadata.py.
#[test]
fn importer_metadata_py() -> Result<()> {
    run_py_test("test_importer_metadata.py")
}

/// Run test_importer_resource_collector.py.
#[test]
fn importer_resource_collector_py() -> Result<()> {
    run_py_test("test_importer_resource_collector.py")
}

/// Run test_importer_resources.py.
#[test]
fn importer_resources_py() -> Result<()> {
    run_py_test("test_importer_resources.py")
}

/// Run test_importer_resource_scanning.py.
#[test]
fn importer_resource_scanning_py() -> Result<()> {
    run_py_test("test_importer_resource_scanning.py")
}

/// Run test_importer_module_loading.py.
#[test]
fn importer_module_loading_py() -> Result<()> {
    run_py_test("test_importer_module_loading.py")
}

/// Run test_importer_resource_reading.py.
#[test]
fn importer_resource_reading_py() -> Result<()> {
    run_py_test("test_importer_resource_reading.py")
}
