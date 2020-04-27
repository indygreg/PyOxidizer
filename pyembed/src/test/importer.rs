// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{MainPythonInterpreter, OxidizedPythonInterpreterConfig},
    anyhow::Result,
    cpython::ObjectProtocol,
};

/// We can load our oxidized importer with no resources.
#[test]
fn test_no_resources() -> Result<()> {
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
