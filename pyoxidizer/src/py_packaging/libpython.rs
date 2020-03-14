// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Building a native binary containing Python.
*/

use anyhow::Result;
use itertools::Itertools;
use lazy_static::lazy_static;
use slog::{info, warn};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::fs::create_dir_all;
use std::path::{Path, PathBuf};

use super::bytecode::{BytecodeCompiler, CompileMode};
use super::embedded_resource::EmbeddedPythonResources;
use super::resource::BytecodeOptimizationLevel;
use super::standalone_distribution::{LicenseInfo, StandaloneDistribution};

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

/// Holds bytecode for importlib bootstrap modules.
///
/// These are made available as frozen modules to the Python interpreter to bootstrap
/// the importlib module import system.
#[derive(Clone, Debug)]
pub struct ImportlibBytecode {
    pub bootstrap: Vec<u8>,
    pub bootstrap_external: Vec<u8>,
}

/// Produce frozen importlib bytecode data.
///
/// importlib._bootstrap isn't modified.
///
/// importlib._bootstrap_external is modified. We take the original Python
/// source and concatenate with code that provides the memory importer.
/// Bytecode is then derived from it.
pub fn derive_importlib(
    bootstrap_source: &[u8],
    bootstrap_external_source: &[u8],
    compiler: &mut BytecodeCompiler,
) -> Result<ImportlibBytecode> {
    let module_name = "<frozen importlib._bootstrap>";
    let bootstrap_bytecode = compiler.compile(
        &bootstrap_source,
        module_name,
        BytecodeOptimizationLevel::Zero,
        CompileMode::Bytecode,
    )?;

    let mut bootstrap_external_source = Vec::from(bootstrap_external_source);
    bootstrap_external_source.extend("\n# END OF importlib/_bootstrap_external.py\n\n".bytes());
    bootstrap_external_source.extend(PYTHON_IMPORTER);
    let module_name = "<frozen importlib._bootstrap_external>";
    let bootstrap_external_bytecode = compiler.compile(
        &bootstrap_external_source,
        module_name,
        BytecodeOptimizationLevel::Zero,
        CompileMode::Bytecode,
    )?;

    Ok(ImportlibBytecode {
        bootstrap: bootstrap_bytecode,
        bootstrap_external: bootstrap_external_bytecode,
    })
}

/// Produce the content of the config.c file containing built-in extensions.
pub fn make_config_c(extensions: &[(String, String)]) -> String {
    // It is easier to construct the file from scratch than parse the template
    // and insert things in the right places.
    let mut lines: Vec<String> = Vec::new();

    lines.push(String::from("#include \"Python.h\""));

    // Declare the initialization functions.
    for (_name, init_fn) in extensions {
        if init_fn != "NULL" {
            lines.push(format!("extern PyObject* {}(void);", init_fn));
        }
    }

    lines.push(String::from("struct _inittab _PyImport_Inittab[] = {"));

    for (name, init_fn) in extensions {
        lines.push(format!("{{\"{}\", {}}},", name, init_fn));
    }

    lines.push(String::from("{0, 0}"));
    lines.push(String::from("};"));

    lines.join("\n")
}

#[derive(Debug)]
pub struct LibpythonInfo {
    pub libpython_path: PathBuf,
    pub libpyembeddedconfig_path: PathBuf,
    pub cargo_metadata: Vec<String>,
    pub license_infos: BTreeMap<String, Vec<LicenseInfo>>,
}

/// Create a static libpython from a Python distribution.
///
/// Returns a vector of cargo: lines that can be printed in build scripts.
#[allow(clippy::cognitive_complexity)]
pub fn link_libpython(
    logger: &slog::Logger,
    dist: &StandaloneDistribution,
    resources: &EmbeddedPythonResources,
    out_dir: &Path,
    host: &str,
    target: &str,
    opt_level: &str,
) -> Result<LibpythonInfo> {
    let mut cargo_metadata: Vec<String> = Vec::new();

    let temp_dir = tempdir::TempDir::new("libpython")?;
    let temp_dir_path = temp_dir.path();

    let extension_modules = &resources.extension_modules;
    let built_extension_modules = &resources.built_extension_modules;

    let windows = match target {
        "i686-pc-windows-msvc" => true,
        "x86_64-pc-windows-msvc" => true,
        _ => false,
    };

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
    let config_c_source = make_config_c(&resources.builtin_extensions());
    let config_c_path = out_dir.join("config.c");
    let config_c_temp_path = temp_dir_path.join("config.c");

    fs::write(&config_c_path, config_c_source.as_bytes())?;
    fs::write(&config_c_temp_path, config_c_source.as_bytes())?;

    // We need to make all .h includes accessible.
    for (name, fs_path) in &dist.includes {
        let full = temp_dir_path.join(name);
        create_dir_all(full.parent().expect("parent directory"))?;
        fs::copy(fs_path, full)?;
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

    let libpyembeddedconfig_path = out_dir.join(if windows {
        "pyembeddedconfig.lib"
    } else {
        "libpyembeddedconfig.a"
    });

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
        create_dir_all(parent)?;

        let full = temp_dir_path.join(rel_path);
        fs::copy(fs_path, &full)?;

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

            fs::write(&out_path, object_data)?;
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
    if windows {
        needed_system_libraries.insert("msvcrt");
    }

    let mut extra_library_paths = BTreeSet::new();

    for library in needed_libraries.iter() {
        if OS_IGNORE_LIBRARIES.contains(&library) {
            continue;
        }

        // Find the library in the distribution and statically link against it.
        let fs_path = dist
            .libraries
            .get(*library)
            .unwrap_or_else(|| panic!("unable to find library {}", library));

        extra_library_paths.insert(fs_path.parent().unwrap().to_path_buf());

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

    let libpython_path = out_dir.join(if windows {
        "pythonXY.lib"
    } else {
        "libpythonXY.a"
    });

    cargo_metadata.push("cargo:rustc-link-lib=static=pythonXY".to_string());
    cargo_metadata.push(format!(
        "cargo:rustc-link-search=native={}",
        out_dir.display()
    ));

    for path in extra_library_paths {
        cargo_metadata.push(format!("cargo:rustc-link-search=native={}", path.display()));
    }

    let mut license_infos = BTreeMap::new();

    if let Some(li) = dist.license_infos.get("python") {
        license_infos.insert("python".to_string(), li.clone());
    }

    for name in extension_modules.keys() {
        if let Some(li) = dist.license_infos.get(name) {
            license_infos.insert(name.clone(), li.clone());
        }
    }

    Ok(LibpythonInfo {
        libpython_path,
        libpyembeddedconfig_path,
        cargo_metadata,
        license_infos,
    })
}
