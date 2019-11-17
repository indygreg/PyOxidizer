// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use lazy_static::lazy_static;
use serde::Deserialize;
use slog::warn;
use std::collections::{BTreeMap, HashMap};
use std::fs::{create_dir_all, read_dir, read_to_string};
use std::path::{Path, PathBuf};

use super::distribution::PythonDistributionInfo;
use super::resource::BuiltExtensionModule;

lazy_static! {
    static ref MODIFIED_DISTUTILS_FILES: BTreeMap<&'static str, &'static [u8]> = {
        let mut res: BTreeMap<&'static str, &'static [u8]> = BTreeMap::new();

        res.insert(
            "command/build_ext.py",
            include_bytes!("../distutils/command/build_ext.py"),
        );
        res.insert(
            "_msvccompiler.py",
            include_bytes!("../distutils/_msvccompiler.py"),
        );
        res.insert(
            "unixccompiler.py",
            include_bytes!("../distutils/unixccompiler.py"),
        );

        res
    };
}

/// Prepare a hacked install of distutils to use with Python packaging.
///
/// The idea is we use the distutils in the distribution as a base then install
/// our own hacks on top of it to make it perform the functionality that we want.
/// This enables things using it (like setup.py scripts) to invoke our
/// functionality, without requiring them to change anything.
///
/// An alternate considered implementation was to "prepend" code to the invoked
/// setup.py or Python process so that the in-process distutils was monkeypatched.
/// This approach felt less robust than modifying distutils itself because a
/// modified distutils will survive multiple process invocations, unlike a
/// monkeypatch. People do weird things in setup.py scripts and we want to
/// support as many as possible.
pub fn prepare_hacked_distutils(
    logger: &slog::Logger,
    dist: &PythonDistributionInfo,
    dest_dir: &Path,
    extra_python_paths: &[&Path],
) -> Result<HashMap<String, String>, String> {
    let extra_sys_path = dest_dir.join("packages");

    warn!(
        logger,
        "installing modified distutils to {}",
        extra_sys_path.display()
    );

    let orig_distutils_path = dist.stdlib_path.join("distutils");
    let dest_distutils_path = extra_sys_path.join("distutils");

    for entry in walkdir::WalkDir::new(&orig_distutils_path) {
        match entry {
            Ok(entry) => {
                if entry.path().is_dir() {
                    continue;
                }

                let source_path = entry.path();
                let rel_path = source_path
                    .strip_prefix(&orig_distutils_path)
                    .or_else(|_| Err("unable to strip prefix"))?;
                let dest_path = dest_distutils_path.join(rel_path);

                let dest_dir = dest_path.parent().unwrap();
                std::fs::create_dir_all(&dest_dir).or_else(|e| Err(e.to_string()))?;
                std::fs::copy(&source_path, &dest_path).or_else(|e| Err(e.to_string()))?;
            }
            Err(e) => return Err(e.to_string()),
        }
    }

    for (path, data) in MODIFIED_DISTUTILS_FILES.iter() {
        let dest_path = dest_distutils_path.join(path);

        warn!(logger, "modifying distutils/{} for oxidation", path);
        std::fs::write(dest_path, data).or_else(|e| Err(e.to_string()))?;
    }

    let state_dir = dest_dir.join("pyoxidizer-build-state");
    create_dir_all(&state_dir).or_else(|e| Err(e.to_string()))?;

    let mut python_paths = vec![extra_sys_path.display().to_string()];
    python_paths.extend(extra_python_paths.iter().map(|p| p.display().to_string()));

    let path_separator = if cfg!(windows) { ";" } else { ":" };

    let python_path = python_paths.join(path_separator);

    let mut res = HashMap::new();
    res.insert("PYTHONPATH".to_string(), python_path);
    res.insert(
        "PYOXIDIZER_DISTUTILS_STATE_DIR".to_string(),
        state_dir.display().to_string(),
    );
    res.insert("PYOXIDIZER".to_string(), "1".to_string());

    Ok(res)
}

#[derive(Debug, Deserialize)]
struct DistutilsExtensionState {
    name: String,
    objects: Vec<String>,
    output_filename: String,
    libraries: Vec<String>,
    library_dirs: Vec<String>,
    runtime_library_dirs: Vec<String>,
}

pub fn read_built_extensions(state_dir: &Path) -> Result<Vec<BuiltExtensionModule>, String> {
    let mut res = Vec::new();

    let entries = read_dir(state_dir).or_else(|e| Err(e.to_string()))?;

    for entry in entries {
        let entry = entry.or_else(|e| Err(e.to_string()))?;
        let path = entry.path();
        let file_name = path.file_name().unwrap().to_str().unwrap();

        if !file_name.starts_with("extension.") || !file_name.ends_with(".json") {
            continue;
        }

        let data = read_to_string(&path).or_else(|e| Err(e.to_string()))?;

        let info: DistutilsExtensionState =
            serde_json::from_str(&data).or_else(|e| Err(e.to_string()))?;

        let module_components: Vec<&str> = info.name.split('.').collect();
        let final_name = module_components[module_components.len() - 1];
        let init_fn = "PyInit_".to_string() + final_name;

        let mut object_file_data = Vec::new();

        for object_path in &info.objects {
            let path = PathBuf::from(object_path);
            let data = std::fs::read(path).or_else(|e| Err(e.to_string()))?;

            object_file_data.push(data);
        }

        // TODO packaging rule functionality for requiring / denying shared library
        // linking, annotating licenses of 3rd party libraries, disabling libraries
        // wholesale, etc.

        res.push(BuiltExtensionModule {
            name: info.name.clone(),
            init_fn,
            object_file_data,
            is_package: final_name == "__init__",
            libraries: info.libraries,
            library_dirs: info.library_dirs.iter().map(PathBuf::from).collect(),
        });
    }

    Ok(res)
}
