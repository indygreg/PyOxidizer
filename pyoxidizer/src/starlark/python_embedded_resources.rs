// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::py_packaging::binary::PythonBinaryBuilder,
    anyhow::Result,
    slog::warn,
    starlark::values::{Mutable, TypedValue, Value},
    starlark_dialect_build_targets::{BuildContext, BuildTarget, ResolvedTarget, RunMode},
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

impl BuildTarget for PythonEmbeddedResourcesValue {
    fn build(&mut self, context: &dyn BuildContext) -> Result<ResolvedTarget> {
        let output_path = context.get_state_path("output_path")?;

        warn!(
            context.logger(),
            "writing Python embedded artifacts to {}",
            output_path.display()
        );

        let embedded = self
            .exe
            .to_embedded_python_context(context.logger(), context.get_state_string("opt_level")?)?;

        embedded.write_files(output_path)?;

        Ok(ResolvedTarget {
            run_mode: RunMode::None,
            output_path: output_path.to_path_buf(),
        })
    }
}
