// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::fs;

use super::bytecode::{BytecodeCompiler, CompileMode};
use super::dist::PythonDistributionInfo;

pub const PYTHON_IMPORTER: &[u8] = include_bytes!("memoryimporter.py");

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
    let mut compiler = BytecodeCompiler::new(&dist.python_exe);

    let mod_bootstrap_path = &dist.py_modules["importlib._bootstrap"];
    let mod_bootstrap_external_path = &dist.py_modules["importlib._bootstrap_external"];

    let bootstrap_source = fs::read(&mod_bootstrap_path).expect("unable to read bootstrap source");
    let module_name = "<frozen importlib._bootstrap>";
    let bootstrap_bytecode = compiler
        .compile(&bootstrap_source, module_name, 0, CompileMode::Bytecode)
        .expect("error compiling bytecode");

    let mut bootstrap_external_source =
        fs::read(&mod_bootstrap_external_path).expect("unable to read bootstrap_external source");
    bootstrap_external_source.extend("\n# END OF importlib/_bootstrap_external.py\n\n".bytes());
    bootstrap_external_source.extend(PYTHON_IMPORTER);
    let module_name = "<frozen importlib._bootstrap_external>";
    let bootstrap_external_bytecode = compiler
        .compile(
            &bootstrap_external_source,
            module_name,
            0,
            CompileMode::Bytecode,
        )
        .expect("error compiling bytecode");

    ImportlibData {
        bootstrap_source,
        bootstrap_bytecode,
        bootstrap_external_source,
        bootstrap_external_bytecode,
    }
}
