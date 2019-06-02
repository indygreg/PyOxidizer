// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::path::{Path, PathBuf};

use clap::{App, AppSettings, Arg, SubCommand};

mod analyze;
mod environment;
mod projectmgmt;
#[allow(unused)]
mod pyrepackager;
mod python_distributions;

const ADD_ABOUT: &str = "\
Add PyOxidizer to an existing Rust project.

The PATH argument is a filesystem path to a directory containing an
existing Cargo.toml file. The destination directory MUST NOT have files
belonging to PyOxidizer.

This command will install files and make file modifications required to
embed a Python interpreter in the existing Rust project.

It is highly recommended to have the destination directory under version
control so any unwanted changes can be reverted.

The installed PyOxidizer scaffolding inherits settings such as Python
distribution URLs and dependency crate versions and locations from the
PyOxidizer executable that runs this command.
";

const INIT_ABOUT: &str = "\
Create a new Rust project embedding Python.

The PATH argument is a filesystem path that should be created to hold the
new Rust project.

This command will call `cargo init PATH` and then install files and make
modifications required to embed a Python interpreter in that application.

The new project's binary will be configured to launch a Python REPL by
default.

Created projects inherit settings such as Python distribution URLs and
dependency crate versions and locations from the PyOxidizer executable
they were created with. 

On success, instructions on potential next steps are printed.
";

fn main() {
    let matches = App::new("PyOxidizer")
        .setting(AppSettings::ArgRequiredElseHelp)
        .version("0.1")
        .author("Gregory Szorc <gregory.szorc@gmail.com>")
        .long_about("Build and distribute Python applications")
        .subcommand(
            SubCommand::with_name("add")
                .setting(AppSettings::ArgRequiredElseHelp)
                .about("Add PyOxidizer to an existing Rust project.")
                .long_about(ADD_ABOUT)
                .arg(
                    Arg::with_name("path")
                        .required(true)
                        .value_name("PATH")
                        .help("Directory of existing Rust project"),
                ),
        )
        .subcommand(
            SubCommand::with_name("analyze")
                .about("Analyze a built binary")
                .arg(Arg::with_name("path").help("Path to executable to analyze")),
        )
        .subcommand(
            SubCommand::with_name("init")
                .setting(AppSettings::ArgRequiredElseHelp)
                .about("Create a new Rust project embedding Python.")
                .long_about(INIT_ABOUT)
                .arg(
                    Arg::with_name("name")
                        .required(true)
                        .value_name("PATH")
                        .help("Directory to be created for new project"),
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

            let config = pyrepackager::repackage::process_config_and_copy_artifacts(
                &config_path,
                &build_path,
                &dest_path,
            );

            println!("Initialize a Python interpreter with the following struct:\n");
            println!("{}", config.python_config_rs);

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
