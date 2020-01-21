// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use anyhow::{Context, Result};
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
use std::convert::TryFrom;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::env::EnvironmentContext;
use super::python_distribution::PythonDistribution;
use super::python_interpreter_config::PythonInterpreterConfig;
use super::python_resource::{
    PythonExtensionModule, PythonExtensionModuleFlavor, PythonResourceData, PythonSourceModule,
};
use super::python_run_mode::PythonRunMode;
use super::target::{BuildContext, BuildTarget, ResolvedTarget, RunMode};
use super::util::{
    optional_dict_arg, optional_list_arg, optional_type_arg, required_bool_arg, required_str_arg,
    required_type_arg,
};
use crate::project_building::build_python_executable;
use crate::py_packaging::binary::{EmbeddedPythonBinaryData, PreBuiltPythonExecutable};
use crate::py_packaging::distribution::ExtensionModuleFilter;
use crate::py_packaging::embedded_resource::EmbeddedPythonResourcesPrePackaged;
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

starlark_module! { python_executable_env =>
    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable(
        env env,
        call_stack cs,
        name,
        distribution,
        run_mode,
        config=None,
        extension_module_filter="all",
        preferred_extension_module_variants=None,
        include_sources=true,
        include_resources=false,
        include_test=false)
    {
        let name = required_str_arg("name", &name)?;
        required_type_arg("distribution", "PythonDistribution", &distribution)?;
        required_type_arg("run_mode", "PythonRunMode", &run_mode)?;
        optional_type_arg("config", "PythonInterpreterConfig", &config)?;
        let extension_module_filter = required_str_arg("extension_module_filter", &extension_module_filter)?;
        optional_dict_arg("preferred_extension_module_variants", "string", "string", &preferred_extension_module_variants)?;
        let include_sources = required_bool_arg("include_sources", &include_sources)?;
        let include_resources = required_bool_arg("include_resources", &include_resources)?;
        let include_test = required_bool_arg("include_test", &include_test)?;

        let context = env.get("CONTEXT").expect("CONTEXT not defined");
        let logger = context.downcast_apply(|x: &EnvironmentContext| x.logger.clone());

        let extension_module_filter = ExtensionModuleFilter::try_from(extension_module_filter.as_str()).or_else(|e| Err(RuntimeError {
            code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
            message: e,
            label: "invalid policy value".to_string(),
        }.into()))?;

        let preferred_extension_module_variants = match preferred_extension_module_variants.get_type() {
            "NoneType" => None,
            "dict" => {
                let mut m = HashMap::new();

                for k in preferred_extension_module_variants.into_iter()? {
                    let v = preferred_extension_module_variants.at(k.clone())?.to_string();
                    m.insert(k.to_string(), v);
                }

                Some(m)
            }
            _ => panic!("type should have been validated above")
        };

        let mut distribution = distribution;

        let distribution = distribution.downcast_apply_mut(|dist: &mut PythonDistribution| {
            dist.ensure_distribution_resolved(&logger);
            dist.distribution.as_ref().unwrap().clone()
        });

        let mut resources = EmbeddedPythonResourcesPrePackaged::from_distribution(
            &logger,
            distribution.clone(),
            &extension_module_filter,
            preferred_extension_module_variants,
            include_sources,
            include_resources,
            include_test,
        ).or_else(|e| Err(RuntimeError {
            code: "PYOXIDIZER_BUILD",
            message: e.to_string(),
            label: "PythonExecutable()".to_string(),
        }.into()))?;

        let config = if config.get_type() == "NoneType" {
            let v = env.get("PythonInterpreterConfig").expect("PythonInterpreterConfig not defined");
            v.call(cs, env, Vec::new(), HashMap::new(), None, None)?.downcast_apply(|c: &PythonInterpreterConfig| {
                c.config.clone()
            })
        } else {
            config.downcast_apply(|c: &PythonInterpreterConfig| c.config.clone())
        };

        let run_mode = run_mode.downcast_apply(|m: &PythonRunMode| m.run_mode.clone());

        // Always ensure minimal extension modules are present, otherwise we get
        // missing symbol errors at link time.
        for ext in distribution.filter_extension_modules(&logger, &ExtensionModuleFilter::Minimal, None) {
            if !resources.extension_modules.contains_key(&ext.module) {
                resources.add_extension_module(&ext);
            }
        }

        let pre_built = PreBuiltPythonExecutable {
            name,
            distribution,
            resources,
            config,
            run_mode
        };

        context.downcast_apply(|context: &EnvironmentContext| -> Result<()> {
            if let Some(path) = &context.write_artifacts_path {
                warn!(&logger, "writing PyOxidizer build artifacts to {}", path.display());
                let embedded = EmbeddedPythonBinaryData::from_pre_built_python_executable(
                    &pre_built,
                    &logger,
                    &context.build_host_triple,
                    &context.build_target_triple,
                    &context.build_opt_level,
                )?;

                embedded.write_files(path)?;
            }

            Ok(())
        }).or_else(|e| Err(RuntimeError {
            code: "PYOXIDIZER_BUILD",
            message: e.to_string(),
            label: "PythonExecutable()".to_string(),
        }.into()))?;

        Ok(Value::new(pre_built))
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.add_module_source(this, module) {
        required_type_arg("module", "PythonSourceModule", &module)?;

        this.downcast_apply_mut(|exe: &mut PreBuiltPythonExecutable| {
            let m = module.downcast_apply(|m: &PythonSourceModule| m.module.clone());
            exe.resources.add_source_module(&m);
        });

        Ok(Value::new(None))
    }

    // TODO consider unifying with add_module_source() so there only needs to be
    // a single function call.
    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.add_module_bytecode(this, module, optimize_level=0) {
        required_type_arg("module", "PythonSourceModule", &module)?;
        required_type_arg("optimize_level", "int", &optimize_level)?;

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
                }.into());
            }
        };

        this.downcast_apply_mut(|exe: &mut PreBuiltPythonExecutable| {
            let m = module.downcast_apply(|m: &PythonSourceModule| m.module.clone());
            exe.resources.add_bytecode_module(&BytecodeModule {
                name: m.name.clone(),
                source: m.source.clone(),
                optimize_level,
                is_package: m.is_package,
            });
        });

        Ok(Value::new(None))
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.add_resource_data(this, resource) {
        required_type_arg("resource", "PythonResourceData", &resource)?;

        this.downcast_apply_mut(|exe: &mut PreBuiltPythonExecutable| {
            let r = resource.downcast_apply(|r: &PythonResourceData| r.data.clone());
            exe.resources.add_resource(&r);
        });

        Ok(Value::new(None))
    }

    #[allow(clippy::ptr_arg)]
    PythonExecutable.add_extension_module(this, module) {
        required_type_arg("resource", "PythonExtensionModule", &module)?;

        this.downcast_apply_mut(|exe: &mut PreBuiltPythonExecutable| {
            let m = module.downcast_apply(|m: &PythonExtensionModule| m.em.clone());
            match m {
                PythonExtensionModuleFlavor::Persisted(m) => {
                    exe.resources.add_extension_module(&m);
                    Ok(())
                },
                PythonExtensionModuleFlavor::Built(_) => Err(RuntimeError {
                    code: "PYOXIDIZER_BUILD",
                    message: "support for built extension modules not yet implemented".to_string(),
                    label: "add_extension_module()".to_string(),
                }.into())
            }
        })?;

        Ok(Value::new(None))
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
    fn test_no_args() {
        let err = starlark_nok("PythonExecutable()");
        assert!(err.message.starts_with("Missing parameter name"));
    }

    #[test]
    fn test_default_values() {
        let mut env = starlark_env();

        starlark_eval_in_env(&mut env, "dist = default_python_distribution()").unwrap();
        starlark_eval_in_env(&mut env, "run_mode = python_run_mode_noop()").unwrap();

        let exe =
            starlark_eval_in_env(&mut env, "PythonExecutable('testapp', dist, run_mode)").unwrap();

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
            "PythonExecutable('testapp', dist, run_mode, include_sources=False)",
        )
        .unwrap();

        assert_eq!(exe.get_type(), "PythonExecutable");

        exe.downcast_apply(|exe: &PreBuiltPythonExecutable| {
            assert!(exe.resources.source_modules.is_empty());
        });
    }
}
