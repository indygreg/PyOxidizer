// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{interpreter::run_python, yaml::run_yaml_path},
    anyhow::{anyhow, Context, Result},
    clap::{value_parser, Arg, ArgAction, Command},
    std::{
        ffi::OsString,
        path::{Path, PathBuf},
    },
};

const PYOXY_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn run() -> Result<i32> {
    let exe = std::env::current_exe().context("resolving current executable")?;

    // If the current executable looks like `python`, we effectively dispatch to
    // `pyoxy run-python -- <args>`.
    if let Some(stem) = exe.file_stem() {
        if stem.to_string_lossy().starts_with("python") {
            return run_python(&exe, &std::env::args_os().skip(1).collect::<Vec<_>>());
        }
    }

    run_normal(&exe)
}

fn run_normal(exe: &Path) -> Result<i32> {
    let app = Command::new("pyoxy")
        .version(PYOXY_VERSION)
        .author("Gregory Szorc <gregory.szorc@gmail.com>")
        .arg_required_else_help(true);

    let app = app.subcommand(
        Command::new("run-python")
            .about("Make the executable behave like a `python` executable")
            .arg(
                Arg::new("args")
                    .help("Arguments to Python interpreter")
                    .action(ArgAction::Append)
                    .num_args(0..)
                    .value_parser(value_parser!(OsString))
                    .last(true),
            ),
    );

    let app = app.subcommand(
        Command::new("run-yaml")
            .about("Run a Python interpreter defined via a YAML file")
            .arg_required_else_help(true)
            .arg(
                Arg::new("yaml_path")
                    .value_name("FILE")
                    .action(ArgAction::Set)
                    .value_parser(value_parser!(PathBuf))
                    .help("Path to YAML file to evaluate"),
            )
            .arg(
                Arg::new("args")
                    .help("Arguments to Python interpreter")
                    .action(ArgAction::Append)
                    .num_args(0..)
                    .value_parser(value_parser!(OsString))
                    .last(true),
            ),
    );

    let matches = app.get_matches();

    match matches.subcommand() {
        Some(("run-python", args)) => {
            let program_args = args
                .get_many::<OsString>("args")
                .unwrap_or_default()
                .collect::<Vec<_>>();

            run_python(exe, &program_args)
        }
        Some(("run-yaml", args)) => {
            let yaml_path = args
                .get_one::<PathBuf>("yaml_path")
                .expect("yaml_path should be set");

            let program_args = args
                .get_many::<OsString>("args")
                .unwrap_or_default()
                .collect::<Vec<_>>();

            run_yaml_path(yaml_path, &program_args)
        }
        _ => Err(anyhow!("invalid sub-command")),
    }
}
