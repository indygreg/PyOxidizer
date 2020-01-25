// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use anyhow::{Context, Result};
use slog::{info, warn};
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
use std::io::Write;
use std::path::{Path, PathBuf};

use super::env::EnvironmentContext;
use super::python_resource::{
    PythonExtensionModule, PythonExtensionModuleFlavor, PythonResourceData, PythonSourceModule,
};
use super::target::{BuildContext, BuildTarget, ResolvedTarget, RunMode};
use super::util::{optional_list_arg, required_bool_arg, required_type_arg};
use crate::project_building::build_python_executable;
use crate::py_packaging::binary::PreBuiltPythonExecutable;
use crate::py_packaging::resource::{BytecodeModule, BytecodeOptimizationLevel};

impl TypedValue for PreBuiltPythonExecutable {
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

impl BuildTarget for PreBuiltPythonExecutable {
    fn build(&mut self, context: &BuildContext) -> Result<ResolvedTarget> {
        // Build an executable by writing out a temporary Rust project
        // and building it.
        let (exe_name, exe_data) = build_python_executable(
            &context.logger,
            &self.name,
            &self,
            &context.host_triple,
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
        })
    }
}

// Starlark functions.
impl PreBuiltPythonExecutable {
    /// PythonExecutable.add_module_source(module)
    pub fn starlark_add_module_source(&mut self, env: &Environment, module: &Value) -> ValueResult {
        required_type_arg("module", "PythonSourceModule", &module)?;

        let context = env.get("CONTEXT").expect("CONTEXT not set");
        let logger = context.downcast_apply(|x: &EnvironmentContext| x.logger.clone());

        let m = module.downcast_apply(|m: &PythonSourceModule| m.module.clone());
        info!(&logger, "adding embedded source module {}", m.name);
        self.resources.add_source_module(&m);

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
        self.resources.add_bytecode_module(&BytecodeModule {
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
        self.resources.add_resource(&r);

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
            PythonExtensionModuleFlavor::Persisted(m) => {
                self.resources.add_extension_module(&m);
            }
            PythonExtensionModuleFlavor::Built(m) => {
                self.resources.add_extension_module_data(&m);
            }
        }

        Ok(Value::new(None))
    }
}

starlark_module! { python_executable_env =>
    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.add_module_source(env env, this, module) {
        this.downcast_apply_mut(|exe: &mut PreBuiltPythonExecutable| {
            exe.starlark_add_module_source(&env, &module)
        })
    }

    // TODO consider unifying with add_module_source() so there only needs to be
    // a single function call.
    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.add_module_bytecode(env env, this, module, optimize_level=0) {
        this.downcast_apply_mut(|exe: &mut PreBuiltPythonExecutable| {
            exe.starlark_add_module_bytecode(&env, &module, &optimize_level)
        })
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.add_resource_data(env env, this, resource) {
        this.downcast_apply_mut(|exe: &mut PreBuiltPythonExecutable| {
            exe.starlark_add_resource_data(&env, &resource)
        })
    }

    #[allow(clippy::ptr_arg)]
    PythonExecutable.add_extension_module(env env, this, module) {
        this.downcast_apply_mut(|exe: &mut PreBuiltPythonExecutable| {
            exe.starlark_add_extension_module(&env, &module)
        })
    }

    #[allow(clippy::ptr_arg)]
    PythonExecutable.add_python_resource(
        call_stack call_stack,
        env env,
        this,
        resource,
        add_source_module=true,
        add_bytecode_module=true,
        optimize_level=0
        ) {
        let add_source_module = required_bool_arg("add_source_module", &add_source_module)?;
        let add_bytecode_module = required_bool_arg("add_bytecode_module", &add_bytecode_module)?;
        required_type_arg("optimize_level", "int", &optimize_level)?;

        match resource.get_type() {
            "PythonSourceModule" => {
                if add_source_module {
                    let f = env.get_type_value(&this, "add_module_source").unwrap();
                    f.call(call_stack, env.clone(), vec![this.clone(), resource.clone()], HashMap::new(), None, None)?;
                }
                if add_bytecode_module {
                    let f = env.get_type_value(&this, "add_module_bytecode").unwrap();
                    f.call(call_stack, env, vec![this, resource, optimize_level], HashMap::new(), None, None)?;
                }

                Ok(Value::new(None))
            }
            "PythonBytecodeModule" => {
                let f = env.get_type_value(&this, "add_module_bytecode").unwrap();
                f.call(call_stack, env, vec![this, resource, optimize_level], HashMap::new(), None, None)?;
                Ok(Value::new(None))
            }
            "PythonResourceData" => {
                let f = env.get_type_value(&this, "add_resource_data").unwrap();
                f.call(call_stack, env, vec![this, resource], HashMap::new(), None, None)?;
                Ok(Value::new(None))
            }
            "PythonExtensionModule" => {
                let f = env.get_type_value(&this, "add_extension_module").unwrap();
                f.call(call_stack, env, vec![this, resource], HashMap::new(), None, None)?;
                Ok(Value::new(None))
            }
            _ => Err(RuntimeError {
                code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                message: "resource argument must be a Python resource type".to_string(),
                label: ".add_python_resource()".to_string(),
            }.into())
        }
    }

    #[allow(clippy::ptr_arg)]
    PythonExecutable.add_python_resources(
        call_stack call_stack,
        env env,
        this,
        resources,
        add_source_module=true,
        add_bytecode_module=true,
        optimize_level=0
    ) {
        required_bool_arg("add_source_module", &add_source_module)?;
        required_bool_arg("add_bytecode_module", &add_bytecode_module)?;
        required_type_arg("optimize_level", "int", &optimize_level)?;

        let f = env.get_type_value(&this, "add_python_resource").unwrap();

        for resource in resources.into_iter()? {
            let args = vec![
                this.clone(),
                resource,
                add_source_module.clone(),
                add_bytecode_module.clone(),
                optimize_level.clone(),
            ];
            f.call(call_stack, env.clone(), args, HashMap::new(), None, None)?;
        }

        Ok(Value::new(None))
    }

    #[allow(clippy::ptr_arg)]
    PythonExecutable.filter_resources_from_files(
        env env,
        this,
        files=None,
        glob_files=None) {
        optional_list_arg("files", "string", &files)?;
        optional_list_arg("glob_files", "string", &glob_files)?;

        let files = match files.get_type() {
            "list" => files.into_iter()?.map(|x| PathBuf::from(x.to_string())).collect(),
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

        this.downcast_apply_mut(|exe: &mut PreBuiltPythonExecutable| {
            exe.resources.filter_from_files(&logger, &files_refs, &glob_files_refs)
        }).or_else(|e| Err(
            RuntimeError {
                code: "RUNTIME_ERROR",
                message: e.to_string(),
                label: "filter_from_files()".to_string(),
            }.into()
        ))?;

        Ok(Value::new(None))
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
        starlark_eval_in_env(&mut env, "run_mode = python_run_mode_noop()").unwrap();

        let exe = starlark_eval_in_env(&mut env, "dist.to_python_executable('testapp', run_mode)")
            .unwrap();

        assert_eq!(exe.get_type(), "PythonExecutable");

        exe.downcast_apply(|exe: &PreBuiltPythonExecutable| {
            assert_eq!(exe.run_mode, crate::py_packaging::config::RunMode::Noop);
            assert!(!exe.resources.extension_modules.is_empty());
            assert!(!exe.resources.source_modules.is_empty());
            assert!(!exe.resources.bytecode_modules.is_empty());
            assert!(exe.resources.resources.is_empty());
        });
    }

    #[test]
    fn test_no_sources() {
        let mut env = starlark_env();

        starlark_eval_in_env(&mut env, "dist = default_python_distribution()").unwrap();
        starlark_eval_in_env(&mut env, "run_mode = python_run_mode_noop()").unwrap();

        let exe = starlark_eval_in_env(
            &mut env,
            "dist.to_python_executable('testapp', run_mode, include_sources=False)",
        )
        .unwrap();

        assert_eq!(exe.get_type(), "PythonExecutable");

        exe.downcast_apply(|exe: &PreBuiltPythonExecutable| {
            assert!(exe.resources.source_modules.is_empty());
        });
    }
}
