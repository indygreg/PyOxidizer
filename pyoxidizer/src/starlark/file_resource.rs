// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use anyhow::Result;
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

use super::env::required_str_arg;
use super::python_resource::{
    PythonBytecodeModule, PythonExtensionModule, PythonResourceData, PythonSourceModule,
};
use crate::app_packaging::environment::EnvironmentContext;
use crate::app_packaging::resource::{
    FileContent as RawFileContent, FileManifest as RawFileManifest,
};
use crate::project_building::build_python_executable;
use crate::py_packaging::binary::PreBuiltPythonExecutable;
use crate::py_packaging::distribution::ExtensionModule;
use crate::py_packaging::resource::{BytecodeModule, ResourceData, SourceModule};

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
    fn add_source_module(&mut self, prefix: &str, module: &SourceModule) -> Result<()> {
        let content = RawFileContent {
            data: module.source.clone(),
            executable: false,
        };

        let mut module_path = PathBuf::from(prefix);
        module_path.extend(module.name.split('.'));

        // Packages get normalized to /__init__.py.
        if module.is_package {
            module_path.push("__init__");
        }

        module_path.set_file_name(format!(
            "{}.py",
            module_path.file_name().unwrap().to_string_lossy()
        ));

        self.manifest.add_file(&module_path, &content)
    }

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

    fn add_python_executable(
        &mut self,
        logger: &slog::Logger,
        prefix: &str,
        exe: &PreBuiltPythonExecutable,
        target: &str,
    ) -> Result<()> {
        let (filename, data) =
            build_python_executable(logger, &exe.name, exe, env!("HOST"), target, "0", true)?;

        let content = RawFileContent {
            data,
            executable: true,
        };

        let path = Path::new(&prefix).join(filename);
        self.manifest.add_file(&path, &content)?;
        Ok(())
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

starlark_module! { file_resource_env =>
    #[allow(non_snake_case, clippy::ptr_arg)]
    FileManifest(env _env) {
        let manifest = RawFileManifest::default();

        Ok(Value::new(FileManifest { manifest }))
    }

    #[allow(clippy::ptr_arg)]
    FileManifest.add_python_resource(env env, this, prefix, resource) {
        let prefix = required_str_arg("prefix", &prefix)?;

        this.downcast_apply_mut(|manifest: &mut FileManifest| -> Result<(), ValueError> {
            match resource.get_type() {
                "PythonSourceModule" => {
                    let m = resource.downcast_apply(|m: &PythonSourceModule| m.module.clone());
                    manifest.add_source_module(&prefix, &m).or_else(|e| {
                        Err(RuntimeError {
                            code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                            message: e.to_string(),
                            label: e.to_string(),
                        }.into())
                    })
                },
                "PythonBytecodeModule" => {
                    let m = resource.downcast_apply(|m: &PythonBytecodeModule| m.module.clone());
                    manifest.add_bytecode_module(&prefix, &m);

                    Ok(())
                },
                "PythonResourceData" => {
                    let m = resource.downcast_apply(|m: &PythonResourceData| m.data.clone());
                    manifest.add_resource_data(&prefix, &m).or_else(|e| {
                        Err(RuntimeError {
                            code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                            message: e.to_string(),
                            label: e.to_string(),
                        }.into())
                    })
                },
                "PythonExtensionModule" => {
                    let m = resource.downcast_apply(|m: &PythonExtensionModule| m.em.clone());
                    manifest.add_extension_module(&prefix, &m);

                    Ok(())
                },
                "PythonExecutable" => {
                    let context = env.get("CONTEXT").expect("CONTEXT not defined");
                    let (logger, target) = context.downcast_apply(|x: &EnvironmentContext| {
                        (x.logger.clone(), x.build_target.clone())
                    });

                    let exe = resource.downcast_apply(|exe: &PreBuiltPythonExecutable| exe.clone());

                    manifest.add_python_executable(&logger, &prefix, &exe, &target)
                        .or_else(|e| Err(RuntimeError {
                            code: "PYOXIDIZER_BUILD",
                            message: e.to_string(),
                            label: "add_python_resource".to_string(),
                        }.into()))
                },
                t => Err(RuntimeError {
                    code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                    message: format!("resource should be a Python resource type; got {}", t),
                    label: "bad argument type".to_string(),
                }.into())
            }
        })?;

        Ok(Value::new(None))
    }

    #[allow(clippy::ptr_arg)]
    FileManifest.add_python_resources(call_stack cs, env env, this, prefix, resources) {
        required_str_arg("prefix", &prefix)?;

        let f = env.get_type_value(&this, "add_python_resource").unwrap();

        for resource in resources.into_iter()? {
            f.call(cs, env.clone(), vec![this.clone(), prefix.clone(), resource], HashMap::new(), None, None)?;
        }

        Ok(Value::new(None))
    }
}

#[cfg(test)]
mod tests {
    use super::super::testutil::*;
    use super::*;

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
        starlark_eval_in_env(&mut env, "resources = PythonEmbeddedResources()").unwrap();
        starlark_eval_in_env(&mut env, "run_mode = python_run_mode_noop()").unwrap();
        starlark_eval_in_env(&mut env, "config = EmbeddedPythonConfig()").unwrap();
        starlark_eval_in_env(
            &mut env,
            "exe = PythonExecutable('testapp', dist, resources, config, run_mode)",
        )
        .unwrap();

        let m = Value::new(FileManifest {
            manifest: RawFileManifest::default(),
        });

        env.set("m", m).unwrap();

        starlark_eval_in_env(&mut env, "m.add_python_resource('bin', exe)").unwrap();
    }
}
