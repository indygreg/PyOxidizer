// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

mod cli;

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    std::process::exit(match cli::run_cli().await {
        Ok(_) => 0,
        Err(err) => {
            eprintln!("{:#?}", err);
            1
        }
    });
}
