// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    anyhow::Result,
    assert_cmd::Command,
    libtest_mimic::{run_tests, Arguments, Outcome, Test},
    predicates::prelude::*,
};

fn run() -> Result<()> {
    for yaml_path in glob::glob("tests/yaml/*.yaml")? {
        let yaml_path = yaml_path?;
        let stdout_path = yaml_path.with_extension("stdout");

        assert!(stdout_path.exists());
        let expected_stdout = std::fs::read_to_string(&stdout_path)?;

        Command::cargo_bin("pyoxy")?
            .arg("run-yaml")
            .arg(&yaml_path)
            .assert()
            .success()
            .stdout(predicate::str::contains(&expected_stdout).normalize());
    }

    Ok(())
}

fn main() {
    let args = Arguments::from_args();

    // libtest_mimic doesn't properly handle `--list --ignored`.
    let tests: Vec<Test<()>> = if args.ignored {
        vec![]
    } else {
        vec![Test::test("main")]
    };

    run_tests(&args, tests, |_| match run() {
        Ok(_) => Outcome::Passed,
        Err(e) => Outcome::Failed {
            msg: Some(format!("{:?}", e)),
        },
    })
    .exit();
}
