// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    super::env::EnvironmentContext,
    super::python_embedded_resources::PythonEmbeddedData,
    super::python_resource::{
        PythonExtensionModule, PythonExtensionModuleFlavor, PythonResourceData, PythonSourceModule,
    },
    super::target::{BuildContext, BuildTarget, ResolvedTarget, RunMode},
    super::util::{optional_list_arg, required_bool_arg, required_type_arg},
    crate::project_building::build_python_executable,
    crate::py_packaging::binary::PythonBinaryBuilder,
    crate::py_packaging::resource::{BytecodeModule, BytecodeOptimizationLevel},
    anyhow::{Context, Result},
    slog::{info, warn},
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
    std::collections::HashMap,
    std::io::Write,
    std::ops::Deref,
    std::path::{Path, PathBuf},
};

/// Represents a builder for a Python executable.
pub struct PythonExecutable {
    pub exe: Box<dyn PythonBinaryBuilder>,
}

impl TypedValue for PythonExecutable {
    immutable!();
    any!();
    not_supported!(binop, container, function, get_hash, to_int);

    fn to_str(&self) -> String {
        "PythonExecutable<>".to_string()
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_type(&self) -> &'static str {
        "PythonExecutable"
    }

    fn to_bool(&self) -> bool {
        true
    }

    fn compare(&self, other: &dyn TypedValue, _recursion: u32) -> Result<Ordering, ValueError> {
        default_compare(self, other)
    }
}

impl BuildTarget for PythonExecutable {
    fn build(&mut self, context: &BuildContext) -> Result<ResolvedTarget> {
        // Build an executable by writing out a temporary Rust project
        // and building it.
        let (exe_name, exe_data) = build_python_executable(
            &context.logger,
            &self.exe.name(),
            self.exe.deref(),
            &context.target_triple,
            &context.opt_level,
            context.release,
        )?;

        let dest_path = context.output_path.join(exe_name);
        warn!(
            &context.logger,
            "writing executable to {}",
            dest_path.display()
        );
        let mut fh = std::fs::File::create(&dest_path)
            .context(format!("creating {}", dest_path.display()))?;
        fh.write_all(&exe_data)
            .context(format!("writing {}", dest_path.display()))?;

        crate::app_packaging::resource::set_executable(&mut fh)
            .context("making binary executable")?;

        Ok(ResolvedTarget {
            run_mode: RunMode::Path { path: dest_path },
            output_path: context.output_path.clone(),
        })
    }
}

// Starlark functions.
impl PythonExecutable {
    /// PythonExecutable.add_module_source(module)
    pub fn starlark_add_module_source(&mut self, env: &Environment, module: &Value) -> ValueResult {
        required_type_arg("module", "PythonSourceModule", &module)?;

        let context = env.get("CONTEXT").expect("CONTEXT not set");
        let logger = context.downcast_apply(|x: &EnvironmentContext| x.logger.clone());

        let m = module.downcast_apply(|m: &PythonSourceModule| m.module.clone());
        info!(&logger, "adding embedded source module {}", m.name);
        self.exe.add_in_memory_module_source(&m);

        Ok(Value::new(None))
    }

    /// PythonExecutable.add_module_bytecode(module, optimize_level=0)
    pub fn starlark_add_module_bytecode(
        &mut self,
        env: &Environment,
        module: &Value,
        optimize_level: &Value,
    ) -> ValueResult {
        required_type_arg("module", "PythonSourceModule", &module)?;
        required_type_arg("optimize_level", "int", &optimize_level)?;

        let context = env.get("CONTEXT").expect("CONTEXT not set");
        let logger = context.downcast_apply(|x: &EnvironmentContext| x.logger.clone());

        let optimize_level = optimize_level.to_int().unwrap();

        let optimize_level = match optimize_level {
            0 => BytecodeOptimizationLevel::Zero,
            1 => BytecodeOptimizationLevel::One,
            2 => BytecodeOptimizationLevel::Two,
            i => {
                return Err(RuntimeError {
                    code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                    message: format!("optimize_level must be 0, 1, or 2: got {}", i),
                    label: "invalid optimize_level value".to_string(),
                }
                .into());
            }
        };

        let m = module.downcast_apply(|m: &PythonSourceModule| m.module.clone());
        info!(&logger, "adding embedded bytecode module {}", m.name);
        self.exe.add_in_memory_module_bytecode(&BytecodeModule {
            name: m.name.clone(),
            source: m.source.clone(),
            optimize_level,
            is_package: m.is_package,
        });

        Ok(Value::new(None))
    }

    /// PythonExecutable.add_resource_data(resource)
    pub fn starlark_add_resource_data(
        &mut self,
        env: &Environment,
        resource: &Value,
    ) -> ValueResult {
        required_type_arg("resource", "PythonResourceData", &resource)?;

        let context = env.get("CONTEXT").expect("CONTEXT not set");
        let logger = context.downcast_apply(|x: &EnvironmentContext| x.logger.clone());

        let r = resource.downcast_apply(|r: &PythonResourceData| r.data.clone());
        info!(
            &logger,
            "adding embedded resource data {}:{}", r.package, r.name
        );
        self.exe.add_in_memory_package_resource(&r);

        Ok(Value::new(None))
    }

    /// PythonExecutable.add_extension_module(module)
    pub fn starlark_add_extension_module(
        &mut self,
        env: &Environment,
        module: &Value,
    ) -> ValueResult {
        required_type_arg("resource", "PythonExtensionModule", &module)?;

        let context = env.get("CONTEXT").expect("CONTEXT not set");
        let logger = context.downcast_apply(|x: &EnvironmentContext| x.logger.clone());

        let m = module.downcast_apply(|m: &PythonExtensionModule| m.em.clone());
        info!(&logger, "adding embedded extension module {}", m.name());

        match m {
            PythonExtensionModuleFlavor::Distribution(m) => {
                self.exe.add_distribution_extension_module(&m);
            }
            PythonExtensionModuleFlavor::StaticallyLinked(m) => {
                self.exe.add_extension_module_data(&m);
            }
            PythonExtensionModuleFlavor::DynamicLibrary(m) => {
                self.exe.add_extension_module_data(&m);
            }
        }

        Ok(Value::new(None))
    }

    /// PythonExecutable.add_python_resource(resource, add_source_module=true, add_bytecode_module=true, optimize_level=0)
    pub fn starlark_add_python_resource(
        &mut self,
        env: &Environment,
        resource: &Value,
        add_source_module: &Value,
        add_bytecode_module: &Value,
        optimize_level: &Value,
    ) -> ValueResult {
        let add_source_module = required_bool_arg("add_source_module", &add_source_module)?;
        let add_bytecode_module = required_bool_arg("add_bytecode_module", &add_bytecode_module)?;
        required_type_arg("optimize_level", "int", &optimize_level)?;

        match resource.get_type() {
            "PythonSourceModule" => {
                if add_source_module {
                    self.starlark_add_module_source(env, resource)?;
                }
                if add_bytecode_module {
                    self.starlark_add_module_bytecode(env, resource, optimize_level)?;
                }

                Ok(Value::new(None))
            }
            "PythonBytecodeModule" => {
                self.starlark_add_module_bytecode(env, resource, optimize_level)
            }
            "PythonResourceData" => self.starlark_add_resource_data(env, resource),
            "PythonExtensionModule" => self.starlark_add_extension_module(env, resource),
            _ => Err(RuntimeError {
                code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                message: "resource argument must be a Python resource type".to_string(),
                label: ".add_python_resource()".to_string(),
            }
            .into()),
        }
    }

    /// PythonExecutable.add_python_resources(resources, add_source_module=true, add_bytecode_module=true, optimize_level=0)
    pub fn starlark_add_python_resources(
        &mut self,
        env: &Environment,
        resources: &Value,
        add_source_module: &Value,
        add_bytecode_module: &Value,
        optimize_level: &Value,
    ) -> ValueResult {
        required_bool_arg("add_source_module", &add_source_module)?;
        required_bool_arg("add_bytecode_module", &add_bytecode_module)?;
        required_type_arg("optimize_level", "int", &optimize_level)?;

        for resource in resources.into_iter()? {
            self.starlark_add_python_resource(
                env,
                &resource,
                add_source_module,
                add_bytecode_module,
                optimize_level,
            )?;
        }

        Ok(Value::new(None))
    }

    /// PythonExecutable.to_embedded_data()
    pub fn starlark_to_embedded_data(&self) -> ValueResult {
        Ok(Value::new(PythonEmbeddedData {
            exe: self.exe.clone_box(),
        }))
    }

    /// PythonExecutable.filter_resources_from_files(files=None, glob_files=None)
    pub fn starlark_filter_resources_from_files(
        &mut self,
        env: &Environment,
        files: &Value,
        glob_files: &Value,
    ) -> ValueResult {
        optional_list_arg("files", "string", &files)?;
        optional_list_arg("glob_files", "string", &glob_files)?;

        let files = match files.get_type() {
            "list" => files
                .into_iter()?
                .map(|x| PathBuf::from(x.to_string()))
                .collect(),
            "NoneType" => Vec::new(),
            _ => panic!("type should have been validated above"),
        };

        let glob_files = match glob_files.get_type() {
            "list" => glob_files.into_iter()?.map(|x| x.to_string()).collect(),
            "NoneType" => Vec::new(),
            _ => panic!("type should have been validated above"),
        };

        let files_refs = files.iter().map(|x| x.as_ref()).collect::<Vec<&Path>>();
        let glob_files_refs = glob_files.iter().map(|x| x.as_ref()).collect::<Vec<&str>>();

        let context = env.get("CONTEXT").expect("CONTEXT not defined");
        let logger = context.downcast_apply(|x: &EnvironmentContext| x.logger.clone());

        self.exe
            .filter_resources_from_files(&logger, &files_refs, &glob_files_refs)
            .or_else(|e| {
                Err(RuntimeError {
                    code: "RUNTIME_ERROR",
                    message: e.to_string(),
                    label: "filter_from_files()".to_string(),
                }
                .into())
            })?;

        Ok(Value::new(None))
    }
}

starlark_module! { python_executable_env =>
    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.add_module_source(env env, this, module) {
        this.downcast_apply_mut(|exe: &mut PythonExecutable| {
            exe.starlark_add_module_source(&env, &module)
        })
    }

    // TODO consider unifying with add_module_source() so there only needs to be
    // a single function call.
    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.add_module_bytecode(env env, this, module, optimize_level=0) {
        this.downcast_apply_mut(|exe: &mut PythonExecutable| {
            exe.starlark_add_module_bytecode(&env, &module, &optimize_level)
        })
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.add_resource_data(env env, this, resource) {
        this.downcast_apply_mut(|exe: &mut PythonExecutable| {
            exe.starlark_add_resource_data(&env, &resource)
        })
    }

    #[allow(clippy::ptr_arg)]
    PythonExecutable.add_extension_module(env env, this, module) {
        this.downcast_apply_mut(|exe: &mut PythonExecutable| {
            exe.starlark_add_extension_module(&env, &module)
        })
    }

    #[allow(clippy::ptr_arg)]
    PythonExecutable.add_python_resource(
        env env,
        this,
        resource,
        add_source_module=true,
        add_bytecode_module=true,
        optimize_level=0
        )
    {
        this.downcast_apply_mut(|exe: &mut PythonExecutable| {
            exe.starlark_add_python_resource(
                &env,
                &resource,
                &add_source_module,
                &add_bytecode_module,
                &optimize_level,
            )
        })
    }

    #[allow(clippy::ptr_arg)]
    PythonExecutable.add_python_resources(
        env env,
        this,
        resources,
        add_source_module=true,
        add_bytecode_module=true,
        optimize_level=0
    ) {
        this.downcast_apply_mut(|exe: &mut PythonExecutable| {
            exe.starlark_add_python_resources(
                &env,
                &resources,
                &add_source_module,
                &add_bytecode_module,
                &optimize_level,
            )
        })
    }

    #[allow(clippy::ptr_arg)]
    PythonExecutable.filter_resources_from_files(
        env env,
        this,
        files=None,
        glob_files=None)
    {
        this.downcast_apply_mut(|exe: &mut PythonExecutable| {
            exe.starlark_filter_resources_from_files(&env, &files, &glob_files)
        })
    }

    #[allow(clippy::ptr_arg)]
    PythonExecutable.to_embedded_data(this) {
        this.downcast_apply(|exe: &PythonExecutable| {
            exe.starlark_to_embedded_data()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::testutil::*;
    use super::*;

    #[test]
    fn test_default_values() {
        let mut env = starlark_env();

        starlark_eval_in_env(&mut env, "dist = default_python_distribution()").unwrap();

        let exe = starlark_eval_in_env(&mut env, "dist.to_python_executable('testapp')").unwrap();

        assert_eq!(exe.get_type(), "PythonExecutable");

        exe.downcast_apply(|exe: &PythonExecutable| {
            assert!(!exe.exe.extension_modules().is_empty());
            assert!(!exe.exe.in_memory_module_sources().is_empty());
            assert!(!exe.exe.in_memory_module_bytecodes().is_empty());
            assert!(exe.exe.in_memory_package_resources().is_empty());
        });
    }

    #[test]
    fn test_no_sources() {
        let mut env = starlark_env();

        starlark_eval_in_env(&mut env, "dist = default_python_distribution()").unwrap();

        let exe = starlark_eval_in_env(
            &mut env,
            "dist.to_python_executable('testapp', include_sources=False)",
        )
        .unwrap();

        assert_eq!(exe.get_type(), "PythonExecutable");

        exe.downcast_apply(|exe: &PythonExecutable| {
            assert!(exe.exe.in_memory_module_sources().is_empty());
        });
    }
}
