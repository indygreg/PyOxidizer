// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use pyembed::{default_python_config, MainPythonInterpreter};

fn main() {
    let code = {
        let config = default_python_config();
        let mut interp = MainPythonInterpreter::new(config);
        interp.run_as_main()
    };

    std::process::exit(code);
}
