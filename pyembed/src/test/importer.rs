// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    super::{default_interpreter_config, run_py_test},
    crate::MainPythonInterpreter,
    anyhow::Result,
    pyo3::prelude::*,
    rusty_fork::rusty_fork_test,
};

fn new_interpreter<'interpreter, 'resources>(
) -> Result<MainPythonInterpreter<'interpreter, 'resources>> {
    let mut config = default_interpreter_config();
    config.oxidized_importer = true;
    let interp = MainPythonInterpreter::new(config)?;

    Ok(interp)
}

fn get_importer(interp: &MainPythonInterpreter) -> Result<PyObject> {
    interp.with_gil(|py| {
        let sys = py.import("sys").unwrap();
        let meta_path = sys.getattr("meta_path").unwrap();
        assert_eq!(meta_path.len().unwrap(), 2);

        let importer = meta_path.get_item(0).unwrap();
        assert_eq!(importer.get_type().name().unwrap(), "OxidizedFinder");

        Ok(importer.to_object(py))
    })
}

rusty_fork_test! {

    /// We can load our oxidized importer with no resources.
    #[test]
    fn no_resources() {
        let mut config = default_interpreter_config();
        config.oxidized_importer = true;
        let interp = MainPythonInterpreter::new(config).unwrap();

        interp.with_gil(|py| {
            let sys = py.import("sys").unwrap();
            let meta_path = sys.getattr("meta_path").unwrap();
            assert_eq!(meta_path.len().unwrap(), 2);

            let importer = meta_path.get_item(0).unwrap();
            assert_eq!(importer.get_type().name().unwrap(), "OxidizedFinder");

            let errno = py.import("errno").unwrap();
            let loader = errno.getattr("__loader__").unwrap();
            // It isn't OxidizedFinder because OxidizedFinder is just a proxy.
            assert!(loader
                .to_string()
                .contains("_frozen_importlib.BuiltinImporter"));
        });

    }

    /// find_spec() returns None on missing module.
    #[test]
    fn find_spec_missing() {
        let interp = new_interpreter().unwrap();
        let importer = get_importer(&interp).unwrap();

        interp.with_gil(|py| {
            assert!(importer
                .call_method(py, "find_spec", ("missing_package", py.None()), None)
                .unwrap()
                .is_none(py));
            assert!(importer
                .call_method(py, "find_spec", ("foo.bar", py.None()), None)
                .unwrap()
                .is_none(py));
        });
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

    #[test]
    fn importer_pkg_resources_py() {
        run_py_test("test_importer_pkg_resources.py").unwrap()
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

    /// Run test_zip_importer.py
    #[test]
    fn zip_importer_py() {
        run_py_test("test_zip_importer.py").unwrap()
    }
}
