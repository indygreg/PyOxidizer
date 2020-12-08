// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{MainPythonInterpreter, OxidizedPythonInterpreterConfig},
    anyhow::{anyhow, Result},
    cpython::{ObjectProtocol, PyObject},
    rusty_fork::rusty_fork_test,
    std::path::PathBuf,
};

fn new_interpreter<'python, 'interpreter, 'resources>(
) -> Result<MainPythonInterpreter<'python, 'interpreter, 'resources>> {
    let mut config = OxidizedPythonInterpreterConfig::default();
    // Otherwise Rust test executable args are parsed as Python args.
    config.interpreter_config.parse_argv = Some(false);
    config.oxidized_importer = true;
    config.set_missing_path_configuration = false;
    let interp = MainPythonInterpreter::new(config)?;

    Ok(interp)
}

fn run_py_test(test_filename: &str) -> Result<()> {
    let test_dir = env!("PYEMBED_TESTS_DIR");
    let test_path = PathBuf::from(test_dir).join(test_filename);

    let mut config = OxidizedPythonInterpreterConfig::default();
    // Normalize the arguments in the interpreter and prevent Python from parsing them.
    config.interpreter_config.parse_argv = Some(false);
    config.argv = Some(vec![std::env::current_exe().unwrap().into_os_string()]);
    config.oxidized_importer = true;
    config.interpreter_config.run_filename = Some(test_path);
    config.interpreter_config.buffered_stdio = Some(false);
    config.set_missing_path_configuration = false;

    if MainPythonInterpreter::new(config)?.py_runmain() != 0 {
        Err(anyhow!("Python code did not exit successfully"))
    } else {
        Ok(())
    }
}

fn get_importer(interp: &mut MainPythonInterpreter) -> Result<PyObject> {
    let py = interp.acquire_gil();

    let sys = py.import("sys").unwrap();
    let meta_path = sys.get(py, "meta_path").unwrap();
    assert_eq!(meta_path.len(py).unwrap(), 2);

    let importer = meta_path.get_item(py, 0).unwrap();
    assert_eq!(importer.get_type(py).name(py), "OxidizedFinder");

    Ok(importer)
}

rusty_fork_test! {

    /// We can load our oxidized importer with no resources.
    #[test]
    fn no_resources() {
        let mut config = OxidizedPythonInterpreterConfig::default();
        // Otherwise Rust test executable args are parsed as Python args.
        config.interpreter_config.parse_argv = Some(false);
        config.oxidized_importer = true;
        config.set_missing_path_configuration = false;
        let mut interp = MainPythonInterpreter::new(config).unwrap();

        let py = interp.acquire_gil();
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
    }

    /// find_spec() returns None on missing module.
    #[test]
    fn find_spec_missing() {
        let mut interp = new_interpreter().unwrap();
        let importer = get_importer(&mut interp).unwrap();
        let py = interp.acquire_gil();

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
    }

    /// Run test_importer_builtins.py.
    #[test]
    fn builtins_py() {
        run_py_test("test_importer_builtins.py").unwrap()
    }

    /// Run test_importer_module.py.
    #[test]
    fn importer_module_py() {
        run_py_test("test_importer_module.py").unwrap()
    }

    /// Run test_importer_construction.py.
    #[test]
    fn importer_construction_py() {
        run_py_test("test_importer_construction.py").unwrap()
    }

    #[test]
    fn importer_indexing() {
        run_py_test("test_importer_indexing.py").unwrap()
    }

    /// Run test_importer_iter_modules.py.
    #[test]
    fn importer_iter_modules_py() {
        run_py_test("test_importer_iter_modules.py").unwrap()
    }

    /// Run test_importer_metadata.py.
    #[test]
    fn importer_metadata_py() {
        run_py_test("test_importer_metadata.py").unwrap()
    }

    /// Run test_importer_resource_collector.py.
    #[test]
    fn importer_resource_collector_py() {
        run_py_test("test_importer_resource_collector.py").unwrap()
    }

    /// Run test_importer_resources.py.
    #[test]
    fn importer_resources_py() {
        run_py_test("test_importer_resources.py").unwrap()
    }

    /// Run test_importer_resource_scanning.py.
    #[test]
    fn importer_resource_scanning_py() {
        run_py_test("test_importer_resource_scanning.py").unwrap()
    }

    /// Run test_importer_module_loading.py.
    #[test]
    fn importer_module_loading_py() {
        run_py_test("test_importer_module_loading.py").unwrap()
    }

    /// Run test_importer_resource_reading.py.
    #[test]
    fn importer_resource_reading_py() {
        run_py_test("test_importer_resource_reading.py").unwrap()
    }

    /// Run test_importer_path_entry_finder.py.
    #[test]
    fn importer_path_entry_finder_py() {
        run_py_test("test_importer_path_entry_finder.py").unwrap()
    }
}
