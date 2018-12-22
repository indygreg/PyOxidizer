// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::bytecode::compile_bytecode;
use super::dist::PythonDistributionInfo;

pub const PYTHON_IMPORTER: &'static [u8] = include_bytes!("memoryimporter.py");

pub struct ImportlibData {
    pub bootstrap_source: Vec<u8>,
    pub bootstrap_bytecode: Vec<u8>,
    pub bootstrap_external_source: Vec<u8>,
    pub bootstrap_external_bytecode: Vec<u8>,
}

/// Produce frozen importlib bytecode data.
///
/// importlib._bootstrap isn't modified.
///
/// importlib._bootstrap_external is modified. We take the original Python
/// source and concatenate with code that provides the memory importer.
/// Bytecode is then derived from it.
pub fn derive_importlib(dist: &PythonDistributionInfo) -> ImportlibData {
    let mod_bootstrap = dist.py_modules.get("importlib._bootstrap").unwrap();
    let mod_bootstrap_external = dist.py_modules.get("importlib._bootstrap_external").unwrap();

    let bootstrap_source = &mod_bootstrap.py;
    let module_name = "<frozen importlib._bootstrap>";
    let bootstrap_bytecode = compile_bytecode(bootstrap_source, module_name);

    let mut bootstrap_external_source = mod_bootstrap_external.py.clone();
    bootstrap_external_source.extend("\n# END OF importlib/_bootstrap_external.py\n\n".bytes());
    bootstrap_external_source.extend(PYTHON_IMPORTER);
    let module_name = "<frozen importlib._bootstrap_external>";
    let bootstrap_external_bytecode = compile_bytecode(&bootstrap_external_source, module_name);

    ImportlibData {
        bootstrap_source: bootstrap_source.clone(),
        bootstrap_bytecode,
        bootstrap_external_source: bootstrap_external_source.clone(),
        bootstrap_external_bytecode,
    }
}
