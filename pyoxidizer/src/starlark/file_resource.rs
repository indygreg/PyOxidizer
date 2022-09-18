// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    super::{
        python_executable::PythonExecutableValue,
        python_extension_module::PythonExtensionModuleValue,
        python_module_source::PythonModuleSourceValue,
        python_package_distribution_resource::PythonPackageDistributionResourceValue,
        python_package_resource::PythonPackageResourceValue,
    },
    crate::{
        project_building::build_python_executable,
        py_packaging::{binary::PythonBinaryBuilder, resource::AddToFileManifest},
    },
    anyhow::{anyhow, Context, Result},
    log::warn,
    simple_file_manifest::{FileEntry, FileManifest},
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
    std::{ops::DerefMut, path::Path},
    tugger::starlark::file_manifest::FileManifestValue,
};

#[allow(clippy::too_many_arguments)]
pub fn file_manifest_add_python_executable(
    manifest: &mut FileManifestValue,
    env: &crate::environment::Environment,
    prefix: &str,
    exe: &dyn PythonBinaryBuilder,
    target: &str,
    release: bool,
    opt_level: &str,
) -> Result<()> {
    const LABEL: &str = "FileManifest.add_python_executable()";

    let build = build_python_executable(env, &exe.name(), exe, target, opt_level, release)
        .context("building Python executable")?;

    let content = FileEntry::new_from_data(build.exe_data.clone(), true);

    let use_prefix = if prefix == "." { "" } else { prefix };

    let path = Path::new(use_prefix).join(build.exe_name);
    let mut inner = manifest.inner(LABEL).map_err(|e| anyhow!("{:?}", e))?;
    inner
        .add_file_entry(&path, content)
        .context("adding exe content to manifest")?;

    // Add any additional files that the exe builder requires.
    let mut extra_files = FileManifest::default();

    for (path, entry) in build.binary_data.extra_files.iter_entries() {
        warn!("adding extra file {} to {}", path.display(), prefix);
        extra_files.add_file_entry(&Path::new(use_prefix).join(path), entry.clone())?;
    }

    inner.add_manifest(&extra_files)?;
    drop(inner);

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
    const LABEL: &str = "FileManifest.add_python_resource()";

    match resource.get_type() {
        "PythonModuleSource" => {
            let m = match resource.downcast_ref::<PythonModuleSourceValue>() {
                Some(m) => Ok(m.inner(LABEL)?.m.clone()),
                None => Err(ValueError::IncorrectParameterType),
            }?;
            warn!("adding source module {} to {}", m.name, prefix);

            m.add_to_file_manifest(manifest.inner(LABEL)?.deref_mut(), &prefix)
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
                Some(m) => Ok(m.inner(LABEL)?.r.clone()),
                None => Err(ValueError::IncorrectParameterType),
            }?;

            warn!("adding resource file {} to {}", m.symbolic_name(), prefix);
            m.add_to_file_manifest(manifest.inner(LABEL)?.deref_mut(), &prefix)
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
                Some(m) => Ok(m.inner(LABEL)?.r.clone()),
                None => Err(ValueError::IncorrectParameterType),
            }?;
            warn!(
                "adding package distribution resource file {}:{} to {}",
                m.package, m.name, prefix
            );
            m.add_to_file_manifest(manifest.inner(LABEL)?.deref_mut(), &prefix)
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
                Some(e) => Ok(e.inner(LABEL)?.em.clone()),
                None => Err(ValueError::IncorrectParameterType),
            }?;

            warn!("adding extension module {} to {}", extension.name, prefix);
            extension
                .add_to_file_manifest(manifest.inner(LABEL)?.deref_mut(), &prefix)
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
                    "adding Python executable {} to {}",
                    exe.inner(LABEL)?.name(),
                    prefix
                );
                let exe_manifest_value = exe.to_file_manifest(type_values, prefix)?;
                let exe_manifest = exe_manifest_value
                    .downcast_ref::<FileManifestValue>()
                    .unwrap();
                manifest
                    .inner(LABEL)?
                    .add_manifest(&*exe_manifest.inner(LABEL)?)
                    .map_err(|e| {
                        ValueError::from(RuntimeError {
                            code: "PYOXIDIZER_BUILD",
                            message: format!("{:?}", e),
                            label: "add_python_resource".to_string(),
                        })
                    })?;
                manifest.run_path = exe_manifest.run_path.clone();

                Ok(())
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
        let mut this = this.downcast_mut::<FileManifestValue>().unwrap().unwrap();
        file_manifest_add_python_resource(&mut this, env, prefix, &resource)
    }

    FileManifest.add_python_resources(env env, this, prefix: String, resources) {
        let mut this = this.downcast_mut::<FileManifestValue>().unwrap().unwrap();
        file_manifest_add_python_resources(&mut this, env, prefix, &resources)
    }
}

#[cfg(test)]
mod tests {
    use {
        super::super::testutil::*,
        super::*,
        python_packaging::resource::{PythonModuleSource, PythonPackageResource},
        simple_file_manifest::FileData,
        std::path::PathBuf,
    };

    const DEFAULT_CACHE_TAG: &str = "cpython-39";

    #[test]
    fn test_add_python_source_module() -> Result<()> {
        let m = FileManifestValue::new_from_args().unwrap();

        let v = Value::new(PythonModuleSourceValue::new(PythonModuleSource {
            name: "foo.bar".to_string(),
            source: FileData::Memory(vec![]),
            is_package: false,
            cache_tag: DEFAULT_CACHE_TAG.to_string(),
            is_stdlib: false,
            is_test: false,
        }));

        let mut env = test_evaluation_context_builder()?.into_context()?;
        env.set_var("m", m).unwrap();
        env.set_var("v", v).unwrap();

        env.eval("m.add_python_resource('lib', v)")?;

        let m = env.get_var("m").unwrap();
        let m = m.downcast_ref::<FileManifestValue>().unwrap();
        let inner = m.inner("ignored").unwrap();

        let mut entries = inner.iter_entries();

        let (p, c) = entries.next().unwrap();
        assert_eq!(p, &PathBuf::from("lib/foo/__init__.py"));
        assert_eq!(c, &vec![].into());

        let (p, c) = entries.next().unwrap();
        assert_eq!(p, &PathBuf::from("lib/foo/bar.py"));
        assert_eq!(c, &vec![].into());

        assert!(entries.next().is_none());

        Ok(())
    }

    #[test]
    fn test_add_python_resource_data() -> Result<()> {
        let m = FileManifestValue::new_from_args().unwrap();

        let v = Value::new(PythonPackageResourceValue::new(PythonPackageResource {
            leaf_package: "foo.bar".to_string(),
            relative_name: "resource.txt".to_string(),
            data: FileData::Memory(vec![]),
            is_stdlib: false,
            is_test: false,
        }));

        let mut env = test_evaluation_context_builder()?.into_context()?;
        env.set_var("m", m).unwrap();
        env.set_var("v", v).unwrap();

        env.eval("m.add_python_resource('lib', v)")?;

        let m = env.get_var("m").unwrap();
        let m = m.downcast_ref::<FileManifestValue>().unwrap();
        let inner = m.inner("ignored").unwrap();

        let mut entries = inner.iter_entries();
        let (p, c) = entries.next().unwrap();

        assert_eq!(p, &PathBuf::from("lib/foo/bar/resource.txt"));
        assert_eq!(c, &vec![].into());

        assert!(entries.next().is_none());

        Ok(())
    }

    #[test]
    fn test_add_python_resources() {
        starlark_ok("dist = default_python_distribution(); m = FileManifest(); m.add_python_resources('lib', dist.python_resources())");
    }

    #[test]
    fn test_add_python_executable() -> Result<()> {
        let mut env = test_evaluation_context_builder()?.into_context()?;
        add_exe(&mut env)?;

        let m = FileManifestValue::new_from_args().unwrap();

        env.set_var("m", m).unwrap();
        env.eval("m.add_python_resource('bin', exe)")?;

        Ok(())
    }

    #[test]
    // Python 3.8 not supported on aarch64.
    #[cfg(not(target_arch = "aarch64"))]
    fn test_add_python_executable_38() -> Result<()> {
        let mut env = test_evaluation_context_builder()?.into_context()?;

        env.eval("dist = default_python_distribution(python_version='3.8')")?;
        env.eval("exe = dist.to_python_executable('testapp')")?;

        let m = FileManifestValue::new_from_args().unwrap();

        env.set_var("m", m).unwrap();
        env.eval("m.add_python_resource('bin', exe)")?;

        Ok(())
    }

    #[test]
    fn test_add_python_executable_310() -> Result<()> {
        let mut env = test_evaluation_context_builder()?.into_context()?;

        env.eval("dist = default_python_distribution(python_version='3.10')")?;
        env.eval("exe = dist.to_python_executable('testapp')")?;

        let m = FileManifestValue::new_from_args().unwrap();

        env.set_var("m", m).unwrap();
        env.eval("m.add_python_resource('bin', exe)")?;

        Ok(())
    }

    #[test]
    fn test_install() -> Result<()> {
        let mut env = test_evaluation_context_builder()?.into_context()?;
        add_exe(&mut env)?;

        let m = FileManifestValue::new_from_args().unwrap();

        env.set_var("m", m).unwrap();

        env.eval("m.add_python_resource('bin', exe)")?;
        env.eval("m.install('myapp')")?;

        let dest_path = env.build_path().unwrap().join("myapp");
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
