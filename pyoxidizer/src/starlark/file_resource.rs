// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    super::{
        env::{get_context, PyOxidizerEnvironmentContext},
        python_executable::PythonExecutable,
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
    itertools::Itertools,
    slog::warn,
    starlark::{
        environment::TypeValues,
        values::{
            error::{RuntimeError, ValueError, INCORRECT_PARAMETER_TYPE_ERROR_CODE},
            none::NoneType,
            {Immutable, Mutable, TypedValue, Value, ValueResult},
        },
        {
            starlark_fun, starlark_module, starlark_parse_param_type, starlark_signature,
            starlark_signature_extraction, starlark_signatures,
        },
    },
    starlark_dialect_build_targets::{
        optional_list_arg, optional_str_arg, required_list_arg, BuildContext, BuildTarget,
        ResolvedTarget, RunMode,
    },
    std::{
        collections::HashSet,
        convert::TryFrom,
        ops::Deref,
        path::{Path, PathBuf},
    },
    tugger::{
        file_resource::{FileContent, FileManifest},
        glob::evaluate_glob,
    },
};

// TODO merge this into `FileValue`?
#[derive(Clone, Debug)]
pub struct FileContentValue {
    pub content: FileContent,
}

impl TypedValue for FileContentValue {
    type Holder = Immutable<FileContentValue>;
    const TYPE: &'static str = "FileContent";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }
}

#[derive(Clone, Debug)]
pub struct FileManifestValue {
    pub manifest: FileManifest,
    /// Optional path to be the default run target.
    run_path: Option<PathBuf>,
}

impl FileManifestValue {
    #[allow(clippy::too_many_arguments)]
    fn add_python_executable(
        &mut self,
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
        self.manifest.add_file(&path, &content)?;

        // Add any additional files that the exe builder requires.
        let mut extra_files = FileManifest::default();

        for (path, content) in build.binary_data.extra_files.entries() {
            warn!(logger, "adding extra file {} to {}", path.display(), prefix);
            extra_files.add_file(&Path::new(prefix).join(path), &content)?;
        }

        self.manifest.add_manifest(&extra_files)?;

        // Make the last added Python executable the default run target.
        self.run_path = Some(path);

        Ok(())
    }
}

impl BuildTarget for FileManifestValue {
    fn build(&mut self, context: &dyn BuildContext) -> Result<ResolvedTarget> {
        let output_path = context.get_state_path("output_path")?;

        warn!(
            context.logger(),
            "installing files to {}",
            output_path.display()
        );
        self.manifest.replace_path(output_path)?;

        // Use the stored run target if available, falling back to the single
        // executable file if non-ambiguous.
        // TODO support defining default run target in data structure.
        let run_mode = if let Some(default) = &self.run_path {
            RunMode::Path {
                path: output_path.join(default),
            }
        } else {
            let exes = self
                .manifest
                .entries()
                .filter(|(_, c)| c.executable)
                .collect_vec();

            if exes.len() == 1 {
                RunMode::Path {
                    path: output_path.join(exes[0].0),
                }
            } else {
                RunMode::None
            }
        };

        Ok(ResolvedTarget {
            run_mode,
            output_path: output_path.to_path_buf(),
        })
    }
}

impl TypedValue for FileManifestValue {
    type Holder = Mutable<FileManifestValue>;
    const TYPE: &'static str = "FileManifest";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }
}

// Starlark functions.
impl FileManifestValue {
    /// FileManifest()
    fn new_from_args() -> ValueResult {
        let manifest = FileManifest::default();

        Ok(Value::new(FileManifestValue {
            manifest,
            run_path: None,
        }))
    }

    /// FileManifest.add_manifest(other)
    pub fn add_manifest(&mut self, other: FileManifestValue) -> ValueResult {
        self.manifest.add_manifest(&other.manifest).map_err(|e| {
            ValueError::from(RuntimeError {
                code: "PYOXIDIZER_BUILD",
                message: e.to_string(),
                label: "add_manifest()".to_string(),
            })
        })?;

        Ok(Value::new(NoneType::None))
    }

    /// FileManifest.add_python_resource(prefix, resource)
    pub fn add_python_resource(
        &mut self,
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

                m.add_to_file_manifest(&mut self.manifest, &prefix)
                    .map_err(|e| {
                        ValueError::from(RuntimeError {
                            code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                            message: e.to_string(),
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
                m.add_to_file_manifest(&mut self.manifest, &prefix)
                    .map_err(|e| {
                        RuntimeError {
                            code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                            message: e.to_string(),
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
                    "adding package distribution resource file {}:{} to {}",
                    m.package,
                    m.name,
                    prefix
                );
                m.add_to_file_manifest(&mut self.manifest, &prefix)
                    .map_err(|e| {
                        ValueError::from(RuntimeError {
                            code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                            message: e.to_string(),
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
                    .add_to_file_manifest(&mut self.manifest, &prefix)
                    .map_err(|e| {
                        ValueError::from(RuntimeError {
                            code: "PYOXIDIZER_BUILD",
                            message: e.to_string(),
                            label: "add_python_resource".to_string(),
                        })
                    })
            }

            "PythonExecutable" => match resource.downcast_ref::<PythonExecutable>() {
                Some(exe) => {
                    warn!(
                        pyoxidizer_context.logger(),
                        "adding Python executable {} to {}",
                        exe.exe.name(),
                        prefix
                    );
                    self.add_python_executable(
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
                            message: e.to_string(),
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
    #[allow(clippy::ptr_arg)]
    pub fn add_python_resources(
        &mut self,
        type_values: &TypeValues,
        prefix: String,
        resources: &Value,
    ) -> ValueResult {
        for resource in &resources.iter()? {
            self.add_python_resource(type_values, prefix.clone(), &resource)?;
        }

        Ok(Value::new(NoneType::None))
    }

    /// FileManifest.install(path, replace=true)
    pub fn install(&self, type_values: &TypeValues, path: String, replace: bool) -> ValueResult {
        let pyoxidizer_context_value = get_context(type_values)?;
        let pyoxidizer_context = pyoxidizer_context_value
            .downcast_ref::<PyOxidizerEnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        let dest_path = pyoxidizer_context.build_path.join(path);

        if replace {
            self.manifest.replace_path(&dest_path)
        } else {
            self.manifest.write_to_path(&dest_path)
        }
        .map_err(|e| {
            ValueError::from(RuntimeError {
                code: "PYOXIDIZER_INSTALL",
                message: format!("error installing FileManifest: {}", e),
                label: "FileManifest.install()".to_string(),
            })
        })?;

        Ok(Value::new(NoneType::None))
    }
}

/// glob(include, exclude=None, relative_to=None)
fn starlark_glob(
    type_values: &TypeValues,
    include: &Value,
    exclude: &Value,
    strip_prefix: &Value,
) -> ValueResult {
    required_list_arg("include", "string", include)?;
    optional_list_arg("exclude", "string", exclude)?;
    let strip_prefix = optional_str_arg("strip_prefix", strip_prefix)?;

    let include = include
        .iter()?
        .iter()
        .map(|x| x.to_string())
        .collect::<Vec<String>>();

    let exclude = match exclude.get_type() {
        "list" => exclude.iter()?.iter().map(|x| x.to_string()).collect(),
        _ => Vec::new(),
    };

    let pyoxidizer_context_value = get_context(type_values)?;
    let pyoxidizer_context = pyoxidizer_context_value
        .downcast_ref::<PyOxidizerEnvironmentContext>()
        .ok_or(ValueError::IncorrectParameterType)?;

    let mut result = HashSet::new();

    // Evaluate all the includes first.
    for v in include {
        for p in evaluate_glob(&pyoxidizer_context.cwd, &v).map_err(|e| {
            ValueError::from(RuntimeError {
                code: "PYOXIDIZER_BUILD",
                message: e.to_string(),
                label: "glob()".to_string(),
            })
        })? {
            result.insert(p);
        }
    }

    // Then apply excludes.
    for v in exclude {
        for p in evaluate_glob(&pyoxidizer_context.cwd, &v).map_err(|e| {
            ValueError::from(RuntimeError {
                code: "PYOXIDIZER_BUILD",
                message: e.to_string(),
                label: "glob()".to_string(),
            })
        })? {
            result.remove(&p);
        }
    }

    let mut manifest = FileManifest::default();

    for path in result {
        let content = FileContent::try_from(path.as_path()).map_err(|e| {
            ValueError::from(RuntimeError {
                code: "PYOXIDIZER_BUILD",
                message: e.to_string(),
                label: "glob()".to_string(),
            })
        })?;

        let path = if let Some(prefix) = &strip_prefix {
            path.strip_prefix(prefix)
                .map_err(|e| {
                    ValueError::from(RuntimeError {
                        code: "PYOXIDIZER_BUILD",
                        message: e.to_string(),
                        label: "glob()".to_string(),
                    })
                })?
                .to_path_buf()
        } else {
            path.to_path_buf()
        };

        manifest.add_file(&path, &content).map_err(|e| {
            ValueError::from(RuntimeError {
                code: "PYOXIDIZER_BUILD",
                message: e.to_string(),
                label: "glob()".to_string(),
            })
        })?;
    }

    Ok(Value::new(FileManifestValue {
        manifest,
        run_path: None,
    }))
}

starlark_module! { file_resource_env =>
    #[allow(clippy::ptr_arg)]
    glob(env env, include, exclude=NoneType::None, strip_prefix=NoneType::None) {
        starlark_glob(&env, &include, &exclude, &strip_prefix)
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    FileManifest(env _env) {
        FileManifestValue::new_from_args()
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    FileManifest.add_manifest(this, other: FileManifestValue) {
        match this.clone().downcast_mut::<FileManifestValue>()? {
            Some(mut manifest) => manifest.add_manifest(other),
            None => Err(ValueError::IncorrectParameterType),
        }
    }

    #[allow(clippy::ptr_arg)]
    FileManifest.add_python_resource(env env, this, prefix: String, resource) {
        match this.clone().downcast_mut::<FileManifestValue>()? {
            Some(mut manifest) => manifest.add_python_resource(&env, prefix, &resource),
            None => Err(ValueError::IncorrectParameterType),
        }
    }

    #[allow(clippy::ptr_arg)]
    FileManifest.add_python_resources(env env, this, prefix: String, resources) {
        match this.clone().downcast_mut::<FileManifestValue>()? {
            Some(mut manifest) => manifest.add_python_resources(&env, prefix, &resources),
            None => Err(ValueError::IncorrectParameterType),
        }
    }

    #[allow(clippy::ptr_arg)]
    FileManifest.install(env env, this, path: String, replace: bool = true) {
        match this.clone().downcast_ref::<FileManifestValue>() {
            Some(manifest) => manifest.install(&env, path, replace),
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
    fn test_new_file_manifest() {
        let m = starlark_ok("FileManifest()");
        assert_eq!(m.get_type(), "FileManifest");

        let m = m.downcast_ref::<FileManifestValue>().unwrap();
        assert_eq!(m.manifest, FileManifest::default());
    }

    #[test]
    fn test_add_file_manifest() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;
        env.eval("m1 = FileManifest()")?;
        env.eval("m2 = FileManifest()")?;

        env.eval("m1.add_manifest(m2)")?;

        Ok(())
    }

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

        let dest_path = pyoxidizer_context.build_path.join("myapp");
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
