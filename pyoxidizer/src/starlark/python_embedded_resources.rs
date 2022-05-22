// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{
        py_packaging::binary::PythonBinaryBuilder,
        starlark::env::{get_context, PyOxidizerEnvironmentContext},
    },
    anyhow::{anyhow, Context, Result},
    log::warn,
    starlark::{
        environment::TypeValues,
        values::{
            error::{RuntimeError, ValueError},
            {Mutable, TypedValue, Value, ValueResult},
        },
        {
            starlark_fun, starlark_module, starlark_parse_param_type, starlark_signature,
            starlark_signature_extraction, starlark_signatures,
        },
    },
    starlark_dialect_build_targets::{ResolvedTarget, ResolvedTargetValue, RunMode},
    std::sync::Arc,
};

fn error_context<F, T>(label: &str, f: F) -> Result<T, ValueError>
where
    F: FnOnce() -> anyhow::Result<T>,
{
    f().map_err(|e| {
        ValueError::Runtime(RuntimeError {
            code: "PYOXIDIZER_PYTHON_EMBEDDED_RESOURCES",
            message: format!("{:?}", e),
            label: label.to_string(),
        })
    })
}

pub struct PythonEmbeddedResourcesValue {
    pub exe: Arc<dyn PythonBinaryBuilder>,
}

impl TypedValue for PythonEmbeddedResourcesValue {
    type Holder = Mutable<PythonEmbeddedResourcesValue>;
    const TYPE: &'static str = "PythonEmbeddedResources";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }
}

impl PythonEmbeddedResourcesValue {
    fn build(
        &self,
        type_values: &TypeValues,
        target: &str,
        context: &PyOxidizerEnvironmentContext,
    ) -> Result<ResolvedTarget> {
        let output_path = context
            .get_output_path(type_values, target)
            .map_err(|_| anyhow!("unable to resolve output path"))?;

        warn!(
            "writing Python embedded artifacts to {}",
            output_path.display()
        );

        let embedded = self
            .exe
            .to_embedded_python_context(context.env(), &context.build_opt_level)?;

        std::fs::create_dir_all(&output_path)
            .with_context(|| format!("creating output directory: {}", output_path.display()))?;
        embedded.write_files(&output_path)?;

        Ok(ResolvedTarget {
            run_mode: RunMode::None,
            output_path,
        })
    }

    fn build_starlark(&self, type_values: &TypeValues, target: String) -> ValueResult {
        let pyoxidizer_context_value = get_context(type_values)?;
        let pyoxidizer_context = pyoxidizer_context_value
            .downcast_ref::<PyOxidizerEnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        let inner = error_context("PythonEmbeddedResources.build()", || {
            self.build(type_values, &target, &pyoxidizer_context)
        })?;

        Ok(Value::new(ResolvedTargetValue { inner }))
    }
}

starlark_module! { python_embedded_resources_module =>
    PythonEmbeddedResources.build(
        env env,
        this,
        target: String
    ) {
        let this = this.downcast_ref::<PythonEmbeddedResourcesValue>().unwrap();
        this.build_starlark(env, target)
    }
}
