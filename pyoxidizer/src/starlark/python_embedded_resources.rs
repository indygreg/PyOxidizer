// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    super::target::{BuildContext, BuildTarget, ResolvedTarget, RunMode},
    crate::py_packaging::binary::PythonBinaryBuilder,
    anyhow::Result,
    slog::warn,
    starlark::environment::Environment,
    starlark::values::{default_compare, TypedValue, Value, ValueError, ValueResult},
    starlark::{any, immutable, not_supported},
    std::any::Any,
    std::cmp::Ordering,
    std::collections::HashMap,
};

pub struct PythonEmbeddedResources {
    pub exe: Box<dyn PythonBinaryBuilder>,
}

impl TypedValue for PythonEmbeddedResources {
    immutable!();
    any!();
    not_supported!(binop);
    not_supported!(container);
    not_supported!(function);
    not_supported!(get_hash);
    not_supported!(to_int);

    fn to_str(&self) -> String {
        "PythonEmbeddedResources".to_string()
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_type(&self) -> &'static str {
        "PythonEmbeddedResources"
    }

    fn to_bool(&self) -> bool {
        true
    }

    fn compare(&self, other: &dyn TypedValue, _recursion: u32) -> Result<Ordering, ValueError> {
        default_compare(self, other)
    }
}

impl BuildTarget for PythonEmbeddedResources {
    fn build(&mut self, context: &BuildContext) -> Result<ResolvedTarget> {
        warn!(
            &context.logger,
            "writing Python embedded artifacts to {}",
            context.output_path.display()
        );

        let embedded = self
            .exe
            .as_embedded_python_binary_data(&context.logger, &context.opt_level)?;

        embedded.write_files(&context.output_path)?;

        Ok(ResolvedTarget {
            run_mode: RunMode::None,
            output_path: context.output_path.clone(),
        })
    }
}
