// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::OxidizedPythonInterpreterConfig;

mod importer;
mod interpreter_config;
mod main_python_interpreter;

/// Obtain an [OxidizedPythonInterpreterConfig] suitable for use in tests.
pub fn default_interpreter_config<'a>() -> OxidizedPythonInterpreterConfig<'a> {
    let mut config = OxidizedPythonInterpreterConfig::default();

    // Otherwise arguments to the Rust test binary can be interpreted as Python
    // arguments.
    config.interpreter_config.parse_argv = Some(false);

    // Prevent pyembed from setting program_name and home automatically. If these
    // were set, the Python interpreter would assume the Rust test executable
    // is the Python interpreter and would calculate paths (e.g. to the stdlib)
    // accordingly. In the context of tests this is wrong because there are no
    // embedded resources.
    config.set_missing_path_configuration = false;

    config
}
