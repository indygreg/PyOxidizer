// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::path::{Path, PathBuf};

use clap::{App, Arg, SubCommand};

mod analyze;
mod environment;
mod projectmgmt;
#[allow(unused)]
mod pyrepackager;
mod python_distributions;

fn main() {
    let matches = App::new("PyOxidizer")
        .version("0.1")
        .author("Gregory Szorc <gregory.szorc@gmail.com>")
        .about("Integrate Python into Rust")
        .subcommand(
            SubCommand::with_name("add")
                .about("Add PyOxidizer to an existing Rust project")
                .arg(
                    Arg::with_name("path")
                        .required(true)
                        .value_name("PATH")
                        .help("Path to existing Rust project to modify"),
                ),
        )
        .subcommand(
            SubCommand::with_name("analyze")
                .about("Analyze a built binary")
                .arg(Arg::with_name("path").help("Path to executable to analyze")),
        )
        .subcommand(
            SubCommand::with_name("init")
                .about("Initialize a new Rust project embedding Python")
                .arg(
                    Arg::with_name("name")
                        .required(true)
                        .value_name("NAME")
                        .help("Name of project to initialize"),
                ),
        )
        .subcommand(
            SubCommand::with_name("build-artifacts")
                .about("Process a PyOxidizer config file and build derived artifacts")
                .arg(
                    Arg::with_name("config_path")
                        .required(true)
                        .value_name("CONFIG_PATH")
                        .help("Path to PyOxidizer config file to process"),
                )
                .arg(
                    Arg::with_name("build_path")
                        .long("build-dir")
                        .value_name("DIR")
                        .help("Directory for intermediate build state"),
                )
                .arg(
                    Arg::with_name("dest_path")
                        .required(true)
                        .value_name("DIR")
                        .help("Directory to write artifacts to"),
                ),
        )
        .get_matches();

    let result = match matches.subcommand() {
        ("add", Some(args)) => {
            let path = args.value_of("path").unwrap();

            projectmgmt::add_pyoxidizer(Path::new(path), false)
        }

        ("analyze", Some(args)) => {
            let path = args.value_of("path").unwrap();
            let path = PathBuf::from(path);
            analyze::analyze_file(path);

            Ok(())
        }

        ("build-artifacts", Some(args)) => {
            let config_path = args.value_of("config_path").unwrap();
            let config_path = PathBuf::from(config_path);

            let (build_path, _temp_dir) = match args.value_of("build_path") {
                Some(path) => (PathBuf::from(path), None),
                None => {
                    let temp_dir = tempdir::TempDir::new("pyoxidizer-build-artifacts")
                        .expect("unable to create temp dir");

                    (PathBuf::from(temp_dir.path()), Some(temp_dir))
                }
            };

            let dest_path = args.value_of("dest_path").unwrap();
            let dest_path = PathBuf::from(dest_path);

            pyrepackager::repackage::process_config_and_copy_artifacts(
                &config_path,
                &build_path,
                &dest_path,
            );

            Ok(())
        }

        ("init", Some(args)) => {
            let name = args.value_of("name").unwrap();

            projectmgmt::init(name)
        }
        _ => Err("invalid sub-command".to_string()),
    };

    match result {
        Ok(_) => {}
        Err(e) => {
            println!("error: {}", e);
            std::process::exit(1);
        }
    }
}
