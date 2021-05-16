// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    super::{default_interpreter_config, run_py_test},
    crate::MainPythonInterpreter,
    rusty_fork::rusty_fork_test,
};

rusty_fork_test! {
    #[test]
    fn test_instantiate_interpreter() {
        let config = default_interpreter_config();
        let mut interp = MainPythonInterpreter::new(config).unwrap();
        let py = interp.acquire_gil();
        py.import("sys").unwrap();
    }

    #[test]
    fn multiprocessing_py() {
        run_py_test("test_multiprocessing.py").unwrap()
    }
}
