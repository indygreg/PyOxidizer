// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    anyhow::{Context, Result},
    assert_cmd::{cargo::cargo_bin, Command},
    libtest_mimic::{Arguments, Trial},
    predicates::prelude::*,
};

fn run() -> Result<()> {
    Command::cargo_bin("pyoxy")?
        .arg("run-python")
        .arg("--")
        .arg("-c")
        .arg("print('hello, world')")
        .assert()
        .success()
        .stdout(predicate::eq("hello, world\n").normalize());

    // If the executable is named `python` it behaves like `python`.
    for bin_name in ["python", "python3", "python3.9", "pythonfoo"] {
        let td = tempfile::Builder::new().prefix("pyoxy-test-").tempdir()?;

        let source_exe = cargo_bin("pyoxy");
        let test_exe = td
            .path()
            .join(format!("{}{}", bin_name, std::env::consts::EXE_SUFFIX));
        std::fs::copy(&source_exe, &test_exe).context("creating python executable")?;

        Command::new(&test_exe)
            .arg("-c")
            .arg("print('hello, world')")
            .assert()
            .success()
            .stdout(predicate::eq("hello, world\n").normalize());
    }

    Ok(())
}

fn main() {
    let args = Arguments::from_args();
    let test = Trial::test("main", move || run().map_err(Into::into));
    libtest_mimic::run(&args, vec![test]).exit();
}
