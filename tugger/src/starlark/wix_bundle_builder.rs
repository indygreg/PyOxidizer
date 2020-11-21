// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::wix::WiXBundleInstallerBuilder,
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
    starlark_dialect_build_targets::{
        get_context_value, EnvironmentContext, ResolvedTarget, ResolvedTargetValue, RunMode,
    },
};

#[derive(Clone)]
pub struct WiXBundleBuilderValue<'a> {
    pub inner: WiXBundleInstallerBuilder<'a>,
    pub target_triple: String,
    pub id_prefix: String,
}

impl TypedValue for WiXBundleBuilderValue<'static> {
    type Holder = Mutable<WiXBundleBuilderValue<'static>>;
    const TYPE: &'static str = "WiXBundleBuilder";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }
}

impl<'a> WiXBundleBuilderValue<'a> {
    pub fn new_from_args(
        id_prefix: String,
        name: String,
        version: String,
        manufacturer: String,
    ) -> ValueResult {
        let inner = WiXBundleInstallerBuilder::new(name, version, manufacturer);

        Ok(Value::new(WiXBundleBuilderValue {
            inner,
            target_triple: env!("HOST").to_string(),
            id_prefix,
        }))
    }

    /// WiXBundleBuilder.build(target)
    pub fn build(&self, type_values: &TypeValues, target: String) -> ValueResult {
        let context_value = get_context_value(type_values)?;
        let context = context_value
            .downcast_ref::<EnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        let output_path = context.target_build_path(&target);

        let builder = self
            .inner
            .to_installer_builder(&self.id_prefix, &self.target_triple, &output_path)
            .map_err(|e| {
                ValueError::Runtime(RuntimeError {
                    code: "TUGGER_WIX_BUNDLE_BUILDER",
                    message: format!("{:?}", e),
                    label: "build()".to_string(),
                })
            })?;

        let exe_path = output_path.join(self.inner.default_exe_filename());

        builder.build(context.logger(), &exe_path).map_err(|e| {
            ValueError::Runtime(RuntimeError {
                code: "TUGGER_WIX_BUNDLE_BUILDER",
                message: format!("{:?}", e),
                label: "build()".to_string(),
            })
        })?;

        Ok(Value::new(ResolvedTargetValue {
            inner: ResolvedTarget {
                run_mode: RunMode::Path { path: exe_path },
                output_path,
            },
        }))
    }
}

starlark_module! { wix_bundle_builder_module =>
    #[allow(non_snake_case)]
    WiXBundleBuilder(
        id_prefix: String,
        name: String,
        version: String,
        manufacturer: String
    ) {
        WiXBundleBuilderValue::new_from_args(id_prefix, name, version, manufacturer)
    }

    WiXBundleBuilder.build(env env, this, target: String) {
        let this = this.downcast_ref::<WiXBundleBuilderValue>().unwrap();
        this.build(env, target)
    }
}

#[cfg(test)]
mod tests {
    use {super::*, crate::starlark::testutil::*, anyhow::Result};

    #[test]
    fn test_new() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        let v = env.eval("WiXBundleBuilder('prefix', 'name', '0.1', 'manufacturer')")?;
        assert_eq!(v.get_type(), "WiXBundleBuilder");

        let builder = v.downcast_ref::<WiXBundleBuilderValue>().unwrap();
        assert_eq!(builder.id_prefix, "prefix");
        assert_eq!(builder.target_triple, env!("HOST"));

        Ok(())
    }
}
