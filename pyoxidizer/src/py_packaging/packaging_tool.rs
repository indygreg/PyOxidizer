// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Interaction with Python packaging tools (pip, setuptools, etc).
*/

use {
    super::{
        binary::LibpythonLinkMode, distribution::PythonDistribution,
        distutils::read_built_extensions, standalone_distribution::resolve_python_paths,
    },
    crate::environment::Environment,
    anyhow::{anyhow, Context, Result},
    duct::{cmd, ReaderHandle},
    log::warn,
    python_packaging::{
        filesystem_scanning::find_python_resources, policy::PythonPackagingPolicy,
        resource::PythonResource, wheel::WheelArchive,
    },
    std::{
        collections::{hash_map::RandomState, HashMap},
        hash::BuildHasher,
        io::{BufRead, BufReader},
        path::{Path, PathBuf},
    },
};

fn log_command_output(handle: &ReaderHandle) {
    let reader = BufReader::new(handle);
    for line in reader.lines() {
        match line {
            Ok(line) => {
                warn!("{}", line);
            }
            Err(err) => {
                warn!("Error when reading output: {:?}", err);
            }
        }
    }
}

/// Find resources installed as part of a packaging operation.
pub fn find_resources<'a>(
    dist: &dyn PythonDistribution,
    policy: &PythonPackagingPolicy,
    path: &Path,
    state_dir: Option<PathBuf>,
) -> Result<Vec<PythonResource<'a>>> {
    let mut res = Vec::new();

    let built_extensions = if let Some(p) = state_dir {
        read_built_extensions(&p)?
            .iter()
            .map(|ext| (ext.name.clone(), ext.clone()))
            .collect()
    } else {
        HashMap::new()
    };

    for r in find_python_resources(
        path,
        dist.cache_tag(),
        &dist.python_module_suffixes()?,
        policy.file_scanner_emit_files(),
        policy.file_scanner_classify_files(),
    )? {
        let r = r?.to_memory()?;

        match r {
            PythonResource::ExtensionModule(e) => {
                // Use a built extension if present, as it will contain more metadata.
                res.push(if let Some(built) = built_extensions.get(&e.name) {
                    PythonResource::from(built.to_memory()?)
                } else {
                    PythonResource::ExtensionModule(e)
                });
            }
            _ => {
                res.push(r);
            }
        }
    }

    Ok(res)
}

/// Run `pip download` and collect resources found from downloaded packages.
///
/// `host_dist` is the Python distribution to use to run `pip`.
///
/// `build_dist` is the Python distribution that packages are being downloaded
/// for.
///
/// The distributions are often the same. But passing a different
/// distribution targeting a different platform allows this command to
/// resolve resources for a non-native platform, which enables it to be used
/// when cross-compiling.
pub fn pip_download<'a>(
    env: &Environment,
    host_dist: &dyn PythonDistribution,
    taget_dist: &dyn PythonDistribution,
    policy: &PythonPackagingPolicy,
    verbose: bool,
    args: &[String],
) -> Result<Vec<PythonResource<'a>>> {
    let temp_dir = env.temporary_directory("pyoxidizer-pip-download")?;

    host_dist.ensure_pip()?;

    let target_dir = temp_dir.path();

    warn!("pip downloading to {}", target_dir.display());

    let mut pip_args = vec![
        "-m".to_string(),
        "pip".to_string(),
        "--disable-pip-version-check".to_string(),
    ];

    if verbose {
        pip_args.push("--verbose".to_string());
    }

    pip_args.extend(vec![
        "download".to_string(),
        // Download packages to our temporary directory.
        "--dest".to_string(),
        format!("{}", target_dir.display()),
        // Only download wheels.
        "--only-binary=:all:".to_string(),
        // We download files compatible with the distribution we're targeting.
        format!(
            "--platform={}",
            taget_dist.python_platform_compatibility_tag()
        ),
        format!("--python-version={}", taget_dist.python_version()),
        format!(
            "--implementation={}",
            taget_dist.python_implementation_short()
        ),
    ]);

    if let Some(abi) = taget_dist.python_abi_tag() {
        pip_args.push(format!("--abi={}", abi));
    }

    pip_args.extend(args.iter().cloned());

    warn!("running python {:?}", pip_args);

    let command = cmd(host_dist.python_exe_path(), &pip_args)
        .stderr_to_stdout()
        .unchecked()
        .reader()?;

    log_command_output(&command);

    let output = command
        .try_wait()?
        .ok_or_else(|| anyhow!("unable to wait on command"))?;
    if !output.status.success() {
        return Err(anyhow!("error running pip"));
    }

    // Since we used --only-binary=:all: above, we should only have .whl files
    // in the destination directory. Iterate over them and collect resources
    // from each.

    let mut files = std::fs::read_dir(target_dir)?
        .map(|entry| Ok(entry?.path()))
        .collect::<Result<Vec<_>>>()?;
    files.sort();

    // TODO there's probably a way to do this using iterators.
    let mut res = Vec::new();

    for path in &files {
        let wheel = WheelArchive::from_path(path)?;

        res.extend(wheel.python_resources(
            taget_dist.cache_tag(),
            &taget_dist.python_module_suffixes()?,
            policy.file_scanner_emit_files(),
            policy.file_scanner_classify_files(),
        )?);
    }

    temp_dir.close().context("closing temporary directory")?;

    Ok(res)
}

/// Run `pip install` and return found resources.
pub fn pip_install<'a, S: BuildHasher>(
    env: &Environment,
    dist: &dyn PythonDistribution,
    policy: &PythonPackagingPolicy,
    libpython_link_mode: LibpythonLinkMode,
    verbose: bool,
    install_args: &[String],
    extra_envs: &HashMap<String, String, S>,
) -> Result<Vec<PythonResource<'a>>> {
    let temp_dir = env.temporary_directory("pyoxidizer-pip-install")?;

    dist.ensure_pip()?;

    let mut env: HashMap<String, String, RandomState> = std::env::vars().collect();
    for (k, v) in dist.resolve_distutils(libpython_link_mode, temp_dir.path(), &[])? {
        env.insert(k, v);
    }

    for (key, value) in extra_envs.iter() {
        env.insert(key.clone(), value.clone());
    }

    let target_dir = temp_dir.path().join("install");

    warn!("pip installing to {}", target_dir.display());

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

    let command = cmd(dist.python_exe_path(), &pip_args)
        .full_env(&env)
        .stderr_to_stdout()
        .unchecked()
        .reader()?;

    log_command_output(&command);

    let output = command
        .try_wait()?
        .ok_or_else(|| anyhow!("unable to wait on command"))?;
    if !output.status.success() {
        return Err(anyhow!("error running pip"));
    }

    let state_dir = env.get("PYOXIDIZER_DISTUTILS_STATE_DIR").map(PathBuf::from);

    let resources =
        find_resources(dist, policy, &target_dir, state_dir).context("scanning for resources")?;

    temp_dir.close().context("closing temporary directory")?;

    Ok(resources)
}

/// Discover Python resources from a populated virtualenv directory.
pub fn read_virtualenv<'a>(
    dist: &dyn PythonDistribution,
    policy: &PythonPackagingPolicy,
    path: &Path,
) -> Result<Vec<PythonResource<'a>>> {
    let python_paths = resolve_python_paths(path, &dist.python_major_minor_version());

    find_resources(dist, policy, &python_paths.site_packages, None)
}

/// Run `setup.py install` against a path and return found resources.
#[allow(clippy::too_many_arguments)]
pub fn setup_py_install<'a, S: BuildHasher>(
    env: &Environment,
    dist: &dyn PythonDistribution,
    policy: &PythonPackagingPolicy,
    libpython_link_mode: LibpythonLinkMode,
    package_path: &Path,
    verbose: bool,
    extra_envs: &HashMap<String, String, S>,
    extra_global_arguments: &[String],
) -> Result<Vec<PythonResource<'a>>> {
    if !package_path.is_absolute() {
        return Err(anyhow!(
            "package_path must be absolute: got {:?}",
            package_path.display()
        ));
    }

    let temp_dir = env.temporary_directory("pyoxidizer-setup-py-install")?;

    let target_dir_path = temp_dir.path().join("install");
    let target_dir_s = target_dir_path.display().to_string();

    let python_paths = resolve_python_paths(&target_dir_path, &dist.python_major_minor_version());

    std::fs::create_dir_all(&python_paths.site_packages)?;

    let mut envs: HashMap<String, String, RandomState> = std::env::vars().collect();
    for (k, v) in dist.resolve_distutils(
        libpython_link_mode,
        temp_dir.path(),
        &[&python_paths.site_packages, &python_paths.stdlib],
    )? {
        envs.insert(k, v);
    }

    for (key, value) in extra_envs {
        envs.insert(key.clone(), value.clone());
    }

    warn!(
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

    args.extend(["install", "--prefix", &target_dir_s, "--no-compile"]);

    let command = cmd(dist.python_exe_path(), &args)
        .dir(package_path)
        .full_env(&envs)
        .stderr_to_stdout()
        .unchecked()
        .reader()?;

    log_command_output(&command);

    let output = command
        .try_wait()?
        .ok_or_else(|| anyhow!("unable to wait on command"))?;
    if !output.status.success() {
        return Err(anyhow!("error running pip"));
    }

    let state_dir = envs
        .get("PYOXIDIZER_DISTUTILS_STATE_DIR")
        .map(PathBuf::from);
    warn!(
        "scanning {} for resources",
        python_paths.site_packages.display()
    );
    let resources = find_resources(dist, policy, &python_paths.site_packages, state_dir)
        .context("scanning for resources")?;

    temp_dir.close().context("closing temporary directory")?;

    Ok(resources)
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::testutil::*,
        std::{collections::BTreeSet, ops::Deref},
    };

    #[test]
    fn test_install_black() -> Result<()> {
        let env = get_env()?;
        let distribution = get_default_distribution(None)?;

        let resources: Vec<PythonResource> = pip_install(
            &env,
            distribution.deref(),
            &distribution.create_packaging_policy()?,
            LibpythonLinkMode::Dynamic,
            false,
            &["black==19.10b0".to_string()],
            &HashMap::new(),
        )?;

        assert!(resources.iter().any(|r| r.full_name() == "appdirs"));
        assert!(resources.iter().any(|r| r.full_name() == "black"));

        Ok(())
    }

    #[test]
    #[cfg(windows)]
    fn test_install_cffi() -> Result<()> {
        let env = get_env()?;
        let distribution = get_default_dynamic_distribution()?;
        let policy = distribution.create_packaging_policy()?;

        let resources: Vec<PythonResource> = pip_install(
            &env,
            distribution.deref(),
            &policy,
            LibpythonLinkMode::Dynamic,
            false,
            &["cffi==1.15.0".to_string()],
            &HashMap::new(),
        )?;

        let ems = resources
            .iter()
            .filter(|r| matches!(r, PythonResource::ExtensionModule { .. }))
            .collect::<Vec<&PythonResource>>();

        assert_eq!(ems.len(), 1);
        assert_eq!(ems[0].full_name(), "_cffi_backend");

        Ok(())
    }

    #[test]
    fn test_pip_download_zstandard() -> Result<()> {
        let env = get_env()?;

        for target_dist in get_all_standalone_distributions()? {
            if target_dist.python_platform_compatibility_tag() == "none" {
                continue;
            }

            let host_dist = get_host_distribution_from_target(&target_dist)?;

            warn!(
                "using distribution {}-{}-{}",
                target_dist.python_implementation,
                target_dist.python_platform_tag,
                target_dist.version
            );

            let policy = target_dist.create_packaging_policy()?;

            let resources = pip_download(
                &env,
                &*host_dist,
                &*target_dist,
                &policy,
                false,
                &["zstandard==0.16.0".to_string()],
            )?;

            assert!(!resources.is_empty());
            let zstandard_resources = resources
                .iter()
                .filter(|r| r.is_in_packages(&["zstandard".to_string()]))
                .collect::<Vec<_>>();
            assert!(!zstandard_resources.is_empty());

            let full_names = zstandard_resources
                .iter()
                .map(|r| r.full_name())
                .collect::<BTreeSet<_>>();

            let mut expected_names = [
                "zstandard",
                "zstandard.__init__.pyi",
                "zstandard.backend_c",
                "zstandard.backend_cffi",
                "zstandard.py.typed",
                "zstandard:LICENSE",
                "zstandard:METADATA",
                "zstandard:RECORD",
                "zstandard:WHEEL",
                "zstandard:top_level.txt",
            ]
            .iter()
            .map(|x| x.to_string())
            .collect::<BTreeSet<String>>();

            let mut expected_extensions_count = 1;
            let mut expected_first_extension_name = "zstandard.backend_c";

            if matches!(
                target_dist.target_triple.as_str(),
                "i686-pc-windows-msvc" | "x86_64-pc-windows-msvc"
            ) {
                expected_names.insert("zstandard._cffi".to_string());
                expected_extensions_count = 2;
                expected_first_extension_name = "zstandard._cffi";
            }

            assert_eq!(
                full_names, expected_names,
                "target triple: {}",
                target_dist.target_triple
            );

            let extensions = zstandard_resources
                .iter()
                .filter_map(|r| match r {
                    PythonResource::ExtensionModule(em) => Some(em),
                    _ => None,
                })
                .collect::<Vec<_>>();

            assert_eq!(
                extensions.len(),
                expected_extensions_count,
                "target triple: {}",
                target_dist.target_triple
            );
            let em = extensions[0];
            assert_eq!(em.name, expected_first_extension_name);
            assert!(em.shared_library.is_some());
        }

        Ok(())
    }

    #[test]
    fn test_pip_download_numpy() -> Result<()> {
        let env = get_env()?;

        for target_dist in get_all_standalone_distributions()? {
            if target_dist.python_platform_compatibility_tag() == "none" {
                continue;
            }

            let host_dist = get_host_distribution_from_target(&target_dist)?;

            warn!(
                "using distribution {}-{}-{}",
                target_dist.python_implementation,
                target_dist.python_platform_tag,
                target_dist.version
            );

            let mut policy = target_dist.create_packaging_policy()?;
            policy.set_file_scanner_emit_files(true);
            policy.set_file_scanner_classify_files(true);

            let res = pip_download(
                &env,
                &*host_dist,
                &*target_dist,
                &policy,
                false,
                &["numpy==1.22.1".to_string()],
            );

            // numpy wheel not available for 3.10 win32.
            if target_dist.python_major_minor_version() == "3.10"
                && target_dist.python_platform_tag() == "win32"
            {
                assert!(res.is_err());
                continue;
            }

            let resources = res?;

            assert!(!resources.is_empty());

            let extensions = resources
                .iter()
                .filter_map(|r| match r {
                    PythonResource::ExtensionModule(em) => Some(em),
                    _ => None,
                })
                .collect::<Vec<_>>();

            assert!(!extensions.is_empty());

            assert!(extensions
                .iter()
                .any(|em| em.name == "numpy.random._common"));
        }

        Ok(())
    }
}
