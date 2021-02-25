// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::starlark::wix_msi_builder::WiXMsiBuilderValue,
    starlark::{
        environment::TypeValues,
        values::{
            error::{RuntimeError, ValueError},
            none::NoneType,
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
    std::convert::TryFrom,
    tugger_windows::VcRedistributablePlatform,
    tugger_wix::{MsiPackage, WiXBundleInstallerBuilder},
};

#[derive(Clone)]
pub struct WiXBundleBuilderValue<'a> {
    pub inner: WiXBundleInstallerBuilder<'a>,
    pub target_triple: String,
    pub id_prefix: String,
    pub build_msis: Vec<WiXMsiBuilderValue>,
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
            build_msis: vec![],
        }))
    }

    /// WiXBundleBuilder.add_condition(condition, message)
    pub fn add_condition(&mut self, condition: String, message: String) -> ValueResult {
        self.inner.add_condition(&message, &condition);

        Ok(Value::new(NoneType::None))
    }

    /// WiXBundleBuilder.add_vc_redistributable(platform)
    pub fn add_vc_redistributable(
        &mut self,
        type_values: &TypeValues,
        platform: String,
    ) -> ValueResult {
        let platform = VcRedistributablePlatform::try_from(platform.as_str()).map_err(|e| {
            ValueError::Runtime(RuntimeError {
                code: "TUGGER_WIX_BUNDLE_BUILDER",
                message: e,
                label: "add_vc_redistributable()".to_string(),
            })
        })?;

        let context_value = get_context_value(type_values)?;
        let context = context_value
            .downcast_ref::<EnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        self.inner
            .add_vc_redistributable(context.logger(), platform, context.build_path())
            .map_err(|e| {
                ValueError::Runtime(RuntimeError {
                    code: "TUGGER_WIX_BUNDLE_BUILDER",
                    message: format!("{:?}", e),
                    label: "add_vc_redistributable()".to_string(),
                })
            })?;

        Ok(Value::new(NoneType::None))
    }

    /// WiXBundleBuilder.add_wix_msi_builder(builder)
    pub fn add_wix_msi_builder(
        &mut self,
        builder: WiXMsiBuilderValue,
        display_internal_ui: bool,
        install_condition: Value,
    ) -> ValueResult {
        let mut package = MsiPackage {
            source_file: Some(builder.msi_filename().into()),
            ..Default::default()
        };

        if display_internal_ui {
            package.display_internal_ui = Some("yes".into());
        }

        if install_condition.get_type() != "NoneType" {
            package.install_condition = Some(install_condition.to_string().into());
        }

        self.build_msis.push(builder);
        self.inner.chain(package.into());

        Ok(Value::new(NoneType::None))
    }

    /// WiXBundleBuilder.build(target)
    pub fn build(&self, type_values: &TypeValues, target: String) -> ValueResult {
        let context_value = get_context_value(type_values)?;
        let context = context_value
            .downcast_ref::<EnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        let output_path = context.target_build_path(&target);

        // We need to ensure dependent MSIs are built.
        for builder in self.build_msis.iter() {
            builder.build(type_values, target.clone())?;
        }

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

    WiXBundleBuilder.add_condition(this, condition: String, message: String) {
        let mut this = this.downcast_mut::<WiXBundleBuilderValue>().unwrap().unwrap();
        this.add_condition(condition, message)
    }

    WiXBundleBuilder.add_vc_redistributable(env env, this, platform: String) {
        let mut this = this.downcast_mut::<WiXBundleBuilderValue>().unwrap().unwrap();
        this.add_vc_redistributable(env, platform)
    }

    WiXBundleBuilder.add_wix_msi_builder(
        this,
        builder: WiXMsiBuilderValue,
        display_internal_ui: bool = false,
        install_condition = NoneType::None
    ) {
        let mut this = this.downcast_mut::<WiXBundleBuilderValue>().unwrap().unwrap();
        this.add_wix_msi_builder(builder, display_internal_ui, install_condition)
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

    #[test]
    fn test_add_condition() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        env.eval("builder = WiXBundleBuilder('prefix', 'name', '0.1', 'manufacturer')")?;
        env.eval("builder.add_condition('condition', 'message')")?;

        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn test_build() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        env.eval("builder = WiXBundleBuilder('prefix', 'name', '0.1', 'manufacturer')")?;
        env.eval("builder.add_vc_redistributable('x64')")?;
        env.eval("builder.build('bundle_builder_test_build')")?;

        let context_value = get_context_value(&env.type_values).unwrap();
        let context = context_value.downcast_ref::<EnvironmentContext>().unwrap();

        let build_path = context.target_build_path("bundle_builder_test_build");
        let exe_path = build_path.join("name-0.1.exe");

        assert!(
            exe_path.exists(),
            format!("exe exists: {}", exe_path.display())
        );

        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn test_add_wix_msi_builder() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        env.eval("msi = WiXMSIBuilder('prefix', 'msi', '0.1', 'manufacturer')")?;
        env.eval("builder = WiXBundleBuilder('prefix', 'name', '0.1', 'manufacturer')")?;
        env.eval("builder.add_wix_msi_builder(msi)")?;
        env.eval("builder.build('bundle_builder_add_wix_msi_builder')")?;

        let context_value = get_context_value(&env.type_values).unwrap();
        let context = context_value.downcast_ref::<EnvironmentContext>().unwrap();

        let build_path = context.target_build_path("bundle_builder_add_wix_msi_builder");
        let exe_path = build_path.join("name-0.1.exe");

        assert!(
            exe_path.exists(),
            format!("exe exists: {}", exe_path.display())
        );

        Ok(())
    }
}
