// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

mod cli;
mod interpreter;
mod yaml;

fn main() {
    let exit_code = match cli::run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {:#?}", e);
            1
        }
    };

    std::process::exit(exit_code);
}
