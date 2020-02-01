// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Interaction with Python packaging tools (pip, setuptools, etc).
*/

use anyhow::{anyhow, Context, Result};
use slog::warn;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::hash::BuildHasher;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use super::distribution::PythonDistribution;
use super::distutils::{prepare_hacked_distutils, read_built_extensions};
use super::fsscan::{find_python_resources, PythonFileResource};
use super::resource::PythonResource;
use super::standalone_distribution::{resolve_python_paths, StandaloneDistribution};

/// Find resources installed as part of a packaging operation.
pub fn find_resources(path: &Path, state_dir: Option<&Path>) -> Result<Vec<PythonResource>> {
    let mut res = Vec::new();

    for r in find_python_resources(&path) {
        match r {
            PythonFileResource::Source { .. } => {
                res.push(
                    PythonResource::try_from(&r)
                        .context("converting source module to PythonResource")?,
                );
            }

            PythonFileResource::Resource(..) => {
                res.push(
                    PythonResource::try_from(&r)
                        .context("converting resource file to PythonResource")?,
                );
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
    dist: &StandaloneDistribution,
    verbose: bool,
    install_args: &[String],
    extra_envs: &HashMap<String, String, S>,
) -> Result<Vec<PythonResource>> {
    let temp_dir = tempdir::TempDir::new("pyoxidizer-pip-install")?;

    dist.ensure_pip(logger);

    let orig_distutils_path = dist.stdlib_path.join("distutils");
    let mut env = prepare_hacked_distutils(logger, &orig_distutils_path, temp_dir.path(), &[])?;

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

    let state_dir = PathBuf::from(env.get("PYOXIDIZER_DISTUTILS_STATE_DIR").unwrap());
    find_resources(&target_dir, Some(&state_dir))
}

/// Discover Python resources from a populated virtualenv directory.
pub fn read_virtualenv(dist: &dyn PythonDistribution, path: &Path) -> Result<Vec<PythonResource>> {
    let python_paths = resolve_python_paths(path, &dist.python_major_minor_version());

    find_resources(&python_paths.site_packages, None)
}

/// Run `setup.py install` against a path and return found resources.
pub fn setup_py_install<S: BuildHasher>(
    logger: &slog::Logger,
    dist: &StandaloneDistribution,
    package_path: &Path,
    verbose: bool,
    extra_envs: &HashMap<String, String, S>,
    extra_global_arguments: &[String],
) -> Result<Vec<PythonResource>> {
    if !package_path.is_absolute() {
        return Err(anyhow!(
            "package_path must be absolute: got {:?}",
            package_path.display()
        ));
    }

    let temp_dir = tempdir::TempDir::new("pyoxidizer-setup-py-install")?;

    let target_dir_path = temp_dir.path().join("install");
    let target_dir_s = target_dir_path.display().to_string();

    let python_paths = resolve_python_paths(&target_dir_path, &dist.version);

    std::fs::create_dir_all(&python_paths.site_packages)?;

    let mut envs = prepare_hacked_distutils(
        &logger,
        &dist.stdlib_path.join("distutils"),
        temp_dir.path(),
        &[&python_paths.site_packages, &python_paths.stdlib],
    )?;

    for (key, value) in extra_envs {
        envs.insert(key.clone(), value.clone());
    }

    warn!(
        logger,
        "python setup.py installing {} to {}",
        package_path.display(),
        target_dir_s
    );

    let mut args = vec!["setup.py"];

    if verbose {
        args.push("--verbose");
    }

    for arg in extra_global_arguments {
        args.push(arg);
    }

    args.extend(&["install", "--prefix", &target_dir_s, "--no-compile"]);

    // TODO send stderr to stdout.
    let mut cmd = std::process::Command::new(&dist.python_exe)
        .current_dir(package_path)
        .args(&args)
        .envs(&envs)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("error running setup.py");
    {
        let stdout = cmd.stdout.as_mut().unwrap();
        let reader = BufReader::new(stdout);

        for line in reader.lines() {
            warn!(logger, "{}", line.unwrap());
        }
    }

    let status = cmd.wait().unwrap();
    if !status.success() {
        return Err(anyhow!("error running setup.py"));
    }

    let state_dir = PathBuf::from(envs.get("PYOXIDIZER_DISTUTILS_STATE_DIR").unwrap());
    warn!(
        logger,
        "scanning {} for resources",
        python_paths.site_packages.display()
    );
    find_resources(&python_paths.site_packages, Some(&state_dir))
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
