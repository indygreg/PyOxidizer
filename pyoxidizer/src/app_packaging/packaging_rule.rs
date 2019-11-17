// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use slog::{info, warn};
use std::collections::BTreeSet;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use super::config::{
    InstallLocation, PackagingPackageRoot, PackagingPipInstallSimple, PackagingPipRequirementsFile,
    PackagingSetupPyInstall, PackagingStdlib, PackagingStdlibExtensionVariant,
    PackagingStdlibExtensionsExplicitExcludes, PackagingStdlibExtensionsExplicitIncludes,
    PackagingStdlibExtensionsPolicy, PackagingVirtualenv, PythonPackaging,
};
use super::state::BuildContext;
use crate::py_packaging::distribution::{is_stdlib_test_package, PythonDistributionInfo};
use crate::py_packaging::distutils::{prepare_hacked_distutils, read_built_extensions};
use crate::py_packaging::fsscan::{find_python_resources, PythonFileResource};
use crate::py_packaging::resource::{AppRelativeResources, PythonResource};

#[derive(Debug)]
pub enum ResourceAction {
    Add,
    Remove,
}

/// Represents the packaging location for a resource.
#[derive(Clone, Debug)]
pub enum ResourceLocation {
    /// Embed the resource in the binary.
    Embedded,

    /// Install the resource in a path relative to the produced binary.
    AppRelative { path: String },
}

impl ResourceLocation {
    pub fn new(v: &InstallLocation) -> Self {
        match v {
            InstallLocation::Embedded => ResourceLocation::Embedded,
            InstallLocation::AppRelative { path } => {
                ResourceLocation::AppRelative { path: path.clone() }
            }
        }
    }
}

#[derive(Debug)]
pub struct PythonResourceAction {
    pub action: ResourceAction,
    pub location: ResourceLocation,
    pub resource: PythonResource,
}

impl AppRelativeResources {
    pub fn package_names(&self) -> BTreeSet<String> {
        let mut packages = packages_from_module_names(self.module_sources.keys().cloned());
        packages.extend(packages_from_module_names(
            self.module_bytecodes.keys().cloned(),
        ));

        packages
    }
}

pub fn packages_from_module_name(module: &str) -> BTreeSet<String> {
    let mut package_names = BTreeSet::new();

    let mut search: &str = &module;

    while let Some(idx) = search.rfind('.') {
        package_names.insert(search[0..idx].to_string());
        search = &search[0..idx];
    }

    package_names
}

pub fn packages_from_module_names<I>(names: I) -> BTreeSet<String>
where
    I: Iterator<Item = String>,
{
    let mut package_names = BTreeSet::new();

    for name in names {
        let mut search: &str = &name;

        while let Some(idx) = search.rfind('.') {
            package_names.insert(search[0..idx].to_string());
            search = &search[0..idx];
        }
    }

    package_names
}

fn is_package_from_path(path: &Path) -> bool {
    let file_name = path.file_name().unwrap().to_str().unwrap();
    file_name.starts_with("__init__.")
}

fn resource_full_name(resource: &PythonFileResource) -> &str {
    match resource {
        PythonFileResource::Source { full_name, .. } => &full_name,
        PythonFileResource::Bytecode { full_name, .. } => &full_name,
        PythonFileResource::BytecodeOpt1 { full_name, .. } => &full_name,
        PythonFileResource::BytecodeOpt2 { full_name, .. } => &full_name,
        PythonFileResource::Resource(resource) => &resource.full_name,
        _ => "",
    }
}

struct PythonPaths {
    main: PathBuf,
    site_packages: PathBuf,
}

/// Resolve the location of Python modules given a base install path.
fn resolve_python_paths(base: &Path, python_version: &str, is_windows: bool) -> PythonPaths {
    let mut p = base.to_path_buf();

    if is_windows {
        p.push("Lib");
    } else {
        p.push("lib");
        p.push(format!("python{}", &python_version[0..3]));
    }

    let site_packages = p.join("site-packages");

    PythonPaths {
        main: p,
        site_packages,
    }
}

fn resolve_built_extensions(
    state_dir: &Path,
    res: &mut Vec<PythonResourceAction>,
    location: &ResourceLocation,
) -> Result<(), String> {
    for ext in read_built_extensions(state_dir)? {
        res.push(PythonResourceAction {
            action: ResourceAction::Add,
            location: location.clone(),
            resource: PythonResource::BuiltExtensionModule(ext),
        });
    }

    Ok(())
}

fn resolve_stdlib_extensions_policy(
    logger: &slog::Logger,
    dist: &PythonDistributionInfo,
    rule: &PackagingStdlibExtensionsPolicy,
) -> Vec<PythonResourceAction> {
    let mut res = Vec::new();

    for ext in dist.filter_extension_modules(logger, &rule.filter) {
        res.push(PythonResourceAction {
            action: ResourceAction::Add,
            location: ResourceLocation::Embedded,
            resource: ext,
        });
    }

    res
}

fn resolve_stdlib_extensions_explicit_includes(
    dist: &PythonDistributionInfo,
    rule: &PackagingStdlibExtensionsExplicitIncludes,
) -> Vec<PythonResourceAction> {
    let mut res = Vec::new();

    for name in &rule.includes {
        if let Some(modules) = &dist.extension_modules.get(name) {
            res.push(PythonResourceAction {
                action: ResourceAction::Add,
                location: ResourceLocation::Embedded,
                resource: PythonResource::ExtensionModule {
                    name: name.clone(),
                    module: modules[0].clone(),
                },
            });
        }
    }

    res
}

fn resolve_stdlib_extensions_explicit_excludes(
    dist: &PythonDistributionInfo,
    rule: &PackagingStdlibExtensionsExplicitExcludes,
) -> Vec<PythonResourceAction> {
    let mut res = Vec::new();

    for (name, modules) in &dist.extension_modules {
        if rule.excludes.contains(name) {
            res.push(PythonResourceAction {
                action: ResourceAction::Remove,
                location: ResourceLocation::Embedded,
                resource: PythonResource::ExtensionModule {
                    name: name.clone(),
                    module: modules[0].clone(),
                },
            });
        } else {
            res.push(PythonResourceAction {
                action: ResourceAction::Add,
                location: ResourceLocation::Embedded,
                resource: PythonResource::ExtensionModule {
                    name: name.clone(),
                    module: modules[0].clone(),
                },
            });
        }
    }

    res
}

fn resolve_stdlib_extension_variant(
    dist: &PythonDistributionInfo,
    rule: &PackagingStdlibExtensionVariant,
) -> Vec<PythonResourceAction> {
    let mut res = Vec::new();

    let variants = &dist.extension_modules[&rule.extension];

    for em in variants {
        if em.variant == rule.variant {
            res.push(PythonResourceAction {
                action: ResourceAction::Add,
                location: ResourceLocation::Embedded,
                resource: PythonResource::ExtensionModule {
                    name: rule.extension.clone(),
                    module: em.clone(),
                },
            });
        }
    }

    if res.is_empty() {
        panic!(
            "extension {} has no variant {}",
            rule.extension, rule.variant
        );
    }

    res
}

fn resolve_stdlib(
    logger: &slog::Logger,
    dist: &PythonDistributionInfo,
    rule: &PackagingStdlib,
) -> Vec<PythonResourceAction> {
    let mut res = Vec::new();

    let location = ResourceLocation::new(&rule.install_location);

    for (name, fs_path) in &dist.py_modules {
        if is_stdlib_test_package(&name) && rule.exclude_test_modules {
            info!(logger, "skipping test stdlib module: {}", name);
            continue;
        }

        let mut relevant = true;

        for exclude in &rule.excludes {
            let prefix = exclude.clone() + ".";

            if name == exclude || name.starts_with(&prefix) {
                relevant = false;
            }
        }

        if !relevant {
            continue;
        }

        let is_package = is_package_from_path(&fs_path);
        let source = fs::read(fs_path).expect("error reading source file");

        if rule.include_source {
            res.push(PythonResourceAction {
                action: ResourceAction::Add,
                location: location.clone(),
                resource: PythonResource::ModuleSource {
                    name: name.clone(),
                    source: source.clone(),
                    is_package,
                },
            });
        }

        res.push(PythonResourceAction {
            action: ResourceAction::Add,
            location: location.clone(),
            resource: PythonResource::ModuleBytecode {
                name: name.clone(),
                source,
                optimize_level: rule.optimize_level as i32,
                is_package,
            },
        });
    }

    if rule.include_resources {
        for (package, resources) in &dist.resources {
            if is_stdlib_test_package(package) && rule.exclude_test_modules {
                info!(
                    logger,
                    "skipping resources associated with test package: {}", package
                );
                continue;
            }

            for (name, fs_path) in resources {
                let data = fs::read(fs_path).expect("error reading resource file");

                res.push(PythonResourceAction {
                    action: ResourceAction::Add,
                    location: location.clone(),
                    resource: PythonResource::Resource {
                        package: package.clone(),
                        name: name.clone(),
                        data,
                    },
                });
            }
        }
    }

    res
}

fn resolve_virtualenv(
    dist: &PythonDistributionInfo,
    rule: &PackagingVirtualenv,
) -> Vec<PythonResourceAction> {
    let mut res = Vec::new();

    let location = ResourceLocation::new(&rule.install_location);

    let python_paths =
        resolve_python_paths(&Path::new(&rule.path), &dist.version, dist.os == "windows");
    let packages_path = python_paths.site_packages;

    for resource in find_python_resources(&packages_path) {
        let mut relevant = true;
        let full_name = resource_full_name(&resource);

        for exclude in &rule.excludes {
            let prefix = exclude.clone() + ".";

            if full_name == exclude || full_name.starts_with(&prefix) {
                relevant = false;
            }
        }

        if !relevant {
            continue;
        }

        match resource {
            PythonFileResource::Source {
                full_name, path, ..
            } => {
                let is_package = is_package_from_path(&path);
                let source = fs::read(path).expect("error reading source file");

                if rule.include_source {
                    res.push(PythonResourceAction {
                        action: ResourceAction::Add,
                        location: location.clone(),
                        resource: PythonResource::ModuleSource {
                            name: full_name.clone(),
                            source: source.clone(),
                            is_package,
                        },
                    });
                }

                res.push(PythonResourceAction {
                    action: ResourceAction::Add,
                    location: location.clone(),
                    resource: PythonResource::ModuleBytecode {
                        name: full_name.clone(),
                        source,
                        optimize_level: rule.optimize_level as i32,
                        is_package,
                    },
                });
            }

            PythonFileResource::Resource(resource) => {
                let data = fs::read(resource.path).expect("error reading resource file");

                res.push(PythonResourceAction {
                    action: ResourceAction::Add,
                    location: location.clone(),
                    resource: PythonResource::Resource {
                        package: resource.package.clone(),
                        name: resource.stem.clone(),
                        data,
                    },
                });
            }

            _ => {}
        }
    }

    res
}

fn resolve_package_root(rule: &PackagingPackageRoot) -> Vec<PythonResourceAction> {
    let mut res = Vec::new();

    let location = ResourceLocation::new(&rule.install_location);
    let path = PathBuf::from(&rule.path);

    for resource in find_python_resources(&path) {
        let mut relevant = false;
        let full_name = resource_full_name(&resource);

        for package in &rule.packages {
            let prefix = package.clone() + ".";

            if full_name == package || full_name.starts_with(&prefix) {
                relevant = true;
            }
        }

        for exclude in &rule.excludes {
            let prefix = exclude.clone() + ".";

            if full_name == exclude || full_name.starts_with(&prefix) {
                relevant = false;
            }
        }

        if !relevant {
            continue;
        }

        match resource {
            PythonFileResource::Source {
                full_name, path, ..
            } => {
                let is_package = is_package_from_path(&path);
                let source = fs::read(path).expect("error reading source file");

                if rule.include_source {
                    res.push(PythonResourceAction {
                        action: ResourceAction::Add,
                        location: location.clone(),
                        resource: PythonResource::ModuleSource {
                            name: full_name.clone(),
                            source: source.clone(),
                            is_package,
                        },
                    });
                }

                res.push(PythonResourceAction {
                    action: ResourceAction::Add,
                    location: location.clone(),
                    resource: PythonResource::ModuleBytecode {
                        name: full_name.clone(),
                        source,
                        optimize_level: rule.optimize_level as i32,
                        is_package,
                    },
                });
            }

            PythonFileResource::Resource(resource) => {
                let data = fs::read(resource.path).expect("error reading resource file");

                res.push(PythonResourceAction {
                    action: ResourceAction::Add,
                    location: location.clone(),
                    resource: PythonResource::Resource {
                        package: resource.package.clone(),
                        name: resource.stem.clone(),
                        data,
                    },
                });
            }

            _ => {}
        }
    }

    res
}

fn resolve_pip_install_simple(
    logger: &slog::Logger,
    dist: &PythonDistributionInfo,
    rule: &PackagingPipInstallSimple,
    verbose: bool,
) -> Vec<PythonResourceAction> {
    let mut res = Vec::new();

    let location = ResourceLocation::new(&rule.install_location);

    dist.ensure_pip();
    let temp_dir =
        tempdir::TempDir::new("pyoxidizer-pip-install").expect("could not creat temp directory");

    let mut extra_envs = prepare_hacked_distutils(logger, dist, temp_dir.path(), &[])
        .expect("unable to hack distutils");

    let target_dir_path = temp_dir.path().join("install");
    let target_dir_s = target_dir_path.display().to_string();
    warn!(logger, "pip installing to {}", target_dir_s);

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
        target_dir_s,
        "--no-binary".to_string(),
        ":all:".to_string(),
        rule.package.clone(),
    ]);

    if rule.extra_args.is_some() {
        pip_args.extend(rule.extra_args.clone().unwrap());
    }

    for (key, value) in rule.extra_env.iter() {
        extra_envs.insert(key.clone(), value.clone());
    }

    // TODO send stderr to stdout.
    let mut cmd = std::process::Command::new(&dist.python_exe)
        .args(&pip_args)
        .envs(&extra_envs)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("error running pip");
    {
        let stdout = cmd.stdout.as_mut().unwrap();
        let reader = BufReader::new(stdout);

        for line in reader.lines() {
            warn!(logger, "{}", line.unwrap());
        }
    }

    let status = cmd.wait().unwrap();
    if !status.success() {
        panic!("error running pip");
    }

    for resource in find_python_resources(&target_dir_path) {
        let mut relevant = true;
        let full_name = resource_full_name(&resource);

        for exclude in &rule.excludes {
            let prefix = exclude.clone() + ".";

            if full_name == exclude || full_name.starts_with(&prefix) {
                relevant = false;
            }
        }

        if !relevant {
            continue;
        }

        match resource {
            PythonFileResource::Source {
                full_name, path, ..
            } => {
                let is_package = is_package_from_path(&path);
                let source = fs::read(path).expect("error reading source file");

                if rule.include_source {
                    res.push(PythonResourceAction {
                        action: ResourceAction::Add,
                        location: location.clone(),
                        resource: PythonResource::ModuleSource {
                            name: full_name.clone(),
                            source: source.clone(),
                            is_package,
                        },
                    });
                }

                res.push(PythonResourceAction {
                    action: ResourceAction::Add,
                    location: location.clone(),
                    resource: PythonResource::ModuleBytecode {
                        name: full_name.clone(),
                        source,
                        optimize_level: rule.optimize_level as i32,
                        is_package,
                    },
                });
            }

            PythonFileResource::Resource(resource) => {
                let data = fs::read(resource.path).expect("error reading resource file");

                res.push(PythonResourceAction {
                    action: ResourceAction::Add,
                    location: location.clone(),
                    resource: PythonResource::Resource {
                        package: resource.package.clone(),
                        name: resource.stem.clone(),
                        data,
                    },
                });
            }

            _ => {}
        }
    }

    resolve_built_extensions(
        &PathBuf::from(extra_envs.get("PYOXIDIZER_DISTUTILS_STATE_DIR").unwrap()),
        &mut res,
        &location,
    )
    .unwrap();

    res
}

fn resolve_pip_requirements_file(
    logger: &slog::Logger,
    dist: &PythonDistributionInfo,
    rule: &PackagingPipRequirementsFile,
    verbose: bool,
) -> Vec<PythonResourceAction> {
    let mut res = Vec::new();

    let location = ResourceLocation::new(&rule.install_location);

    dist.ensure_pip();

    let temp_dir =
        tempdir::TempDir::new("pyoxidizer-pip-install").expect("could not create temp directory");

    let mut extra_envs = prepare_hacked_distutils(logger, dist, temp_dir.path(), &[])
        .expect("unable to hack distutils");

    let target_dir_path = temp_dir.path().join("install");
    let target_dir_s = target_dir_path.display().to_string();
    warn!(logger, "pip installing to {}", target_dir_s);

    let mut args: Vec<String> = vec![
        "-m".to_string(),
        "pip".to_string(),
        "--disable-pip-version-check".to_string(),
    ];

    if verbose {
        args.push("--verbose".to_string());
    }

    args.extend(vec![
        "install".to_string(),
        "--target".to_string(),
        target_dir_s,
        "--no-binary".to_string(),
        ":all:".to_string(),
        "--requirement".to_string(),
        rule.requirements_path.to_string(),
    ]);

    if rule.extra_args.is_some() {
        args.extend(rule.extra_args.clone().unwrap());
    };

    for (key, value) in rule.extra_env.iter() {
        extra_envs.insert(key.clone(), value.clone());
    }

    // TODO send stderr to stdout.
    let mut cmd = std::process::Command::new(&dist.python_exe)
        .args(&args)
        .envs(&extra_envs)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("error running pip");
    {
        let stdout = cmd.stdout.as_mut().unwrap();
        let reader = BufReader::new(stdout);

        for line in reader.lines() {
            warn!(logger, "{}", line.unwrap());
        }
    }

    let status = cmd.wait().unwrap();
    if !status.success() {
        panic!("error running pip");
    }

    for resource in find_python_resources(&target_dir_path) {
        match resource {
            PythonFileResource::Source {
                full_name, path, ..
            } => {
                let is_package = is_package_from_path(&path);
                let source = fs::read(path).expect("error reading source file");

                if rule.include_source {
                    res.push(PythonResourceAction {
                        action: ResourceAction::Add,
                        location: location.clone(),
                        resource: PythonResource::ModuleSource {
                            name: full_name.clone(),
                            source: source.clone(),
                            is_package,
                        },
                    });
                }

                res.push(PythonResourceAction {
                    action: ResourceAction::Add,
                    location: location.clone(),
                    resource: PythonResource::ModuleBytecode {
                        name: full_name.clone(),
                        source,
                        optimize_level: rule.optimize_level as i32,
                        is_package,
                    },
                });
            }

            PythonFileResource::Resource(resource) => {
                let data = fs::read(resource.path).expect("error reading resource file");

                res.push(PythonResourceAction {
                    action: ResourceAction::Add,
                    location: location.clone(),
                    resource: PythonResource::Resource {
                        package: resource.package.clone(),
                        name: resource.stem.clone(),
                        data,
                    },
                });
            }

            _ => {}
        }
    }

    resolve_built_extensions(
        &PathBuf::from(extra_envs.get("PYOXIDIZER_DISTUTILS_STATE_DIR").unwrap()),
        &mut res,
        &location,
    )
    .unwrap();

    res
}

fn resolve_setup_py_install(
    logger: &slog::Logger,
    context: &BuildContext,
    dist: &PythonDistributionInfo,
    rule: &PackagingSetupPyInstall,
    verbose: bool,
) -> Vec<PythonResourceAction> {
    let mut res = Vec::new();

    // Execution directory is resolved relative to the active configuration
    // file unless it is absolute.
    let rule_path = PathBuf::from(&rule.path);

    let cwd = if rule_path.is_absolute() {
        rule_path.to_path_buf()
    } else {
        context.config_parent_path.join(&rule.path)
    };

    let location = ResourceLocation::new(&rule.install_location);

    let temp_dir = tempdir::TempDir::new("pyoxidizer-setup-py-install")
        .expect("could not create temp directory");

    let target_dir_path = temp_dir.path().join("install");
    let target_dir_s = target_dir_path.display().to_string();

    let python_paths = resolve_python_paths(&target_dir_path, &dist.version, dist.os == "windows");

    std::fs::create_dir_all(&python_paths.site_packages)
        .expect("unable to create site-packages directory");

    let mut extra_envs = prepare_hacked_distutils(
        logger,
        dist,
        temp_dir.path(),
        &[&python_paths.site_packages, &python_paths.main],
    )
    .expect("unable to hack distutils");

    for (key, value) in rule.extra_env.iter() {
        extra_envs.insert(key.clone(), value.clone());
    }

    warn!(logger, "python setup.py installing to {}", target_dir_s);

    let mut args = vec!["setup.py"];

    for arg in &rule.extra_global_arguments {
        args.push(arg);
    }

    if verbose {
        args.push("--verbose");
    }

    args.extend(&["install", "--prefix", &target_dir_s, "--no-compile"]);

    // TODO send stderr to stdout.
    let mut cmd = std::process::Command::new(&dist.python_exe)
        .current_dir(cwd)
        .args(&args)
        .envs(&extra_envs)
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
        panic!("error running setup.py");
    }

    let packages_path = python_paths.site_packages;

    for resource in find_python_resources(&packages_path) {
        let mut relevant = true;
        let full_name = resource_full_name(&resource);

        for exclude in &rule.excludes {
            let prefix = exclude.clone() + ".";

            if full_name == exclude || full_name.starts_with(&prefix) {
                relevant = false;
            }
        }

        if !relevant {
            continue;
        }

        match resource {
            PythonFileResource::Source {
                full_name, path, ..
            } => {
                let is_package = is_package_from_path(&path);
                let source = fs::read(path).expect("error reading source");

                if rule.include_source {
                    res.push(PythonResourceAction {
                        action: ResourceAction::Add,
                        location: location.clone(),
                        resource: PythonResource::ModuleSource {
                            name: full_name.clone(),
                            source: source.clone(),
                            is_package,
                        },
                    });
                }

                res.push(PythonResourceAction {
                    action: ResourceAction::Add,
                    location: location.clone(),
                    resource: PythonResource::ModuleBytecode {
                        name: full_name.clone(),
                        source,
                        optimize_level: rule.optimize_level as i32,
                        is_package,
                    },
                });
            }

            PythonFileResource::Resource(resource) => {
                let data = fs::read(resource.path).expect("error reading resource file");

                res.push(PythonResourceAction {
                    action: ResourceAction::Add,
                    location: location.clone(),
                    resource: PythonResource::Resource {
                        package: resource.package.clone(),
                        name: resource.stem.clone(),
                        data,
                    },
                });
            }

            _ => {}
        }
    }

    resolve_built_extensions(
        &PathBuf::from(extra_envs.get("PYOXIDIZER_DISTUTILS_STATE_DIR").unwrap()),
        &mut res,
        &location,
    )
    .unwrap();

    res
}

/// Resolves a Python packaging rule to resources to package.
pub fn resolve_python_packaging(
    logger: &slog::Logger,
    context: &BuildContext,
    package: &PythonPackaging,
    dist: &PythonDistributionInfo,
    verbose: bool,
) -> Vec<PythonResourceAction> {
    match package {
        PythonPackaging::StdlibExtensionsPolicy(rule) => {
            resolve_stdlib_extensions_policy(logger, dist, &rule)
        }

        PythonPackaging::StdlibExtensionsExplicitIncludes(rule) => {
            resolve_stdlib_extensions_explicit_includes(dist, &rule)
        }

        PythonPackaging::StdlibExtensionsExplicitExcludes(rule) => {
            resolve_stdlib_extensions_explicit_excludes(dist, &rule)
        }

        PythonPackaging::StdlibExtensionVariant(rule) => {
            resolve_stdlib_extension_variant(dist, rule)
        }

        PythonPackaging::Stdlib(rule) => resolve_stdlib(logger, dist, &rule),

        PythonPackaging::Virtualenv(rule) => resolve_virtualenv(dist, &rule),

        PythonPackaging::PackageRoot(rule) => resolve_package_root(&rule),

        PythonPackaging::PipInstallSimple(rule) => {
            resolve_pip_install_simple(logger, dist, &rule, verbose)
        }

        PythonPackaging::PipRequirementsFile(rule) => {
            resolve_pip_requirements_file(logger, dist, &rule, verbose)
        }

        PythonPackaging::SetupPyInstall(rule) => {
            resolve_setup_py_install(logger, context, dist, &rule, verbose)
        }

        PythonPackaging::WriteLicenseFiles(_) => Vec::new(),

        // This is a no-op because it can only be handled at a higher level.
        PythonPackaging::FilterInclude(_) => Vec::new(),
    }
}
