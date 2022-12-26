// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Manage PyOxidizer projects.

use {
    crate::{
        environment::{canonicalize_path, default_target_triple, Environment, PyOxidizerSource},
        licensing::{licenses_from_cargo_manifest, log_licensing_info},
        project_building::find_pyoxidizer_config_file_env,
        project_layout::{initialize_project, write_new_pyoxidizer_config_file},
        py_packaging::{
            distribution::{
                default_distribution_location, resolve_distribution,
                resolve_python_distribution_archive, BinaryLibpythonLinkMode, DistributionCache,
                DistributionFlavor, PythonDistribution,
            },
            standalone_distribution::StandaloneDistribution,
        },
        python_distributions::PYTHON_DISTRIBUTIONS,
        starlark::eval::EvaluationContextBuilder,
    },
    anyhow::{anyhow, Context, Result},
    python_packaging::licensing::LicenseFlavor,
    python_packaging::{
        filesystem_scanning::find_python_resources,
        interpreter::{MemoryAllocatorBackend, PythonInterpreterProfile},
        resource::PythonResource,
        wheel::WheelArchive,
    },
    simple_file_manifest::{FileData, FileManifest},
    std::{
        collections::HashMap,
        fs::create_dir_all,
        io::{Cursor, Read},
        path::{Path, PathBuf},
    },
};

/// Attempt to resolve the default Rust target for a build.
pub fn default_target() -> Result<String> {
    // TODO derive these more intelligently.
    if cfg!(target_os = "linux") {
        if cfg!(target_arch = "aarch64") {
            Ok("aarch64-unknown-linux-gnu".to_string())
        } else {
            Ok("x86_64-unknown-linux-gnu".to_string())
        }
    } else if cfg!(target_os = "windows") {
        Ok("x86_64-pc-windows-msvc".to_string())
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            Ok("aarch64-apple-darwin".to_string())
        } else {
            Ok("x86_64-apple-darwin".to_string())
        }
    } else {
        Err(anyhow!("unable to resolve target"))
    }
}

pub fn resolve_target(target: Option<&str>) -> Result<String> {
    if let Some(s) = target {
        Ok(s.to_string())
    } else {
        default_target()
    }
}

pub fn list_targets(env: &Environment, project_path: &Path) -> Result<()> {
    let config_path = find_pyoxidizer_config_file_env(project_path).ok_or_else(|| {
        anyhow!(
            "unable to find PyOxidizder config file at {}",
            project_path.display()
        )
    })?;

    let target_triple = default_target()?;

    let mut context = EvaluationContextBuilder::new(env, config_path.clone(), target_triple)
        .resolve_targets(vec![])
        .into_context()?;

    context.evaluate_file(&config_path)?;

    if context.default_target()?.is_none() {
        println!("(no targets defined)");
        return Ok(());
    }

    for target in context.target_names()? {
        let prefix = if Some(target.clone()) == context.default_target()? {
            "*"
        } else {
            ""
        };
        println!("{}{}", prefix, target);
    }

    Ok(())
}

/// Build a PyOxidizer enabled project.
///
/// This is a glorified wrapper around `cargo build`. Our goal is to get the
/// output from repackaging to give the user something for debugging.
#[allow(clippy::too_many_arguments)]
pub fn build(
    env: &Environment,
    project_path: &Path,
    target_triple: Option<&str>,
    resolve_targets: Option<Vec<String>>,
    extra_vars: HashMap<String, Option<String>>,
    release: bool,
    verbose: bool,
) -> Result<()> {
    let config_path = find_pyoxidizer_config_file_env(project_path).ok_or_else(|| {
        anyhow!(
            "unable to find PyOxidizer config file at {}",
            project_path.display()
        )
    })?;
    let target_triple = resolve_target(target_triple)?;

    let mut context = EvaluationContextBuilder::new(env, config_path.clone(), target_triple)
        .extra_vars(extra_vars)
        .release(release)
        .verbose(verbose)
        .resolve_targets_optional(resolve_targets)
        .into_context()?;

    context.evaluate_file(&config_path)?;

    for target in context.targets_to_resolve()? {
        context.build_resolved_target(&target)?;
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn run(
    env: &Environment,
    project_path: &Path,
    target_triple: Option<&str>,
    release: bool,
    target: Option<&str>,
    extra_vars: HashMap<String, Option<String>>,
    _extra_args: &[&str],
    verbose: bool,
) -> Result<()> {
    let config_path = find_pyoxidizer_config_file_env(project_path).ok_or_else(|| {
        anyhow!(
            "unable to find PyOxidizer config file at {}",
            project_path.display()
        )
    })?;
    let target_triple = resolve_target(target_triple)?;

    let mut context = EvaluationContextBuilder::new(env, config_path.clone(), target_triple)
        .extra_vars(extra_vars)
        .release(release)
        .verbose(verbose)
        .resolve_target_optional(target)
        .into_context()?;

    context.evaluate_file(&config_path)?;

    context.run_target(target)
}

pub fn cache_clear(env: &Environment) -> Result<()> {
    let cache_dir = env.cache_dir();

    println!("removing {}", cache_dir.display());
    remove_dir_all::remove_dir_all(cache_dir)?;

    Ok(())
}

/// Find resources given a source path.
pub fn find_resources(
    env: &Environment,
    path: Option<&Path>,
    distributions_dir: Option<&Path>,
    scan_distribution: bool,
    target_triple: &str,
    classify_files: bool,
    emit_files: bool,
) -> Result<()> {
    let distribution_location =
        default_distribution_location(&DistributionFlavor::Standalone, target_triple, None)?;

    let mut temp_dir = None;

    let extract_path = if let Some(path) = distributions_dir {
        path
    } else {
        temp_dir.replace(env.temporary_directory("python-distribution")?);
        temp_dir.as_ref().unwrap().path()
    };

    let dist = resolve_distribution(&distribution_location, extract_path)?;

    if scan_distribution {
        println!("scanning distribution");
        for resource in dist.python_resources() {
            print_resource(&resource);
        }
    } else if let Some(path) = path {
        if path.is_dir() {
            println!("scanning directory {}", path.display());
            for resource in find_python_resources(
                path,
                dist.cache_tag(),
                &dist.python_module_suffixes()?,
                emit_files,
                classify_files,
            )? {
                print_resource(&resource?);
            }
        } else if path.is_file() {
            if let Some(extension) = path.extension() {
                if extension.to_string_lossy() == "whl" {
                    println!("parsing {} as a wheel archive", path.display());
                    let wheel = WheelArchive::from_path(path)?;

                    for resource in wheel.python_resources(
                        dist.cache_tag(),
                        &dist.python_module_suffixes()?,
                        emit_files,
                        classify_files,
                    )? {
                        print_resource(&resource)
                    }

                    return Ok(());
                }
            }

            println!("do not know how to find resources in {}", path.display());
        } else {
            println!("do not know how to find resources in {}", path.display());
        }
    } else {
        println!("do not know what to scan");
    }

    Ok(())
}

fn print_resource(r: &PythonResource) {
    match r {
        PythonResource::ModuleSource(m) => println!(
            "PythonModuleSource {{ name: {}, is_package: {}, is_stdlib: {}, is_test: {} }}",
            m.name, m.is_package, m.is_stdlib, m.is_test
        ),
        PythonResource::ModuleBytecode(m) => println!(
            "PythonModuleBytecode {{ name: {}, is_package: {}, is_stdlib: {}, is_test: {}, bytecode_level: {} }}",
            m.name, m.is_package, m.is_stdlib, m.is_test, i32::from(m.optimize_level)
        ),
        PythonResource::ModuleBytecodeRequest(_) => println!(
            "PythonModuleBytecodeRequest {{ you should never see this }}"
        ),
        PythonResource::PackageResource(r) => println!(
            "PythonPackageResource {{ package: {}, name: {}, is_stdlib: {}, is_test: {} }}", r.leaf_package, r.relative_name, r.is_stdlib, r.is_test
        ),
        PythonResource::PackageDistributionResource(r) => println!(
            "PythonPackageDistributionResource {{ package: {}, version: {}, name: {} }}", r.package, r.version, r.name
        ),
        PythonResource::ExtensionModule(em) => {
            println!(
                "PythonExtensionModule {{"
            );
            println!("    name: {}", em.name);
            println!("    is_builtin: {}", em.builtin_default);
            println!("    has_shared_library: {}", em.shared_library.is_some());
            println!("    has_object_files: {}", !em.object_file_data.is_empty());
            println!("    link_libraries: {:?}", em.link_libraries);
            println!("}}");
        },
        PythonResource::EggFile(e) => println!(
            "PythonEggFile {{ path: {} }}", match &e.data {
                FileData::Path(p) => p.display().to_string(),
                FileData::Memory(_) => "memory".to_string(),
            }
        ),
        PythonResource::PathExtension(_pe) => println!(
            "PythonPathExtension",
        ),
        PythonResource::File(f) => println!(
            "File {{ path: {}, is_executable: {} }}", f.path().display(), f.entry().is_executable()
        ),
    }
}

/// Initialize a PyOxidizer configuration file in a given directory.
pub fn init_config_file(
    source: &PyOxidizerSource,
    project_dir: &Path,
    code: Option<&str>,
    pip_install: &[&str],
) -> Result<()> {
    if project_dir.exists() && !project_dir.is_dir() {
        return Err(anyhow!(
            "existing path must be a directory: {}",
            project_dir.display()
        ));
    }

    if !project_dir.exists() {
        create_dir_all(project_dir)?;
    }

    let name = project_dir.iter().last().unwrap().to_str().unwrap();

    write_new_pyoxidizer_config_file(source, project_dir, name, code, pip_install)?;

    println!();
    println!("A new PyOxidizer configuration file has been created.");
    println!("This configuration file can be used by various `pyoxidizer`");
    println!("commands");
    println!();
    println!("For example, to build and run the default Python application:");
    println!();
    println!("  $ cd {}", project_dir.display());
    println!("  $ pyoxidizer run");
    println!();
    println!("The default configuration is to invoke a Python REPL. You can");
    println!("edit the configuration file to change behavior.");

    Ok(())
}

/// Initialize a new Rust project with PyOxidizer support.
pub fn init_rust_project(env: &Environment, project_path: &Path) -> Result<()> {
    let cargo_exe = env
        .ensure_rust_toolchain(None)
        .context("resolving Rust environment")?
        .cargo_exe;

    initialize_project(
        &env.pyoxidizer_source,
        project_path,
        &cargo_exe,
        None,
        &[],
        "console",
    )?;
    println!();
    println!(
        "A new Rust binary application has been created in {}",
        project_path.display()
    );
    print!(
        r#"
This application can be built most easily by doing the following:

  $ cd {project_path}
  $ pyoxidizer run

Note however that this will bypass all the Rust code in the project
folder, and build the project as if you had only created a pyoxidizer.bzl
file. Building from Rust is more involved, and requires multiple steps.
Please see the "PyOxidizer Rust Projects" section of the manual for more
information.

The default configuration is to invoke a Python REPL. You can
edit the various pyoxidizer.*.bzl config files or the main.rs
file to change behavior. The application will need to be rebuilt
for configuration changes to take effect.
"#,
        project_path = project_path.display()
    );

    Ok(())
}

pub fn python_distribution_extract(
    download_default: bool,
    archive_path: Option<&str>,
    dest_path: &str,
) -> Result<()> {
    let dist_path = if let Some(path) = archive_path {
        PathBuf::from(path)
    } else if download_default {
        let location = default_distribution_location(
            &DistributionFlavor::Standalone,
            default_target_triple(),
            None,
        )?;

        resolve_python_distribution_archive(&location, Path::new(dest_path))?
    } else {
        return Err(anyhow!("do not know what distribution to operate on"));
    };

    let mut fh = std::fs::File::open(&dist_path)?;
    let mut data = Vec::new();
    fh.read_to_end(&mut data)?;
    let cursor = Cursor::new(data);
    let dctx = zstd::stream::Decoder::new(cursor)?;
    let mut tf = tar::Archive::new(dctx);

    println!("extracting archive to {}", dest_path);
    tf.unpack(dest_path)?;

    Ok(())
}

pub fn python_distribution_info(env: &Environment, dist_path: &str) -> Result<()> {
    let fh = std::fs::File::open(Path::new(dist_path))?;
    let reader = std::io::BufReader::new(fh);

    let temp_dir = env.temporary_directory("python-distribution")?;
    let temp_dir_path = temp_dir.path();

    let dist = StandaloneDistribution::from_tar_zst(reader, temp_dir_path)?;

    println!("High-Level Metadata");
    println!("===================");
    println!();
    println!("Target triple: {}", dist.target_triple);
    println!("Tag:           {}", dist.python_tag);
    println!("Platform tag:  {}", dist.python_platform_tag);
    println!("Version:       {}", dist.version);
    println!();

    println!("Extension Modules");
    println!("=================");
    for (name, ems) in dist.extension_modules {
        println!("{}", name);
        println!("{}", "-".repeat(name.len()));
        println!();

        for em in ems.iter() {
            println!("{}", em.variant.as_ref().unwrap());
            println!("{}", "^".repeat(em.variant.as_ref().unwrap().len()));
            println!();
            println!("Required: {}", em.required);
            println!("Built-in Default: {}", em.builtin_default);
            if let Some(component) = &em.license {
                println!(
                    "Licensing: {}",
                    match component.license() {
                        LicenseFlavor::Spdx(expression) => expression.to_string(),
                        LicenseFlavor::OtherExpression(expression) => expression.to_string(),
                        LicenseFlavor::PublicDomain => "public domain".to_string(),
                        LicenseFlavor::None => "none".to_string(),
                        LicenseFlavor::Unknown(terms) => terms.join(","),
                    }
                );
            }
            if !em.link_libraries.is_empty() {
                println!(
                    "Links: {}",
                    em.link_libraries
                        .iter()
                        .map(|l| l.name.clone())
                        .collect::<Vec<String>>()
                        .join(", ")
                );
            }

            println!();
        }
    }

    println!("Python Modules");
    println!("==============");
    println!();
    for name in dist.py_modules.keys() {
        println!("{}", name);
    }
    println!();

    println!("Python Resources");
    println!("================");
    println!();

    for (package, resources) in dist.resources {
        for name in resources.keys() {
            println!("[{}].{}", package, name);
        }
    }

    Ok(())
}

pub fn python_distribution_licenses(env: &Environment, path: &str) -> Result<()> {
    let fh = std::fs::File::open(Path::new(path))?;
    let reader = std::io::BufReader::new(fh);

    let temp_dir = env.temporary_directory("python-distribution")?;
    let temp_dir_path = temp_dir.path();

    let dist = StandaloneDistribution::from_tar_zst(reader, temp_dir_path)?;

    println!(
        "Python Distribution Licenses: {}",
        match dist.licenses {
            Some(licenses) => itertools::join(licenses, ", "),
            None => "NO LICENSE FOUND".to_string(),
        }
    );
    println!();
    println!("Extension Libraries and License Requirements");
    println!("============================================");
    println!();

    for (name, variants) in &dist.extension_modules {
        for variant in variants.iter() {
            if variant.link_libraries.is_empty() {
                continue;
            }

            let name = if variant.variant.as_ref().unwrap() == "default" {
                name.clone()
            } else {
                format!("{} ({})", name, variant.variant.as_ref().unwrap())
            };

            println!("{}", name);
            println!("{}", "-".repeat(name.len()));
            println!();

            for link in &variant.link_libraries {
                println!("Dependency: {}", &link.name);
                println!(
                    "Link Type: {}",
                    if link.system {
                        "system"
                    } else if link.framework {
                        "framework"
                    } else {
                        "library"
                    }
                );

                println!();
            }

            if let Some(component) = &variant.license {
                match component.license() {
                    LicenseFlavor::Spdx(expression) => {
                        println!("Licensing: Valid SPDX: {}", expression);
                    }
                    LicenseFlavor::OtherExpression(expression) => {
                        println!("Licensing: Invalid SPDX: {}", expression);
                    }
                    LicenseFlavor::PublicDomain => {
                        println!("Licensing: Public Domain");
                    }
                    LicenseFlavor::None => {
                        println!("Licensing: None defined");
                    }
                    LicenseFlavor::Unknown(terms) => {
                        println!("Licensing: {}", terms.join(", "));
                    }
                }
            } else {
                println!("Licensing: UNKNOWN");
            }

            println!();
        }
    }

    Ok(())
}

/// Generate artifacts for embedding Python in a binary.
pub fn generate_python_embedding_artifacts(
    env: &Environment,
    target_triple: &str,
    flavor: &str,
    python_version: Option<&str>,
    dest_path: &Path,
) -> Result<()> {
    let flavor = DistributionFlavor::try_from(flavor).map_err(|e| anyhow!("{}", e))?;

    std::fs::create_dir_all(dest_path)
        .with_context(|| format!("creating directory {}", dest_path.display()))?;

    let dest_path = canonicalize_path(dest_path).context("canonicalizing destination directory")?;

    let distribution_record = PYTHON_DISTRIBUTIONS
        .find_distribution(target_triple, &flavor, python_version)
        .ok_or_else(|| anyhow!("could not find Python distribution matching requirements"))?;

    let distribution_cache = DistributionCache::new(Some(&env.python_distributions_dir()));

    let dist = distribution_cache
        .resolve_distribution(&distribution_record.location, None)
        .context("resolving Python distribution")?;

    let host_dist = distribution_cache
        .host_distribution(Some(dist.python_major_minor_version().as_str()), None)
        .context("resolving host distribution")?;

    let policy = dist
        .create_packaging_policy()
        .context("creating packaging policy")?;

    let mut interpreter_config = dist
        .create_python_interpreter_config()
        .context("creating Python interpreter config")?;

    interpreter_config.config.profile = PythonInterpreterProfile::Python;
    interpreter_config.allocator_backend = MemoryAllocatorBackend::Default;

    let mut builder = dist.as_python_executable_builder(
        default_target_triple(),
        target_triple,
        "python",
        BinaryLibpythonLinkMode::Default,
        &policy,
        &interpreter_config,
        Some(host_dist.clone_trait()),
    )?;

    builder.set_tcl_files_path(Some("tcl".to_string()));

    builder
        .add_distribution_resources(None)
        .context("adding distribution resources")?;

    let embedded_context = builder
        .to_embedded_python_context(env, "1")
        .context("resolving embedded context")?;

    embedded_context
        .write_files(&dest_path)
        .context("writing embedded artifact files")?;

    embedded_context
        .extra_files
        .materialize_files(&dest_path)
        .context("writing extra files")?;

    // Write out a copy of the standard library.
    let mut m = FileManifest::default();
    for resource in find_python_resources(
        &dist.stdlib_path,
        dist.cache_tag(),
        &dist.python_module_suffixes()?,
        true,
        false,
    )? {
        if let PythonResource::File(file) = resource? {
            m.add_file_entry(file.path(), file.entry())?;
        } else {
            panic!("find_python_resources() should only emit File variant");
        }
    }

    m.materialize_files_with_replace(dest_path.join("stdlib"))
        .context("writing standard library")?;

    Ok(())
}

pub fn rust_project_licensing(
    env: &Environment,
    project_path: &Path,
    all_features: bool,
    target_triple: Option<&str>,
    unified_license: bool,
) -> Result<()> {
    let manifest_path = project_path.join("Cargo.toml");

    let toolchain = env
        .ensure_rust_toolchain(None)
        .context("resolving Rust toolchain")?;

    let licensing = licenses_from_cargo_manifest(
        &manifest_path,
        all_features,
        [],
        target_triple,
        &toolchain,
        true,
    )?;

    if unified_license {
        println!("{}", licensing.aggregate_license_document(true)?);
    } else {
        log_licensing_info(&licensing);
    }

    Ok(())
}
