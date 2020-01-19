// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use anyhow::{Context, Result};
use slog::warn;
use starlark::environment::Environment;
use starlark::values::{default_compare, RuntimeError, TypedValue, Value, ValueError, ValueResult};
use starlark::{
    any, immutable, not_supported, starlark_fun, starlark_module, starlark_signature,
    starlark_signature_extraction, starlark_signatures,
};
use std::any::Any;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::io::Write;

use super::embedded_python_config::EmbeddedPythonConfig;
use super::env::EnvironmentContext;
use super::python_distribution::PythonDistribution;
use super::python_resource::PythonEmbeddedResources;
use super::python_run_mode::PythonRunMode;
use super::target::{BuildContext, BuildTarget, ResolvedTarget, RunMode};
use super::util::{required_str_arg, required_type_arg};
use crate::project_building::build_python_executable;
use crate::py_packaging::binary::{EmbeddedPythonBinaryData, PreBuiltPythonExecutable};
use crate::py_packaging::distribution::ExtensionModuleFilter;

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
    PythonExecutable(env env, name, distribution, resources, config, run_mode) {
        let name = required_str_arg("name", &name)?;
        required_type_arg("distribution", "PythonDistribution", &distribution)?;
        required_type_arg("resources", "PythonEmbeddedResources", &resources)?;
        required_type_arg("config", "EmbeddedPythonConfig", &config)?;
        required_type_arg("run_mode", "PythonRunMode", &run_mode)?;

        let context = env.get("CONTEXT").expect("CONTEXT not defined");
        let logger = context.downcast_apply(|x: &EnvironmentContext| x.logger.clone());

        let mut distribution = distribution.clone();

        let distribution = distribution.downcast_apply_mut(|dist: &mut PythonDistribution| {
            dist.ensure_distribution_resolved(&logger);
            dist.distribution.as_ref().unwrap().clone()
        });

        let mut resources = resources.downcast_apply(|r: &PythonEmbeddedResources| r.embedded.clone());
        let config = config.downcast_apply(|c: &EmbeddedPythonConfig| c.config.clone());
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
        starlark_eval_in_env(&mut env, "resources = PythonEmbeddedResources()").unwrap();
        starlark_eval_in_env(&mut env, "run_mode = python_run_mode_noop()").unwrap();
        starlark_eval_in_env(&mut env, "config = EmbeddedPythonConfig()").unwrap();

        let exe = starlark_eval_in_env(
            &mut env,
            "PythonExecutable('testapp', dist, resources, config, run_mode)",
        )
        .unwrap();

        assert_eq!(exe.get_type(), "PythonExecutable");

        exe.downcast_apply(|exe: &PreBuiltPythonExecutable| {
            assert_eq!(exe.run_mode, crate::py_packaging::config::RunMode::Noop);
        });
    }
}
