// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!

`PyOxidizer` is a tool and library for producing binaries that embed
Python.

The over-arching goal of `PyOxidizer` is to make complex Python
packaging and distribution problems simple so application maintainers
can focus on building quality applications instead of toiling with
build systems and packaging tools.

`PyOxidizer` is capable of producing a self-contained executable containing
a fully-featured Python interpreter and all Python modules required to run
a Python application. On Linux, it is possible to create a fully static
executable that doesn't even support dynamic loading and can run on nearly
every Linux machine.

The *Oxidizer* part of the name comes from Rust: binaries built with
`PyOxidizer` are compiled from Rust and Rust code is responsible for
managing the embedded Python interpreter and all its operations. But the
existence of Rust should be invisible to many users, much like the fact
that CPython (the official Python distribution available from www.python.org)
is implemented in C. Rust is simply a tool to achieve an end goal, albeit
a rather effective and powerful tool.
*/

use clap::{App, AppSettings, Arg, SubCommand};
use std::path::{Path, PathBuf};

mod analyze;
mod environment;
mod logging;
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

const BUILD_ABOUT: &str = "\
Build a PyOxidizer project.

The PATH argument is a filesystem path to a directory containing an
existing PyOxidizer enabled project.

This command will invoke Rust's build system tool (Cargo) to build
the project.
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

const RUN_BUILD_SCRIPT_ABOUT: &str = "\
Runs a crate build script to generate Python artifacts.

When the Rust crate embedding Python is built, it needs to consume various
artifacts derived from processing the active PyOxidizer TOML config file.
These files are typically generated when the crate's build script runs.

This command executes the functionality to derive various artifacts and
emits special lines that tell the Rust build system how to consume them.

This command is essentially identical to `build-artifacts` except the
output is tailored for the Rust build system.
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
                    Arg::with_name("no-jemalloc")
                        .long("no-jemalloc")
                        .help("Do not use jemalloc"),
                )
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
            SubCommand::with_name("run-build-script")
                .setting(AppSettings::ArgRequiredElseHelp)
                .about("Run functionality that a build script would perform")
                .long_about(RUN_BUILD_SCRIPT_ABOUT)
                .arg(
                    Arg::with_name("build-script-name")
                        .required(true)
                        .help("Value to use for Rust build script"),
                ),
        )
        .subcommand(
            SubCommand::with_name("init")
                .setting(AppSettings::ArgRequiredElseHelp)
                .about("Create a new Rust project embedding Python.")
                .long_about(INIT_ABOUT)
                .arg(
                    Arg::with_name("no-jemalloc")
                        .long("no-jemalloc")
                        .help("Do not use jemalloc"),
                )
                .arg(
                    Arg::with_name("name")
                        .required(true)
                        .value_name("PATH")
                        .help("Directory to be created for new project"),
                ),
        )
        .subcommand(
            SubCommand::with_name("build")
                .setting(AppSettings::ArgRequiredElseHelp)
                .about("Build a PyOxidizer enabled project")
                .long_about(BUILD_ABOUT)
                .arg(
                    Arg::with_name("target")
                        .long("target")
                        .takes_value(true)
                        .help("Rust target triple to build for"),
                )
                .arg(
                    Arg::with_name("release")
                        .long("release")
                        .help("Build a release binary"),
                )
                .arg(
                    Arg::with_name("path")
                        .default_value(".")
                        .value_name("PATH")
                        .help("Directory containing project to build"),
                ),
        )
        .subcommand(
            SubCommand::with_name("build-artifacts")
                .about("Process a PyOxidizer config file and build derived artifacts")
                .arg(
                    Arg::with_name("target")
                        .long("target")
                        .takes_value(true)
                        .help("Rust target triple to build for"),
                )
                .arg(
                    Arg::with_name("release")
                        .long("release")
                        .help("Build a release binary"),
                )
                .arg(
                    Arg::with_name("path")
                        .default_value(".")
                        .value_name("PROJECT_PATH")
                        .help("Path to PyOxidizer config file to process"),
                )
                .arg(
                    Arg::with_name("dest_path")
                        .required(true)
                        .value_name("DIR")
                        .help("Directory to write artifacts to"),
                ),
        )
        .subcommand(
            SubCommand::with_name("run")
                .setting(AppSettings::TrailingVarArg)
                .about("Build and run a PyOxidizer application")
                .arg(
                    Arg::with_name("target")
                        .long("target")
                        .takes_value(true)
                        .help("Rust target triple to build for"),
                )
                .arg(
                    Arg::with_name("release")
                        .long("release")
                        .help("Run a release binary"),
                )
                .arg(
                    Arg::with_name("path")
                        .default_value(".")
                        .value_name("PATH")
                        .help("Directory containing project to build"),
                )
                .arg(Arg::with_name("extra").multiple(true)),
        )
        .subcommand(
            SubCommand::with_name("python-distribution-licenses")
                .about("Show licenses for a given Python distribution")
                .arg(
                    Arg::with_name("path")
                        .required(true)
                        .value_name("PATH")
                        .help("Path or URL to Python distribution to analyze"),
                ),
        )
        .get_matches();

    let logger_context = logging::logger_from_env();

    let result = match matches.subcommand() {
        ("add", Some(args)) => {
            let path = args.value_of("path").unwrap();
            let jemalloc = !args.is_present("no-jemalloc");

            projectmgmt::add_pyoxidizer(Path::new(path), false, jemalloc)
        }

        ("analyze", Some(args)) => {
            let path = args.value_of("path").unwrap();
            let path = PathBuf::from(path);
            analyze::analyze_file(path);

            Ok(())
        }

        ("build-artifacts", Some(args)) => {
            let target = args.value_of("target");
            let release = args.is_present("release");
            let path = args.value_of("path").unwrap();
            let path = PathBuf::from(path);
            let dest_path = args.value_of("dest_path").unwrap();
            let dest_path = PathBuf::from(dest_path);

            projectmgmt::build_artifacts(&logger_context.logger, &path, &dest_path, target, release)
        }

        ("build", Some(args)) => {
            let release = args.is_present("release");
            let target = args.value_of("target");
            let path = args.value_of("path").unwrap();

            projectmgmt::build(&logger_context.logger, path, target, release)
        }

        ("init", Some(args)) => {
            let name = args.value_of("name").unwrap();
            let jemalloc = !args.is_present("no-jemalloc");

            projectmgmt::init(name, jemalloc)
        }

        ("python-distribution-licenses", Some(args)) => {
            let path = args.value_of("path").unwrap();

            projectmgmt::python_distribution_licenses(path)
        }

        ("run-build-script", Some(args)) => {
            let build_script = args.value_of("build-script-name").unwrap();

            projectmgmt::run_build_script(&logger_context.logger, build_script)
        }

        ("run", Some(args)) => {
            let target = args.value_of("target");
            let release = args.is_present("release");
            let path = args.value_of("path").unwrap();;
            let extra: Vec<&str> = args.values_of("extra").unwrap_or_default().collect();

            projectmgmt::run(&logger_context.logger, path, target, release, &extra)
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
