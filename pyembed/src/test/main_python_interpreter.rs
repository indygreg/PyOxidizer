// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    super::{default_interpreter_config, run_py_test},
    crate::MainPythonInterpreter,
    pyo3::ffi as pyffi,
    rusty_fork::rusty_fork_test,
};

rusty_fork_test! {
    #[test]
    fn test_instantiate_interpreter() {
        let config = default_interpreter_config();
        let interp = MainPythonInterpreter::new(config).unwrap();
        interp.with_gil(|py| {
            py.import("sys").unwrap();
        });
    }

    #[test]
    fn interpreter_gil_state() {
        let config = default_interpreter_config();
        let interp = MainPythonInterpreter::new(config).unwrap();

        assert_eq!(unsafe { pyffi::PyGILState_Check() }, 0);

        interp.with_gil(|_| {
            assert_eq!(unsafe { pyffi::PyGILState_Check() }, 1);
        });

        assert_eq!(unsafe { pyffi::PyGILState_Check() }, 0);

        std::mem::drop(interp);
    }

    #[test]
    fn multiprocessing_py() {
        run_py_test("test_multiprocessing.py").unwrap()
    }
}
