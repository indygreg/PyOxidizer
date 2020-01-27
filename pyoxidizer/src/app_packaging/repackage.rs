// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// THIS MODULE IS DEPRECATED AND THE CODE SHOULD NOT BE USED ANY MORE.
// IT IS AROUND TO SERVE AS A REFERENCE TO HOW THINGS ONCE WERE.

use {
    super::config::Config,
    super::state::BuildContext,
    crate::project_building::HOST,
    crate::py_packaging::bytecode::python_source_encoding,
    crate::py_packaging::distribution::{ExtensionModule, ParsedPythonDistribution},
    crate::py_packaging::embedded_resource::EmbeddedPythonResources,
    crate::py_packaging::resource::{
        packages_from_module_name, packages_from_module_names, AppRelativeResources,
        ExtensionModuleData, PackagedModuleBytecode, PackagedModuleSource,
    },
    anyhow::{anyhow, Context, Result},
    slog::warn,
    std::collections::{BTreeMap, BTreeSet},
    std::fs,
    std::path::{Path, PathBuf},
};

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

        let python_distribution_path = pyoxidizer_artifacts_path.join("python.dummy");

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
    let embedded_built_extension_modules: BTreeMap<String, ExtensionModuleData> = BTreeMap::new();

    let app_relative: BTreeMap<String, AppRelativeResources> = BTreeMap::new();

    let read_files: Vec<PathBuf> = Vec::new();
    let license_files_path = None;

    // Add required extension modules, as some don't show up in the modules list
    // and may have been filtered or not added in the first place.
    for (name, variants) in &dist.extension_modules {
        let em = &variants[0];

        if (em.builtin_default || em.required) && !embedded_extension_modules.contains_key(name) {
            warn!(logger, "adding required embedded extension module {}", name);
            embedded_extension_modules.insert(name.clone(), em.clone());
        }
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

    let mut all_embedded_package_names = annotated_package_names;
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
