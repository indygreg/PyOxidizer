// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use pyembed::{default_python_config, MainPythonInterpreter};

fn main() {
    let code = {
        let config = default_python_config();
        match MainPythonInterpreter::new(config) {
            Ok(mut interp) => interp.run_as_main(),
            Err(msg) => {
                eprintln!("{}", msg);
                1
            }
        }
    };

    std::process::exit(code);
}
