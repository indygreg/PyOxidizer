// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use slog::warn;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use super::distribution::ParsedPythonDistribution;
use super::distutils::{prepare_hacked_distutils, read_built_extensions};
use super::fsscan::{find_python_resources, PythonFileResource};
use super::resource::PythonResource;

/// Run `pip install` and return found resources.
pub fn pip_install(
    logger: &slog::Logger,
    dist: &ParsedPythonDistribution,
    verbose: bool,
    install_args: &[String],
    extra_envs: &HashMap<String, String>,
) -> Result<Vec<PythonResource>, String> {
    let temp_dir = tempdir::TempDir::new("pyoxidizer-pip-install")
        .or_else(|_| Err("could not create temp directory".to_string()))?;

    dist.ensure_pip(logger);

    let mut env = prepare_hacked_distutils(logger, dist, temp_dir.path(), &[])
        .or_else(|e| Err(e.to_string()))?;

    for (key, value) in extra_envs.iter() {
        env.insert(key.clone(), value.clone());
    }

    let target_dir = temp_dir.path().join("install");

    warn!(logger, "pip installing to {}", target_dir.display());

    let mut pip_args: Vec<String> = vec![
        "-m".to_string(),
        "pip".to_string(),
        "--disable-pip-version-check".to_string(),
    ];

    if verbose {
        pip_args.push("--verbose".to_string());
    }

    pip_args.extend(vec![
        "install".to_string(),
        "--target".to_string(),
        format!("{}", target_dir.display()),
    ]);

    pip_args.extend(install_args.iter().map(|x| x.clone()));

    // TODO send stderr to stdout
    let mut cmd = std::process::Command::new(&dist.python_exe)
        .args(&pip_args)
        .envs(&env)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .or(Err("error running pip".to_string()))?;
    {
        let stdout = cmd
            .stdout
            .as_mut()
            .ok_or("could not open stdout from pip".to_string())?;
        let reader = BufReader::new(stdout);

        for line in reader.lines() {
            warn!(
                logger,
                "{}",
                line.or(Err("could not read line".to_string()))?
            );
        }
    }

    let status = cmd.wait().unwrap();
    if !status.success() {
        return Err("error running pip".to_string());
    }

    let mut res = Vec::new();

    for r in find_python_resources(&target_dir) {
        match r {
            PythonFileResource::Source { .. } => {
                res.push(PythonResource::try_from(&r)?);
            }

            PythonFileResource::Resource(..) => {
                res.push(PythonResource::try_from(&r)?);
            }

            _ => {}
        }
    }

    let state_dir = PathBuf::from(env.get("PYOXIDIZER_DISTUTILS_STATE_DIR").unwrap());
    for ext in read_built_extensions(&state_dir).or_else(|e| Err(e.to_string()))? {
        res.push(PythonResource::BuiltExtensionModule(ext));
    }

    Ok(res)
}
