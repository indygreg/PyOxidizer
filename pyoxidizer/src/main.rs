// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::path::PathBuf;

use clap::{App, Arg, SubCommand};

mod analyze;

fn main() {
    let matches = App::new("PyOxidizer")
        .version("0.1")
        .author("Gregory Szorc <gregory.szorc@gmail.com>")
        .about("Integrate Python into Rust")

        .subcommand(SubCommand::with_name("analyze")
            .about("Analyze a built binary")
            .arg(Arg::with_name("path")
                               .help("Path to executable to analyze")))

        .get_matches();

    if let Some(subcommand) = matches.subcommand_matches("analyze") {
        let path = subcommand.value_of("path").unwrap();
        let path = PathBuf::from(path);
        analyze::analyze_file(path);
    } else {
        println!("no sub-command");
    }
}
