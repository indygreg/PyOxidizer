// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Building a native binary containing Python.
*/

use {
    super::standalone_distribution::{LicenseInfo, StandaloneDistribution},
    anyhow::{anyhow, Result},
    lazy_static::lazy_static,
    python_packaging::resource::DataLocation,
    slog::warn,
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

/// Holds state necessary to link a libpython.
///
/// Instances of this are likely populated by a binary builder, taking
/// information from a distribution and added extensions.
///
/// Note that this context is only for producing libpython: it is very
/// linker centric and doesn't track state like Python resources.
#[derive(Clone, Debug)]
pub struct LinkingContext {
    /// Object files that will be linked together.
    pub object_files: Vec<DataLocation>,

    /// System libraries that will be linked against.
    pub system_libraries: BTreeSet<String>,

    /// Dynamic libraries that will be linked against.
    pub dynamic_libraries: BTreeSet<String>,

    /// Static libraries that will be linked against.
    pub static_libraries: BTreeSet<String>,

    /// Frameworks that will be linked against.
    ///
    /// Used on Apple platforms.
    pub frameworks: BTreeSet<String>,

    /// Holds licensing info for things being linked together.
    ///
    /// Keys are entity name (e.g. extension name). Values are license
    /// structures.
    pub license_infos: BTreeMap<String, Vec<LicenseInfo>>,
}

impl Default for LinkingContext {
    fn default() -> Self {
        Self {
            object_files: Vec::new(),
            system_libraries: BTreeSet::new(),
            dynamic_libraries: BTreeSet::new(),
            static_libraries: BTreeSet::new(),
            frameworks: BTreeSet::new(),
            license_infos: BTreeMap::new(),
        }
    }
}

/// Holds state necessary to link an extension module into libpython.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionModuleBuildState {
    /// Extension C initialization function.
    pub init_fn: Option<String>,
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
    context: &LinkingContext,
    builtin_extensions: &[(String, String)],
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

    for (i, location) in context.object_files.iter().enumerate() {
        match location {
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

    let mut extra_library_paths = BTreeSet::new();

    for location in dist.libraries.values() {
        let path = match location {
            DataLocation::Path(p) => p,
            DataLocation::Memory(_) => {
                return Err(anyhow!(
                    "cannot link libraries not backed by the filesystem"
                ))
            }
        };

        extra_library_paths.insert(
            path.parent()
                .ok_or_else(|| anyhow!("unable to resolve parent directory"))?
                .to_path_buf(),
        );
    }

    for framework in &context.frameworks {
        cargo_metadata.push(format!("cargo:rustc-link-lib=framework={}", framework));
    }

    for lib in &context.system_libraries {
        cargo_metadata.push(format!("cargo:rustc-link-lib={}", lib));
    }

    for lib in &context.dynamic_libraries {
        if !OS_IGNORE_LIBRARIES.contains(&lib.as_str()) {
            cargo_metadata.push(format!("cargo:rustc-link-lib={}", lib));
        }
    }

    for lib in &context.static_libraries {
        if !OS_IGNORE_LIBRARIES.contains(&lib.as_str()) {
            cargo_metadata.push(format!("cargo:rustc-link-lib=static={}", lib));
        }
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

    Ok(LibpythonInfo {
        libpython_path,
        libpyembeddedconfig_path,
        cargo_metadata,
        license_infos: context.license_infos.clone(),
    })
}
