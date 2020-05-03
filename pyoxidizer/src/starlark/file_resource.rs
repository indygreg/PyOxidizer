// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    super::env::EnvironmentContext,
    super::python_executable::PythonExecutable,
    super::python_resource::PythonExtensionModuleFlavor,
    super::python_resource::{
        PythonBytecodeModule, PythonExtensionModule, PythonPackageDistributionResource,
        PythonPackageResource, PythonSourceModule,
    },
    super::target::{BuildContext, BuildTarget, ResolvedTarget, RunMode},
    super::util::{
        optional_list_arg, optional_str_arg, required_bool_arg, required_list_arg,
        required_str_arg, required_type_arg,
    },
    crate::app_packaging::glob::evaluate_glob,
    crate::app_packaging::resource::{
        FileContent as RawFileContent, FileManifest as RawFileManifest,
    },
    crate::project_building::build_python_executable,
    crate::py_packaging::binary::PythonBinaryBuilder,
    crate::py_packaging::resource::AddToFileManifest,
    crate::py_packaging::standalone_distribution::DistributionExtensionModule,
    anyhow::Result,
    itertools::Itertools,
    python_packaging::resource::PythonModuleBytecodeFromSource,
    slog::warn,
    starlark::environment::Environment,
    starlark::values::{
        default_compare, RuntimeError, TypedValue, Value, ValueError, ValueResult,
        INCORRECT_PARAMETER_TYPE_ERROR_CODE,
    },
    starlark::{
        any, immutable, not_supported, starlark_fun, starlark_module, starlark_signature,
        starlark_signature_extraction, starlark_signatures,
    },
    std::any::Any,
    std::cmp::Ordering,
    std::collections::{HashMap, HashSet},
    std::convert::TryFrom,
    std::ops::Deref,
    std::path::Path,
};

#[derive(Clone, Debug)]
pub struct FileContent {
    pub content: RawFileContent,
}

impl TypedValue for FileContent {
    immutable!();
    any!();
    not_supported!(binop, container, function, get_hash, to_int);

    fn to_str(&self) -> String {
        "FileContent<>".to_string()
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_type(&self) -> &'static str {
        "FileContent"
    }

    fn to_bool(&self) -> bool {
        true
    }

    fn compare(&self, other: &dyn TypedValue, _recursion: u32) -> Result<Ordering, ValueError> {
        default_compare(self, other)
    }
}

#[derive(Clone, Debug)]
pub struct FileManifest {
    pub manifest: RawFileManifest,
}

impl FileManifest {
    // TODO implement.
    fn add_bytecode_module(&self, _prefix: &str, _module: &PythonModuleBytecodeFromSource) {
        println!("support for adding bytecode modules not yet implemented");
    }

    // TODO implement.
    fn add_extension_module(&self, _prefix: &str, _em: &DistributionExtensionModule) {
        println!("support for adding extension modules not yet implemented");
    }

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

        let content = RawFileContent {
            data: build.exe_data.clone(),
            executable: true,
        };

        let path = Path::new(&prefix).join(build.exe_name);
        self.manifest.add_file(&path, &content)?;

        // Add any additional files that the exe builder requires.
        let mut extra_files = RawFileManifest::default();

        for (path, content) in build.binary_data.extra_files.entries() {
            warn!(logger, "adding extra file {} to {}", path.display(), prefix);
            extra_files.add_file(&Path::new(prefix).join(path), &content)?;
        }

        self.manifest.add_manifest(&extra_files)?;

        Ok(())
    }
}

impl BuildTarget for FileManifest {
    fn build(&mut self, context: &BuildContext) -> Result<ResolvedTarget> {
        warn!(
            &context.logger,
            "installing files to {}",
            context.output_path.display()
        );
        self.manifest.replace_path(&context.output_path)?;

        // If there exists a single executable, make it the run target.
        // TODO support defining default run target in data structure.

        let exes = self
            .manifest
            .entries()
            .filter(|(_, c)| c.executable)
            .collect_vec();
        let run_mode = if exes.len() == 1 {
            RunMode::Path {
                path: context.output_path.join(exes[0].0),
            }
        } else {
            RunMode::None
        };

        Ok(ResolvedTarget {
            run_mode,
            output_path: context.output_path.clone(),
        })
    }
}

impl TypedValue for FileManifest {
    immutable!();
    any!();
    not_supported!(binop, container, function, get_hash, to_int);

    fn to_str(&self) -> String {
        "FileManifest<>".to_string()
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_type(&self) -> &'static str {
        "FileManifest"
    }

    fn to_bool(&self) -> bool {
        true
    }

    fn compare(&self, other: &dyn TypedValue, _recursion: u32) -> Result<Ordering, ValueError> {
        default_compare(self, other)
    }
}

// Starlark functions.
impl FileManifest {
    /// FileManifest()
    fn new_from_args() -> ValueResult {
        let manifest = RawFileManifest::default();

        Ok(Value::new(FileManifest { manifest }))
    }

    /// FileManifest.add_manifest(other)
    pub fn add_manifest(&mut self, other: &Value) -> ValueResult {
        required_type_arg("other", "FileManifest", other)?;

        let other = other.downcast_apply(|other: &FileManifest| other.manifest.clone());

        self.manifest.add_manifest(&other).or_else(|e| {
            Err(RuntimeError {
                code: "PYOXIDIZER_BUILD",
                message: e.to_string(),
                label: "add_manifest()".to_string(),
            }
            .into())
        })?;

        Ok(Value::new(None))
    }

    /// FileManifest.add_python_resource(prefix, resource)
    pub fn add_python_resource(
        &mut self,
        env: &Environment,
        prefix: &Value,
        resource: &Value,
    ) -> ValueResult {
        let prefix = required_str_arg("prefix", &prefix)?;

        let context = env.get("CONTEXT").expect("CONTEXT not set");
        let logger = context.downcast_apply(|x: &EnvironmentContext| x.logger.clone());

        match resource.get_type() {
            "PythonSourceModule" => {
                let m = resource.downcast_apply(|m: &PythonSourceModule| m.module.clone());
                warn!(logger, "adding source module {} to {}", m.name, prefix);

                m.add_to_file_manifest(&mut self.manifest, &prefix)
                    .or_else(|e| {
                        Err(RuntimeError {
                            code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                            message: e.to_string(),
                            label: e.to_string(),
                        }
                        .into())
                    })
            }
            "PythonBytecodeModule" => {
                let m = resource.downcast_apply(|m: &PythonBytecodeModule| m.module.clone());
                warn!(logger, "adding bytecode module {} to {}", m.name, prefix);
                self.add_bytecode_module(&prefix, &m);

                Ok(())
            }
            "PythonPackageResource" => {
                let m = resource.downcast_apply(|m: &PythonPackageResource| m.data.clone());
                warn!(
                    logger,
                    "adding resource file {} to {}",
                    m.symbolic_name(),
                    prefix
                );
                m.add_to_file_manifest(&mut self.manifest, &prefix)
                    .or_else(|e| {
                        Err(RuntimeError {
                            code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                            message: e.to_string(),
                            label: e.to_string(),
                        }
                        .into())
                    })
            }
            "PythonPackageDistributionResource" => {
                let m = resource
                    .downcast_apply(|m: &PythonPackageDistributionResource| m.resource.clone());
                warn!(
                    logger,
                    "adding package distribution resource file {}:{} to {}",
                    m.package,
                    m.name,
                    prefix
                );
                m.add_to_file_manifest(&mut self.manifest, &prefix)
                    .or_else(|e| {
                        Err(RuntimeError {
                            code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                            message: e.to_string(),
                            label: e.to_string(),
                        }
                        .into())
                    })
            }
            "PythonExtensionModule" => {
                let m = resource.downcast_apply(|m: &PythonExtensionModule| m.em.clone());

                match m {
                    PythonExtensionModuleFlavor::Distribution(m) => {
                        warn!(
                            logger,
                            "adding distribution module {} to {}", m.module, prefix
                        );
                        self.add_extension_module(&prefix, &m);
                        Ok(())
                    }
                    PythonExtensionModuleFlavor::StaticallyLinked(m) => {
                        warn!(
                            logger,
                            "adding statically linked extension module {} to {}", m.name, prefix
                        );
                        m.add_to_file_manifest(&mut self.manifest, &prefix)
                            .or_else(|e| {
                                Err(RuntimeError {
                                    code: "PYOXIDIZER_BUILD",
                                    message: e.to_string(),
                                    label: "add_python_resource".to_string(),
                                }
                                .into())
                            })
                    }
                    PythonExtensionModuleFlavor::DynamicLibrary(m) => {
                        warn!(
                            logger,
                            "adding dynamic library extension module {} to {}", m.name, prefix
                        );
                        m.add_to_file_manifest(&mut self.manifest, &prefix)
                            .or_else(|e| {
                                Err(RuntimeError {
                                    code: "PYOXIDIZER_BUILD",
                                    message: e.to_string(),
                                    label: "add_python_resource".to_string(),
                                }
                                .into())
                            })
                    }
                }
            }
            "PythonExecutable" => {
                let context = env.get("CONTEXT").expect("CONTEXT not defined");
                let (target, release, opt_level) =
                    context.downcast_apply(|x: &EnvironmentContext| {
                        (
                            x.build_target_triple.clone(),
                            x.build_release,
                            x.build_opt_level.clone(),
                        )
                    });

                let raw_exe = resource.0.borrow();
                let exe = raw_exe.as_any().downcast_ref::<PythonExecutable>().unwrap();
                warn!(
                    logger,
                    "adding Python executable {} to {}",
                    exe.exe.name(),
                    prefix
                );
                self.add_python_executable(
                    &logger,
                    &prefix,
                    exe.exe.deref(),
                    &target,
                    release,
                    &opt_level,
                )
                .or_else(|e| {
                    Err(RuntimeError {
                        code: "PYOXIDIZER_BUILD",
                        message: e.to_string(),
                        label: "add_python_resource".to_string(),
                    }
                    .into())
                })
            }
            t => Err(RuntimeError {
                code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                message: format!("resource should be a Python resource type; got {}", t),
                label: "bad argument type".to_string(),
            }
            .into()),
        }?;

        Ok(Value::new(None))
    }

    /// FileManifest.add_python_resources(prefix, resources)
    #[allow(clippy::ptr_arg)]
    pub fn add_python_resources(
        &mut self,
        env: &Environment,
        prefix: &Value,
        resources: &Value,
    ) -> ValueResult {
        required_str_arg("prefix", &prefix)?;

        for resource in resources.into_iter()? {
            self.add_python_resource(env, &prefix.clone(), &resource)?;
        }

        Ok(Value::new(None))
    }

    /// FileManifest.install(path, replace=true)
    pub fn install(&self, env: &Environment, path: &Value, replace: &Value) -> ValueResult {
        let path = required_str_arg("path", &path)?;
        let replace = required_bool_arg("replace", &replace)?;

        let context = env.get("CONTEXT").expect("CONTEXT not defined");
        let build_path = context.downcast_apply(|x: &EnvironmentContext| x.build_path.clone());

        let dest_path = build_path.join(path);

        if replace {
            self.manifest.replace_path(&dest_path)
        } else {
            self.manifest.write_to_path(&dest_path)
        }
        .or_else(|e| {
            Err(RuntimeError {
                code: "PYOXIDIZER_INSTALL",
                message: format!("error installing FileManifest: {}", e),
                label: "FileManifest.install()".to_string(),
            }
            .into())
        })?;

        Ok(Value::new(None))
    }
}

/// glob(include, exclude=None, relative_to=None)
fn starlark_glob(
    env: &Environment,
    include: &Value,
    exclude: &Value,
    strip_prefix: &Value,
) -> ValueResult {
    required_list_arg("include", "string", include)?;
    optional_list_arg("exclude", "string", exclude)?;
    let strip_prefix = optional_str_arg("strip_prefix", strip_prefix)?;

    let include = include
        .into_iter()?
        .map(|x| x.to_string())
        .collect::<Vec<String>>();

    let exclude = match exclude.get_type() {
        "list" => exclude.into_iter()?.map(|x| x.to_string()).collect(),
        _ => Vec::new(),
    };

    let context = env.get("CONTEXT").expect("unable to get CONTEXT");
    let cwd = context.downcast_apply(|x: &EnvironmentContext| x.cwd.clone());

    let mut result = HashSet::new();

    // Evaluate all the includes first.
    for v in include {
        for p in evaluate_glob(&cwd, &v).or_else(|e| {
            Err(RuntimeError {
                code: "PYOXIDIZER_BUILD",
                message: e.to_string(),
                label: "glob()".to_string(),
            }
            .into())
        })? {
            result.insert(p);
        }
    }

    // Then apply excludes.
    for v in exclude {
        for p in evaluate_glob(&cwd, &v).or_else(|e| {
            Err(RuntimeError {
                code: "PYOXIDIZER_BUILD",
                message: e.to_string(),
                label: "glob()".to_string(),
            }
            .into())
        })? {
            result.remove(&p);
        }
    }

    let mut manifest = RawFileManifest::default();

    for path in result {
        let content = RawFileContent::try_from(path.as_path()).or_else(|e| {
            Err(RuntimeError {
                code: "PYOXIDIZER_BUILD",
                message: e.to_string(),
                label: "glob()".to_string(),
            }
            .into())
        })?;

        let path = if let Some(prefix) = &strip_prefix {
            path.strip_prefix(prefix)
                .or_else(|e| {
                    Err(RuntimeError {
                        code: "PYOXIDIZER_BUILD",
                        message: e.to_string(),
                        label: "glob()".to_string(),
                    }
                    .into())
                })?
                .to_path_buf()
        } else {
            path.to_path_buf()
        };

        manifest.add_file(&path, &content).or_else(|e| {
            Err(RuntimeError {
                code: "PYOXIDIZER_BUILD",
                message: e.to_string(),
                label: "glob()".to_string(),
            }
            .into())
        })?;
    }

    Ok(Value::new(FileManifest { manifest }))
}

starlark_module! { file_resource_env =>
    #[allow(clippy::ptr_arg)]
    glob(env env, include, exclude=None, strip_prefix=None) {
        starlark_glob(&env, &include, &exclude, &strip_prefix)
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    FileManifest(env _env) {
        FileManifest::new_from_args()
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    FileManifest.add_manifest(this, other) {
        this.downcast_apply_mut(|manifest: &mut FileManifest| {
            manifest.add_manifest(&other)
        })
    }

    #[allow(clippy::ptr_arg)]
    FileManifest.add_python_resource(env env, this, prefix, resource) {
        this.downcast_apply_mut(|manifest: &mut FileManifest| {
            manifest.add_python_resource(&env, &prefix, &resource)
        })
    }

    #[allow(clippy::ptr_arg)]
    FileManifest.add_python_resources(env env, this, prefix, resources) {
        this.downcast_apply_mut(|manifest: &mut FileManifest| {
            manifest.add_python_resources(&env, &prefix, &resources)
        })
    }

    #[allow(clippy::ptr_arg)]
    FileManifest.install(env env, this, path, replace=true) {
        this.downcast_apply(|manifest: &FileManifest| {
            manifest.install(&env, &path, &replace)
        })
    }
}

#[cfg(test)]
mod tests {
    use {
        super::super::testutil::*,
        super::*,
        python_packaging::resource::{
            DataLocation, PythonModuleSource, PythonPackageResource as RawPackageResource,
        },
        std::path::PathBuf,
    };

    const DEFAULT_CACHE_TAG: &str = "cpython-37";

    #[test]
    fn test_new_file_manifest() {
        let m = starlark_ok("FileManifest()");
        assert_eq!(m.get_type(), "FileManifest");

        m.downcast_apply(|m: &FileManifest| {
            assert_eq!(m.manifest, RawFileManifest::default());
        })
    }

    #[test]
    fn test_add_python_source_module() {
        let m = Value::new(FileManifest {
            manifest: RawFileManifest::default(),
        });

        let v = Value::new(PythonSourceModule {
            module: PythonModuleSource {
                name: "foo.bar".to_string(),
                source: DataLocation::Memory(vec![]),
                is_package: false,
                cache_tag: DEFAULT_CACHE_TAG.to_string(),
            },
        });

        let mut env = starlark_env();
        env.set("m", m).unwrap();
        env.set("v", v).unwrap();

        starlark_eval_in_env(&mut env, "m.add_python_resource('lib', v)").unwrap();

        let m = env.get("m").unwrap();
        m.downcast_apply(|m: &FileManifest| {
            let mut entries = m.manifest.entries();

            let (p, c) = entries.next().unwrap();
            assert_eq!(p, &PathBuf::from("lib/foo/__init__.py"));
            assert_eq!(
                c,
                &RawFileContent {
                    data: vec![],
                    executable: false,
                }
            );

            let (p, c) = entries.next().unwrap();
            assert_eq!(p, &PathBuf::from("lib/foo/bar.py"));
            assert_eq!(
                c,
                &RawFileContent {
                    data: vec![],
                    executable: false,
                }
            );

            assert!(entries.next().is_none());
        });
    }

    #[test]
    fn test_add_python_resource_data() {
        let m = Value::new(FileManifest {
            manifest: RawFileManifest::default(),
        });

        let v = Value::new(PythonPackageResource {
            data: RawPackageResource {
                leaf_package: "foo.bar".to_string(),
                relative_name: "resource.txt".to_string(),
                data: DataLocation::Memory(vec![]),
            },
        });

        let mut env = starlark_env();
        env.set("m", m).unwrap();
        env.set("v", v).unwrap();

        starlark_eval_in_env(&mut env, "m.add_python_resource('lib', v)").unwrap();

        let m = env.get("m").unwrap();
        m.downcast_apply(|m: &FileManifest| {
            let mut entries = m.manifest.entries();
            let (p, c) = entries.next().unwrap();

            assert_eq!(p, &PathBuf::from("lib/foo/bar/resource.txt"));
            assert_eq!(
                c,
                &RawFileContent {
                    data: vec![],
                    executable: false,
                }
            );

            assert!(entries.next().is_none());
        });
    }

    #[test]
    fn test_add_python_resources() {
        starlark_ok("dist = default_python_distribution(); m = FileManifest(); m.add_python_resources('lib', dist.source_modules())");
    }

    #[test]
    fn test_add_python_executable() {
        let mut env = starlark_env();

        starlark_eval_in_env(&mut env, "dist = default_python_distribution()").unwrap();
        starlark_eval_in_env(&mut env, "exe = dist.to_python_executable('testapp')").unwrap();

        let m = Value::new(FileManifest {
            manifest: RawFileManifest::default(),
        });

        env.set("m", m).unwrap();

        starlark_eval_in_env(&mut env, "m.add_python_resource('bin', exe)").unwrap();
    }

    #[test]
    fn test_install() {
        let mut env = starlark_env();

        starlark_eval_in_env(&mut env, "dist = default_python_distribution()").unwrap();
        starlark_eval_in_env(&mut env, "exe = dist.to_python_executable('testapp')").unwrap();

        let m = Value::new(FileManifest {
            manifest: RawFileManifest::default(),
        });

        env.set("m", m).unwrap();

        starlark_eval_in_env(&mut env, "m.add_python_resource('bin', exe)").unwrap();
        starlark_eval_in_env(&mut env, "m.install('myapp')").unwrap();

        let context = env
            .get("CONTEXT")
            .unwrap()
            .downcast_apply(|x: &EnvironmentContext| x.clone());

        let dest_path = context.build_path.join("myapp");
        assert!(dest_path.exists());

        // There should be an executable at myapp/bin/testapp[.exe].
        let app_exe = if cfg!(windows) {
            dest_path.join("bin").join("testapp.exe")
        } else {
            dest_path.join("bin").join("testapp")
        };

        assert!(app_exe.exists());
    }
}
