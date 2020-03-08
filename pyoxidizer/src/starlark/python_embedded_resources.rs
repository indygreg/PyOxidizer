// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    super::target::{BuildContext, BuildTarget, ResolvedTarget, RunMode},
    crate::py_packaging::binary::PythonBinaryBuilder,
    anyhow::Result,
    slog::warn,
    starlark::values::error::ValueError,
    starlark::values::{default_compare, TypedValue},
    starlark::{any, immutable},
    std::cmp::Ordering,
};

pub struct PythonEmbeddedResources {
    pub exe: Box<dyn PythonBinaryBuilder>,
}

impl TypedValue for PythonEmbeddedResources {
    immutable!();
    any!();

    fn get_type(&self) -> &'static str {
        "PythonEmbeddedResources"
    }

    fn is_descendant(&self, _other: &dyn TypedValue) -> bool {
        false
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
            .to_embedded_python_context(&context.logger, &context.opt_level)?;

        embedded.write_files(&context.output_path)?;

        Ok(ResolvedTarget {
            run_mode: RunMode::None,
            output_path: context.output_path.clone(),
        })
    }
}
