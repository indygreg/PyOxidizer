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
use crate::py_packaging::distribution::{
    is_stdlib_test_package, resolve_python_paths, ParsedPythonDistribution,
};
use crate::py_packaging::distutils::read_built_extensions;
use crate::py_packaging::fsscan::{
    find_python_resources, is_package_from_path, PythonFileResource,
};
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

// part of new approach from pip.rs; unused atm
fn python_resource_full_name(resource: &PythonResource) -> String {
    match resource {
        PythonResource::ModuleSource { name, .. } => name.clone(),
        PythonResource::Resource { package, name, .. } => format!("{}.{}", package, name),
        PythonResource::BuiltExtensionModule(em) => em.name.clone(),
        _ => "".to_string(),
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

/// Processes resources in a path
/// Args includes and excludes are ignored if None or an empty Vec.
fn process_resources(
    logger: &slog::Logger,
    path: &PathBuf,
    location: &ResourceLocation,
    state_dir: Option<&PathBuf>,
    include_source: bool,
    optimize_level: i64,
    includes: Option<&Vec<String>>,
    excludes: Option<&Vec<String>>,
) -> Vec<PythonResourceAction> {
    let mut res = Vec::new();

    let path_s = path.display().to_string();
    warn!(logger, "processing resources from {}", path_s);

    for resource in find_python_resources(path) {
        let full_name = resource_full_name(&resource);

        let excluded = match includes {
            Some(values) => values.iter().any(|v| {
                let prefix = v.clone() + ".";
                full_name != v && !full_name.starts_with(&prefix)
            }),
            None => false,
        };

        if excluded {
            info!(
                logger,
                "whitelist skipping {}", full_name
            );
            continue;
        }

        let excluded = match excludes {
            Some(values) => match values.is_empty() {
                true => false,
                false => values.iter().all(|v| {
                    let prefix = v.clone() + ".";
                    full_name == v || full_name.starts_with(&prefix)
                }),
            },
            None => false,
        };

        if excluded {
            info!(
                logger,
                "blacklist skipping {}", full_name
            );
            continue;
        }

        match resource {
            PythonFileResource::Source {
                full_name, path, ..
            } => {
                let is_package = is_package_from_path(&path);
                let source = fs::read(path).expect("error reading source file");

                if include_source {
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
                    resource: PythonResource::ModuleBytecodeRequest {
                        name: full_name.clone(),
                        source,
                        optimize_level: optimize_level as i32,
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

    match state_dir {
        Some(dir) => {
            if dir.exists() {
                resolve_built_extensions(&dir, &mut res, &location).unwrap();
            }
        }
        None => {}
    };

    res
}

fn resolve_stdlib_extensions_policy(
    logger: &slog::Logger,
    dist: &ParsedPythonDistribution,
    rule: &PackagingStdlibExtensionsPolicy,
) -> Vec<PythonResourceAction> {
    let mut res = Vec::new();

    for ext in dist.filter_extension_modules(logger, &rule.filter) {
        res.push(PythonResourceAction {
            action: ResourceAction::Add,
            location: ResourceLocation::Embedded,
            resource: PythonResource::ExtensionModule {
                name: ext.module.clone(),
                module: ext,
            },
        });
    }

    res
}

fn resolve_stdlib_extensions_explicit_includes(
    dist: &ParsedPythonDistribution,
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
    dist: &ParsedPythonDistribution,
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
    dist: &ParsedPythonDistribution,
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
    dist: &ParsedPythonDistribution,
    rule: &PackagingStdlib,
) -> Vec<PythonResourceAction> {
    let mut res = Vec::new();

    let location = ResourceLocation::new(&rule.install_location);

    for m in dist.source_modules() {
        if is_stdlib_test_package(&m.name) && rule.exclude_test_modules {
            info!(logger, "skipping test stdlib module: {}", m.name);
            continue;
        }

        let mut relevant = true;

        for exclude in &rule.excludes {
            let prefix = exclude.clone() + ".";

            if &m.name == exclude || m.name.starts_with(&prefix) {
                relevant = false;
            }
        }

        if !relevant {
            continue;
        }

        if rule.include_source {
            res.push(PythonResourceAction {
                action: ResourceAction::Add,
                location: location.clone(),
                resource: m.as_python_resource(),
            });
        }

        res.push(PythonResourceAction {
            action: ResourceAction::Add,
            location: location.clone(),
            resource: PythonResource::ModuleBytecodeRequest {
                name: m.name,
                source: m.source,
                optimize_level: rule.optimize_level as i32,
                is_package: m.is_package,
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
    logger: &slog::Logger,
    dist: &ParsedPythonDistribution,
    rule: &PackagingVirtualenv,
) -> Vec<PythonResourceAction> {
    let location = ResourceLocation::new(&rule.install_location);

    let python_paths =
        resolve_python_paths(&Path::new(&rule.path), &dist.version);

    process_resources(
        &logger,
        &python_paths.site_packages,
        &location,
        Some(&python_paths.pyoxidizer_state_dir),
        rule.include_source,
        rule.optimize_level,
        None,
        Some(&rule.excludes),
    )
}

fn resolve_package_root(
    logger: &slog::Logger,
    rule: &PackagingPackageRoot,
) -> Vec<PythonResourceAction> {
    let location = ResourceLocation::new(&rule.install_location);
    let path = PathBuf::from(&rule.path);

    process_resources(
        &logger,
        &path,
        &location,
        None,
        rule.include_source,
        rule.optimize_level,
        Some(&rule.packages),
        None,
    )
}

fn resolve_pip_install_simple(
    logger: &slog::Logger,
    dist: &ParsedPythonDistribution,
    rule: &PackagingPipInstallSimple,
    verbose: bool,
) -> Vec<PythonResourceAction> {
    let location = ResourceLocation::new(&rule.install_location);

    let (python_paths, mut extra_envs) = dist.prepare_venv(&logger, rule.venv_path.as_ref());

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
        "--no-binary".to_string(),
        ":all:".to_string(),
        rule.package.clone(),
    ]);

    if let Some(ref args) = rule.extra_args {
        pip_args.extend(args.clone());
    }

    for (key, value) in rule.extra_env.iter() {
        extra_envs.insert(key.clone(), value.clone());
    }

    // TODO send stderr to stdout.
    let mut cmd = std::process::Command::new(&python_paths.python_exe)
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

    process_resources(
        &logger,
        &python_paths.site_packages,
        &location,
        Some(&python_paths.pyoxidizer_state_dir),
        rule.include_source,
        rule.optimize_level,
        None,
        Some(&rule.excludes),
    )
}

fn resolve_pip_requirements_file(
    logger: &slog::Logger,
    dist: &ParsedPythonDistribution,
    rule: &PackagingPipRequirementsFile,
    verbose: bool,
) -> Vec<PythonResourceAction> {
    let location = ResourceLocation::new(&rule.install_location);

    let (python_paths, mut extra_envs) = dist.prepare_venv(&logger, rule.venv_path.as_ref());

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
        "--no-binary".to_string(),
        ":all:".to_string(),
        "--requirement".to_string(),
        rule.requirements_path.to_string(),
    ]);

    if let Some(ref args) = rule.extra_args {
        pip_args.extend(args.clone());
    }

    for (key, value) in rule.extra_env.iter() {
        extra_envs.insert(key.clone(), value.clone());
    }

    warn!(
        logger,
        "Running {} {}",
        python_paths.python_exe.display(),
        pip_args.join(" ")
    );

    // TODO send stderr to stdout.
    let mut cmd = std::process::Command::new(&python_paths.python_exe)
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

    process_resources(
        &logger,
        &python_paths.site_packages,
        &location,
        Some(&python_paths.pyoxidizer_state_dir),
        rule.include_source,
        rule.optimize_level,
        None,
        None,
    )
}

fn resolve_setup_py_install(
    logger: &slog::Logger,
    context: &BuildContext,
    dist: &ParsedPythonDistribution,
    rule: &PackagingSetupPyInstall,
    verbose: bool,
) -> Vec<PythonResourceAction> {
    // Execution directory is resolved relative to the active configuration
    // file unless it is absolute.
    let rule_path = PathBuf::from(&rule.path);

    let cwd = if rule_path.is_absolute() {
        rule_path.to_path_buf()
    } else {
        context.config_parent_path.join(&rule.path)
    };

    let location = ResourceLocation::new(&rule.install_location);

    let (python_paths, mut extra_envs) = dist.prepare_venv(&logger, rule.venv_path.as_ref());

    for (key, value) in rule.extra_env.iter() {
        extra_envs.insert(key.clone(), value.clone());
    }

    let mut args = vec!["setup.py"];

    for arg in &rule.extra_global_arguments {
        args.push(arg);
    }

    if verbose {
        args.push("--verbose");
    }

    let prefix_dir_s = python_paths.prefix.display().to_string();

    args.extend(&["install", "--prefix", &prefix_dir_s, "--no-compile"]);

    warn!(
        logger,
        "Running {} {}",
        python_paths.python_exe.display(),
        args.join(" ")
    );

    // TODO send stderr to stdout.
    let mut cmd = std::process::Command::new(&python_paths.python_exe)
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

    process_resources(
        &logger,
        &python_paths.site_packages,
        &location,
        Some(&python_paths.pyoxidizer_state_dir),
        rule.include_source,
        rule.optimize_level,
        None,
        Some(&rule.excludes),
    )
}

/// Resolves a Python packaging rule to resources to package.
pub fn resolve_python_packaging(
    logger: &slog::Logger,
    context: &BuildContext,
    package: &PythonPackaging,
    dist: &ParsedPythonDistribution,
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

        PythonPackaging::Virtualenv(rule) => resolve_virtualenv(logger, dist, &rule),

        PythonPackaging::PackageRoot(rule) => resolve_package_root(logger, &rule),

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
