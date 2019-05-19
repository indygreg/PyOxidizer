// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use pyembed::{MainPythonInterpreter, PythonConfig};

fn main() {
    let config = PythonConfig::default();
    let mut interp = MainPythonInterpreter::new(config);
    interp.run_and_handle_error();
}
