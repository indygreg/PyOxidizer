// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    super::{
        env::{get_context, PyOxidizerEnvironmentContext},
        python_executable::PythonExecutableValue,
        python_resource::{
            PythonExtensionModuleValue, PythonModuleSourceValue,
            PythonPackageDistributionResourceValue, PythonPackageResourceValue,
        },
    },
    crate::{
        project_building::build_python_executable,
        py_packaging::{binary::PythonBinaryBuilder, resource::AddToFileManifest},
    },
    anyhow::Result,
    slog::warn,
    starlark::{
        environment::TypeValues,
        values::{
            error::{RuntimeError, ValueError, INCORRECT_PARAMETER_TYPE_ERROR_CODE},
            none::NoneType,
            {Value, ValueResult},
        },
        {
            starlark_fun, starlark_module, starlark_parse_param_type, starlark_signature,
            starlark_signature_extraction, starlark_signatures,
        },
    },
    std::{ops::Deref, path::Path},
    tugger::{
        file_resource::{FileContent, FileManifest},
        starlark::file_resource::FileManifestValue,
    },
};

#[allow(clippy::too_many_arguments)]
fn file_manifest_add_python_executable(
    manifest: &mut FileManifestValue,
    logger: &slog::Logger,
    prefix: &str,
    exe: &dyn PythonBinaryBuilder,
    target: &str,
    release: bool,
    opt_level: &str,
) -> Result<()> {
    let build = build_python_executable(logger, &exe.name(), exe, target, opt_level, release)?;

    let content = FileContent {
        data: build.exe_data.clone(),
        executable: true,
    };

    let path = Path::new(&prefix).join(build.exe_name);
    manifest.manifest.add_file(&path, &content)?;

    // Add any additional files that the exe builder requires.
    let mut extra_files = FileManifest::default();

    for (path, content) in build.binary_data.extra_files.entries() {
        warn!(logger, "adding extra file {} to {}", path.display(), prefix);
        extra_files.add_file(&Path::new(prefix).join(path), &content)?;
    }

    manifest.manifest.add_manifest(&extra_files)?;

    // Make the last added Python executable the default run target.
    manifest.run_path = Some(path);

    Ok(())
}

/// FileManifest.add_python_resource(prefix, resource)
pub fn file_manifest_add_python_resource(
    manifest: &mut FileManifestValue,
    type_values: &TypeValues,
    prefix: String,
    resource: &Value,
) -> ValueResult {
    let pyoxidizer_context_value = get_context(type_values)?;
    let pyoxidizer_context = pyoxidizer_context_value
        .downcast_ref::<PyOxidizerEnvironmentContext>()
        .ok_or(ValueError::IncorrectParameterType)?;

    match resource.get_type() {
        "PythonModuleSource" => {
            let m = match resource.downcast_ref::<PythonModuleSourceValue>() {
                Some(m) => Ok(m.inner.clone()),
                None => Err(ValueError::IncorrectParameterType),
            }?;
            warn!(
                pyoxidizer_context.logger(),
                "adding source module {} to {}", m.name, prefix
            );

            m.add_to_file_manifest(&mut manifest.manifest, &prefix)
                .map_err(|e| {
                    ValueError::from(RuntimeError {
                        code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                        message: format!("{:?}", e),
                        label: e.to_string(),
                    })
                })
        }
        "PythonPackageResource" => {
            let m = match resource.downcast_ref::<PythonPackageResourceValue>() {
                Some(m) => Ok(m.inner.clone()),
                None => Err(ValueError::IncorrectParameterType),
            }?;

            warn!(
                pyoxidizer_context.logger(),
                "adding resource file {} to {}",
                m.symbolic_name(),
                prefix
            );
            m.add_to_file_manifest(&mut manifest.manifest, &prefix)
                .map_err(|e| {
                    RuntimeError {
                        code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                        message: format!("{:?}", e),
                        label: e.to_string(),
                    }
                    .into()
                })
        }
        "PythonPackageDistributionResource" => {
            let m = match resource.downcast_ref::<PythonPackageDistributionResourceValue>() {
                Some(m) => Ok(m.inner.clone()),
                None => Err(ValueError::IncorrectParameterType),
            }?;
            warn!(
                pyoxidizer_context.logger(),
                "adding package distribution resource file {}:{} to {}", m.package, m.name, prefix
            );
            m.add_to_file_manifest(&mut manifest.manifest, &prefix)
                .map_err(|e| {
                    ValueError::from(RuntimeError {
                        code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                        message: format!("{:?}", e),
                        label: e.to_string(),
                    })
                })
        }
        "PythonExtensionModule" => {
            let extension = match resource.downcast_ref::<PythonExtensionModuleValue>() {
                Some(e) => Ok(e.inner.clone()),
                None => Err(ValueError::IncorrectParameterType),
            }?;

            warn!(
                pyoxidizer_context.logger(),
                "adding extension module {} to {}", extension.name, prefix
            );
            extension
                .add_to_file_manifest(&mut manifest.manifest, &prefix)
                .map_err(|e| {
                    ValueError::from(RuntimeError {
                        code: "PYOXIDIZER_BUILD",
                        message: format!("{:?}", e),
                        label: "add_python_resource".to_string(),
                    })
                })
        }

        "PythonExecutable" => match resource.downcast_ref::<PythonExecutableValue>() {
            Some(exe) => {
                warn!(
                    pyoxidizer_context.logger(),
                    "adding Python executable {} to {}",
                    exe.exe.name(),
                    prefix
                );
                file_manifest_add_python_executable(
                    manifest,
                    pyoxidizer_context.logger(),
                    &prefix,
                    exe.exe.deref(),
                    &pyoxidizer_context.build_target_triple,
                    pyoxidizer_context.build_release,
                    &pyoxidizer_context.build_opt_level,
                )
                .map_err(|e| {
                    ValueError::from(RuntimeError {
                        code: "PYOXIDIZER_BUILD",
                        message: format!("{:?}", e),
                        label: "add_python_resource".to_string(),
                    })
                })
            }
            None => Err(ValueError::IncorrectParameterType),
        },

        t => Err(ValueError::from(RuntimeError {
            code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
            message: format!("resource should be a Python resource type; got {}", t),
            label: "bad argument type".to_string(),
        })),
    }?;

    Ok(Value::new(NoneType::None))
}

/// FileManifest.add_python_resources(prefix, resources)
pub fn file_manifest_add_python_resources(
    manifest: &mut FileManifestValue,
    type_values: &TypeValues,
    prefix: String,
    resources: &Value,
) -> ValueResult {
    for resource in &resources.iter()? {
        file_manifest_add_python_resource(manifest, type_values, prefix.clone(), &resource)?;
    }

    Ok(Value::new(NoneType::None))
}

starlark_module! { file_resource_env =>
    FileManifest.add_python_resource(env env, this, prefix: String, resource) {
        match this.clone().downcast_mut::<FileManifestValue>()? {
            Some(mut manifest) => file_manifest_add_python_resource(&mut manifest, &env, prefix, &resource),
            None => Err(ValueError::IncorrectParameterType),
        }
    }

    FileManifest.add_python_resources(env env, this, prefix: String, resources) {
        match this.clone().downcast_mut::<FileManifestValue>()? {
            Some(mut manifest) => file_manifest_add_python_resources(&mut manifest, &env, prefix, &resources),
            None => Err(ValueError::IncorrectParameterType),
        }
    }
}

#[cfg(test)]
mod tests {
    use {
        super::super::testutil::*,
        super::*,
        python_packaging::resource::{DataLocation, PythonModuleSource, PythonPackageResource},
        std::path::PathBuf,
    };

    const DEFAULT_CACHE_TAG: &str = "cpython-37";

    #[test]
    fn test_add_python_source_module() -> Result<()> {
        let m = Value::new(FileManifestValue {
            manifest: FileManifest::default(),
            run_path: None,
        });

        let v = Value::new(PythonModuleSourceValue::new(PythonModuleSource {
            name: "foo.bar".to_string(),
            source: DataLocation::Memory(vec![]),
            is_package: false,
            cache_tag: DEFAULT_CACHE_TAG.to_string(),
            is_stdlib: false,
            is_test: false,
        }));

        let mut env = StarlarkEnvironment::new()?;
        env.set("m", m)?;
        env.set("v", v)?;

        env.eval("m.add_python_resource('lib', v)")?;

        let m = env.get("m")?;
        let m = m.downcast_ref::<FileManifestValue>().unwrap();

        let mut entries = m.manifest.entries();

        let (p, c) = entries.next().unwrap();
        assert_eq!(p, &PathBuf::from("lib/foo/__init__.py"));
        assert_eq!(
            c,
            &FileContent {
                data: vec![],
                executable: false,
            }
        );

        let (p, c) = entries.next().unwrap();
        assert_eq!(p, &PathBuf::from("lib/foo/bar.py"));
        assert_eq!(
            c,
            &FileContent {
                data: vec![],
                executable: false,
            }
        );

        assert!(entries.next().is_none());

        Ok(())
    }

    #[test]
    fn test_add_python_resource_data() -> Result<()> {
        let m = Value::new(FileManifestValue {
            manifest: FileManifest::default(),
            run_path: None,
        });

        let v = Value::new(PythonPackageResourceValue::new(PythonPackageResource {
            leaf_package: "foo.bar".to_string(),
            relative_name: "resource.txt".to_string(),
            data: DataLocation::Memory(vec![]),
            is_stdlib: false,
            is_test: false,
        }));

        let mut env = StarlarkEnvironment::new()?;
        env.set("m", m)?;
        env.set("v", v)?;

        env.eval("m.add_python_resource('lib', v)")?;

        let m = env.get("m")?;
        let m = m.downcast_ref::<FileManifestValue>().unwrap();

        let mut entries = m.manifest.entries();
        let (p, c) = entries.next().unwrap();

        assert_eq!(p, &PathBuf::from("lib/foo/bar/resource.txt"));
        assert_eq!(
            c,
            &FileContent {
                data: vec![],
                executable: false,
            }
        );

        assert!(entries.next().is_none());

        Ok(())
    }

    #[test]
    fn test_add_python_resources() {
        starlark_ok("dist = default_python_distribution(); m = FileManifest(); m.add_python_resources('lib', dist.python_resources())");
    }

    #[test]
    fn test_add_python_executable() -> Result<()> {
        let mut env = StarlarkEnvironment::new_with_exe()?;

        let m = Value::new(FileManifestValue {
            manifest: FileManifest::default(),
            run_path: None,
        });

        env.set("m", m)?;
        env.eval("m.add_python_resource('bin', exe)")?;

        Ok(())
    }

    #[test]
    fn test_add_python_executable_39() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        env.eval("dist = default_python_distribution(python_version='3.9')")?;
        env.eval("exe = dist.to_python_executable('testapp')")?;

        let m = Value::new(FileManifestValue {
            manifest: FileManifest::default(),
            run_path: None,
        });

        env.set("m", m)?;
        env.eval("m.add_python_resource('bin', exe)")?;

        Ok(())
    }

    #[test]
    fn test_install() -> Result<()> {
        let mut env = StarlarkEnvironment::new_with_exe()?;

        let m = Value::new(FileManifestValue {
            manifest: FileManifest::default(),
            run_path: None,
        });

        env.set("m", m).unwrap();

        env.eval("m.add_python_resource('bin', exe)")?;
        env.eval("m.install('myapp')")?;
        let pyoxidizer_context_value = get_context(&env.type_values).unwrap();
        let pyoxidizer_context = pyoxidizer_context_value
            .downcast_ref::<PyOxidizerEnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)
            .unwrap();

        let dest_path = pyoxidizer_context
            .build_path(&env.type_values)
            .unwrap()
            .join("myapp");
        assert!(dest_path.exists());

        // There should be an executable at myapp/bin/testapp[.exe].
        let app_exe = if cfg!(windows) {
            dest_path.join("bin").join("testapp.exe")
        } else {
            dest_path.join("bin").join("testapp")
        };

        assert!(app_exe.exists());

        Ok(())
    }
}
