// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{MainPythonInterpreter, OxidizedPythonInterpreterConfig},
    rusty_fork::rusty_fork_test,
};

rusty_fork_test! {
    #[test]
    fn test_instantiate_interpreter() {
        let mut config = OxidizedPythonInterpreterConfig::default();
        config.interpreter_config.parse_argv = Some(false);
        config.set_missing_path_configuration = false;
        let mut interp = MainPythonInterpreter::new(config).unwrap();
        let py = interp.acquire_gil();
        py.import("sys").unwrap();
    }
}
