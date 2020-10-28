// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{
        py_packaging::binary::PythonBinaryBuilder,
        starlark::env::{get_context, PyOxidizerEnvironmentContext},
    },
    anyhow::{anyhow, Result},
    slog::warn,
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
            context.logger(),
            "writing Python embedded artifacts to {}",
            output_path.display()
        );

        let embedded = self
            .exe
            .to_embedded_python_context(context.logger(), &context.build_opt_level)?;

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

        Ok(Value::new(ResolvedTargetValue {
            inner: self
                .build(type_values, &target, &pyoxidizer_context)
                .map_err(|e| {
                    ValueError::from(RuntimeError {
                        code: "PYOXIDIZER",
                        message: e.to_string(),
                        label: "build()".to_string(),
                    })
                })?,
        }))
    }
}

starlark_module! { python_embedded_resources_module =>
    PythonEmbeddedResources.build(
        env env,
        this,
        target: String
    ) {
        match this.clone().downcast_ref::<PythonEmbeddedResourcesValue>() {
            Some(resources) => resources.build_starlark(env, target),
            None => Err(ValueError::IncorrectParameterType),
        }
    }
}
