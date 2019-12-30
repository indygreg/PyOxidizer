// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use anyhow::{anyhow, Context, Result};
use slog::{info, warn};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::fs::create_dir_all;
use std::path::{Path, PathBuf};

use super::config::{eval_starlark_config_file, find_pyoxidizer_config_file_env, Config};
use super::state::BuildContext;
use crate::py_packaging::bytecode::python_source_encoding;
use crate::py_packaging::distribution::{
    ExtensionModule, ParsedPythonDistribution, PythonDistributionLocation,
};
use crate::py_packaging::embedded_resource::{EmbeddedPythonResources, OS_IGNORE_EXTENSIONS};
use crate::py_packaging::resource::{
    packages_from_module_name, packages_from_module_names, AppRelativeResources,
    BuiltExtensionModule, PackagedModuleBytecode, PackagedModuleSource,
};

pub const HOST: &str = env!("HOST");

impl BuildContext {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        project_path: &Path,
        config: Config,
        host: Option<&str>,
        target: &str,
        release: bool,
        force_artifacts_path: Option<&Path>,
        verbose: bool,
    ) -> Result<Self> {
        let config_parent_path = config
            .config_path
            .parent()
            .with_context(|| "resolving parent path of config")?;

        let host_triple = if let Some(v) = host {
            v.to_string()
        } else {
            HOST.to_string()
        };

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
            PythonDistributionLocation::Local { sha256, .. } => sha256,
            PythonDistributionLocation::Url { sha256, .. } => sha256,
        };

        // Take the prefix so paths are shorter.
        let distribution_hash = &distribution_hash[0..12];

        let python_distribution_path =
            pyoxidizer_artifacts_path.join(format!("python.{}", distribution_hash));

        let cargo_toml_path = project_path.join("Cargo.toml");
        if !cargo_toml_path.exists() {
            return Err(anyhow!("{} does not exist", cargo_toml_path.display()));
        }

        let cargo_toml_data = fs::read(&cargo_toml_path)?;
        let cargo_config = cargo_toml::Manifest::from_slice(&cargo_toml_data)?;

        Ok(BuildContext {
            project_path: project_path.to_path_buf(),
            config_path: config.config_path.clone(),
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
        })
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

struct BytecodeRequest {
    source: Vec<u8>,
}

/// Resolves a series of packaging rules to a final set of resources to package.
#[allow(clippy::cognitive_complexity)]
pub fn resolve_python_resources(
    logger: &slog::Logger,
    _context: &BuildContext,
    dist: &ParsedPythonDistribution,
) -> PythonResources {
    // Since bytecode has a non-trivial cost to generate, our strategy is to accumulate
    // requests for bytecode then generate bytecode for the final set of inputs at the
    // end of processing. That way we don't generate bytecode only to throw it away later.

    let mut embedded_extension_modules: BTreeMap<String, ExtensionModule> = BTreeMap::new();
    let embedded_sources: BTreeMap<String, PackagedModuleSource> = BTreeMap::new();
    let mut embedded_bytecode_requests: BTreeMap<String, BytecodeRequest> = BTreeMap::new();
    let embedded_resources: BTreeMap<String, BTreeMap<String, Vec<u8>>> = BTreeMap::new();
    let embedded_built_extension_modules: BTreeMap<String, BuiltExtensionModule> = BTreeMap::new();

    let app_relative: BTreeMap<String, AppRelativeResources> = BTreeMap::new();

    let read_files: Vec<PathBuf> = Vec::new();
    let license_files_path = None;

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
        embedded_bytecode_requests.insert(package.clone(), BytecodeRequest { source: Vec::new() });
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

    let embedded_bytecodes: BTreeMap<String, PackagedModuleBytecode> = BTreeMap::new();
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

/// Install all app-relative files next to the generated binary.
#[allow(dead_code)]
fn install_app_relative(
    logger: &slog::Logger,
    context: &BuildContext,
    path: &str,
    app_relative: &AppRelativeResources,
) -> Result<()> {
    let dest_path = context.app_exe_path.parent().unwrap().join(path);

    create_dir_all(&dest_path)?;

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
        create_dir_all(&parent_dir)?;
        fs::write(&module_path, &module_source.source)?;
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
        create_dir_all(&parent_dir)?;

        fs::write(&module_path, &module_bytecode.bytecode)?;
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

            create_dir_all(dest_path.parent().unwrap())?;

            fs::write(&dest_path, data)?;
        }
    }

    Ok(())
}

/// Package a built Rust project into its packaging directory.
///
/// This will delete all content in the application's package directory.
pub fn package_project(logger: &slog::Logger, context: &mut BuildContext) -> Result<()> {
    warn!(
        logger,
        "packaging application into {}",
        context.app_path.display()
    );

    if context.app_path.exists() {
        warn!(logger, "purging {}", context.app_path.display());
        std::fs::remove_dir_all(&context.app_path)?;
    }

    create_dir_all(&context.app_path)?;

    warn!(
        logger,
        "copying {} to {}",
        context.app_exe_target_path.display(),
        context.app_exe_path.display()
    );
    std::fs::copy(&context.app_exe_target_path, &context.app_exe_path)?;

    // TODO remember to port license files writing.

    warn!(
        logger,
        "{} packaged into {}",
        context.app_name,
        context.app_path.display()
    );

    Ok(())
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

    // TODO use these variables?
    //let host = env::var("HOST").expect("HOST not defined");
    let target = env::var("TARGET").expect("TARGET not defined");
    //let opt_level = env::var("OPT_LEVEL").expect("OPT_LEVEL not defined");
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not found");
    //let profile = env::var("PROFILE").expect("PROFILE not defined");

    //let project_path = PathBuf::from(&manifest_dir);

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

    eval_starlark_config_file(logger, &config_path, &target, Some(&dest_dir)).unwrap();

    let cargo_metadata = dest_dir.join("cargo_metadata.txt");
    let content = std::fs::read(&cargo_metadata).unwrap();
    let content = String::from_utf8(content).unwrap();
    print!("{}", content);
}
