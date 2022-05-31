// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    anyhow::Result,
    assert_cmd::Command,
    assert_fs::{prelude::*, TempDir},
    predicates::prelude::*,
};

fn get_command() -> Result<Command> {
    let mut command = Command::cargo_bin("pyoxidizer")?;
    command.arg("generate-python-embedding-artifacts").arg("--");

    Ok(command)
}

fn no_args_fails() -> Result<()> {
    get_command()?.assert().failure().stderr(
        predicates::str::contains("error: The following required arguments were not provided:")
            .normalize(),
    );

    Ok(())
}

fn default_behavior() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let out_dir = temp_dir.path();

    get_command()?
        .arg(format!("{}", out_dir.display()))
        .assert()
        .success();

    temp_dir
        .child("default_python_config.rs")
        .assert(predicates::path::is_file());
    temp_dir
        .child("packed-resources")
        .assert(predicates::path::is_file());
    temp_dir
        .child("pyo3-build-config-file.txt")
        .assert(predicates::path::is_file());
    temp_dir.child("tcl").assert(predicates::path::is_dir());

    if cfg!(target_family = "unix") {
        temp_dir
            .child("libpython3.a")
            .assert(predicates::path::is_file());
    }
    if cfg!(target_family = "windows") {
        temp_dir
            .child("python3.dll")
            .assert(predicates::path::is_file());
        temp_dir
            .child("python310.dll")
            .assert(predicates::path::is_file());
    }

    Ok(())
}

fn run() -> Result<()> {
    no_args_fails()?;
    default_behavior()?;

    Ok(())
}

fn main() {
    run().expect("all tests should pass");
}
