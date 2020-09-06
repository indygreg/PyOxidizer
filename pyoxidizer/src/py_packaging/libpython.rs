// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Building a native binary containing Python.
*/

use {
    super::standalone_distribution::{LicenseInfo, StandaloneDistribution},
    anyhow::Result,
    itertools::Itertools,
    lazy_static::lazy_static,
    python_packaging::resource::DataLocation,
    slog::{info, warn},
    std::collections::{BTreeMap, BTreeSet},
    std::fs,
    std::fs::create_dir_all,
    std::path::{Path, PathBuf},
};

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

/// Holds state necessary to link an extension module into libpython.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionModuleBuildState {
    /// Extension C initialization function.
    pub init_fn: Option<String>,

    /// Object files to link into produced binary.
    pub link_object_files: Vec<DataLocation>,

    /// Frameworks this extension module needs to link against.
    pub link_frameworks: BTreeSet<String>,

    /// System libraries this extension module needs to link against.
    pub link_system_libraries: BTreeSet<String>,

    /// Static libraries this extension module needs to link against.
    pub link_static_libraries: BTreeSet<String>,

    /// Dynamic libraries this extension module needs to link against.
    pub link_dynamic_libraries: BTreeSet<String>,

    /// Dynamic libraries this extension module needs to link against.
    pub link_external_libraries: BTreeSet<String>,
}

/// Holds state necessary to link libpython.
pub struct LibpythonLinkingInfo {
    /// Object files that need to be linked.
    pub object_files: Vec<DataLocation>,

    pub link_libraries: BTreeSet<String>,
    pub link_frameworks: BTreeSet<String>,
    pub link_system_libraries: BTreeSet<String>,
    pub link_libraries_external: BTreeSet<String>,
}

/// Resolve state needed to link a libpython.
fn resolve_libpython_linking_info(
    logger: &slog::Logger,
    extension_modules: &BTreeMap<String, ExtensionModuleBuildState>,
) -> Result<LibpythonLinkingInfo> {
    let mut object_files = Vec::new();
    let mut link_libraries = BTreeSet::new();
    let mut link_frameworks = BTreeSet::new();
    let mut link_system_libraries = BTreeSet::new();
    let mut link_libraries_external = BTreeSet::new();

    warn!(
        logger,
        "resolving inputs for {} extension modules...",
        extension_modules.len()
    );

    for (name, state) in extension_modules {
        if !state.link_object_files.is_empty() {
            info!(
                logger,
                "adding {} object files for {} extension module",
                state.link_object_files.len(),
                name
            );
            object_files.extend(state.link_object_files.iter().cloned());
        }

        for framework in &state.link_frameworks {
            warn!(logger, "framework {} required by {}", framework, name);
            link_frameworks.insert(framework.clone());
        }

        for library in &state.link_system_libraries {
            warn!(logger, "system library {} required by {}", library, name);
            link_system_libraries.insert(library.clone());
        }

        for library in &state.link_static_libraries {
            warn!(logger, "static library {} required by {}", library, name);
            link_libraries.insert(library.clone());
        }

        for library in &state.link_dynamic_libraries {
            warn!(logger, "dynamic library {} required by {}", library, name);
            link_libraries.insert(library.clone());
        }

        for library in &state.link_external_libraries {
            warn!(logger, "dynamic library {} required by {}", library, name);
            link_libraries_external.insert(library.clone());
        }
    }

    Ok(LibpythonLinkingInfo {
        object_files,
        link_libraries,
        link_frameworks,
        link_system_libraries,
        link_libraries_external,
    })
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
#[allow(clippy::cognitive_complexity, clippy::too_many_arguments)]
pub fn link_libpython(
    logger: &slog::Logger,
    dist: &StandaloneDistribution,
    builtin_extensions: &[(String, String)],
    extension_modules: &BTreeMap<String, ExtensionModuleBuildState>,
    out_dir: &Path,
    host_triple: &str,
    target_triple: &str,
    opt_level: &str,
) -> Result<LibpythonInfo> {
    let mut cargo_metadata: Vec<String> = Vec::new();

    let temp_dir = tempdir::TempDir::new("libpython")?;
    let temp_dir_path = temp_dir.path();

    let windows = match target_triple {
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
        builtin_extensions.len()
    );
    let config_c_source = make_config_c(&builtin_extensions);
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

    warn!(logger, "compiling custom config.c to object file");
    let mut build = cc::Build::new();

    for flag in &dist.inittab_cflags {
        build.flag(flag);
    }

    build
        .out_dir(out_dir)
        .host(host_triple)
        .target(target_triple)
        .opt_level_str(opt_level)
        .file(config_c_temp_path)
        .include(temp_dir_path)
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
    build.host(host_triple);
    build.target(target_triple);
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
        // We're deriving our own _PyImport_Inittab. So ignore the object
        // file containing it.
        if fs_path == &dist.inittab_object {
            warn!(
                logger,
                "ignoring {} since it may conflict with our version",
                rel_path.display()
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
    let mut needed_libraries: BTreeSet<String> = BTreeSet::new();
    let mut needed_frameworks = BTreeSet::new();
    let mut needed_system_libraries = BTreeSet::new();
    let mut needed_libraries_external = BTreeSet::new();

    warn!(
        logger,
        "resolving libraries required by core distribution..."
    );
    for entry in &dist.links_core {
        if entry.framework {
            warn!(logger, "framework {} required by core", entry.name);
            needed_frameworks.insert(entry.name.clone());
        } else if entry.system {
            warn!(logger, "system library {} required by core", entry.name);
            needed_system_libraries.insert(entry.name.clone());
        }
        // TODO handle static/dynamic libraries.
    }

    let linking_info = resolve_libpython_linking_info(logger, extension_modules)?;

    needed_libraries.extend(linking_info.link_libraries);
    needed_frameworks.extend(linking_info.link_frameworks);
    needed_system_libraries.extend(linking_info.link_system_libraries);
    needed_libraries_external.extend(linking_info.link_libraries_external);

    for (i, object_file) in linking_info.object_files.iter().enumerate() {
        match object_file {
            DataLocation::Memory(data) => {
                let out_path = temp_dir_path.join(format!("libpython.{}.o", i));

                fs::write(&out_path, data)?;
                build.object(&out_path);
            }
            DataLocation::Path(p) => {
                build.object(&p);
            }
        }
    }

    // Windows requires dynamic linking against msvcrt. Ensure that happens.
    // TODO this workaround feels like a bug in the Python distribution not
    // advertising a dependency on the CRT linkage type. Consider adding this
    // to the distribution metadata.
    if windows {
        needed_system_libraries.insert("msvcrt".to_string());
    }

    let mut extra_library_paths = BTreeSet::new();

    for library in needed_libraries {
        if OS_IGNORE_LIBRARIES.contains(&library.as_ref()) {
            continue;
        }

        // Find the library in the distribution and statically link against it.
        let data = dist
            .libraries
            .get(&library)
            .unwrap_or_else(|| panic!("unable to find library {}", library));

        let fs_path = match data {
            DataLocation::Path(fs_path) => fs_path,
            DataLocation::Memory(_) => panic!("cannot link libraries not backed by the filesystem"),
        };

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

    // TODO capture license info for extensions outside the distribution.
    for (name, _) in builtin_extensions.iter() {
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
