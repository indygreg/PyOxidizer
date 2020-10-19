// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Interaction with Python packaging tools (pip, setuptools, etc).
*/

use {
    super::{
        binary::LibpythonLinkMode,
        distribution::{download_distribution, PythonDistribution},
        distutils::read_built_extensions,
        standalone_distribution::resolve_python_paths,
    },
    crate::python_distributions::GET_PIP_PY_19,
    anyhow::{anyhow, Context, Result},
    duct::cmd,
    python_packaging::{
        filesystem_scanning::find_python_resources, policy::PythonPackagingPolicy,
        resource::PythonResource, wheel::WheelArchive,
    },
    slog::warn,
    std::{
        collections::{hash_map::RandomState, HashMap},
        hash::BuildHasher,
        io::{BufRead, BufReader},
        iter::FromIterator,
        path::{Path, PathBuf},
    },
};

/// Pip requirements file for bootstrapping packaging tools.
pub const PIP_BOOTSTRAP_REQUIREMENTS: &str = indoc::indoc!(
    "wheel==0.34.2 \\
        --hash=sha256:8788e9155fe14f54164c1b9eb0a319d98ef02c160725587ad60f14ddc57b6f96 \\
        --hash=sha256:df277cb51e61359aba502208d680f90c0493adec6f0e848af94948778aed386e
    pip==20.0.2 \\
        --hash=sha256:4ae14a42d8adba3205ebeb38aa68cfc0b6c346e1ae2e699a0b3bad4da19cef5c \\\
         --hash=sha256:7db0c8ea4c7ea51c8049640e8e6e7fde949de672bfa4949920675563a5a6967f
    setuptools==45.1.0 \\
        --hash=sha256:68e7fd3508687f94367f1aa090a3ed921cd045a60b73d8b0aa1f305199a0ca28 \\
        --hash=sha256:91f72d83602a6e5e4a9e4fe296e27185854038d7cbda49dcd7006c4d3b3b89d5"
);

/// Bootstrap Python packaging tools given a Python executable.
///
/// Bootstrapping packaging tools in a secure and deterministic manner is
/// quite difficult in practice! That's because `get-pip.py` doesn't
/// work deterministically by default. See
/// https://github.com/pypa/get-pip/issues/60.
///
/// Our solution to this is to download `get-pip.py` and then hack its source
/// code to allow use of a requirements file for installing all dependencies.
///
/// We can't just run a vanilla `get-pip.py` with a requirements file because
/// `get-pip` will internally always run the equivalent of
/// `pip install --upgrade pip`. You can control the value of `pip` here. But
/// if you define a `pip` entry in a requirements file (which is necessary to
/// make the operation secure and deterministic), pip complains because the
/// `pip` from the command line argument doesn't have a hash!
///
/// We also tried to install `setuptools`, `pip`, etc direct from their
/// source distributions. However we couldn't get this to work either!
/// Modern versions of setuptools can't self-bootstrap: setuptools depends
/// on setuptools. The ancient way of bootstrapping setuptools was to use
/// `ez_setup.py`. But this script (again) doesn't pin content hashes or
/// versions, and isn't secure nor deterministic. We tried to download
/// an old version of setuptools that didn't require itself to install. But
/// we couldn't get this working either, possibly due to incompatibilities
/// with modern Python versions.
///
/// Since modern versions of `get-pip.py` just work in their default
/// non-deterministic mode, hacking `get-pip.py` to do what we want was
/// the path of least resistance.
#[allow(unused)]
pub fn bootstrap_packaging_tools(
    logger: &slog::Logger,
    python_exe: &Path,
    cache_dir: &Path,
    bin_dir: &Path,
    lib_dir: &Path,
) -> Result<()> {
    let get_pip_py_path =
        download_distribution(&GET_PIP_PY_19.url, &GET_PIP_PY_19.sha256, cache_dir)?;

    let temp_dir = tempdir::TempDir::new("pyoxidizer-bootstrap-packaging")?;

    // We need to hack `get-pip.py`'s source code to allow exclusive use of a
    // requirements file for installing `pip`. The `implicit_*` variables control
    // the packages installed via command line arguments. We force their value
    // to false so all packages come from the requirements file.
    let get_pip_py_data = std::fs::read_to_string(&get_pip_py_path)?;
    let get_pip_py_data = get_pip_py_data
        .replace("implicit_pip = True", "implicit_pip = False")
        .replace("implicit_setuptools = True", "implicit_setuptools = False")
        .replace("implicit_wheel = True", "implicit_wheel = False");

    let get_pip_py_path = temp_dir.path().join("get-pip.py");
    std::fs::write(&get_pip_py_path, get_pip_py_data)?;

    let bootstrap_txt_path = temp_dir.path().join("pip-bootstrap.txt");
    std::fs::write(&bootstrap_txt_path, PIP_BOOTSTRAP_REQUIREMENTS)?;

    // We install to a temp directory then copy files into the target directory.
    // We do this because we may not want to modify the source Python distribution
    // and the default install layout may not be appropriate.
    let install_dir = temp_dir.path().join("pip-installed");

    warn!(logger, "running get-pip.py to bootstrap pip");
    let command = cmd(
        python_exe,
        vec![
            format!("{}", get_pip_py_path.display()),
            "--require-hashes".to_string(),
            "-r".to_string(),
            format!("{}", bootstrap_txt_path.display()),
            "--prefix".to_string(),
            format!("{}", install_dir.display()),
        ],
    )
    .dir(temp_dir.path())
    .stderr_to_stdout()
    .reader()?;
    {
        let reader = BufReader::new(&command);
        for line in reader.lines() {
            warn!(logger, "{}", line?);
        }
    }

    let output = command
        .try_wait()?
        .ok_or_else(|| anyhow!("unable to wait on command"))?;
    if !output.status.success() {
        return Err(anyhow!("error installing pip"));
    }

    // TODO support non-Windows install layouts.
    let source_bin_dir = install_dir.join("Scripts");
    let source_lib_dir = install_dir.join("Lib").join("site-packages");

    for entry in walkdir::WalkDir::new(&source_bin_dir) {
        let entry = entry?;

        if entry.file_type().is_dir() {
            continue;
        }

        let rel = entry.path().strip_prefix(&source_bin_dir)?;

        let dest_path = bin_dir.join(rel);
        let parent_dir = dest_path
            .parent()
            .ok_or_else(|| anyhow!("unable to determine parent directory"))?;
        std::fs::create_dir_all(parent_dir)?;
        std::fs::copy(entry.path(), &dest_path).context("copying bin file")?;
    }

    for entry in walkdir::WalkDir::new(&source_lib_dir) {
        let entry = entry?;

        if entry.file_type().is_dir() {
            continue;
        }

        let rel = entry.path().strip_prefix(&source_lib_dir)?;

        let dest_path = lib_dir.join(rel);
        let parent_dir = dest_path
            .parent()
            .ok_or_else(|| anyhow!("unable to determine parent directory"))?;
        std::fs::create_dir_all(parent_dir)?;
        std::fs::copy(entry.path(), &dest_path).context("copying lib file")?;
    }

    Ok(())
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
        HashMap::from_iter(
            read_built_extensions(&p)?
                .iter()
                .map(|ext| (ext.name.clone(), ext.clone())),
        )
    } else {
        HashMap::new()
    };

    for r in find_python_resources(
        &path,
        dist.cache_tag(),
        &dist.python_module_suffixes()?,
        policy.file_scanner_emit_files(),
        policy.file_scanner_classify_files(),
    ) {
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
    logger: &slog::Logger,
    host_dist: &dyn PythonDistribution,
    taget_dist: &dyn PythonDistribution,
    policy: &PythonPackagingPolicy,
    verbose: bool,
    args: &[String],
) -> Result<Vec<PythonResource<'a>>> {
    let temp_dir = tempdir::TempDir::new("pyoxidizer-pip-download")?;

    host_dist.ensure_pip(logger)?;

    let target_dir = temp_dir.path();

    warn!(logger, "pip downloading to {}", target_dir.display());

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

    warn!(logger, "running python {:?}", pip_args);

    let command = cmd(host_dist.python_exe_path(), &pip_args)
        .stderr_to_stdout()
        .reader()?;

    {
        let reader = BufReader::new(&command);
        for line in reader.lines() {
            warn!(logger, "{}", line?);
        }
    }

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

    Ok(res)
}

/// Run `pip install` and return found resources.
pub fn pip_install<'a, S: BuildHasher>(
    logger: &slog::Logger,
    dist: &dyn PythonDistribution,
    policy: &PythonPackagingPolicy,
    libpython_link_mode: LibpythonLinkMode,
    verbose: bool,
    install_args: &[String],
    extra_envs: &HashMap<String, String, S>,
) -> Result<Vec<PythonResource<'a>>> {
    let temp_dir = tempdir::TempDir::new("pyoxidizer-pip-install")?;

    dist.ensure_pip(logger)?;

    let mut env: HashMap<String, String, RandomState> = HashMap::from_iter(std::env::vars());
    for (k, v) in dist.resolve_distutils(logger, libpython_link_mode, temp_dir.path(), &[])? {
        env.insert(k, v);
    }

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

    let command = cmd(dist.python_exe_path(), &pip_args)
        .full_env(&env)
        .stderr_to_stdout()
        .reader()?;
    {
        let reader = BufReader::new(&command);
        for line in reader.lines() {
            warn!(logger, "{}", line?);
        }
    }

    let output = command
        .try_wait()?
        .ok_or_else(|| anyhow!("unable to wait on command"))?;
    if !output.status.success() {
        return Err(anyhow!("error running pip"));
    }

    let state_dir = match env.get("PYOXIDIZER_DISTUTILS_STATE_DIR") {
        Some(p) => Some(PathBuf::from(p)),
        None => None,
    };

    find_resources(dist, policy, &target_dir, state_dir)
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
pub fn setup_py_install<'a, S: BuildHasher>(
    logger: &slog::Logger,
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

    let temp_dir = tempdir::TempDir::new("pyoxidizer-setup-py-install")?;

    let target_dir_path = temp_dir.path().join("install");
    let target_dir_s = target_dir_path.display().to_string();

    let python_paths = resolve_python_paths(&target_dir_path, &dist.python_major_minor_version());

    std::fs::create_dir_all(&python_paths.site_packages)?;

    let mut envs: HashMap<String, String, RandomState> = HashMap::from_iter(std::env::vars());
    for (k, v) in dist.resolve_distutils(
        &logger,
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

    let command = cmd(dist.python_exe_path(), &args)
        .dir(package_path)
        .full_env(&envs)
        .stderr_to_stdout()
        .reader()?;
    {
        let reader = BufReader::new(&command);
        for line in reader.lines() {
            warn!(logger, "{}", line.unwrap());
        }
    }

    let output = command
        .try_wait()?
        .ok_or_else(|| anyhow!("unable to wait on command"))?;
    if !output.status.success() {
        return Err(anyhow!("error running pip"));
    }

    let state_dir = match envs.get("PYOXIDIZER_DISTUTILS_STATE_DIR") {
        Some(p) => Some(PathBuf::from(p)),
        None => None,
    };
    warn!(
        logger,
        "scanning {} for resources",
        python_paths.site_packages.display()
    );
    find_resources(dist, policy, &python_paths.site_packages, state_dir)
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::testutil::*,
        std::{collections::BTreeSet, iter::FromIterator, ops::Deref},
    };

    #[test]
    fn test_install_black() -> Result<()> {
        let logger = get_logger()?;
        let distribution = get_default_distribution()?;

        let resources: Vec<PythonResource> = pip_install(
            &logger,
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
        let logger = get_logger()?;

        let distribution = get_default_dynamic_distribution()?;
        let policy = distribution.create_packaging_policy()?;

        let resources: Vec<PythonResource> = pip_install(
            &logger,
            distribution.deref(),
            &policy,
            LibpythonLinkMode::Dynamic,
            false,
            &["cffi==1.14.0".to_string()],
            &HashMap::new(),
        )?;

        let ems = resources
            .iter()
            .filter(|r| match r {
                PythonResource::ExtensionModule { .. } => true,
                _ => false,
            })
            .collect::<Vec<&PythonResource>>();

        assert_eq!(ems.len(), 1);
        assert_eq!(ems[0].full_name(), "_cffi_backend");

        Ok(())
    }

    #[test]
    fn test_pip_download_zstandard() -> Result<()> {
        let logger = get_logger()?;

        let host_dist = get_default_distribution()?;

        for target_dist in get_all_standalone_distributions()? {
            if target_dist.python_platform_compatibility_tag() == "none" {
                continue;
            }

            // TODO enable once Python 3.9 wheel is published.
            if target_dist.python_major_minor_version() == "3.9" {
                continue;
            }

            warn!(
                logger,
                "using distribution {}-{}-{}",
                target_dist.python_implementation,
                target_dist.python_platform_tag,
                target_dist.version
            );

            let policy = target_dist.create_packaging_policy()?;

            let resources = pip_download(
                &logger,
                &*host_dist,
                &*target_dist,
                &policy,
                false,
                &["zstandard==0.14.0".to_string()],
            )?;

            assert!(!resources.is_empty());
            let zstandard_resources = resources
                .iter()
                .filter(|r| r.is_in_packages(&["zstandard".to_string(), "zstd".to_string()]))
                .collect::<Vec<_>>();
            assert!(!zstandard_resources.is_empty());

            let full_names = BTreeSet::from_iter(zstandard_resources.iter().map(|r| r.full_name()));

            assert_eq!(
                full_names,
                BTreeSet::from_iter(
                    [
                        "zstd",
                        "zstandard",
                        "zstandard.cffi",
                        "zstandard:LICENSE",
                        "zstandard:top_level.txt",
                        "zstandard:WHEEL",
                        "zstandard:RECORD",
                        "zstandard:METADATA",
                    ]
                    .iter()
                    .map(|x| x.to_string())
                )
            );

            let extensions = zstandard_resources
                .iter()
                .filter_map(|r| match r {
                    PythonResource::ExtensionModule(em) => Some(em),
                    _ => None,
                })
                .collect::<Vec<_>>();

            assert_eq!(extensions.len(), 1);
            let em = extensions[0];
            assert_eq!(em.name, "zstd");
            assert!(em.shared_library.is_some());
        }

        Ok(())
    }

    #[test]
    fn test_pip_download_numpy() -> Result<()> {
        let logger = get_logger()?;

        let host_dist = get_default_distribution()?;

        for target_dist in get_all_standalone_distributions()? {
            if target_dist.python_platform_compatibility_tag() == "none" {
                continue;
            }

            // TODO support once a Python 3.9 wheel is published.
            if target_dist.python_major_minor_version() == "3.9" {
                continue;
            }

            warn!(
                logger,
                "using distribution {}-{}-{}",
                target_dist.python_implementation,
                target_dist.python_platform_tag,
                target_dist.version
            );

            let mut policy = target_dist.create_packaging_policy()?;
            policy.set_file_scanner_emit_files(true);
            policy.set_file_scanner_classify_files(true);

            let resources = pip_download(
                &logger,
                &*host_dist,
                &*target_dist,
                &policy,
                false,
                &["numpy==1.19.2".to_string()],
            )?;

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
