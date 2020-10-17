// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    super::target::{BuildContext, BuildTarget, ResolvedTarget, RunMode},
    crate::py_packaging::binary::PythonBinaryBuilder,
    anyhow::Result,
    slog::warn,
    starlark::values::{Mutable, TypedValue, Value},
    std::sync::Arc,
};

pub struct PythonEmbeddedResources {
    pub exe: Arc<dyn PythonBinaryBuilder>,
}

impl TypedValue for PythonEmbeddedResources {
    type Holder = Mutable<PythonEmbeddedResources>;
    const TYPE: &'static str = "PythonEmbeddedResources";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
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
