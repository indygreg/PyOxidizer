// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Build a custom libraries containing Python.
*/

use {
    crate::{
        environment::Environment,
        py_packaging::{distribution::AppleSdkInfo, embedding::LinkingAnnotation},
    },
    anyhow::{anyhow, Context, Result},
    apple_sdk::AppleSdk,
    duct::cmd,
    log::warn,
    python_packaging::libpython::LibPythonBuildContext,
    simple_file_manifest::FileData,
    std::{
        collections::BTreeSet,
        ffi::OsStr,
        fs,
        fs::create_dir_all,
        hash::Hasher,
        io::{BufRead, BufReader, Cursor},
        path::{Path, PathBuf},
    },
};

#[cfg(target_family = "unix")]
use std::os::unix::ffi::OsStrExt;

#[cfg(unix)]
fn osstr_to_bytes(s: &OsStr) -> Result<Vec<u8>> {
    Ok(s.as_bytes().to_vec())
}

#[cfg(not(unix))]
fn osstr_to_bytes(s: &OsStr) -> Result<Vec<u8>> {
    let utf8: &str = s
        .to_str()
        .ok_or_else(|| anyhow!("invalid UTF-8 filename"))?;
    Ok(utf8.as_bytes().to_vec())
}

/// Produce the content of the config.c file containing built-in extensions.
pub fn make_config_c<T>(extensions: &[(T, T)]) -> String
where
    T: AsRef<str>,
{
    // It is easier to construct the file from scratch than parse the template
    // and insert things in the right places.
    let mut lines: Vec<String> = vec!["#include \"Python.h\"".to_string()];

    // Declare the initialization functions.
    for (_name, init_fn) in extensions {
        if init_fn.as_ref() != "NULL" {
            lines.push(format!("extern PyObject* {}(void);", init_fn.as_ref()));
        }
    }

    lines.push(String::from("struct _inittab _PyImport_Inittab[] = {"));

    for (name, init_fn) in extensions {
        lines.push(format!("{{\"{}\", {}}},", name.as_ref(), init_fn.as_ref()));
    }

    lines.push(String::from("{0, 0}"));
    lines.push(String::from("};"));

    lines.join("\n")
}

/// The `ar` crate doesn't support emitting the symbols index. So call out to `ar s` ourselves.
fn create_ar_symbols_index(dest_dir: &Path, lib_data: &[u8]) -> Result<Vec<u8>> {
    let lib_path = dest_dir.join("lib.a");

    std::fs::write(&lib_path, lib_data).context("writing archive to temporary file")?;

    warn!("invoking `ar s` to index archive symbols");
    let command = cmd("ar", &["s".to_string(), lib_path.display().to_string()])
        .stderr_to_stdout()
        .unchecked()
        .reader()?;
    {
        let reader = BufReader::new(&command);
        for line in reader.lines() {
            warn!("{}", line?);
        }
    }
    let output = command
        .try_wait()?
        .ok_or_else(|| anyhow!("unable to wait on ar"))?;

    if !output.status.success() {
        return Err(anyhow!("failed to invoke `ar s`"));
    }

    Ok(std::fs::read(&lib_path)?)
}

fn ar_header(path: &Path) -> Result<ar::Header> {
    let filename = path
        .file_name()
        .ok_or_else(|| anyhow!("could not determine file name"))?;

    let identifier = osstr_to_bytes(filename)?;

    let metadata = std::fs::metadata(path)?;

    let mut header = ar::Header::from_metadata(identifier, &metadata);

    header.set_uid(0);
    header.set_gid(0);
    header.set_mtime(0);
    header.set_mode(0o644);

    Ok(header)
}

fn assemble_archive_gnu(objects: &[PathBuf], temp_dir: &Path) -> Result<Vec<u8>> {
    let buffer = Cursor::new(vec![]);

    let identifiers = objects
        .iter()
        .map(|p| {
            Ok(p.file_name()
                .ok_or_else(|| anyhow!("object file name could not be determined"))?
                .to_string_lossy()
                .as_bytes()
                .to_vec())
        })
        .collect::<Result<Vec<_>>>()?;

    let mut builder = ar::GnuBuilder::new(buffer, identifiers);

    for path in objects {
        let header = ar_header(path)
            .with_context(|| format!("resolving ar header for {}", path.display()))?;
        let fh = std::fs::File::open(path)?;

        builder.append(&header, fh)?;
    }

    let data = builder.into_inner()?.into_inner();

    create_ar_symbols_index(temp_dir, &data)
}

fn assemble_archive_bsd(objects: &[PathBuf], temp_dir: &Path) -> Result<Vec<u8>> {
    let buffer = Cursor::new(vec![]);

    let mut builder = ar::Builder::new(buffer);

    for path in objects {
        let header = ar_header(path)
            .with_context(|| format!("resolving ar header for {}", path.display()))?;
        let fh = std::fs::File::open(path)?;

        builder.append(&header, fh)?;
    }

    let data = builder.into_inner()?.into_inner();

    create_ar_symbols_index(temp_dir, &data)
}

/// Represents a built libpython.
#[derive(Debug)]
pub struct LibpythonInfo {
    /// Raw data constituting static libpython library.
    pub libpython_data: Vec<u8>,

    /// Describes annotations necessary to link this libpython.
    pub linking_annotations: Vec<LinkingAnnotation>,
}

/// Create a static libpython from a Python distribution.
///
/// Returns a struct describing the generated libpython.
#[allow(clippy::too_many_arguments)]
pub fn link_libpython(
    env: &Environment,
    context: &LibPythonBuildContext,
    host_triple: &str,
    target_triple: &str,
    opt_level: &str,
    apple_sdk_info: Option<&AppleSdkInfo>,
) -> Result<LibpythonInfo> {
    let temp_dir = env.temporary_directory("pyoxidizer-libpython")?;

    let config_c_dir = temp_dir.path().join("config_c");
    std::fs::create_dir(&config_c_dir).context("creating config_c subdirectory")?;

    let libpython_dir = temp_dir.path().join("libpython");
    std::fs::create_dir(&libpython_dir).context("creating libpython subdirectory")?;

    let mut linking_annotations = vec![];

    let windows = crate::environment::WINDOWS_TARGET_TRIPLES.contains(&target_triple);

    // We derive a custom Modules/config.c from the set of extension modules.
    // We need to do this because config.c defines the built-in extensions and
    // their initialization functions and the file generated by the source
    // distribution may not align with what we want.
    warn!(
        "deriving custom config.c from {} extension modules",
        context.init_functions.len()
    );
    let config_c_source = make_config_c(&context.init_functions.iter().collect::<Vec<_>>());
    let config_c_path = config_c_dir.join("config.c");

    // The output file name is dependent on whether the input file name is absolute.
    let config_object_path = if config_c_path.has_root() {
        let dirname = config_c_path
            .parent()
            .ok_or_else(|| anyhow!("could not determine parent directory"))?;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        hasher.write(dirname.to_string_lossy().as_bytes());

        config_c_dir.join(format!("{:016x}-{}", hasher.finish(), "config.o"))
    } else {
        config_c_dir.join("config.o")
    };

    fs::write(&config_c_path, config_c_source.as_bytes())?;

    // Gather all includes into the temporary directory.
    for (rel_path, location) in &context.includes {
        let full = config_c_dir.join(rel_path);
        create_dir_all(
            full.parent()
                .ok_or_else(|| anyhow!("unable to resolve parent directory"))?,
        )?;
        let data = location.resolve_content()?;
        std::fs::write(&full, &data)?;
    }

    warn!("compiling custom config.c to object file");
    let mut build = cc::Build::new();

    if let Some(flags) = &context.inittab_cflags {
        for flag in flags {
            build.flag(flag);
        }
    }

    // The cc crate will pick up the default Apple SDK by default. There could be a mismatch
    // between it and what we want. For example, if we're building for aarch64 but the default
    // SDK is a 10.15 SDK that doesn't support ARM. We attempt to mitigate this by resolving
    // a compatible Apple SDK and pointing the compiler invocation at it via compiler flags.
    if target_triple.contains("-apple-") {
        let sdk_info = apple_sdk_info.ok_or_else(|| {
            anyhow!("Apple SDK info should be defined when targeting Apple platforms")
        })?;

        let sdk = env
            .resolve_apple_sdk(sdk_info)
            .context("resolving Apple SDK to use")?;

        build.flag("-isysroot");
        build.flag(&format!("{}", sdk.path().display()));
    }

    build
        .out_dir(&config_c_dir)
        .host(host_triple)
        .target(target_triple)
        .opt_level_str(opt_level)
        .file(&config_c_path)
        .include(&config_c_dir)
        .cargo_metadata(false)
        .compile("irrelevant");

    warn!("resolving inputs for custom Python library...");

    let mut objects = BTreeSet::new();

    // Link our custom config.c's object file.
    objects.insert(config_object_path);

    for (i, location) in context.object_files.iter().enumerate() {
        match location {
            FileData::Memory(data) => {
                let out_path = libpython_dir.join(format!("libpython.{}.o", i));
                fs::write(&out_path, data)?;
                objects.insert(out_path);
            }
            FileData::Path(p) => {
                objects.insert(p.clone());
            }
        }
    }

    for framework in &context.frameworks {
        linking_annotations.push(LinkingAnnotation::LinkFramework(framework.to_string()));
    }

    for lib in &context.system_libraries {
        linking_annotations.push(LinkingAnnotation::LinkLibrary(lib.to_string()));
    }

    for lib in &context.dynamic_libraries {
        linking_annotations.push(LinkingAnnotation::LinkLibrary(lib.to_string()));
    }

    for lib in &context.static_libraries {
        linking_annotations.push(LinkingAnnotation::LinkLibraryStatic(lib.to_string()));
    }

    // Python 3.9+ on macOS uses __builtin_available(), which requires
    // ___isOSVersionAtLeast(), which is part of libclang_rt. However,
    // libclang_rt isn't linked by default by Rust. So unless something else
    // pulls it in, we'll get unresolved symbol errors when attempting to link
    // the final binary. Our solution to this is to always annotate
    // `clang_rt.<platform>` as a library dependency of our static libpython.
    if target_triple.ends_with("-apple-darwin") {
        if let Some(path) = macos_clang_search_path()? {
            linking_annotations.push(LinkingAnnotation::Search(path));
        }

        linking_annotations.push(LinkingAnnotation::LinkLibrary("clang_rt.osx".to_string()));
    }

    warn!("linking customized Python library...");

    let objects = objects.into_iter().collect::<Vec<_>>();

    let libpython_data = if target_triple.contains("-linux-") {
        assemble_archive_gnu(&objects, &libpython_dir)?
    } else if target_triple.contains("-apple-") {
        assemble_archive_bsd(&objects, &libpython_dir)?
    } else {
        let mut build = cc::Build::new();
        build.out_dir(&libpython_dir);
        build.host(host_triple);
        build.target(target_triple);
        build.opt_level_str(opt_level);
        // We handle this ourselves.
        build.cargo_metadata(false);

        for object in objects {
            build.object(object);
        }

        build.compile("python");

        std::fs::read(libpython_dir.join(if windows { "python.lib" } else { "libpython.a" }))
            .context("reading libpython")?
    };

    warn!("{} byte Python library created", libpython_data.len());

    for path in &context.library_search_paths {
        linking_annotations.push(LinkingAnnotation::SearchNative(path.clone()));
    }

    temp_dir.close().context("closing temporary directory")?;

    Ok(LibpythonInfo {
        libpython_data,
        linking_annotations,
    })
}

/// Attempt to resolve the linker search path for clang libraries.
fn macos_clang_search_path() -> Result<Option<PathBuf>> {
    let output = std::process::Command::new("clang")
        .arg("--print-search-dirs")
        .output()?;
    if !output.status.success() {
        return Ok(None);
    }

    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if line.contains("libraries: =") {
            let path = line
                .split('=')
                .nth(1)
                .ok_or_else(|| anyhow!("could not parse libraries line"))?;
            return Ok(Some(PathBuf::from(path).join("lib").join("darwin")));
        }
    }

    Ok(None)
}
