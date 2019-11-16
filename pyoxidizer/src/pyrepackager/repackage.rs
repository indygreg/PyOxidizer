// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use byteorder::{LittleEndian, WriteBytesExt};
use glob::glob as findglob;
use itertools::Itertools;
use lazy_static::lazy_static;
use slog::{info, warn};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::fs::create_dir_all;
use std::io::{BufRead, BufReader, Cursor, Error as IOError, Read, Write};
use std::path::{Path, PathBuf};

use super::bytecode::{BytecodeCompiler, CompileMode};
use super::config::{
    eval_starlark_config_file, Config, PythonDistribution, PythonPackaging, RawAllocator, RunMode,
    TerminfoResolution,
};
use super::dist::{
    analyze_python_distribution_tar_zst, resolve_python_distribution_archive, ExtensionModule,
    LicenseInfo, PythonDistributionInfo,
};
use super::packaging_rule::{
    packages_from_module_name, packages_from_module_names, resolve_python_packaging,
};
use super::resource::{
    AppRelativeResources, BuiltExtensionModule, PackagedModuleBytecode, PackagedModuleSource,
    PythonResource, ResourceAction, ResourceLocation,
};
use super::state::{BuildContext, PackagingState};

pub const PYTHON_IMPORTER: &[u8] = include_bytes!("memoryimporter.py");

lazy_static! {
    /// Libraries provided by the host that we can ignore in Python module library dependencies.
    ///
    /// Libraries in this data structure are not provided by the Python distribution.
    /// A library should only be in this data structure if it is universally distributed
    /// by the OS. It is assumed that all binaries produced for the target will link
    /// against these libraries by default.
    static ref OS_IGNORE_LIBRARIES: Vec<&'static str> = {
        let mut v = Vec::new();

        if cfg!(target_os = "linux") || cfg!(target_os = "macos") {
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

pub const HOST: &str = env!("HOST");

impl BuildContext {
    pub fn new(
        project_path: &Path,
        config_path: &Path,
        host: Option<&str>,
        target: &str,
        release: bool,
        force_artifacts_path: Option<&Path>,
        verbose: bool,
    ) -> Result<Self, String> {
        let config_parent_path = config_path
            .parent()
            .ok_or("could not resolve parent path of config".to_string())?;

        let host_triple = if let Some(v) = host {
            v.to_string()
        } else {
            HOST.to_string()
        };

        let config = eval_starlark_config_file(&config_path, target)?;

        let build_path = config.build_config.build_path.clone();

        // Build Rust artifacts into build path, not wherever Rust chooses.
        let target_base_path = build_path.join("target");

        let apps_base_path = build_path.join("apps");

        // This assumes we invoke as `cargo build --target`, otherwise we don't get the
        // target triple in the directory path unless cross compiling.
        let target_triple_base_path =
            target_base_path
                .join(target)
                .join(if release { "release" } else { "debug" });

        let app_name = config.build_config.application_name.clone();

        let exe_name = if target.contains("pc-windows") {
            format!("{}.exe", &app_name)
        } else {
            app_name.clone()
        };

        let app_target_path = target_triple_base_path.join(&app_name);

        let app_path = apps_base_path
            .join(&app_name)
            .join(target)
            .join(if release { "release" } else { "debug" });
        let app_exe_target_path = target_triple_base_path.join(&exe_name);
        let app_exe_path = app_path.join(&exe_name);

        // Artifacts path is:
        // 1. force_artifacts_path (if defined)
        // 2. A "pyoxidizer" directory in the target directory.
        let pyoxidizer_artifacts_path = match force_artifacts_path {
            Some(path) => path.to_path_buf(),
            None => target_triple_base_path.join("pyoxidizer"),
        };

        let distributions_path = build_path.join("distribution");

        let distribution_hash = match &config.python_distribution {
            PythonDistribution::Local { sha256, .. } => sha256,
            PythonDistribution::Url { sha256, .. } => sha256,
        };

        // Take the prefix so paths are shorter.
        let distribution_hash = &distribution_hash[0..12];

        let python_distribution_path =
            pyoxidizer_artifacts_path.join(format!("python.{}", distribution_hash));

        let cargo_toml_path = project_path.join("Cargo.toml");
        if !cargo_toml_path.exists() {
            return Err(format!("{} does not exist", cargo_toml_path.display()));
        }

        let cargo_toml_data = fs::read(&cargo_toml_path).or_else(|e| Err(e.to_string()))?;
        let cargo_config =
            cargo_toml::Manifest::from_slice(&cargo_toml_data).or_else(|e| Err(e.to_string()))?;

        Ok(BuildContext {
            project_path: project_path.to_path_buf(),
            config_path: config_path.to_path_buf(),
            config_parent_path: config_parent_path.to_path_buf(),
            config,
            cargo_config,
            verbose,
            build_path,
            app_name,
            app_path,
            app_exe_path,
            distributions_path,
            host_triple,
            target_triple: target.to_string(),
            release,
            target_base_path,
            target_triple_base_path,
            app_target_path,
            app_exe_target_path,
            pyoxidizer_artifacts_path,
            python_distribution_path,
            packaging_state: None,
        })
    }

    /// Obtain the PackagingState instance for this configuration.
    ///
    /// This basically reads the packaging_state.cbor file from the artifacts
    /// directory.
    pub fn get_packaging_state(&mut self) -> Result<PackagingState, String> {
        if self.packaging_state.is_none() {
            let path = self.pyoxidizer_artifacts_path.join("packaging_state.cbor");
            let fh = std::io::BufReader::new(
                std::fs::File::open(&path).or_else(|e| Err(e.to_string()))?,
            );

            let state: PackagingState =
                serde_cbor::from_reader(fh).or_else(|e| Err(e.to_string()))?;

            self.packaging_state = Some(state);
        }

        // Ideally we'd return a ref. But lifetimes and mutable borrows can get
        // tricky. So just stomach the clone() for now.
        Ok(self.packaging_state.clone().unwrap())
    }
}

/// Represents a single module's data record.
pub struct ModuleEntry {
    pub name: String,
    pub is_package: bool,
    pub source: Option<Vec<u8>>,
    pub bytecode: Option<Vec<u8>>,
}

/// Represents an ordered collection of module entries.
pub type ModuleEntries = Vec<ModuleEntry>;

/// Represents Python resources to embed in a binary.
#[derive(Debug)]
pub struct EmbeddedPythonResources {
    pub module_sources: BTreeMap<String, PackagedModuleSource>,
    pub module_bytecodes: BTreeMap<String, PackagedModuleBytecode>,
    pub all_modules: BTreeSet<String>,
    pub all_packages: BTreeSet<String>,
    pub resources: BTreeMap<String, BTreeMap<String, Vec<u8>>>,
    pub extension_modules: BTreeMap<String, ExtensionModule>,
    pub built_extension_modules: BTreeMap<String, BuiltExtensionModule>,
}

impl EmbeddedPythonResources {
    /// Obtain records for all modules in this resources collection.
    pub fn modules_records(&self) -> ModuleEntries {
        let mut records = ModuleEntries::new();

        for name in &self.all_modules {
            let source = self.module_sources.get(name);
            let bytecode = self.module_bytecodes.get(name);

            records.push(ModuleEntry {
                name: name.clone(),
                is_package: self.all_packages.contains(name),
                source: match source {
                    Some(value) => Some(value.source.clone()),
                    None => None,
                },
                bytecode: match bytecode {
                    Some(value) => Some(value.bytecode.clone()),
                    None => None,
                },
            });
        }

        records
    }

    pub fn write_blobs(
        &self,
        module_names_path: &PathBuf,
        modules_path: &PathBuf,
        resources_path: &PathBuf,
    ) {
        let mut fh = fs::File::create(module_names_path).expect("error creating file");
        for name in &self.all_modules {
            fh.write_all(name.as_bytes()).expect("failed to write");
            fh.write_all(b"\n").expect("failed to write");
        }

        let fh = fs::File::create(modules_path).unwrap();
        write_modules_entries(&fh, &self.modules_records()).unwrap();

        let fh = fs::File::create(resources_path).unwrap();
        write_resources_entries(&fh, &self.resources).unwrap();
    }

    pub fn embedded_extension_module_names(&self) -> BTreeSet<String> {
        let mut res = BTreeSet::new();

        for name in self.extension_modules.keys() {
            res.insert(name.clone());
        }
        for name in self.built_extension_modules.keys() {
            res.insert(name.clone());
        }

        res
    }
}

/// Represents resources to package with an application.
#[derive(Debug)]
pub struct PythonResources {
    /// Resources to be embedded in the binary.
    pub embedded: EmbeddedPythonResources,

    /// Resources to install in paths relative to the produced binary.
    pub app_relative: BTreeMap<String, AppRelativeResources>,

    /// Files that are read to resolve this data structure.
    pub read_files: Vec<PathBuf>,

    /// Path where to write license files.
    pub license_files_path: Option<String>,
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
            warn!(logger, "removing {}", key);
            m.remove(&key);
        }
    }
}

lazy_static! {
    static ref RE_CODING: regex::bytes::Regex =
        { regex::bytes::Regex::new(r"^[ \t\f]*#.*?coding[:=][ \t]*([-_.a-zA-Z0-9]+)").unwrap() };
}

/// Derive the source encoding from Python source code.
pub fn python_source_encoding(source: &[u8]) -> Vec<u8> {
    // Default source encoding is UTF-8. But per PEP 263, the first or second
    // line of source can match a regular expression to define a custom
    // encoding.
    let lines = source.split(|v| v == &b'\n');

    for (i, line) in lines.enumerate() {
        if i > 1 {
            break;
        }

        if let Some(m) = RE_CODING.find(line) {
            return m.as_bytes().to_vec();
        }
    }

    b"utf-8".to_vec()
}

struct BytecodeRequest {
    source: Vec<u8>,
    optimize_level: i32,
    is_package: bool,
}

/// Resolves a series of packaging rules to a final set of resources to package.
#[allow(clippy::cognitive_complexity)]
pub fn resolve_python_resources(
    logger: &slog::Logger,
    context: &BuildContext,
    dist: &PythonDistributionInfo,
) -> PythonResources {
    let packages = &context.config.python_packaging;

    // Since bytecode has a non-trivial cost to generate, our strategy is to accumulate
    // requests for bytecode then generate bytecode for the final set of inputs at the
    // end of processing. That way we don't generate bytecode only to throw it away later.

    let mut embedded_extension_modules: BTreeMap<String, ExtensionModule> = BTreeMap::new();
    let mut embedded_sources: BTreeMap<String, PackagedModuleSource> = BTreeMap::new();
    let mut embedded_bytecode_requests: BTreeMap<String, BytecodeRequest> = BTreeMap::new();
    let mut embedded_resources: BTreeMap<String, BTreeMap<String, Vec<u8>>> = BTreeMap::new();
    let mut embedded_built_extension_modules = BTreeMap::new();

    let mut app_relative: BTreeMap<String, AppRelativeResources> = BTreeMap::new();
    let mut app_relative_bytecode_requests: BTreeMap<String, BTreeMap<String, BytecodeRequest>> =
        BTreeMap::new();

    let mut read_files: Vec<PathBuf> = Vec::new();
    let mut license_files_path = None;

    for packaging in packages {
        warn!(logger, "processing packaging rule: {:?}", packaging);

        let verbose_rule = if let PythonPackaging::Stdlib(_) = packaging {
            true
        } else {
            false
        };

        for entry in resolve_python_packaging(logger, context, packaging, dist, context.verbose) {
            match (entry.action, entry.location, entry.resource) {
                (
                    ResourceAction::Add,
                    ResourceLocation::Embedded,
                    PythonResource::ExtensionModule { name, module },
                ) => {
                    warn!(logger, "adding embedded extension module: {}", name);
                    embedded_extension_modules.insert(name, module);
                }
                (
                    ResourceAction::Add,
                    ResourceLocation::AppRelative { .. },
                    PythonResource::ExtensionModule { .. },
                ) => {
                    panic!("should not have gotten an app-relative extension module");
                }
                (
                    ResourceAction::Remove,
                    ResourceLocation::Embedded,
                    PythonResource::ExtensionModule { name, .. },
                ) => {
                    warn!(logger, "removing embedded extension module: {}", name);
                    embedded_extension_modules.remove(&name);
                }
                (
                    ResourceAction::Add,
                    ResourceLocation::Embedded,
                    PythonResource::ModuleSource {
                        name,
                        source,
                        is_package,
                    },
                ) => {
                    if verbose_rule {
                        info!(logger, "adding embedded module source: {}", name);
                    } else {
                        warn!(logger, "adding embedded module source: {}", name);
                    }
                    embedded_sources
                        .insert(name.clone(), PackagedModuleSource { source, is_package });
                }
                (
                    ResourceAction::Add,
                    ResourceLocation::AppRelative { path },
                    PythonResource::ModuleSource {
                        name,
                        source,
                        is_package,
                    },
                ) => {
                    if verbose_rule {
                        info!(
                            logger,
                            "adding app-relative module source to {}: {}", path, name
                        );
                    } else {
                        warn!(
                            logger,
                            "adding app-relative module source to {}: {}", path, name
                        );
                    }
                    if !app_relative.contains_key(&path) {
                        app_relative.insert(path.clone(), AppRelativeResources::default());
                    }

                    app_relative
                        .get_mut(&path)
                        .unwrap()
                        .module_sources
                        .insert(name.clone(), PackagedModuleSource { source, is_package });
                }
                (
                    ResourceAction::Remove,
                    ResourceLocation::Embedded,
                    PythonResource::ModuleSource { name, .. },
                ) => {
                    warn!(logger, "removing embedded module source: {}", name);
                    embedded_sources.remove(&name);
                }
                (
                    ResourceAction::Add,
                    ResourceLocation::Embedded,
                    PythonResource::ModuleBytecode {
                        name,
                        source,
                        optimize_level,
                        is_package,
                    },
                ) => {
                    if verbose_rule {
                        info!(logger, "adding embedded module bytecode: {}", name);
                    } else {
                        warn!(logger, "adding embedded module bytecode: {}", name);
                    }
                    embedded_bytecode_requests.insert(
                        name.clone(),
                        BytecodeRequest {
                            source,
                            optimize_level,
                            is_package,
                        },
                    );
                }
                (
                    ResourceAction::Add,
                    ResourceLocation::AppRelative { path },
                    PythonResource::ModuleBytecode {
                        name,
                        source,
                        optimize_level,
                        is_package,
                    },
                ) => {
                    if verbose_rule {
                        info!(
                            logger,
                            "adding app-relative module bytecode to {}: {}", path, name
                        );
                    } else {
                        warn!(
                            logger,
                            "adding app-relative module bytecode to {}: {}", path, name
                        );
                    }

                    if !app_relative_bytecode_requests.contains_key(&path) {
                        app_relative_bytecode_requests.insert(path.clone(), BTreeMap::new());
                    }

                    app_relative_bytecode_requests
                        .get_mut(&path)
                        .unwrap()
                        .insert(
                            name.clone(),
                            BytecodeRequest {
                                source,
                                optimize_level,
                                is_package,
                            },
                        );
                }
                (
                    ResourceAction::Remove,
                    ResourceLocation::Embedded,
                    PythonResource::ModuleBytecode { name, .. },
                ) => {
                    warn!(logger, "removing embedded module bytecode: {}", name);
                    embedded_bytecode_requests.remove(&name);
                }
                (
                    ResourceAction::Add,
                    ResourceLocation::Embedded,
                    PythonResource::Resource {
                        package,
                        name,
                        data,
                    },
                ) => {
                    warn!(logger, "adding embedded resource: {} / {}", package, name);

                    if !embedded_resources.contains_key(&package) {
                        embedded_resources.insert(package.clone(), BTreeMap::new());
                    }

                    embedded_resources
                        .get_mut(&package)
                        .unwrap()
                        .insert(name, data);
                }
                (
                    ResourceAction::Add,
                    ResourceLocation::AppRelative { path },
                    PythonResource::Resource {
                        package,
                        name,
                        data,
                    },
                ) => {
                    warn!(logger, "adding app-relative resource to {}: {}", path, name);

                    if !app_relative.contains_key(&path) {
                        app_relative.insert(path.clone(), AppRelativeResources::default());
                    }

                    let app_relative = app_relative.get_mut(&path).unwrap();

                    if !app_relative.resources.contains_key(&package) {
                        app_relative
                            .resources
                            .insert(package.clone(), BTreeMap::new());
                    }

                    app_relative
                        .resources
                        .get_mut(&package)
                        .unwrap()
                        .insert(name, data);
                }
                (
                    ResourceAction::Remove,
                    ResourceLocation::Embedded,
                    PythonResource::Resource { name, .. },
                ) => {
                    warn!(logger, "removing embedded resource: {}", name);
                    embedded_resources.remove(&name);
                }
                (ResourceAction::Remove, ResourceLocation::AppRelative { .. }, _) => {
                    panic!("should not have gotten an action to remove an app-relative resource");
                }
                (
                    ResourceAction::Add,
                    ResourceLocation::Embedded,
                    PythonResource::BuiltExtensionModule(em),
                ) => {
                    warn!(
                        logger,
                        "adding embedded built extension module: {}", em.name
                    );

                    embedded_built_extension_modules.insert(em.name.clone(), em.clone());
                }
                (
                    ResourceAction::Add,
                    ResourceLocation::AppRelative { path },
                    PythonResource::BuiltExtensionModule(em),
                ) => {
                    warn!(
                        logger,
                        "adding app-relative built extension module {} to {}", em.name, path
                    );
                    warn!(
                        logger,
                        "WARNING: incomplete support for app-relative built extension modules: adding a built-in");
                    embedded_built_extension_modules.insert(em.name.clone(), em.clone());
                }
                (
                    ResourceAction::Remove,
                    ResourceLocation::Embedded,
                    PythonResource::BuiltExtensionModule(em),
                ) => {
                    warn!(
                        logger,
                        "removing embedded built extension module {}", em.name
                    );
                    embedded_built_extension_modules.remove(&em.name);
                }
            }
        }

        if let PythonPackaging::WriteLicenseFiles(rule) = packaging {
            license_files_path = Some(rule.path.clone());
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

            warn!(
                logger,
                "filtering embedded extension modules from {:?}", packaging
            );
            filter_btreemap(logger, &mut embedded_extension_modules, &include_names);
            warn!(
                logger,
                "filtering embedded module sources from {:?}", packaging
            );
            filter_btreemap(logger, &mut embedded_sources, &include_names);
            warn!(
                logger,
                "filtering app-relative module sources from {:?}", packaging
            );
            for value in app_relative.values_mut() {
                filter_btreemap(logger, &mut value.module_sources, &include_names);
            }
            warn!(
                logger,
                "filtering embedded module bytecode from {:?}", packaging
            );
            filter_btreemap(logger, &mut embedded_bytecode_requests, &include_names);
            warn!(
                logger,
                "filtering app-relative module bytecode from {:?}", packaging
            );
            for value in app_relative_bytecode_requests.values_mut() {
                filter_btreemap(logger, value, &include_names);
            }
            warn!(logger, "filtering embedded resources from {:?}", packaging);
            filter_btreemap(logger, &mut embedded_resources, &include_names);
            warn!(
                logger,
                "filtering app-relative resources from {:?}", packaging
            );
            for value in app_relative.values_mut() {
                filter_btreemap(logger, &mut value.resources, &include_names);
            }
            warn!(
                logger,
                "filtering embedded built extension modules from {:?}", packaging
            );
            filter_btreemap(
                logger,
                &mut embedded_built_extension_modules,
                &include_names,
            );
        }
    }

    // Add empty modules for missing parent packages. This could happen if there are
    // namespace packages, for example.
    let mut missing_packages = BTreeSet::new();
    for name in embedded_bytecode_requests.keys() {
        for package in packages_from_module_name(&name) {
            if !embedded_bytecode_requests.contains_key(&package) {
                missing_packages.insert(package.clone());
            }
        }
    }

    for package in missing_packages {
        warn!(
            logger,
            "adding empty module for missing package {}", package
        );
        embedded_bytecode_requests.insert(
            package.clone(),
            BytecodeRequest {
                source: Vec::new(),
                optimize_level: 0,
                is_package: true,
            },
        );
    }

    // Add required extension modules, as some don't show up in the modules list
    // and may have been filtered or not added in the first place.
    for (name, variants) in &dist.extension_modules {
        let em = &variants[0];

        if (em.builtin_default || em.required) && !embedded_extension_modules.contains_key(name) {
            warn!(logger, "adding required embedded extension module {}", name);
            embedded_extension_modules.insert(name.clone(), em.clone());
        }
    }

    // Remove extension modules that have problems.
    for e in OS_IGNORE_EXTENSIONS.as_slice() {
        warn!(
            logger,
            "removing extension module due to incompatibility: {}", e
        );
        embedded_extension_modules.remove(&String::from(*e));
    }

    // Audit Python source for __file__, which could be problematic.
    let mut file_seen = false;

    for (name, request) in &embedded_bytecode_requests {
        // We can't just look for b"__file__ because the source file may be in
        // encodings like UTF-16. So we need to decode to Unicode first then look for
        // the code points.
        let encoding = python_source_encoding(&request.source);

        let encoder = match encoding_rs::Encoding::for_label(&encoding) {
            Some(encoder) => encoder,
            None => encoding_rs::UTF_8,
        };

        let (source, ..) = encoder.decode(&request.source);

        if source.contains("__file__") {
            warn!(logger, "warning: {} contains __file__", name);
            file_seen = true;
        }
    }

    if file_seen {
        warn!(logger, "__file__ was encountered in some modules; PyOxidizer does not set __file__ and this may create problems at run-time; see https://github.com/indygreg/PyOxidizer/issues/69 for more");
    }

    let mut embedded_bytecodes: BTreeMap<String, PackagedModuleBytecode> = BTreeMap::new();

    {
        let mut compiler = bytecode_compiler(&dist);

        for (name, request) in embedded_bytecode_requests {
            let bytecode = match compiler.compile(
                &request.source,
                &name,
                request.optimize_level,
                CompileMode::Bytecode,
            ) {
                Ok(res) => res,
                Err(msg) => panic!("error compiling bytecode for {}: {}", name, msg),
            };

            embedded_bytecodes.insert(
                name.clone(),
                PackagedModuleBytecode {
                    bytecode,
                    is_package: request.is_package,
                },
            );
        }
    }

    // Compile app-relative bytecode requests.
    {
        let mut compiler = bytecode_compiler(&dist);

        for (path, requests) in app_relative_bytecode_requests {
            if !app_relative.contains_key(&path) {
                app_relative.insert(path.clone(), AppRelativeResources::default());
            }

            let app_relative = app_relative.get_mut(&path).unwrap();

            for (name, request) in requests {
                let bytecode = match compiler.compile(
                    &request.source,
                    &name,
                    request.optimize_level,
                    // Bytecode in app-relative directories should never be mutated. So we
                    // shouldn't need to verify its hash at run-time.
                    // TODO consider making this configurable.
                    CompileMode::PycUncheckedHash,
                ) {
                    Ok(res) => res,
                    Err(msg) => panic!("error compiling bytecode for {}: {}", name, msg),
                };

                app_relative.module_bytecodes.insert(
                    name.clone(),
                    PackagedModuleBytecode {
                        bytecode,
                        is_package: request.is_package,
                    },
                );
            }
        }
    }

    let mut all_embedded_modules = BTreeSet::new();
    let mut annotated_package_names = BTreeSet::new();

    for (name, source) in &embedded_sources {
        all_embedded_modules.insert(name.clone());

        if source.is_package {
            annotated_package_names.insert(name.clone());
        }
    }
    for (name, bytecode) in &embedded_bytecodes {
        all_embedded_modules.insert(name.clone());

        if bytecode.is_package {
            annotated_package_names.insert(name.clone());
        }
    }

    for (name, extension) in &embedded_built_extension_modules {
        all_embedded_modules.insert(name.clone());

        if extension.is_package {
            annotated_package_names.insert(name.clone());
        }
    }

    let derived_package_names = packages_from_module_names(all_embedded_modules.iter().cloned());

    let mut all_embedded_package_names = annotated_package_names.clone();
    for package in derived_package_names {
        if !all_embedded_package_names.contains(&package) {
            warn!(
                logger,
                "package {} not initially detected as such; is package detection buggy?", package
            );
            all_embedded_package_names.insert(package);
        }
    }

    // Prune resource files that belong to packages that don't have a corresponding
    // Python module package, as they won't be loadable by our custom importer.
    let embedded_resources = embedded_resources
        .iter()
        .filter_map(|(package, values)| {
            if !all_embedded_package_names.contains(package) {
                warn!(
                    logger,
                    "package {} does not exist; excluding resources: {:?}",
                    package,
                    values.keys()
                );
                None
            } else {
                Some((package.clone(), values.clone()))
            }
        })
        .collect();

    PythonResources {
        embedded: EmbeddedPythonResources {
            module_sources: embedded_sources,
            module_bytecodes: embedded_bytecodes,
            all_modules: all_embedded_modules,
            all_packages: all_embedded_package_names,
            resources: embedded_resources,
            extension_modules: embedded_extension_modules,
            built_extension_modules: embedded_built_extension_modules,
        },
        app_relative,
        read_files,
        license_files_path,
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
        .compile(&bootstrap_source, module_name, 0, CompileMode::Bytecode)
        .expect("error compiling bytecode");

    let mut bootstrap_external_source =
        fs::read(&mod_bootstrap_external_path).expect("unable to read bootstrap_external source");
    bootstrap_external_source.extend("\n# END OF importlib/_bootstrap_external.py\n\n".bytes());
    bootstrap_external_source.extend(PYTHON_IMPORTER);
    let module_name = "<frozen importlib._bootstrap_external>";
    let bootstrap_external_bytecode = compiler
        .compile(
            &bootstrap_external_source,
            module_name,
            0,
            CompileMode::Bytecode,
        )
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

        let mut flags = 0;
        if entry.is_package {
            flags |= 1;
        }

        dest.write_u32::<LittleEndian>(flags)?;
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

/// Serializes resource data to a writer.
///
/// See the documentation in the `pyembed` crate for the data format.
pub fn write_resources_entries<W: Write>(
    mut dest: W,
    entries: &BTreeMap<String, BTreeMap<String, Vec<u8>>>,
) -> std::io::Result<()> {
    dest.write_u32::<LittleEndian>(entries.len() as u32)?;

    // All the numeric index data is written in pass 1.
    for (package, resources) in entries {
        let package_bytes = package.as_bytes();

        dest.write_u32::<LittleEndian>(package_bytes.len() as u32)?;
        dest.write_u32::<LittleEndian>(resources.len() as u32)?;

        for (name, value) in resources {
            let name_bytes = name.as_bytes();

            dest.write_u32::<LittleEndian>(name_bytes.len() as u32)?;
            dest.write_u32::<LittleEndian>(value.len() as u32)?;
        }
    }

    // All the name strings are written in pass 2.
    for (package, resources) in entries {
        dest.write_all(package.as_bytes())?;

        for name in resources.keys() {
            dest.write_all(name.as_bytes())?;
        }
    }

    // All the resource data is written in pass 3.
    for resources in entries.values() {
        for value in resources.values() {
            dest.write_all(value.as_slice())?;
        }
    }

    Ok(())
}

/// Produce the content of the config.c file containing built-in extensions.
fn make_config_c(
    extension_modules: &BTreeMap<String, ExtensionModule>,
    built_extension_modules: &BTreeMap<String, BuiltExtensionModule>,
) -> String {
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

    for em in built_extension_modules.values() {
        lines.push(format!("extern PyObject* {}(void);", em.init_fn));
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

    for em in built_extension_modules.values() {
        lines.push(format!("{{\"{}\", {}}},", em.name, em.init_fn));
    }

    lines.push(String::from("{0, 0}"));
    lines.push(String::from("};"));

    lines.join("\n")
}

#[derive(Debug)]
pub struct LibpythonInfo {
    path: PathBuf,
    cargo_metadata: Vec<String>,
    license_infos: BTreeMap<String, Vec<LicenseInfo>>,
}

/// Create a static libpython from a Python distribution.
///
/// Returns a vector of cargo: lines that can be printed in build scripts.
#[allow(clippy::cognitive_complexity)]
pub fn link_libpython(
    logger: &slog::Logger,
    dist: &PythonDistributionInfo,
    resources: &EmbeddedPythonResources,
    out_dir: &Path,
    host: &str,
    target: &str,
    opt_level: &str,
) -> LibpythonInfo {
    let mut cargo_metadata: Vec<String> = Vec::new();

    let temp_dir = tempdir::TempDir::new("libpython").unwrap();
    let temp_dir_path = temp_dir.path();

    let extension_modules = &resources.extension_modules;
    let built_extension_modules = &resources.built_extension_modules;

    // Sometimes we have canonicalized paths. These can break cc/cl.exe when they
    // are \\?\ paths on Windows for some reason. We hack around this by doing
    // operations in the temp directory and copying files to their final resting
    // place.

    // We derive a custom Modules/config.c from the set of extension modules.
    // We need to do this because config.c defines the built-in extensions and
    // their initialization functions and the file generated by the source
    // distribution may not align with what we want.
    warn!(
        logger,
        "deriving custom config.c from {} extension modules",
        extension_modules.len() + built_extension_modules.len()
    );
    let config_c_source = make_config_c(&extension_modules, &built_extension_modules);
    let config_c_path = out_dir.join("config.c");
    let config_c_temp_path = temp_dir_path.join("config.c");

    fs::write(&config_c_path, config_c_source.as_bytes()).expect("unable to write config.c");
    fs::write(&config_c_temp_path, config_c_source.as_bytes()).expect("unable to write config.c");

    // We need to make all .h includes accessible.
    for (name, fs_path) in &dist.includes {
        let full = temp_dir_path.join(name);
        create_dir_all(full.parent().expect("parent directory")).expect("create include directory");
        fs::copy(fs_path, full).expect("unable to copy include file");
    }

    // TODO flags should come from parsed distribution config.
    warn!(logger, "compiling custom config.c to object file");
    cc::Build::new()
        .out_dir(out_dir)
        .host(host)
        .target(target)
        .opt_level_str(opt_level)
        .file(config_c_temp_path)
        .include(temp_dir_path)
        .define("NDEBUG", None)
        .define("Py_BUILD_CORE", None)
        .flag("-std=c99")
        .cargo_metadata(false)
        .compile("pyembeddedconfig");

    // Since we disabled cargo metadata lines above.
    cargo_metadata.push("cargo:rustc-link-lib=static=pyembeddedconfig".to_string());

    warn!(logger, "resolving inputs for custom Python library...");
    let mut build = cc::Build::new();
    build.out_dir(out_dir);
    build.host(host);
    build.target(target);
    build.opt_level_str(opt_level);
    // We handle this ourselves.
    build.cargo_metadata(false);

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
            warn!(
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
    let mut needed_libraries_external: BTreeSet<&str> = BTreeSet::new();

    warn!(
        logger,
        "resolving libraries required by core distribution..."
    );
    for entry in &dist.links_core {
        if entry.framework {
            warn!(logger, "framework {} required by core", entry.name);
            needed_frameworks.insert(&entry.name);
        } else if entry.system {
            warn!(logger, "system library {} required by core", entry.name);
            needed_system_libraries.insert(&entry.name);
        }
        // TODO handle static/dynamic libraries.
    }

    warn!(
        logger,
        "resolving inputs for {} extension modules...",
        extension_modules.len() + built_extension_modules.len()
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
                warn!(logger, "framework {} required by {}", entry.name, name);
            } else if entry.system {
                warn!(logger, "system library {} required by {}", entry.name, name);
                needed_system_libraries.insert(&entry.name);
            } else if let Some(_lib) = &entry.static_path {
                needed_libraries.insert(&entry.name);
                warn!(logger, "static library {} required by {}", entry.name, name);
            } else if let Some(_lib) = &entry.dynamic_path {
                needed_libraries.insert(&entry.name);
                warn!(
                    logger,
                    "dynamic library {} required by {}", entry.name, name
                );
            }
        }
    }

    warn!(
        logger,
        "resolving inputs for {} built extension modules...",
        built_extension_modules.len()
    );

    for (name, em) in built_extension_modules {
        info!(
            logger,
            "adding {} object files for {} built extension module",
            em.object_file_data.len(),
            name
        );
        for (i, object_data) in em.object_file_data.iter().enumerate() {
            let out_path = temp_dir_path.join(format!("{}.{}.o", name, i));

            fs::write(&out_path, object_data).expect("unable to write object file");
            build.object(&out_path);
        }

        for library in &em.libraries {
            warn!(logger, "library {} required by {}", library, name);
            needed_libraries_external.insert(&library);
        }

        // TODO do something with library_dirs.
    }

    // Windows requires dynamic linking against msvcrt. Ensure that happens.
    // TODO this workaround feels like a bug in the Python distribution not
    // advertising a dependency on the CRT linkage type. Consider adding this
    // to the distribution metadata.
    if match target {
        "i686-pc-windows-msvc" => true,
        "x86_64-pc-windows-msvc" => true,
        _ => false,
    } {
        needed_system_libraries.insert("msvcrt");
    }

    for library in needed_libraries.iter() {
        if OS_IGNORE_LIBRARIES.contains(&library) {
            continue;
        }

        // Otherwise find the library in the distribution. Extract it. And statically link against it.
        let fs_path = dist
            .libraries
            .get(*library)
            .unwrap_or_else(|| panic!("unable to find library {}", library));
        warn!(logger, "{}", fs_path.display());

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

    for lib in needed_libraries_external {
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

    warn!(logger, "compiling libpythonXY...");
    build.compile("pythonXY");
    warn!(logger, "libpythonXY created");

    cargo_metadata.push("cargo:rustc-link-lib=static=pythonXY".to_string());
    cargo_metadata.push(format!(
        "cargo:rustc-link-search=native={}",
        out_dir.display()
    ));

    let mut license_infos = BTreeMap::new();

    if let Some(li) = dist.license_infos.get("python") {
        license_infos.insert("python".to_string(), li.clone());
    }

    for name in extension_modules.keys() {
        if let Some(li) = dist.license_infos.get(name) {
            license_infos.insert(name.clone(), li.clone());
        }
    }

    LibpythonInfo {
        path: out_dir.join("libpythonXY.a"),
        cargo_metadata,
        license_infos,
    }
}

/// Obtain the Rust source code to construct a PythonConfig instance.
pub fn derive_python_config(
    config: &Config,
    importlib_bootstrap_path: &PathBuf,
    importlib_bootstrap_external_path: &PathBuf,
    py_modules_path: &PathBuf,
    py_resources_path: &PathBuf,
) -> String {
    let embedded = &config.embedded_python_config;

    format!(
        "PythonConfig {{\n    \
         standard_io_encoding: {},\n    \
         standard_io_errors: {},\n    \
         opt_level: {},\n    \
         use_custom_importlib: true,\n    \
         filesystem_importer: {},\n    \
         sys_paths: [{}].to_vec(),\n    \
         bytes_warning: {},\n    \
         import_site: {},\n    \
         import_user_site: {},\n    \
         ignore_python_env: {},\n    \
         inspect: {},\n    \
         interactive: {},\n    \
         dont_write_bytecode: {},\n    \
         unbuffered_stdio: {},\n    \
         parser_debug: {},\n    \
         quiet: {},\n    \
         verbose: {},\n    \
         frozen_importlib_data: include_bytes!(r#\"{}\"#),\n    \
         frozen_importlib_external_data: include_bytes!(r#\"{}\"#),\n    \
         py_modules_data: include_bytes!(r#\"{}\"#),\n    \
         py_resources_data: include_bytes!(r#\"{}\"#),\n    \
         extra_extension_modules: vec![],\n    \
         argvb: false,\n    \
         sys_frozen: {},\n    \
         sys_meipass: {},\n    \
         raw_allocator: {},\n    \
         terminfo_resolution: {},\n    \
         write_modules_directory_env: {},\n    \
         run: {},\n\
         }}",
        match &embedded.stdio_encoding_name {
            Some(value) => format_args!("Some(\"{}\")", value).to_string(),
            None => "None".to_owned(),
        },
        match &embedded.stdio_encoding_errors {
            Some(value) => format_args!("Some(\"{}\")", value).to_string(),
            None => "None".to_owned(),
        },
        embedded.optimize_level,
        embedded.filesystem_importer,
        &embedded
            .sys_paths
            .iter()
            .map(|p| "\"".to_owned() + p + "\".to_string()")
            .collect::<Vec<String>>()
            .join(", "),
        embedded.bytes_warning,
        !embedded.no_site,
        !embedded.no_user_site_directory,
        embedded.ignore_environment,
        embedded.inspect,
        embedded.interactive,
        embedded.dont_write_bytecode,
        embedded.unbuffered_stdio,
        embedded.parser_debug,
        embedded.quiet,
        embedded.verbose,
        importlib_bootstrap_path.display(),
        importlib_bootstrap_external_path.display(),
        py_modules_path.display(),
        py_resources_path.display(),
        embedded.sys_frozen,
        embedded.sys_meipass,
        match embedded.raw_allocator {
            RawAllocator::Jemalloc => "PythonRawAllocator::Jemalloc",
            RawAllocator::Rust => "PythonRawAllocator::Rust",
            RawAllocator::System => "PythonRawAllocator::System",
        },
        match embedded.terminfo_resolution {
            TerminfoResolution::Dynamic => "TerminfoResolution::Dynamic".to_string(),
            TerminfoResolution::None => "TerminfoResolution::None".to_string(),
            TerminfoResolution::Static(ref v) => {
                format!("TerminfoResolution::Static(r###\"{}\"###", v)
            }
        },
        match &embedded.write_modules_directory_env {
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
                "PythonRunMode::Eval { code: r###\"".to_owned() + code + "\"###.to_string() }"
            }
        },
    )
}

pub fn write_data_rs(path: &PathBuf, python_config_rs: &str) {
    let mut f = fs::File::create(&path).unwrap();

    f.write_all(b"use super::config::{PythonConfig, PythonRawAllocator, PythonRunMode, TerminfoResolution};\n\n")
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

/// Install all app-relative files next to the generated binary.
fn install_app_relative(
    logger: &slog::Logger,
    context: &BuildContext,
    path: &str,
    app_relative: &AppRelativeResources,
) -> Result<(), String> {
    let dest_path = context.app_exe_path.parent().unwrap().join(path);

    create_dir_all(&dest_path).or_else(|_| Err("could not create app-relative path"))?;

    warn!(
        logger,
        "installing {} app-relative Python source modules to {}",
        app_relative.module_sources.len(),
        dest_path.display(),
    );

    for (module_name, module_source) in &app_relative.module_sources {
        // foo.bar -> foo/bar
        let mut module_path = dest_path.clone();
        module_path.extend(module_name.split('.'));

        // Packages need to get normalized to /__init__.py.
        if module_source.is_package {
            module_path.push("__init__");
        }

        module_path.set_file_name(format!(
            "{}.py",
            module_path.file_name().unwrap().to_string_lossy()
        ));

        info!(
            logger,
            "installing Python module {} to {}",
            module_name,
            module_path.display()
        );

        let parent_dir = module_path.parent().unwrap();
        create_dir_all(&parent_dir).or_else(|_| {
            Err(format!(
                "failed to create directory {}",
                parent_dir.display()
            ))
        })?;

        fs::write(&module_path, &module_source.source)
            .or_else(|_| Err(format!("failed to write {}", module_path.display())))?;
    }

    warn!(
        logger,
        "resolved {} app-relative Python bytecode modules in {}",
        app_relative.module_bytecodes.len(),
        path,
    );

    for (module_name, module_bytecode) in &app_relative.module_bytecodes {
        // foo.bar -> foo/bar
        let mut module_path = dest_path.clone();

        // .pyc files go into a __pycache__ directory next to the package.

        // __init__ is special.
        if module_bytecode.is_package {
            module_path.extend(module_name.split('.'));
            module_path.push("__pycache__");
            module_path.push("__init__");
        } else if module_name.contains('.') {
            let parts: Vec<&str> = module_name.split('.').collect();

            module_path.extend(parts[0..parts.len() - 1].to_vec());
            module_path.push("__pycache__");
            module_path.push(parts[parts.len() - 1].to_string());
        } else {
            module_path.push("__pycache__");
            module_path.push(module_name);
        }

        module_path.set_file_name(format!(
            // TODO determine string from Python distribution in use.
            "{}.cpython-37.pyc",
            module_path.file_name().unwrap().to_string_lossy()
        ));

        info!(
            logger,
            "installing Python module bytecode {} to {}",
            module_name,
            module_path.display()
        );

        let parent_dir = module_path.parent().unwrap();
        create_dir_all(&parent_dir).or_else(|_| {
            Err(format!(
                "failed to create directory {}",
                parent_dir.display()
            ))
        })?;

        fs::write(&module_path, &module_bytecode.bytecode)
            .or_else(|_| Err(format!("failed to write {}", module_path.display())))?;
    }

    let mut resource_count = 0;
    let mut resource_map = BTreeMap::new();
    for (package, entries) in &app_relative.resources {
        let mut names = BTreeSet::new();
        names.extend(entries.keys());
        resource_map.insert(package.clone(), names);
        resource_count += entries.len();
    }

    warn!(
        logger,
        "resolved {} app-relative resource files across {} packages",
        resource_count,
        app_relative.resources.len(),
    );

    for (package, entries) in &app_relative.resources {
        let package_path = dest_path.join(package);

        warn!(
            logger,
            "installing {} app-relative resource files to {}:{}",
            entries.len(),
            path,
            package,
        );

        for (name, data) in entries {
            let dest_path = package_path.join(name);

            info!(
                logger,
                "installing app-relative resource {}:{} to {}",
                package,
                name,
                dest_path.display()
            );

            create_dir_all(dest_path.parent().unwrap()).or_else(|e| Err(e.to_string()))?;

            fs::write(&dest_path, data)
                .or_else(|_| Err(format!("failed to write {}", dest_path.display())))?;
        }
    }

    Ok(())
}

/// Package a built Rust project into its packaging directory.
///
/// This will delete all content in the application's package directory.
pub fn package_project(logger: &slog::Logger, context: &mut BuildContext) -> Result<(), String> {
    warn!(
        logger,
        "packaging application into {}",
        context.app_path.display()
    );

    if context.app_path.exists() {
        warn!(logger, "purging {}", context.app_path.display());
        std::fs::remove_dir_all(&context.app_path).or_else(|e| Err(e.to_string()))?;
    }

    create_dir_all(&context.app_path).or_else(|e| Err(e.to_string()))?;

    warn!(
        logger,
        "copying {} to {}",
        context.app_exe_target_path.display(),
        context.app_exe_path.display()
    );
    std::fs::copy(&context.app_exe_target_path, &context.app_exe_path)
        .or_else(|_| Err("failed to copy built application"))?;

    warn!(logger, "resolving packaging state...");
    let state = context.get_packaging_state()?;

    if let Some(licenses_path) = state.license_files_path {
        let licenses_path = if licenses_path.is_empty() {
            context.app_path.clone()
        } else {
            context.app_path.join(licenses_path)
        };

        for (name, lis) in &state.license_infos {
            for li in lis {
                let path = licenses_path.join(&li.license_filename);
                warn!(logger, "writing license for {} to {}", name, path.display());
                fs::write(&path, li.license_text.as_bytes()).or_else(|e| Err(e.to_string()))?;
            }
        }
    }

    if !state.app_relative_resources.is_empty() {
        warn!(
            logger,
            "installing resources into {} app-relative directories",
            state.app_relative_resources.len(),
        );
    }

    for (path, v) in &state.app_relative_resources {
        install_app_relative(logger, context, path.as_str(), v).unwrap();
    }

    warn!(
        logger,
        "{} packaged into {}",
        context.app_name,
        context.app_path.display()
    );

    Ok(())
}

/// Defines files, etc to embed Python in a larger binary.
///
/// Instances are typically produced by processing a PyOxidizer config file.
#[derive(Debug)]
pub struct EmbeddedPythonConfig {
    /// Parsed starlark config.
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

    /// Path to file containing packed Python resources data.
    pub resources_path: PathBuf,

    /// Path to library file containing Python.
    pub libpython_path: PathBuf,

    /// Lines that can be emitted from Cargo build scripts to describe this
    /// configuration.
    pub cargo_metadata: Vec<String>,

    /// Rust source code to instantiate a PythonConfig instance using this config.
    pub python_config_rs: String,

    /// Path to file containing packaging state.
    pub packaging_state_path: PathBuf,
}

/// Derive build artifacts from a PyOxidizer configuration.
///
/// This function processes the PyOxidizer configuration and turns it into a set
/// of derived files that can power an embedded Python interpreter.
///
/// Returns a data structure describing the results.
pub fn process_config(
    logger: &slog::Logger,
    context: &mut BuildContext,
    opt_level: &str,
) -> EmbeddedPythonConfig {
    let mut cargo_metadata: Vec<String> = Vec::new();

    let config = &context.config;
    let dest_dir = &context.pyoxidizer_artifacts_path;

    warn!(
        logger,
        "processing config file {}",
        config.config_path.display()
    );

    cargo_metadata.push(format!(
        "cargo:rerun-if-changed={}",
        config.config_path.display()
    ));

    if !dest_dir.exists() {
        create_dir_all(dest_dir).unwrap();
    }

    if let PythonDistribution::Local { local_path, .. } = &config.python_distribution {
        cargo_metadata.push(format!("cargo:rerun-if-changed={}", local_path));
    }

    // Obtain the configured Python distribution and parse it to a data structure.
    warn!(logger, "resolving Python distribution...");
    let python_distribution_path = resolve_python_distribution_archive(&config, &dest_dir);
    warn!(
        logger,
        "Python distribution available at {}",
        python_distribution_path.display()
    );
    let mut fh = fs::File::open(&python_distribution_path).unwrap();
    let mut python_distribution_data = Vec::new();
    fh.read_to_end(&mut python_distribution_data).unwrap();
    let dist_cursor = Cursor::new(python_distribution_data);
    warn!(logger, "reading data from Python distribution...");

    let dist = analyze_python_distribution_tar_zst(dist_cursor, &context.python_distribution_path)
        .unwrap();
    warn!(logger, "distribution info: {:#?}", dist.as_minimal_info());

    // Produce the custom frozen importlib modules.
    warn!(
        logger,
        "compiling custom importlib modules to support in-memory importing"
    );
    let importlib = derive_importlib(&dist);

    let importlib_bootstrap_path = Path::new(&dest_dir).join("importlib_bootstrap");
    let mut fh = fs::File::create(&importlib_bootstrap_path).unwrap();
    fh.write_all(&importlib.bootstrap_bytecode).unwrap();

    let importlib_bootstrap_external_path =
        Path::new(&dest_dir).join("importlib_bootstrap_external");
    let mut fh = fs::File::create(&importlib_bootstrap_external_path).unwrap();
    fh.write_all(&importlib.bootstrap_external_bytecode)
        .unwrap();

    warn!(
        logger,
        "resolving Python resources (modules, extensions, resource data, etc)..."
    );
    let resources = resolve_python_resources(logger, context, &dist);

    warn!(
        logger,
        "resolved {} embedded Python source modules",
        resources.embedded.module_sources.len(),
    );
    info!(logger, "{:#?}", resources.embedded.module_sources.keys());
    warn!(
        logger,
        "resolved {} embedded Python bytecode modules",
        resources.embedded.module_bytecodes.len(),
    );
    info!(logger, "{:#?}", resources.embedded.module_bytecodes.keys());
    warn!(
        logger,
        "resolved {} unique embedded Python modules",
        resources.embedded.all_modules.len(),
    );
    info!(logger, "{:#?}", resources.embedded.all_modules);

    let mut resource_count = 0;
    let mut resource_map = BTreeMap::new();
    for (package, entries) in &resources.embedded.resources {
        let mut names = BTreeSet::new();
        names.extend(entries.keys());
        resource_map.insert(package.clone(), names);
        resource_count += entries.len();
    }

    warn!(
        logger,
        "resolved {} embedded resource files across {} packages",
        resource_count,
        resources.embedded.resources.len(),
    );
    info!(logger, "{:#?}", resource_map);

    let all_extension_modules = resources.embedded.embedded_extension_module_names();
    warn!(
        logger,
        "resolved {} embedded extension modules",
        all_extension_modules.len()
    );
    info!(logger, "{:#?}", all_extension_modules);

    // Produce the packed data structures containing Python modules.
    // TODO there is tons of room to customize this behavior, including
    // reordering modules so the memory order matches import order.

    warn!(logger, "writing packed Python module and resource data...");
    let module_names_path = Path::new(&dest_dir).join("py-module-names");
    let py_modules_path = Path::new(&dest_dir).join("py-modules");
    let resources_path = Path::new(&dest_dir).join("python-resources");
    resources
        .embedded
        .write_blobs(&module_names_path, &py_modules_path, &resources_path);

    warn!(
        logger,
        "{} bytes of Python module data written to {}",
        py_modules_path.metadata().unwrap().len(),
        py_modules_path.display()
    );
    warn!(
        logger,
        "{} bytes of resources data written to {}",
        resources_path.metadata().unwrap().len(),
        resources_path.display()
    );

    // Produce a static library containing the Python bits we need.
    warn!(
        logger,
        "generating custom link library containing Python..."
    );
    let libpython_info = link_libpython(
        logger,
        &dist,
        &resources.embedded,
        dest_dir,
        &context.host_triple,
        &context.target_triple,
        opt_level,
    );
    cargo_metadata.extend(libpython_info.cargo_metadata);

    for p in &resources.read_files {
        cargo_metadata.push(format!("cargo:rerun-if-changed={}", p.display()));
    }

    warn!(logger, "processing python run mode: {:?}", config.run);
    warn!(
        logger,
        "processing embedded python config: {:?}", config.embedded_python_config
    );

    let python_config_rs = derive_python_config(
        &config,
        &importlib_bootstrap_path,
        &importlib_bootstrap_external_path,
        &py_modules_path,
        &resources_path,
    );

    let dest_path = Path::new(&dest_dir).join("data.rs");
    write_data_rs(&dest_path, &python_config_rs);
    // Define the path to the written file in an environment variable so it can
    // be anywhere.
    cargo_metadata.push(format!(
        "cargo:rustc-env=PYEMBED_DATA_RS_PATH={}",
        dest_path.display()
    ));

    // Write a file containing the cargo metadata lines. This allows those
    // lines to be consumed elsewhere and re-emitted without going through all the
    // logic in this function.
    let cargo_metadata_path = Path::new(&dest_dir).join("cargo_metadata.txt");
    fs::write(&cargo_metadata_path, cargo_metadata.join("\n").as_bytes())
        .expect("unable to write cargo_metadata.txt");

    let packaging_state = PackagingState {
        license_files_path: resources.license_files_path,
        license_infos: libpython_info.license_infos,
        app_relative_resources: resources.app_relative,
    };

    let packaging_state_path = dest_dir.join("packaging_state.cbor");
    warn!(
        logger,
        "writing packaging state to {}",
        packaging_state_path.display()
    );
    let mut fh = std::io::BufWriter::new(
        fs::File::create(&packaging_state_path).expect("unable to create packaging_state.cbor"),
    );
    serde_cbor::to_writer(&mut fh, &packaging_state).unwrap();

    context.packaging_state = Some(packaging_state);

    EmbeddedPythonConfig {
        config: config.clone(),
        python_distribution_path,
        importlib_bootstrap_path,
        importlib_bootstrap_external_path,
        module_names_path,
        py_modules_path,
        resources_path,
        libpython_path: libpython_info.path,
        cargo_metadata,
        python_config_rs,
        packaging_state_path,
    }
}

/// Find a pyoxidizer.toml configuration file by walking directory ancestry.
pub fn find_pyoxidizer_config_file(start_dir: &Path) -> Option<PathBuf> {
    for test_dir in start_dir.ancestors() {
        let candidate = test_dir.to_path_buf().join("pyoxidizer.bzl");

        if candidate.exists() {
            return Some(candidate);
        }
    }

    None
}

/// Find a PyOxidizer configuration file from walking the filesystem or an
/// environment variable override.
pub fn find_pyoxidizer_config_file_env(logger: &slog::Logger, start_dir: &Path) -> Option<PathBuf> {
    match env::var("PYOXIDIZER_CONFIG") {
        Ok(config_env) => {
            warn!(
                logger,
                "using PyOxidizer config file from PYOXIDIZER_CONFIG: {}", config_env
            );
            Some(PathBuf::from(config_env))
        }
        Err(_) => find_pyoxidizer_config_file(start_dir),
    }
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
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not found");
    let profile = env::var("PROFILE").expect("PROFILE not defined");

    let project_path = PathBuf::from(&manifest_dir);

    let config_path = match find_pyoxidizer_config_file_env(logger, &PathBuf::from(manifest_dir)) {
        Some(v) => v,
        None => panic!("Could not find PyOxidizer config file"),
    };

    if !config_path.exists() {
        panic!("PyOxidizer config file does not exist");
    }

    let dest_dir = match env::var("PYOXIDIZER_ARTIFACT_DIR") {
        Ok(ref v) => PathBuf::from(v),
        Err(_) => PathBuf::from(env::var("OUT_DIR").unwrap()),
    };

    let mut context = BuildContext::new(
        &project_path,
        &config_path,
        Some(&host),
        &target,
        profile == "release",
        // TODO Config value won't be honored here. Is that OK?
        Some(&dest_dir),
        true,
    )
    .unwrap();

    for line in process_config(logger, &mut context, &opt_level).cargo_metadata {
        println!("{}", line);
    }
}
