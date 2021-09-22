// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::yaml::run_yaml_path,
    anyhow::{anyhow, Result},
    clap::{App, AppSettings, Arg, SubCommand},
    std::path::PathBuf,
};

const PYOXY_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn run() -> Result<i32> {
    let app = App::new("pyoxy")
        .setting(AppSettings::ArgRequiredElseHelp)
        .version(PYOXY_VERSION)
        .author("Gregory Szorc <gregory.szorc@gmail.com>");

    let app = app.subcommand(
        SubCommand::with_name("run-yaml")
            .about("Run a Python interpreter defined via a YAML file")
            .setting(AppSettings::ArgRequiredElseHelp)
            .arg(
                Arg::with_name("yaml_path")
                    .value_name("FILE")
                    .help("Path to YAML file to evaluate"),
            )
            .arg(
                Arg::with_name("args")
                    .help("Arguments to Python interpreter")
                    .multiple(true)
                    .last(true),
            ),
    );

    let matches = app.get_matches();

    match matches.subcommand() {
        ("run-yaml", Some(args)) => {
            let yaml_path = PathBuf::from(
                args.value_of_os("yaml_path")
                    .expect("yaml_path should be set"),
            );
            let program_args = args
                .values_of_os("args")
                .unwrap_or_default()
                .collect::<Vec<_>>();

            run_yaml_path(&yaml_path, &program_args)
        }
        _ => Err(anyhow!("invalid sub-command")),
    }
}
