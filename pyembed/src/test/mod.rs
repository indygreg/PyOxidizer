// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::OxidizedPythonInterpreterConfig;

mod importer;
mod interpreter_config;
mod main_python_interpreter;

pub const PYTHON_INTERPRETER_PATH: &str = env!("PYTHON_INTERPRETER_PATH");

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
    //
    // In theory this is all we need to do to get a working interpreter, as the
    // default path layout baked into the interpreter is appropriate. However,
    // because Python calculates paths relative to argv[0] and argv[0] is a
    // Rust executable, the resulting calculation would be wrong. So we
    // forcefully set argv to as if it were the actual interpreter path as a
    // workaround. Tests related to argv handling need to overwrite accordingly.
    config.set_missing_path_configuration = false;
    config.argv = Some(vec![std::ffi::OsString::from(PYTHON_INTERPRETER_PATH)]);

    config
}

/// Set `sys.paths` on the config to pick up resources from the Python interpreter.
pub fn set_sys_paths(config: &mut OxidizedPythonInterpreterConfig) {
    // This is only needed on Windows, as UNIX builds seem to do the right
    // thing.
    if cfg!(target_family = "windows") {
        let parent = std::path::PathBuf::from(PYTHON_INTERPRETER_PATH)
            .parent()
            .expect("could not compute Python interpreter parent directory")
            .to_path_buf();

        config.interpreter_config.module_search_paths =
            Some(vec![parent.join("DLLs"), parent.join("Lib")]);
    }
}
