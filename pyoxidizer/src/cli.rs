// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{
        environment::PYOXIDIZER_VERSION, logging, project_building, project_layout, projectmgmt,
    },
    anyhow::{anyhow, Result},
    clap::{App, AppSettings, Arg, SubCommand},
    std::path::{Path, PathBuf},
};

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
artifacts derived from processing the active PyOxidizer config file.
These files are typically generated when the crate's build script runs.

This command executes the functionality to derive various artifacts and
emits special lines that tell the Rust build system how to consume them.
";

const RESOURCES_SCAN_ABOUT: &str = "\
Scan a directory or file for Python resources.

This command invokes the logic used by various PyOxidizer functionality
walking a directory tree or parsing a file and categorizing seen files.

The directory walking functionality is used by
`oxidized_importer.find_resources_in_path()` and Starlark methods like
`PythonExecutable.pip_install()` and
`PythonExecutable.read_package_root()`.

The file parsing logic is used for parsing the contents of wheels.

This command can be used to debug failures with PyOxidizer's code
for converting files/directories into strongly typed objects. This
conversion is critical for properly packaging Python applications and
bugs can result in incorrect install layouts, missing resources, etc.
";

pub fn run_cli() -> Result<()> {
    let env = crate::environment::resolve_environment()?;

    let matches = App::new("PyOxidizer")
        .setting(AppSettings::ArgRequiredElseHelp)
        .version(PYOXIDIZER_VERSION)
        .long_version(env.version_long().as_str())
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
                )
                .arg(
                    Arg::with_name("target")
                        .long("target")
                        .takes_value(true)
                        .help("The config file target to resolve"),
                ),
        )
        .subcommand(
            SubCommand::with_name("find-resources")
                .about("Find resources in a file or directory")
                .long_about(RESOURCES_SCAN_ABOUT)
                .setting(AppSettings::ArgRequiredElseHelp)
                .arg(
                    Arg::with_name("distributions_dir")
                        .long("distributions-dir")
                        .takes_value(true)
                        .value_name("PATH")
                        .help("Directory to extract downloaded Python distributions into"),
                )
                .arg(
                    Arg::with_name("scan_distribution")
                        .long("--scan-distribution")
                        .help("Scan the Python distribution instead of a path"),
                )
                .arg(
                    Arg::with_name("target_triple")
                        .long("target-triple")
                        .takes_value(true)
                        .default_value(env!("HOST"))
                        .help("Target triple of Python distribution to use"),
                )
                .arg(
                    Arg::with_name("no_classify_files")
                        .long("no-classify-files")
                        .help("Whether to skip classifying files as typed resources"),
                )
                .arg(
                    Arg::with_name("no_emit_files")
                        .long("no-emit-files")
                        .help("Whether to skip emitting File resources"),
                )
                .arg(Arg::with_name("path").value_name("PATH").help(
                    "Filesystem path to scan for resources. Must be a directory or Python wheel",
                )),
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
            SubCommand::with_name("run")
                .setting(AppSettings::TrailingVarArg)
                .about("Run a target in a PyOxidizer configuration file")
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
                .arg(
                    Arg::with_name("target")
                        .long("target")
                        .takes_value(true)
                        .help("Build target to run"),
                )
                .arg(Arg::with_name("extra").multiple(true)),
        )
        .subcommand(
            SubCommand::with_name("python-distribution-extract")
                .about("Extract a Python distribution archive to a directory")
                .arg(
                    Arg::with_name("download-default")
                        .long("--download-default")
                        .help("Download and extract the default distribution for this platform"),
                )
                .arg(
                    Arg::with_name("archive-path")
                        .long("--archive-path")
                        .value_name("DISTRIBUTION_PATH")
                        .help("Path to a Python distribution archive"),
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
            tugger_binary_analysis::analyze_file(path);

            Ok(())
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

        ("find-resources", Some(args)) => {
            let path = if let Some(value) = args.value_of("path") {
                Some(Path::new(value))
            } else {
                None
            };
            let distributions_dir = if let Some(value) = args.value_of("distributions_dir") {
                Some(Path::new(value))
            } else {
                None
            };
            let scan_distribution = args.is_present("scan_distribution");
            let target_triple = args.value_of("target_triple").unwrap();
            let classify_files = !args.is_present("no_classify_files");
            let emit_files = !args.is_present("no_emit_files");

            if path.is_none() && !scan_distribution {
                Err(anyhow!("must specify a path or --scan-distribution"))
            } else {
                projectmgmt::find_resources(
                    &logger_context.logger,
                    path,
                    distributions_dir,
                    scan_distribution,
                    target_triple,
                    classify_files,
                    emit_files,
                )
            }
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
            let download_default = args.is_present("download-default");
            let archive_path = args.value_of("archive-path");
            let dest_path = args.value_of("dest_path").unwrap();

            if !download_default && archive_path.is_none() {
                Err(anyhow!("must specify --download-default or --archive-path"))
            } else if download_default && archive_path.is_some() {
                Err(anyhow!(
                    "must only specify one of --download-default or --archive-path"
                ))
            } else {
                projectmgmt::python_distribution_extract(download_default, archive_path, dest_path)
            }
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
            let target = args.value_of("target");

            project_building::run_from_build(&logger_context.logger, build_script, target)
        }

        ("run", Some(args)) => {
            let target_triple = args.value_of("target_triple");
            let release = args.is_present("release");
            let path = args.value_of("path").unwrap();
            let target = args.value_of("target");
            let extra: Vec<&str> = args.values_of("extra").unwrap_or_default().collect();

            projectmgmt::run(
                &logger_context.logger,
                Path::new(path),
                target_triple,
                release,
                target,
                &extra,
                verbose,
            )
        }

        _ => Err(anyhow!("invalid sub-command")),
    }
}
