// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use anyhow::{anyhow, Result};
use clap::{App, AppSettings, Arg, SubCommand};
use std::path::{Path, PathBuf};

use super::analyze;
use super::environment::BUILD_SEMVER_LIGHTWEIGHT;
use super::logging;
use super::project_layout;
use super::projectmgmt;

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

const INIT_RUST_PROJECT_ABOUT: &str = "\
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

pub fn run_cli() -> Result<()> {
    let matches = App::new("PyOxidizer")
        .setting(AppSettings::ArgRequiredElseHelp)
        .version(BUILD_SEMVER_LIGHTWEIGHT)
        .author("Gregory Szorc <gregory.szorc@gmail.com>")
        .long_about("Build and distribute Python applications")
        .arg(
            Arg::with_name("verbose")
                .long("verbose")
                .help("Enable verbose output"),
        )
        .subcommand(
            SubCommand::with_name("add")
                .setting(AppSettings::ArgRequiredElseHelp)
                .about("Add PyOxidizer to an existing Rust project. (EXPERIMENTAL)")
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
                .setting(AppSettings::ArgRequiredElseHelp)
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
            SubCommand::with_name("init-config-file")
                .setting(AppSettings::ArgRequiredElseHelp)
                .about("Create a new PyOxidizer configuration file.")
                .arg(
                    Arg::with_name("python-code")
                        .long("python-code")
                        .takes_value(true)
                        .help("Default Python code to execute in built executable"),
                )
                .arg(
                    Arg::with_name("pip-install")
                        .long("pip-install")
                        .takes_value(true)
                        .multiple(true)
                        .number_of_values(1)
                        .help("Python package to install via `pip install`"),
                )
                .arg(
                    Arg::with_name("path")
                        .required(true)
                        .value_name("PATH")
                        .help("Directory where configuration file should be created"),
                ),
        )
        .subcommand(
            SubCommand::with_name("init-rust-project")
                .setting(AppSettings::ArgRequiredElseHelp)
                .about("Create a new Rust project embedding a Python interpreter")
                .long_about(INIT_RUST_PROJECT_ABOUT)
                .arg(
                    Arg::with_name("path")
                        .required(true)
                        .value_name("PATH")
                        .help("Path of project directory to create"),
                ),
        )
        .subcommand(
            SubCommand::with_name("list-targets")
                .setting(AppSettings::ArgRequiredElseHelp)
                .about("List targets available to resolve in a configuration file")
                .arg(
                    Arg::with_name("path")
                        .default_value(".")
                        .value_name("PATH")
                        .help("Path to project to evaluate"),
                ),
        )
        .subcommand(
            SubCommand::with_name("build")
                .setting(AppSettings::ArgRequiredElseHelp)
                .about("Build a PyOxidizer enabled project")
                .long_about(BUILD_ABOUT)
                .arg(
                    Arg::with_name("target_triple")
                        .long("target-triple")
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
                        .long("path")
                        .takes_value(true)
                        .default_value(".")
                        .value_name("PATH")
                        .help("Directory containing project to build"),
                )
                .arg(
                    Arg::with_name("targets")
                        .value_name("TARGET")
                        .multiple(true)
                        .help("Target to resolve"),
                ),
        )
        .subcommand(
            SubCommand::with_name("build-artifacts")
                .about("Process a PyOxidizer config file and build derived artifacts")
                .arg(
                    Arg::with_name("dest_path")
                        .required(true)
                        .value_name("DIR")
                        .help("Directory to write artifacts to"),
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
                        .help("Path to PyOxidizer project to process"),
                ),
        )
        .subcommand(
            SubCommand::with_name("run")
                .setting(AppSettings::TrailingVarArg)
                .about("Build and run a PyOxidizer application")
                .arg(
                    Arg::with_name("target_triple")
                        .long("target-triple")
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
                        .long("path")
                        .default_value(".")
                        .value_name("PATH")
                        .help("Directory containing project to build"),
                )
                .arg(Arg::with_name("extra").multiple(true)),
        )
        .subcommand(
            SubCommand::with_name("python-distribution-extract")
                .about("Extract a Python distribution archive to a directory")
                .arg(
                    Arg::with_name("dist_path")
                        .required(true)
                        .value_name("DISTRIBUTION_PATH")
                        .help("Path to a Python distribution"),
                )
                .arg(
                    Arg::with_name("dest_path")
                        .required(true)
                        .value_name("DESTINATION_PATH")
                        .help("Path to directory where distribution should be extracted"),
                ),
        )
        .subcommand(
            SubCommand::with_name("python-distribution-info")
                .about("Show information about a Python distribution archive")
                .arg(
                    Arg::with_name("path")
                        .required(true)
                        .value_name("PATH")
                        .help("Path to Python distribution archive to analyze"),
                ),
        )
        .subcommand(
            SubCommand::with_name("python-distribution-licenses")
                .about("Show licenses for a given Python distribution")
                .arg(
                    Arg::with_name("path")
                        .required(true)
                        .value_name("PATH")
                        .help("Path to Python distribution to analyze"),
                ),
        )
        .get_matches();

    let verbose = matches.is_present("verbose");

    let log_level = if verbose {
        slog::Level::Info
    } else {
        slog::Level::Warning
    };

    let logger_context = logging::logger_from_env(log_level);

    match matches.subcommand() {
        ("add", Some(args)) => {
            let path = args.value_of("path").unwrap();

            project_layout::add_pyoxidizer(Path::new(path), false)
        }

        ("analyze", Some(args)) => {
            let path = args.value_of("path").unwrap();
            let path = PathBuf::from(path);
            analyze::analyze_file(path);

            Ok(())
        }

        ("build-artifacts", Some(args)) => {
            let path = args.value_of("path").unwrap();
            let path = PathBuf::from(path);
            let release = args.is_present("release");
            let dest_path = args.value_of("dest_path").unwrap();
            let dest_path = PathBuf::from(dest_path);

            projectmgmt::build_artifacts(&logger_context.logger, &path, &dest_path, release)
        }

        ("build", Some(args)) => {
            let release = args.is_present("release");
            let target_triple = args.value_of("target_triple");
            let path = args.value_of("path").unwrap();
            let resolve_targets = if let Some(values) = args.values_of("targets") {
                Some(values.map(|x| x.to_string()).collect())
            } else {
                None
            };

            projectmgmt::build(
                &logger_context.logger,
                Path::new(path),
                target_triple,
                resolve_targets,
                release,
                verbose,
            )
        }

        ("init-config-file", Some(args)) => {
            let code = args.value_of("python-code");
            let pip_install = if args.is_present("pip-install") {
                args.values_of("pip-install").unwrap().collect()
            } else {
                Vec::new()
            };
            let path = args.value_of("path").unwrap();
            let config_path = Path::new(path);

            projectmgmt::init_config_file(&config_path, code, &pip_install)
        }

        ("list-targets", Some(args)) => {
            let path = args.value_of("path").unwrap();

            projectmgmt::list_targets(&logger_context.logger, Path::new(path))
        }

        ("init-rust-project", Some(args)) => {
            let path = args.value_of("path").unwrap();
            let project_path = Path::new(path);

            projectmgmt::init_rust_project(&project_path)
        }

        ("python-distribution-extract", Some(args)) => {
            let dist_path = args.value_of("dist_path").unwrap();
            let dest_path = args.value_of("dest_path").unwrap();

            projectmgmt::python_distribution_extract(dist_path, dest_path)
        }

        ("python-distribution-info", Some(args)) => {
            let dist_path = args.value_of("path").unwrap();

            projectmgmt::python_distribution_info(dist_path)
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
            let target_triple = args.value_of("target_triple");
            let release = args.is_present("release");
            let path = args.value_of("path").unwrap();
            let extra: Vec<&str> = args.values_of("extra").unwrap_or_default().collect();

            projectmgmt::run(
                &logger_context.logger,
                Path::new(path),
                target_triple,
                release,
                &extra,
                verbose,
            )
        }

        _ => Err(anyhow!("invalid sub-command")),
    }
}
