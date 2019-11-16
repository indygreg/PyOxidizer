// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::collections::BTreeMap;
use std::fs;

use super::bytecode::{BytecodeCompiler, CompileMode};
use super::dist::{ExtensionModule, PythonDistributionInfo};
use super::resource::BuiltExtensionModule;

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

/// Produce the content of the config.c file containing built-in extensions.
pub fn make_config_c(
    extension_modules: &BTreeMap<String, ExtensionModule>,
    built_extension_modules: &BTreeMap<String, BuiltExtensionModule>,
) -> String {
    // It is easier to construct the file from scratch than parse the template
    // and insert things in the right places.
    let mut lines: Vec<String> = Vec::new();

    lines.push(String::from("#include \"Python.h\""));

    // Declare the initialization functions.
    for em in extension_modules.values() {
        if let Some(init_fn) = &em.init_fn {
            if init_fn == "NULL" {
                continue;
            }

            lines.push(format!("extern PyObject* {}(void);", init_fn));
        }
    }

    for em in built_extension_modules.values() {
        lines.push(format!("extern PyObject* {}(void);", em.init_fn));
    }

    lines.push(String::from("struct _inittab _PyImport_Inittab[] = {"));

    for em in extension_modules.values() {
        if let Some(init_fn) = &em.init_fn {
            if init_fn == "NULL" {
                continue;
            }

            lines.push(format!("{{\"{}\", {}}},", em.module, init_fn));
        }
    }

    for em in built_extension_modules.values() {
        lines.push(format!("{{\"{}\", {}}},", em.name, em.init_fn));
    }

    lines.push(String::from("{0, 0}"));
    lines.push(String::from("};"));

    lines.join("\n")
}
