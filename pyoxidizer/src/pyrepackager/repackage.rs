// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use byteorder::{LittleEndian, WriteBytesExt};
use glob::glob as findglob;
use itertools::Itertools;
use lazy_static::lazy_static;
use slog::info;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::fs::create_dir_all;
use std::io::{BufRead, BufReader, Cursor, Error as IOError, Read, Write};
use std::path::{Path, PathBuf};

use super::bytecode::BytecodeCompiler;
use super::config::{
    parse_config, Config, PackagingPackageRoot, PackagingPipInstallSimple,
    PackagingPipRequirementsFile, PackagingSetupPyInstall, PackagingStdlib,
    PackagingStdlibExtensionVariant, PackagingStdlibExtensionsExplicitExcludes,
    PackagingStdlibExtensionsExplicitIncludes, PackagingStdlibExtensionsPolicy,
    PackagingVirtualenv, PythonDistribution, PythonPackaging, RawAllocator, RunMode,
};
use super::dist::{
    analyze_python_distribution_tar_zst, resolve_python_distribution_archive, ExtensionModule,
    PythonDistributionInfo,
};
use super::fsscan::{find_python_resources, PythonResourceType};

pub const PYTHON_IMPORTER: &[u8] = include_bytes!("memoryimporter.py");

const STDLIB_TEST_PACKAGES: &[&str] = &[
    "bsddb.test",
    "ctypes.test",
    "distutils.tests",
    "email.test",
    "idlelib.idle_test",
    "json.tests",
    "lib-tk.test",
    "lib2to3.tests",
    "sqlite3.test",
    "test",
    "tkinter.test",
    "unittest.test",
];

lazy_static! {
    /// Libraries provided by the host that we can ignore in Python module library dependencies.
    ///
    /// Libraries in this data structure are not provided by the Python distribution.
    /// A library should only be in this data structure if it is universally distributed
    /// by the OS. It is assumed that all binaries produced for the target will link
    /// against these libraries by default.
    static ref OS_IGNORE_LIBRARIES: Vec<&'static str> = {
        let mut v = Vec::new();

        if cfg!(target_os = "linux") {
            v.push("dl");
            v.push("m");
        } else if cfg!(target_os = "macos") {
            v.push("dl");
            v.push("m");
        }

        v
    };
}

lazy_static! {
    /// Python extension modules that should never be included.
    ///
    /// Ideally this data structure doesn't exist. But there are some problems
    /// with various extensions on various targets.
    static ref OS_IGNORE_EXTENSIONS: Vec<&'static str> = {
        let mut v = Vec::new();

        if cfg!(target_os = "linux") {
            // Linking issues.
            v.push("_crypt");

            // Linking issues.
            v.push("nis");
        }

        else if cfg!(target_os = "macos") {
            // curses and readline have linking issues.
            v.push("_curses");
            v.push("_curses_panel");
            v.push("readline");
        }

        v
    };
}

pub fn is_stdlib_test_package(name: &str) -> bool {
    for package in STDLIB_TEST_PACKAGES {
        let prefix = format!("{}.", package);

        if &name == package || name.starts_with(&prefix) {
            return true;
        }
    }

    false
}

/// Represents a single module's data record.
pub struct ModuleEntry {
    pub name: String,
    pub source: Option<Vec<u8>>,
    pub bytecode: Option<Vec<u8>>,
}

/// Represents an ordered collection of module entries.
pub type ModuleEntries = Vec<ModuleEntry>;

/// Represents a resource to make available to the Python interpreter.
#[derive(Debug)]
pub enum PythonResource {
    ExtensionModule {
        name: String,
        module: ExtensionModule,
    },
    ModuleSource {
        name: String,
        source: Vec<u8>,
    },
    ModuleBytecode {
        name: String,
        source: Vec<u8>,
        optimize_level: i32,
    },
    Resource {
        package: String,
        name: String,
        data: Vec<u8>,
    },
}

#[derive(Debug)]
pub enum ResourceAction {
    Add,
    Remove,
}

#[derive(Debug)]
pub struct PythonResourceEntry {
    action: ResourceAction,
    resource: PythonResource,
}

/// Represents Python resources to embed in a binary.
pub struct PythonResources {
    pub module_sources: BTreeMap<String, Vec<u8>>,
    pub module_bytecodes: BTreeMap<String, Vec<u8>>,
    pub all_modules: BTreeSet<String>,
    pub resources: BTreeMap<String, BTreeMap<String, Vec<u8>>>,
    pub extension_modules: BTreeMap<String, ExtensionModule>,
    pub read_files: Vec<PathBuf>,
}

impl PythonResources {
    /// Obtain records for all modules in this resources collection.
    pub fn modules_records(&self) -> ModuleEntries {
        let mut records = ModuleEntries::new();

        for name in &self.all_modules {
            let source = self.module_sources.get(name);
            let bytecode = self.module_bytecodes.get(name);

            records.push(ModuleEntry {
                name: name.clone(),
                source: match source {
                    Some(value) => Some(value.clone()),
                    None => None,
                },
                bytecode: match bytecode {
                    Some(value) => Some(value.clone()),
                    None => None,
                },
            });
        }

        records
    }

    pub fn write_blobs(&self, module_names_path: &PathBuf, modules_path: &PathBuf) {
        let mut fh = fs::File::create(module_names_path).expect("error creating file");
        for name in &self.all_modules {
            fh.write_all(name.as_bytes()).expect("failed to write");
            fh.write_all(b"\n").expect("failed to write");
        }

        let fh = fs::File::create(modules_path).unwrap();
        write_modules_entries(&fh, &self.modules_records()).unwrap();
    }
}

fn read_resource_names_file(path: &Path) -> Result<BTreeSet<String>, IOError> {
    let fh = fs::File::open(path)?;

    let mut res: BTreeSet<String> = BTreeSet::new();

    for line in BufReader::new(fh).lines() {
        let line = line?;

        if line.starts_with('#') || line.is_empty() {
            continue;
        }

        res.insert(line);
    }

    Ok(res)
}

fn bytecode_compiler(dist: &PythonDistributionInfo) -> BytecodeCompiler {
    BytecodeCompiler::new(&dist.python_exe)
}

fn filter_btreemap<V>(logger: &slog::Logger, m: &mut BTreeMap<String, V>, f: &BTreeSet<String>) {
    let keys: Vec<String> = m.keys().cloned().collect();

    for key in keys {
        if !f.contains(&key) {
            info!(logger, "removing {}", key);
            m.remove(&key);
        }
    }
}

fn resolve_stdlib_extensions_policy(
    dist: &PythonDistributionInfo,
    rule: &PackagingStdlibExtensionsPolicy,
) -> Vec<PythonResourceEntry> {
    let mut res = Vec::new();

    for (name, variants) in &dist.extension_modules {
        match rule.policy.as_str() {
            "minimal" => {
                let em = &variants[0];

                if em.builtin_default || em.required {
                    res.push(PythonResourceEntry {
                        action: ResourceAction::Add,
                        resource: PythonResource::ExtensionModule {
                            name: name.clone(),
                            module: em.clone(),
                        },
                    });
                }
            }

            "all" => {
                let em = &variants[0];
                res.push(PythonResourceEntry {
                    action: ResourceAction::Add,
                    resource: PythonResource::ExtensionModule {
                        name: name.clone(),
                        module: em.clone(),
                    },
                });
            }

            "no-libraries" => {
                for em in variants {
                    if em.links.is_empty() {
                        res.push(PythonResourceEntry {
                            action: ResourceAction::Add,
                            resource: PythonResource::ExtensionModule {
                                name: name.clone(),
                                module: em.clone(),
                            },
                        });

                        break;
                    }
                }
            }

            other => {
                panic!("illegal policy value: {}", other);
            }
        }
    }

    res
}

fn resolve_stdlib_extensions_explicit_includes(
    dist: &PythonDistributionInfo,
    rule: &PackagingStdlibExtensionsExplicitIncludes,
) -> Vec<PythonResourceEntry> {
    let mut res = Vec::new();

    for name in &rule.includes {
        if let Some(modules) = &dist.extension_modules.get(name) {
            res.push(PythonResourceEntry {
                action: ResourceAction::Add,
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
) -> Vec<PythonResourceEntry> {
    let mut res = Vec::new();

    for (name, modules) in &dist.extension_modules {
        if rule.excludes.contains(name) {
            continue;
        }

        res.push(PythonResourceEntry {
            action: ResourceAction::Add,
            resource: PythonResource::ExtensionModule {
                name: name.clone(),
                module: modules[0].clone(),
            },
        });
    }

    res
}

fn resolve_stdlib_extension_variant(
    dist: &PythonDistributionInfo,
    rule: &PackagingStdlibExtensionVariant,
) -> Vec<PythonResourceEntry> {
    let mut res = Vec::new();

    let variants = &dist.extension_modules[&rule.extension];

    for em in variants {
        if &em.variant == &rule.variant {
            res.push(PythonResourceEntry {
                action: ResourceAction::Add,
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
) -> Vec<PythonResourceEntry> {
    let mut res = Vec::new();

    for (name, fs_path) in &dist.py_modules {
        if is_stdlib_test_package(&name) && rule.exclude_test_modules {
            info!(logger, "skipping test stdlib module: {}", name);
            continue;
        }

        let source = fs::read(fs_path).expect("error reading source file");

        if rule.include_source {
            res.push(PythonResourceEntry {
                action: ResourceAction::Add,
                resource: PythonResource::ModuleSource {
                    name: name.clone(),
                    source: source.clone(),
                },
            });
        }

        res.push(PythonResourceEntry {
            action: ResourceAction::Add,
            resource: PythonResource::ModuleBytecode {
                name: name.clone(),
                source,
                optimize_level: rule.optimize_level as i32,
            },
        });
    }

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

            res.push(PythonResourceEntry {
                action: ResourceAction::Add,
                resource: PythonResource::Resource {
                    package: package.clone(),
                    name: name.clone(),
                    data,
                },
            });
        }
    }

    res
}

fn resolve_virtualenv(
    dist: &PythonDistributionInfo,
    rule: &PackagingVirtualenv,
) -> Vec<PythonResourceEntry> {
    let mut res = Vec::new();

    let mut packages_path = PathBuf::from(&rule.path);

    if dist.os == "windows" {
        packages_path.push("Lib");
    } else {
        packages_path.push("lib");
    }

    packages_path.push("python".to_owned() + &dist.version[0..3]);
    packages_path.push("site-packages");

    for resource in find_python_resources(&packages_path) {
        let mut relevant = true;

        for exclude in &rule.excludes {
            let prefix = exclude.clone() + ".";

            if &resource.full_name == exclude || resource.full_name.starts_with(&prefix) {
                relevant = false;
            }
        }

        if !relevant {
            continue;
        }

        match resource.flavor {
            PythonResourceType::Source => {
                let source = fs::read(resource.path).expect("error reading source file");

                if rule.include_source {
                    res.push(PythonResourceEntry {
                        action: ResourceAction::Add,
                        resource: PythonResource::ModuleSource {
                            name: resource.full_name.clone(),
                            source: source.clone(),
                        },
                    });
                }

                res.push(PythonResourceEntry {
                    action: ResourceAction::Add,
                    resource: PythonResource::ModuleBytecode {
                        name: resource.full_name.clone(),
                        source,
                        optimize_level: rule.optimize_level as i32,
                    },
                });
            }

            PythonResourceType::Resource => {
                let data = fs::read(resource.path).expect("error reading resource file");

                res.push(PythonResourceEntry {
                    action: ResourceAction::Add,
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

fn resolve_package_root(rule: &PackagingPackageRoot) -> Vec<PythonResourceEntry> {
    let mut res = Vec::new();

    let path = PathBuf::from(&rule.path);

    for resource in find_python_resources(&path) {
        let mut relevant = false;

        for package in &rule.packages {
            let prefix = package.clone() + ".";

            if &resource.full_name == package || resource.full_name.starts_with(&prefix) {
                relevant = true;
            }
        }

        for exclude in &rule.excludes {
            let prefix = exclude.clone() + ".";

            if &resource.full_name == exclude || resource.full_name.starts_with(&prefix) {
                relevant = false;
            }
        }

        if !relevant {
            continue;
        }

        match resource.flavor {
            PythonResourceType::Source => {
                let source = fs::read(resource.path).expect("error reading source file");

                if rule.include_source {
                    res.push(PythonResourceEntry {
                        action: ResourceAction::Add,
                        resource: PythonResource::ModuleSource {
                            name: resource.full_name.clone(),
                            source: source.clone(),
                        },
                    });
                }

                res.push(PythonResourceEntry {
                    action: ResourceAction::Add,
                    resource: PythonResource::ModuleBytecode {
                        name: resource.full_name.clone(),
                        source,
                        optimize_level: rule.optimize_level as i32,
                    },
                });
            }

            PythonResourceType::Resource => {
                let data = fs::read(resource.path).expect("error reading resource file");

                res.push(PythonResourceEntry {
                    action: ResourceAction::Add,
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
) -> Vec<PythonResourceEntry> {
    let mut res = Vec::new();

    dist.ensure_pip();
    let temp_dir =
        tempdir::TempDir::new("pyoxidizer-pip-install").expect("could not creat temp directory");

    let temp_dir_path = temp_dir.path();
    let temp_dir_s = temp_dir_path.display().to_string();
    info!(logger, "pip installing to {}", temp_dir_s);

    std::process::Command::new(&dist.python_exe)
        .args(&[
            "-m",
            "pip",
            "--disable-pip-version-check",
            "install",
            "--target",
            &temp_dir_s,
            &rule.package,
        ])
        .status()
        .expect("error running pip");

    for resource in find_python_resources(&temp_dir_path) {
        match resource.flavor {
            PythonResourceType::Source => {
                let source = fs::read(resource.path).expect("error reading source file");

                if rule.include_source {
                    res.push(PythonResourceEntry {
                        action: ResourceAction::Add,
                        resource: PythonResource::ModuleSource {
                            name: resource.full_name.clone(),
                            source: source.clone(),
                        },
                    });
                }

                res.push(PythonResourceEntry {
                    action: ResourceAction::Add,
                    resource: PythonResource::ModuleBytecode {
                        name: resource.full_name.clone(),
                        source,
                        optimize_level: rule.optimize_level as i32,
                    },
                });
            }

            PythonResourceType::Resource => {
                let data = fs::read(resource.path).expect("error reading resource file");

                res.push(PythonResourceEntry {
                    action: ResourceAction::Add,
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

fn resolve_pip_requirements_file(
    logger: &slog::Logger,
    dist: &PythonDistributionInfo,
    rule: &PackagingPipRequirementsFile,
) -> Vec<PythonResourceEntry> {
    let mut res = Vec::new();

    dist.ensure_pip();

    let temp_dir =
        tempdir::TempDir::new("pyoxidizer-pip-install").expect("could not create temp directory");

    let temp_dir_path = temp_dir.path();
    let temp_dir_s = temp_dir_path.display().to_string();
    info!(logger, "pip installing to {}", temp_dir_s);

    std::process::Command::new(&dist.python_exe)
        .args(&[
            "-m",
            "pip",
            "--disable-pip-version-check",
            "install",
            "--target",
            &temp_dir_s,
            "--no-binary",
            ":all:",
            "--requirement",
            &rule.requirements_path,
        ])
        .status()
        .expect("error running pip");

    for resource in find_python_resources(&temp_dir_path) {
        match resource.flavor {
            PythonResourceType::Source => {
                let source = fs::read(resource.path).expect("error reading source file");

                if rule.include_source {
                    res.push(PythonResourceEntry {
                        action: ResourceAction::Add,
                        resource: PythonResource::ModuleSource {
                            name: resource.full_name.clone(),
                            source: source.clone(),
                        },
                    });
                }

                res.push(PythonResourceEntry {
                    action: ResourceAction::Add,
                    resource: PythonResource::ModuleBytecode {
                        name: resource.full_name.clone(),
                        source,
                        optimize_level: rule.optimize_level as i32,
                    },
                });
            }

            PythonResourceType::Resource => {
                let data = fs::read(resource.path).expect("error reading resource file");

                res.push(PythonResourceEntry {
                    action: ResourceAction::Add,
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

fn resolve_setup_py_install(
    logger: &slog::Logger,
    dist: &PythonDistributionInfo,
    rule: &PackagingSetupPyInstall,
) -> Vec<PythonResourceEntry> {
    let mut res = Vec::new();

    let temp_dir = tempdir::TempDir::new("pyoxidizer-setup-py-install")
        .expect("could not create temp directory");
    let temp_dir_path = temp_dir.path();
    let temp_dir_s = temp_dir_path.display().to_string();
    info!(logger, "python setup.py installing to {}", temp_dir_s);

    std::process::Command::new(&dist.python_exe)
        .current_dir(&rule.path)
        .args(&[
            "setup.py",
            "install",
            "--prefix",
            &temp_dir_s,
            "--no-compile",
        ])
        .status()
        .expect("error running setup.py");

    let mut packages_path = temp_dir_path.to_path_buf();

    if dist.os == "windows" {
        packages_path.push("Lib");
    } else {
        packages_path.push("lib");
    }

    packages_path.push("python".to_owned() + &dist.version[0..3]);
    packages_path.push("site-packages");

    for resource in find_python_resources(&packages_path) {
        match resource.flavor {
            PythonResourceType::Source => {
                let source = fs::read(resource.path).expect("error reading source");

                if rule.include_source {
                    res.push(PythonResourceEntry {
                        action: ResourceAction::Add,
                        resource: PythonResource::ModuleSource {
                            name: resource.full_name.clone(),
                            source: source.clone(),
                        },
                    });
                }

                res.push(PythonResourceEntry {
                    action: ResourceAction::Add,
                    resource: PythonResource::ModuleBytecode {
                        name: resource.full_name.clone(),
                        source,
                        optimize_level: rule.optimize_level as i32,
                    },
                });
            }

            PythonResourceType::Resource => {
                let data = fs::read(resource.path).expect("error reading resource file");

                res.push(PythonResourceEntry {
                    action: ResourceAction::Add,
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

/// Resolves a Python packaging rule to resources to package.
fn resolve_python_packaging(
    logger: &slog::Logger,
    package: &PythonPackaging,
    dist: &PythonDistributionInfo,
) -> Vec<PythonResourceEntry> {
    match package {
        PythonPackaging::StdlibExtensionsPolicy(rule) => {
            resolve_stdlib_extensions_policy(dist, &rule)
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

        PythonPackaging::PipInstallSimple(rule) => resolve_pip_install_simple(logger, dist, &rule),

        PythonPackaging::PipRequirementsFile(rule) => {
            resolve_pip_requirements_file(logger, dist, &rule)
        }

        PythonPackaging::SetupPyInstall(rule) => resolve_setup_py_install(logger, dist, &rule),

        // This is a no-op because it can only be handled at a higher level.
        PythonPackaging::FilterInclude(_) => Vec::new(),
    }
}

/// Resolves a series of packaging rules to a final set of resources to package.
pub fn resolve_python_resources(
    logger: &slog::Logger,
    config: &Config,
    dist: &PythonDistributionInfo,
) -> PythonResources {
    let packages = &config.python_packaging;

    // Since bytecode has a non-trivial cost to generate, our strategy is to accumulate
    // requests for bytecode then generate bytecode for the final set of inputs at the
    // end of processing. That way we don't generate bytecode only to throw it away later.

    let mut extension_modules: BTreeMap<String, ExtensionModule> = BTreeMap::new();
    let mut sources: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut bytecode_requests: BTreeMap<String, (Vec<u8>, i32)> = BTreeMap::new();
    let mut resources: BTreeMap<String, BTreeMap<String, Vec<u8>>> = BTreeMap::new();
    let mut read_files: Vec<PathBuf> = Vec::new();

    for packaging in packages {
        info!(logger, "processing packaging rule: {:?}", packaging);
        for entry in resolve_python_packaging(logger, packaging, dist) {
            match (entry.action, entry.resource) {
                (ResourceAction::Add, PythonResource::ExtensionModule { name, module }) => {
                    info!(logger, "adding extension module: {}", name);
                    extension_modules.insert(name, module);
                }
                (ResourceAction::Remove, PythonResource::ExtensionModule { name, .. }) => {
                    info!(logger, "removing extension module: {}", name);
                    extension_modules.remove(&name);
                }
                (ResourceAction::Add, PythonResource::ModuleSource { name, source }) => {
                    info!(logger, "adding module source: {}", name);
                    sources.insert(name.clone(), source);
                }
                (ResourceAction::Remove, PythonResource::ModuleSource { name, .. }) => {
                    info!(logger, "removing module source: {}", name);
                    sources.remove(&name);
                }
                (
                    ResourceAction::Add,
                    PythonResource::ModuleBytecode {
                        name,
                        source,
                        optimize_level,
                    },
                ) => {
                    info!(logger, "adding module bytecode: {}", name);
                    bytecode_requests.insert(name.clone(), (source, optimize_level));
                }
                (ResourceAction::Remove, PythonResource::ModuleBytecode { name, .. }) => {
                    info!(logger, "removing module bytecode: {}", name);
                    bytecode_requests.remove(&name);
                }
                (
                    ResourceAction::Add,
                    PythonResource::Resource {
                        package,
                        name,
                        data,
                    },
                ) => {
                    info!(logger, "adding resource: {}", name);

                    if !resources.contains_key(&package) {
                        resources.insert(package.clone(), BTreeMap::new());
                    }

                    resources.get_mut(&package).unwrap().insert(name, data);
                }
                (ResourceAction::Remove, PythonResource::Resource { name, .. }) => {
                    info!(logger, "removing resource: {}", name);
                    resources.remove(&name);
                }
            }
        }

        if let PythonPackaging::FilterInclude(rule) = packaging {
            let mut include_names: BTreeSet<String> = BTreeSet::new();

            for path in &rule.files {
                let path = PathBuf::from(path);
                let new_names =
                    read_resource_names_file(&path).expect("failed to read resource names file");

                include_names.extend(new_names);
                read_files.push(path);
            }

            for glob in &rule.glob_files {
                let mut new_names: BTreeSet<String> = BTreeSet::new();

                for entry in findglob(glob).expect("glob_files glob match failed") {
                    match entry {
                        Ok(path) => {
                            new_names.extend(
                                read_resource_names_file(&path)
                                    .expect("failed to read resource names"),
                            );
                            read_files.push(path);
                        }
                        Err(e) => {
                            panic!("error reading resource names file: {:?}", e);
                        }
                    }
                }

                if new_names.is_empty() {
                    panic!(
                        "glob filter resolves to empty set; are you sure the paths are correct?"
                    );
                }

                include_names.extend(new_names);
            }

            info!(logger, "filtering extension modules from {:?}", packaging);
            filter_btreemap(logger, &mut extension_modules, &include_names);
            info!(logger, "filtering module sources from {:?}", packaging);
            filter_btreemap(logger, &mut sources, &include_names);
            info!(logger, "filtering module bytecode from {:?}", packaging);
            filter_btreemap(logger, &mut bytecode_requests, &include_names);
            info!(logger, "filtering resources from {:?}", packaging);
            filter_btreemap(logger, &mut resources, &include_names);
        }
    }

    // Add required extension modules, as some don't show up in the modules list
    // and may have been filtered or not added in the first place.
    for (name, variants) in &dist.extension_modules {
        let em = &variants[0];

        if (em.builtin_default || em.required) && !extension_modules.contains_key(name) {
            info!(logger, "adding required extension module {}", name);
            extension_modules.insert(name.clone(), em.clone());
        }
    }

    // Remove extension modules that have problems.
    for e in OS_IGNORE_EXTENSIONS.as_slice() {
        info!(
            logger,
            "removing extension module due to incompatibility: {}", e
        );
        extension_modules.remove(&String::from(*e));
    }

    let mut bytecodes: BTreeMap<String, Vec<u8>> = BTreeMap::new();

    {
        let mut compiler = bytecode_compiler(&dist);

        for (name, (source, optimize_level)) in bytecode_requests {
            let bytecode = match compiler.compile(&source, &name, optimize_level) {
                Ok(res) => res,
                Err(msg) => panic!("error compiling bytecode for {}: {}", name, msg),
            };

            bytecodes.insert(name.clone(), bytecode);
        }
    }

    let mut all_modules: BTreeSet<String> = BTreeSet::new();
    for name in sources.keys() {
        all_modules.insert(name.to_string());
    }
    for name in bytecodes.keys() {
        all_modules.insert(name.to_string());
    }

    PythonResources {
        module_sources: sources,
        module_bytecodes: bytecodes,
        all_modules,
        resources,
        extension_modules,
        read_files,
    }
}

pub struct ImportlibData {
    pub bootstrap_source: Vec<u8>,
    pub bootstrap_bytecode: Vec<u8>,
    pub bootstrap_external_source: Vec<u8>,
    pub bootstrap_external_bytecode: Vec<u8>,
}

/// Produce frozen importlib bytecode data.
///
/// importlib._bootstrap isn't modified.
///
/// importlib._bootstrap_external is modified. We take the original Python
/// source and concatenate with code that provides the memory importer.
/// Bytecode is then derived from it.
pub fn derive_importlib(dist: &PythonDistributionInfo) -> ImportlibData {
    let mut compiler = bytecode_compiler(&dist);

    let mod_bootstrap_path = &dist.py_modules["importlib._bootstrap"];
    let mod_bootstrap_external_path = &dist.py_modules["importlib._bootstrap_external"];

    let bootstrap_source = fs::read(&mod_bootstrap_path).expect("unable to read bootstrap source");
    let module_name = "<frozen importlib._bootstrap>";
    let bootstrap_bytecode = compiler
        .compile(&bootstrap_source, module_name, 0)
        .expect("error compiling bytecode");

    let mut bootstrap_external_source =
        fs::read(&mod_bootstrap_external_path).expect("unable to read bootstrap_external source");
    bootstrap_external_source.extend("\n# END OF importlib/_bootstrap_external.py\n\n".bytes());
    bootstrap_external_source.extend(PYTHON_IMPORTER);
    let module_name = "<frozen importlib._bootstrap_external>";
    let bootstrap_external_bytecode = compiler
        .compile(&bootstrap_external_source, module_name, 0)
        .expect("error compiling bytecode");

    ImportlibData {
        bootstrap_source,
        bootstrap_bytecode,
        bootstrap_external_source,
        bootstrap_external_bytecode,
    }
}

/// Serialize a ModulesEntries to a writer.
///
/// See the documentation in the `pyembed` crate for the data format.
pub fn write_modules_entries<W: Write>(
    mut dest: W,
    entries: &[ModuleEntry],
) -> std::io::Result<()> {
    dest.write_u32::<LittleEndian>(entries.len() as u32)?;

    for entry in entries.iter() {
        let name_bytes = entry.name.as_bytes();
        dest.write_u32::<LittleEndian>(name_bytes.len() as u32)?;
        dest.write_u32::<LittleEndian>(if let Some(ref v) = entry.source {
            v.len() as u32
        } else {
            0
        })?;
        dest.write_u32::<LittleEndian>(if let Some(ref v) = entry.bytecode {
            v.len() as u32
        } else {
            0
        })?;
    }

    for entry in entries.iter() {
        let name_bytes = entry.name.as_bytes();
        dest.write_all(name_bytes)?;
    }

    for entry in entries.iter() {
        if let Some(ref v) = entry.source {
            dest.write_all(v.as_slice())?;
        }
    }

    for entry in entries.iter() {
        if let Some(ref v) = entry.bytecode {
            dest.write_all(v.as_slice())?;
        }
    }

    Ok(())
}

/// Produce the content of the config.c file containing built-in extensions.
fn make_config_c(extension_modules: &BTreeMap<String, ExtensionModule>) -> String {
    // It is easier to construct the file from scratch than parse the template
    // and insert things in the right places.
    let mut lines: Vec<String> = Vec::new();

    lines.push(String::from("#include \"Python.h\""));

    // Declare the initialization functions.
    for em in extension_modules.values() {
        if let Some(init_fn) = &em.init_fn {
            if init_fn == "NULL" {
                continue;
            }

            lines.push(format!("extern PyObject* {}(void);", init_fn));
        }
    }

    lines.push(String::from("struct _inittab _PyImport_Inittab[] = {"));

    for em in extension_modules.values() {
        if let Some(init_fn) = &em.init_fn {
            if init_fn == "NULL" {
                continue;
            }

            lines.push(format!("{{\"{}\", {}}},", em.module, init_fn));
        }
    }

    lines.push(String::from("{0, 0}"));
    lines.push(String::from("};"));

    lines.join("\n")
}

#[derive(Debug)]
pub struct LibpythonInfo {
    path: PathBuf,
    cargo_metadata: Vec<String>,
}

/// Create a static libpython from a Python distribution.
///
/// Returns a vector of cargo: lines that can be printed in build scripts.
pub fn link_libpython(
    logger: &slog::Logger,
    dist: &PythonDistributionInfo,
    resources: &PythonResources,
    out_dir: &Path,
    host: &str,
    target: &str,
    opt_level: &str,
) -> LibpythonInfo {
    let mut cargo_metadata: Vec<String> = Vec::new();

    let temp_dir = tempdir::TempDir::new("libpython").unwrap();
    let temp_dir_path = temp_dir.path();

    let extension_modules = &resources.extension_modules;

    // We derive a custom Modules/config.c from the set of extension modules.
    // We need to do this because config.c defines the built-in extensions and
    // their initialization functions and the file generated by the source
    // distribution may not align with what we want.
    info!(
        logger,
        "deriving custom config.c from {} extension modules",
        extension_modules.len()
    );
    let config_c_source = make_config_c(&extension_modules);
    let config_c_path = out_dir.join("config.c");

    fs::write(&config_c_path, config_c_source.as_bytes()).expect("unable to write config.c");

    // We need to make all .h includes accessible.
    for (name, fs_path) in &dist.includes {
        let full = temp_dir_path.join(name);
        create_dir_all(full.parent().expect("parent directory")).expect("create include directory");
        fs::copy(fs_path, full).expect("unable to copy include file");
    }

    // TODO flags should come from parsed distribution config.
    info!(logger, "compiling custom config.c to object file");
    cc::Build::new()
        .out_dir(out_dir)
        .host(host)
        .target(target)
        .opt_level_str(opt_level)
        .file(config_c_path)
        .include(temp_dir_path)
        .define("NDEBUG", None)
        .define("Py_BUILD_CORE", None)
        .flag("-std=c99")
        .cargo_metadata(false)
        .compile("pyembeddedconfig");

    // Since we disabled cargo metadata lines above.
    cargo_metadata.push("cargo:rustc-link-lib=static=pyembeddedconfig".to_string());

    info!(logger, "resolving inputs for custom Python library...");
    let mut build = cc::Build::new();
    build.out_dir(out_dir);
    build.host(host);
    build.target(target);
    build.opt_level_str(opt_level);

    info!(
        logger,
        "adding {} object files required by Python core: {:#?}",
        dist.objs_core.len(),
        dist.objs_core.keys().map(|k| k.display()).collect_vec()
    );
    for (rel_path, fs_path) in &dist.objs_core {
        // TODO this is a bit hacky. Perhaps the distribution should advertise
        // which object file contains _PyImport_Inittab. Or perhaps we could
        // scan all the object files for this symbol and ignore it automatically?
        if rel_path.ends_with("Modules/config.o") {
            info!(
                logger,
                "ignoring config.o since it may conflict with our version"
            );
            continue;
        }

        let parent = temp_dir_path.join(rel_path.parent().unwrap());
        create_dir_all(parent).unwrap();

        let full = temp_dir_path.join(rel_path);
        fs::copy(fs_path, &full).expect("unable to copy object file");

        build.object(&full);
    }

    // For each extension module, extract and use its object file. We also
    // use this pass to collect the set of libraries that we need to link
    // against.
    let mut needed_libraries: BTreeSet<&str> = BTreeSet::new();
    let mut needed_frameworks: BTreeSet<&str> = BTreeSet::new();
    let mut needed_system_libraries: BTreeSet<&str> = BTreeSet::new();

    info!(
        logger,
        "resolving libraries required by core distribution..."
    );
    for entry in &dist.links_core {
        if entry.framework {
            info!(logger, "framework {} required by core", entry.name);
            needed_frameworks.insert(&entry.name);
        } else if entry.system {
            info!(logger, "system library {} required by core", entry.name);
            needed_system_libraries.insert(&entry.name);
        }
        // TODO handle static/dynamic libraries.
    }

    info!(
        logger,
        "resolving inputs for {} extension modules...",
        extension_modules.len()
    );
    for (name, em) in extension_modules {
        if em.builtin_default {
            continue;
        }

        info!(
            logger,
            "adding {} object files for {} extension module: {:#?}",
            em.object_paths.len(),
            name,
            em.object_paths
        );
        for path in &em.object_paths {
            build.object(path);
        }

        for entry in &em.links {
            if entry.framework {
                needed_frameworks.insert(&entry.name);
                info!(logger, "framework {} required by {}", entry.name, name);
            } else if entry.system {
                info!(logger, "system library {} required by {}", entry.name, name);
                needed_system_libraries.insert(&entry.name);
            } else if let Some(_lib) = &entry.static_path {
                needed_libraries.insert(&entry.name);
                info!(logger, "static library {} required by {}", entry.name, name);
            } else if let Some(_lib) = &entry.dynamic_path {
                needed_libraries.insert(&entry.name);
                info!(
                    logger,
                    "dynamic library {} required by {}", entry.name, name
                );
            }
        }
    }

    for library in needed_libraries {
        if OS_IGNORE_LIBRARIES.contains(&library) {
            continue;
        }

        // Otherwise find the library in the distribution. Extract it. And statically link against it.
        let fs_path = dist
            .libraries
            .get(library)
            .expect(&format!("unable to find library {}", library));
        info!(logger, "{}", fs_path.display());

        let library_path = out_dir.join(format!("lib{}.a", library));
        fs::copy(fs_path, library_path).expect("unable to copy library file");

        cargo_metadata.push(format!("cargo:rustc-link-lib=static={}", library))
    }

    for framework in needed_frameworks {
        cargo_metadata.push(format!("cargo:rustc-link-lib=framework={}", framework));
    }

    for lib in needed_system_libraries {
        cargo_metadata.push(format!("cargo:rustc-link-lib={}", lib));
    }

    // python3-sys uses #[link(name="pythonXY")] attributes heavily on Windows. Its
    // build.rs then remaps ``pythonXY`` to e.g. ``python37``. This causes Cargo to
    // link against ``python37.lib`` (or ``pythonXY.lib`` if the
    // ``rustc-link-lib=pythonXY:python{}{}`` line is missing, which is the case
    // in our invocation).
    //
    // We don't want the "real" libpython being linked. And this is a very real
    // possibility since the path to it could be in an environment variable
    // outside of our control!
    //
    // In addition, we can't naively remap ``pythonXY`` ourselves without adding
    // a ``#[link]`` to the crate.
    //
    // Our current workaround is to produce a ``pythonXY.lib`` file. This satisfies
    // the requirement of ``python3-sys`` that a ``pythonXY.lib`` file exists.

    info!(logger, "compiling libpythonXY...");
    build.compile("pythonXY");
    info!(logger, "libpythonXY created");

    LibpythonInfo {
        path: out_dir.join("libpythonXY.a"),
        cargo_metadata,
    }
}

/// Obtain the Rust source code to construct a PythonConfig instance.
pub fn derive_python_config(
    config: &Config,
    importlib_bootstrap_path: &PathBuf,
    importlib_bootstrap_external_path: &PathBuf,
    py_modules_path: &PathBuf,
) -> String {
    format!(
        "PythonConfig {{\n    \
         program_name: \"{}\".to_string(),\n    \
         standard_io_encoding: {},\n    \
         standard_io_errors: {},\n    \
         opt_level: {},\n    \
         use_custom_importlib: true,\n    \
         filesystem_importer: {},\n    \
         sys_paths: [{}].to_vec(),\n    \
         import_site: {},\n    \
         import_user_site: {},\n    \
         ignore_python_env: {},\n    \
         dont_write_bytecode: {},\n    \
         unbuffered_stdio: {},\n    \
         frozen_importlib_data: include_bytes!(\"{}\"),\n    \
         frozen_importlib_external_data: include_bytes!(\"{}\"),\n    \
         py_modules_data: include_bytes!(\"{}\"),\n    \
         argvb: false,\n    \
         raw_allocator: {},\n    \
         write_modules_directory_env: {},\n    \
         run: {},\n\
         }}",
        config.program_name,
        match &config.stdio_encoding_name {
            Some(value) => format_args!("Some(\"{}\")", value).to_string(),
            None => "None".to_owned(),
        },
        match &config.stdio_encoding_errors {
            Some(value) => format_args!("Some(\"{}\")", value).to_string(),
            None => "None".to_owned(),
        },
        config.optimize_level,
        config.filesystem_importer,
        &config
            .sys_paths
            .iter()
            .map(|p| "\"".to_owned() + p + "\".to_string()")
            .collect::<Vec<String>>()
            .join(", "),
        !config.no_site,
        !config.no_user_site_directory,
        config.ignore_environment,
        config.dont_write_bytecode,
        config.unbuffered_stdio,
        importlib_bootstrap_path.display(),
        importlib_bootstrap_external_path.display(),
        py_modules_path.display(),
        match config.raw_allocator {
            RawAllocator::Jemalloc => "PythonRawAllocator::Jemalloc",
            RawAllocator::Rust => "PythonRawAllocator::Rust",
            RawAllocator::System => "PythonRawAllocator::System",
        },
        match &config.write_modules_directory_env {
            Some(path) => "Some(\"".to_owned() + &path + "\".to_string())",
            _ => "None".to_owned(),
        },
        match config.run {
            RunMode::Noop => "PythonRunMode::None".to_owned(),
            RunMode::Repl => "PythonRunMode::Repl".to_owned(),
            RunMode::Module { ref module } => {
                "PythonRunMode::Module { module: \"".to_owned() + module + "\".to_string() }"
            }
            RunMode::Eval { ref code } => {
                "PythonRunMode::Eval { code: \"".to_owned() + code + "\".to_string() }"
            }
        },
    )
}

pub fn write_data_rs(path: &PathBuf, python_config_rs: &str) {
    let mut f = fs::File::create(&path).unwrap();

    f.write_all(b"use super::config::{PythonConfig, PythonRawAllocator, PythonRunMode};\n\n")
        .unwrap();

    // Ideally we would have a const struct, but we need to do some
    // dynamic allocations. Using a function avoids having to pull in a
    // dependency on lazy_static.
    let indented = python_config_rs
        .split('\n')
        .map(|line| "    ".to_owned() + line)
        .join("\n");

    f.write_fmt(format_args!(
        "/// Obtain the default Python configuration\n\
         ///\n\
         /// The crate is compiled with a default Python configuration embedded
         /// in the crate. This function will return an instance of that
         /// configuration.
         pub fn default_python_config() -> PythonConfig {{\n{}\n}}\n",
        indented
    ))
    .unwrap();
}

/// Defines files, etc to embed Python in a larger binary.
///
/// Instances are typically produced by processing a PyOxidizer config file.
#[derive(Debug)]
pub struct EmbeddedPythonConfig {
    /// Parsed TOML config.
    pub config: Config,

    /// Path to archive with source Python distribution.
    pub python_distribution_path: PathBuf,

    /// Path to frozen importlib._bootstrap bytecode.
    pub importlib_bootstrap_path: PathBuf,

    /// Path to frozen importlib._bootstrap_external bytecode.
    pub importlib_bootstrap_external_path: PathBuf,

    /// Path to file containing all known module names.
    pub module_names_path: PathBuf,

    /// Path to file containing packed Python module source data.
    pub py_modules_path: PathBuf,

    /// Path to library file containing Python.
    pub libpython_path: PathBuf,

    /// Lines that can be emitted from Cargo build scripts to describe this
    /// configuration.
    pub cargo_metadata: Vec<String>,

    /// Rust source code to instantiate a PythonConfig instance using this config.
    pub python_config_rs: String,
}

/// Derive build artifacts from a PyOxidizer config file.
///
/// This function reads a PyOxidizer config file and turns it into a set
/// of derived files that can power an embedded Python interpreter.
///
/// Artifacts will be written to ``out_dir``.
///
/// Returns a data structure describing the results.
pub fn process_config(
    logger: &slog::Logger,
    config_path: &Path,
    out_dir: &Path,
    host: &str,
    target: &str,
    opt_level: &str,
) -> EmbeddedPythonConfig {
    let mut cargo_metadata: Vec<String> = Vec::new();

    info!(logger, "processing config file {}", config_path.display());

    let mut fh = fs::File::open(config_path).unwrap();

    let mut config_data = Vec::new();
    fh.read_to_end(&mut config_data).unwrap();

    let config = match parse_config(&config_data, target) {
        Ok(v) => v,
        Err(message) => panic!(
            "error reading config {}: {}",
            config_path.display(),
            message
        ),
    };

    if let PythonDistribution::Local { local_path, .. } = &config.python_distribution {
        cargo_metadata.push(format!("cargo:rerun-if-changed={}", local_path));
    }

    // Obtain the configured Python distribution and parse it to a data structure.
    info!(logger, "resolving Python distribution...");
    let python_distribution_path = resolve_python_distribution_archive(&config, &out_dir);
    info!(
        logger,
        "Python distribution available at {}",
        python_distribution_path.display()
    );
    let mut fh = fs::File::open(&python_distribution_path).unwrap();
    let mut python_distribution_data = Vec::new();
    fh.read_to_end(&mut python_distribution_data).unwrap();
    let dist_cursor = Cursor::new(python_distribution_data);
    info!(logger, "reading data from Python distribution...");
    let dist = analyze_python_distribution_tar_zst(dist_cursor).unwrap();
    info!(logger, "distribution info: {:#?}", dist.as_minimal_info());

    // Produce the custom frozen importlib modules.
    info!(
        logger,
        "compiling custom importlib modules to support in-memory importing"
    );
    let importlib = derive_importlib(&dist);

    let importlib_bootstrap_path = Path::new(&out_dir).join("importlib_bootstrap");
    let mut fh = fs::File::create(&importlib_bootstrap_path).unwrap();
    fh.write_all(&importlib.bootstrap_bytecode).unwrap();

    let importlib_bootstrap_external_path =
        Path::new(&out_dir).join("importlib_bootstrap_external");
    let mut fh = fs::File::create(&importlib_bootstrap_external_path).unwrap();
    fh.write_all(&importlib.bootstrap_external_bytecode)
        .unwrap();

    info!(
        logger,
        "resolving Python resources (modules, extensions, resource data, etc)..."
    );
    let resources = resolve_python_resources(logger, &config, &dist);

    info!(
        logger,
        "resolved {} Python source modules: {:#?}",
        resources.module_sources.len(),
        resources.module_sources.keys()
    );
    info!(
        logger,
        "resolved {} Python bytecode modules: {:#?}",
        resources.module_bytecodes.len(),
        resources.module_bytecodes.keys()
    );
    info!(
        logger,
        "resolved {} unique Python modules: {:#?}",
        resources.all_modules.len(),
        resources.all_modules
    );

    let mut resource_count = 0;
    let mut resource_map = BTreeMap::new();
    for (package, entries) in &resources.resources {
        let mut names = BTreeSet::new();
        names.extend(entries.keys());
        resource_map.insert(package.clone(), names);
        resource_count += entries.len();
    }

    info!(
        logger,
        "resolved {} resource files across {} packages: {:#?}",
        resource_count,
        resources.resources.len(),
        resource_map
    );
    info!(
        logger,
        "resolved {} extension modules: {:#?}",
        resources.extension_modules.len(),
        resources.extension_modules.keys()
    );

    // Produce the packed data structures containing Python modules.
    // TODO there is tons of room to customize this behavior, including
    // reordering modules so the memory order matches import order.

    info!(logger, "writing packed Python module and resource data...");
    let module_names_path = Path::new(&out_dir).join("py-module-names");
    let py_modules_path = Path::new(&out_dir).join("py-modules");
    resources.write_blobs(&module_names_path, &py_modules_path);

    info!(
        logger,
        "{} bytes of Python module data written to {}",
        py_modules_path.metadata().unwrap().len(),
        py_modules_path.display()
    );
    info!(logger, "(Python resource files not yet supported)");

    // Produce a static library containing the Python bits we need.
    info!(
        logger,
        "generating custom link library containing Python..."
    );
    let libpython_info =
        link_libpython(logger, &dist, &resources, out_dir, host, target, opt_level);
    cargo_metadata.extend(libpython_info.cargo_metadata);

    for p in &resources.read_files {
        cargo_metadata.push(format!("cargo:rerun-if-changed={}", p.display()));
    }

    let python_config_rs = derive_python_config(
        &config,
        &importlib_bootstrap_path,
        &importlib_bootstrap_external_path,
        &py_modules_path,
    );

    let dest_path = Path::new(&out_dir).join("data.rs");
    write_data_rs(&dest_path, &python_config_rs);

    EmbeddedPythonConfig {
        config,
        python_distribution_path,
        importlib_bootstrap_path,
        importlib_bootstrap_external_path,
        module_names_path,
        py_modules_path,
        libpython_path: libpython_info.path,
        cargo_metadata,
        python_config_rs,
    }
}

/// Process a PyOxidizer config file and copy important artifacts to a directory.
///
/// ``build_dir`` holds state for building artifacts. It should be consistent between
/// invocations or else operations will be slow.
///
/// Important artifacts from ``build_dir`` are copied to ``out_dir``.
pub fn process_config_and_copy_artifacts(
    logger: &slog::Logger,
    config_path: &Path,
    build_dir: &Path,
    out_dir: &Path,
) -> EmbeddedPythonConfig {
    // TODO derive these more intelligently.
    let host = if cfg!(target_os = "linux") {
        "x86_64-unknown-linux-gnu"
    } else if cfg!(target_os = "windows") {
        "x86_64-pc-windows-msvc"
    } else if cfg!(target_os = "macos") {
        "x86_64-apple-darwin"
    } else {
        panic!("unable to resolve target for current binary (this is a known issue)");
    };

    let target = host;
    let opt_level = "0";

    create_dir_all(build_dir).expect("unable to create build directory");
    let build_dir = std::fs::canonicalize(build_dir).expect("unable to canonicalize build_dir");

    create_dir_all(out_dir).expect("unable to create output directory");
    let orig_out_dir = out_dir.to_path_buf();
    let out_dir = std::fs::canonicalize(out_dir).expect("unable to canonicalize out_dir");

    let embedded_config = process_config(logger, config_path, &build_dir, host, target, opt_level);

    let importlib_bootstrap_path = out_dir.join("importlib_bootstrap");
    let importlib_bootstrap_external_path = out_dir.join("importlib_bootstrap_external");
    let py_modules_path = out_dir.join("py-modules");
    let libpython_path = out_dir.join("libpythonXY.a");

    // It is possible to use the output directory as the build directory.
    if build_dir != out_dir {
        fs::copy(
            embedded_config.importlib_bootstrap_path,
            &importlib_bootstrap_path,
        )
        .expect("error copying file");
        fs::copy(
            embedded_config.importlib_bootstrap_external_path,
            &importlib_bootstrap_external_path,
        )
        .expect("error copying file");
        fs::copy(embedded_config.py_modules_path, &py_modules_path).expect("error copying file");
        fs::copy(embedded_config.libpython_path, &libpython_path).expect("error copying file");
    }

    let python_config_rs = derive_python_config(
        &embedded_config.config,
        &orig_out_dir.join("importlib_bootstrap"),
        &orig_out_dir.join("importlib_bootstrap_external"),
        &orig_out_dir.join("py-modules"),
    );

    EmbeddedPythonConfig {
        config: embedded_config.config,
        python_distribution_path: embedded_config.python_distribution_path,
        importlib_bootstrap_path,
        importlib_bootstrap_external_path,
        module_names_path: embedded_config.module_names_path,
        py_modules_path,
        libpython_path,
        cargo_metadata: embedded_config.cargo_metadata,
        python_config_rs,
    }
}

pub fn find_pyoxidizer_config_file(start_dir: &Path) -> Option<PathBuf> {
    for test_dir in start_dir.ancestors() {
        let candidate = test_dir.to_path_buf().join("pyoxidizer.toml");

        if candidate.exists() {
            return Some(candidate);
        }
    }

    None
}

/// Runs packaging/embedding from the context of a build script.
///
/// This function should be called by the build script for the package
/// that wishes to embed a Python interpreter/application. When called,
/// a PyOxidizer configuration file is found and read. The configuration
/// is then applied to the current build. This involves obtaining a
/// Python distribution to embed (possibly by downloading it from the Internet),
/// analyzing the contents of that distribution, extracting relevant files
/// from the distribution, compiling Python bytecode, and generating
/// resources required to build the ``pyembed`` crate/modules.
///
/// If everything works as planned, this whole process should be largely
/// invisible and the calling application will have an embedded Python
/// interpreter when it is built.
pub fn run_from_build(logger: &slog::Logger, build_script: &str) {
    // Adding our our rerun-if-changed lines will overwrite the default, so
    // we need to emit the build script name explicitly.
    println!("cargo:rerun-if-changed={}", build_script);

    println!("cargo:rerun-if-env-changed=PYOXIDIZER_CONFIG");

    let host = env::var("HOST").expect("HOST not defined");
    let target = env::var("TARGET").expect("TARGET not defined");
    let opt_level = env::var("OPT_LEVEL").expect("OPT_LEVEL not defined");

    let config_path = match env::var("PYOXIDIZER_CONFIG") {
        Ok(config_env) => {
            info!(
                logger,
                "using PyOxidizer config file from PYOXIDIZER_CONFIG: {}", config_env
            );
            PathBuf::from(config_env)
        }
        Err(_) => {
            let manifest_dir =
                env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not found");

            let path = find_pyoxidizer_config_file(&PathBuf::from(manifest_dir));

            if path.is_none() {
                panic!("Could not find PyOxidizer config file");
            }

            path.unwrap()
        }
    };

    if !config_path.exists() {
        panic!("PyOxidizer config file does not exist");
    }

    println!(
        "cargo:rerun-if-changed={}",
        config_path.to_str().expect("could not convert path to str")
    );

    let out_dir = env::var("OUT_DIR").unwrap();
    let out_dir_path = Path::new(&out_dir);

    for line in process_config(
        logger,
        &config_path,
        out_dir_path,
        &host,
        &target,
        &opt_level,
    )
    .cargo_metadata
    {
        println!("{}", line);
    }
}
