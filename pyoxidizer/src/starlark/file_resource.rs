// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use anyhow::Result;
use itertools::Itertools;
use slog::warn;
use starlark::environment::Environment;
use starlark::values::{
    default_compare, RuntimeError, TypedValue, Value, ValueError, ValueResult,
    INCORRECT_PARAMETER_TYPE_ERROR_CODE,
};
use starlark::{
    any, immutable, not_supported, starlark_fun, starlark_module, starlark_signature,
    starlark_signature_extraction, starlark_signatures,
};
use std::any::Any;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::env::EnvironmentContext;
use super::python_resource::PythonExtensionModuleFlavor;
use super::python_resource::{
    PythonBytecodeModule, PythonExtensionModule, PythonResourceData, PythonSourceModule,
};
use super::target::{BuildContext, BuildTarget, ResolvedTarget, RunMode};
use super::util::{required_bool_arg, required_str_arg};
use crate::app_packaging::resource::{
    FileContent as RawFileContent, FileManifest as RawFileManifest,
};
use crate::project_building::build_python_executable;
use crate::py_packaging::binary::PreBuiltPythonExecutable;
use crate::py_packaging::distribution::ExtensionModule;
use crate::py_packaging::resource::{BytecodeModule, ExtensionModuleData, ResourceData};

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
    fn add_bytecode_module(&self, _prefix: &str, _module: &BytecodeModule) {
        println!("support for adding bytecode modules not yet implemented");
    }

    fn add_resource_data(&mut self, prefix: &str, resource: &ResourceData) -> Result<()> {
        let mut dest_path = PathBuf::from(prefix);
        dest_path.extend(resource.package.split('.'));
        dest_path.push(&resource.name);

        let content = RawFileContent {
            data: resource.data.clone(),
            executable: false,
        };

        self.manifest.add_file(&dest_path, &content)
    }

    // TODO implement.
    fn add_extension_module(&self, _prefix: &str, _em: &ExtensionModule) {
        println!("support for adding extension modules not yet implemented");
    }

    fn add_built_extension_module(&mut self, prefix: &str, em: &ExtensionModuleData) -> Result<()> {
        let mut dest_path = PathBuf::from(prefix);
        dest_path.extend(em.package_parts());
        dest_path.push(em.file_name());

        let content = RawFileContent {
            data: em.extension_data.clone(),
            executable: true,
        };

        self.manifest.add_file(&dest_path, &content)
    }

    #[allow(clippy::too_many_arguments)]
    fn add_python_executable(
        &mut self,
        logger: &slog::Logger,
        prefix: &str,
        exe: &PreBuiltPythonExecutable,
        host: &str,
        target: &str,
        release: bool,
        opt_level: &str,
    ) -> Result<()> {
        let (filename, data) =
            build_python_executable(logger, &exe.name, exe, host, target, opt_level, release)?;

        let content = RawFileContent {
            data,
            executable: true,
        };

        let path = Path::new(&prefix).join(filename);
        self.manifest.add_file(&path, &content)?;
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

        Ok(ResolvedTarget { run_mode })
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
            "PythonResourceData" => {
                let m = resource.downcast_apply(|m: &PythonResourceData| m.data.clone());
                warn!(
                    logger,
                    "adding resource file {} to {}",
                    m.full_name(),
                    prefix
                );
                self.add_resource_data(&prefix, &m).or_else(|e| {
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
                    PythonExtensionModuleFlavor::Persisted(m) => {
                        warn!(logger, "adding extension module {} to {}", m.module, prefix);
                        self.add_extension_module(&prefix, &m);
                        Ok(())
                    }
                    PythonExtensionModuleFlavor::Built(m) => {
                        warn!(
                            logger,
                            "adding built extension module {} to {}", m.name, prefix
                        );
                        self.add_built_extension_module(&prefix, &m).or_else(|e| {
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
                let (host, target, release, opt_level) =
                    context.downcast_apply(|x: &EnvironmentContext| {
                        (
                            x.build_host_triple.clone(),
                            x.build_target_triple.clone(),
                            x.build_release,
                            x.build_opt_level.clone(),
                        )
                    });

                let raw_exe = resource.0.borrow();
                let exe = raw_exe
                    .as_any()
                    .downcast_ref::<PreBuiltPythonExecutable>()
                    .unwrap();
                warn!(
                    logger,
                    "adding Python executable {} to {}", exe.name, prefix
                );
                self.add_python_executable(
                    &logger, &prefix, exe, &host, &target, release, &opt_level,
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

starlark_module! { file_resource_env =>
    #[allow(non_snake_case, clippy::ptr_arg)]
    FileManifest(env _env) {
        FileManifest::new_from_args()
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
    use super::super::testutil::*;
    use super::*;
    use crate::py_packaging::resource::SourceModule;

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
            module: SourceModule {
                name: "foo.bar".to_string(),
                source: vec![],
                is_package: false,
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

        let v = Value::new(PythonResourceData {
            data: ResourceData {
                package: "foo.bar".to_string(),
                name: "resource.txt".to_string(),
                data: vec![],
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
        starlark_eval_in_env(&mut env, "run_mode = python_run_mode_noop()").unwrap();
        starlark_eval_in_env(
            &mut env,
            "exe = dist.to_python_executable('testapp', run_mode)",
        )
        .unwrap();

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
        starlark_eval_in_env(&mut env, "run_mode = python_run_mode_noop()").unwrap();
        starlark_eval_in_env(
            &mut env,
            "exe = dist.to_python_executable('testapp', run_mode)",
        )
        .unwrap();

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
