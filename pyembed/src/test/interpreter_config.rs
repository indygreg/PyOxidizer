// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{MainPythonInterpreter, OxidizedPythonInterpreterConfig},
    anyhow::Result,
    cpython::ObjectProtocol,
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
