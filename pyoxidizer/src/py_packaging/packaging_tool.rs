// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use anyhow::{anyhow, Result};
use slog::warn;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::hash::BuildHasher;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use super::distribution::ParsedPythonDistribution;
use super::distutils::{prepare_hacked_distutils, read_built_extensions};
use super::fsscan::{find_python_resources, PythonFileResource};
use super::resource::PythonResource;

/// Find resources installed as part of a packaging operation.
pub fn find_resources(path: &Path, state_dir: Option<&Path>) -> Result<Vec<PythonResource>> {
    let mut res = Vec::new();

    for r in find_python_resources(&path) {
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

    if let Some(p) = state_dir {
        for ext in read_built_extensions(&p)? {
            res.push(PythonResource::BuiltExtensionModule(ext));
        }
    }

    Ok(res)
}

/// Run `pip install` and return found resources.
pub fn pip_install<S: BuildHasher>(
    logger: &slog::Logger,
    dist: &ParsedPythonDistribution,
    verbose: bool,
    install_args: &[String],
    extra_envs: &HashMap<String, String, S>,
) -> Result<Vec<PythonResource>> {
    let temp_dir = tempdir::TempDir::new("pyoxidizer-pip-install")?;

    dist.ensure_pip(logger);

    let mut env = prepare_hacked_distutils(logger, dist, temp_dir.path(), &[])?;

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

    pip_args.extend(install_args.iter().cloned());

    // TODO send stderr to stdout
    let mut cmd = std::process::Command::new(&dist.python_exe)
        .args(&pip_args)
        .envs(&env)
        .stdout(std::process::Stdio::piped())
        .spawn()?;
    {
        let stdout = cmd
            .stdout
            .as_mut()
            .ok_or_else(|| anyhow!("unable to get stdout"))?;
        let reader = BufReader::new(stdout);

        for line in reader.lines() {
            warn!(logger, "{}", line?);
        }
    }

    let status = cmd.wait().unwrap();
    if !status.success() {
        return Err(anyhow!("error running pip"));
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
    for ext in read_built_extensions(&state_dir)? {
        res.push(PythonResource::BuiltExtensionModule(ext));
    }

    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;

    #[test]
    fn test_install_black() -> Result<()> {
        let logger = get_logger()?;
        let distribution = get_default_distribution()?;

        let resources: Vec<PythonResource> = pip_install(
            &logger,
            &distribution,
            false,
            &["black==19.10b0".to_string()],
            &HashMap::new(),
        )?;

        assert!(resources.iter().any(|r| r.full_name() == "appdirs"));
        assert!(resources.iter().any(|r| r.full_name() == "black"));

        Ok(())
    }
}
