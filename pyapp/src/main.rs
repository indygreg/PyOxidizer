// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

extern crate pyembed;

use pyembed::{MainPythonInterpreter, PythonConfig};

fn main() {
    let config = PythonConfig::default();
    let mut interp = MainPythonInterpreter::new(config);

    match interp.run() {
        Ok(_) => {},
        Err(err) => interp.print_err(err),
    }
}
