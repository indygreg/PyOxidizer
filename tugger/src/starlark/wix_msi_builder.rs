// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::starlark::file_resource::FileManifestValue,
    anyhow::Result,
    starlark::{
        environment::TypeValues,
        values::{
            error::{
                RuntimeError, UnsupportedOperation, ValueError, INCORRECT_PARAMETER_TYPE_ERROR_CODE,
            },
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
    tugger_wix::WiXSimpleMsiBuilder,
};

#[derive(Clone)]
pub struct WiXMsiBuilderValue {
    pub inner: WiXSimpleMsiBuilder,
    /// Explicit filename to use for the built MSI.
    msi_filename: Option<String>,
    /// The target architecture we are building for.
    pub target_triple: String,
}

impl TypedValue for WiXMsiBuilderValue {
    type Holder = Mutable<WiXMsiBuilderValue>;
    const TYPE: &'static str = "WiXMSIBuilder";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }

    fn set_attr(&mut self, attribute: &str, value: Value) -> Result<(), ValueError> {
        match attribute {
            "banner_bmp_path" => {
                self.inner = self.inner.clone().banner_bmp_path(value.to_string());
            }
            "dialog_bmp_path" => {
                self.inner = self.inner.clone().dialog_bmp_path(value.to_string());
            }
            "eula_rtf_path" => {
                self.inner = self.inner.clone().eula_rtf_path(value.to_string());
            }
            "help_url" => {
                self.inner = self.inner.clone().help_url(value.to_string());
            }
            "license_path" => {
                self.inner = self.inner.clone().license_path(value.to_string());
            }
            "msi_filename" => {
                self.msi_filename = Some(value.to_string());
            }
            "package_description" => {
                self.inner = self.inner.clone().package_description(value.to_string());
            }
            "package_keywords" => {
                self.inner = self.inner.clone().package_keywords(value.to_string());
            }
            "product_icon_path" => {
                self.inner = self.inner.clone().product_icon_path(value.to_string());
            }
            "target_triple" => {
                self.target_triple = value.to_string();
            }
            "upgrade_code" => {
                self.inner = self.inner.clone().upgrade_code(value.to_string());
            }
            attr => {
                return Err(ValueError::OperationNotSupported {
                    op: UnsupportedOperation::SetAttr(attr.to_string()),
                    left: Self::TYPE.to_string(),
                    right: None,
                })
            }
        }

        Ok(())
    }
}

impl WiXMsiBuilderValue {
    pub fn new_from_args(
        id_prefix: String,
        product_name: String,
        product_version: String,
        product_manufacturer: String,
    ) -> ValueResult {
        let inner = WiXSimpleMsiBuilder::new(
            &id_prefix,
            &product_name,
            &product_version,
            &product_manufacturer,
        );

        Ok(Value::new(WiXMsiBuilderValue {
            inner,
            msi_filename: None,
            target_triple: env!("HOST").to_string(),
        }))
    }

    pub fn add_program_files_manifest(&mut self, manifest: FileManifestValue) -> ValueResult {
        self.inner
            .add_program_files_manifest(&manifest.manifest)
            .map_err(|e| {
                ValueError::Runtime(RuntimeError {
                    code: "TUGGER_WIX_MSI_BUILDER",
                    message: format!("{:?}", e),
                    label: "add_program_files_manifest()".to_string(),
                })
            })?;

        Ok(Value::new(NoneType::None))
    }

    pub fn add_visual_cpp_redistributable(
        &mut self,
        redist_version: String,
        platform: String,
    ) -> ValueResult {
        let platform = VcRedistributablePlatform::try_from(platform.as_str()).map_err(|e| {
            ValueError::Runtime(RuntimeError {
                code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                message: e,
                label: "add_visual_cpp_redistributable()".to_string(),
            })
        })?;

        self.inner
            .add_visual_cpp_redistributable(&redist_version, platform)
            .map_err(|e| {
                ValueError::Runtime(RuntimeError {
                    code: "TUGGER_WIX_MSI_BUILDER",
                    message: format!("{:?}", e),
                    label: "add_visual_cpp_redistributable()".to_string(),
                })
            })?;

        Ok(Value::new(NoneType::None))
    }

    pub fn build(&self, type_values: &TypeValues, target: String) -> ValueResult {
        let context_value = get_context_value(type_values)?;
        let context = context_value
            .downcast_ref::<EnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        let output_path = context.target_build_path(&target);

        let builder = self
            .inner
            .to_installer_builder(&self.target_triple, &output_path)
            .map_err(|e| {
                ValueError::Runtime(RuntimeError {
                    code: "TUGGER_WIX_MSI_BUILDER",
                    message: format!("{:?}", e),
                    label: "build()".to_string(),
                })
            })?;

        let msi_path = output_path.join(self.msi_filename());

        builder.build(context.logger(), &msi_path).map_err(|e| {
            ValueError::Runtime(RuntimeError {
                code: "TUGGER_WIX_MSI_BUILDER",
                message: format!("{:?}", e),
                label: "build()".to_string(),
            })
        })?;

        Ok(Value::new(ResolvedTargetValue {
            inner: ResolvedTarget {
                run_mode: RunMode::Path { path: msi_path },
                output_path,
            },
        }))
    }

    pub fn msi_filename(&self) -> String {
        if let Some(filename) = &self.msi_filename {
            filename.clone()
        } else {
            self.inner.default_msi_filename()
        }
    }
}

starlark_module! { wix_msi_builder_module =>
    #[allow(non_snake_case)]
    WiXMSIBuilder(
        id_prefix: String,
        product_name: String,
        product_version: String,
        product_manufacturer: String
    ) {
        WiXMsiBuilderValue::new_from_args(id_prefix, product_name, product_version, product_manufacturer)
    }

    #[allow(non_snake_case)]
    WiXMSIBuilder.add_program_files_manifest(this, manifest: FileManifestValue) {
        let mut this = this.downcast_mut::<WiXMsiBuilderValue>().unwrap().unwrap();
        this.add_program_files_manifest(manifest)
    }

    #[allow(non_snake_case)]
    WiXMSIBuilder.add_visual_cpp_redistributable(
        this,
        redist_version: String,
        platform: String
    ) {
        let mut this = this.downcast_mut::<WiXMsiBuilderValue>().unwrap().unwrap();
        this.add_visual_cpp_redistributable(redist_version, platform)
    }

    #[allow(non_snake_case)]
    WiXMSIBuilder.build(env env, this, target: String) {
        let this = this.downcast_ref::<WiXMsiBuilderValue>().unwrap();
        this.build(env, target)
    }
}

#[cfg(test)]
mod tests {
    use {super::*, crate::starlark::testutil::*};

    #[test]
    fn test_new() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        let v = env.eval("WiXMSIBuilder('prefix', 'name', '0.1', 'manufacturer')")?;
        assert_eq!(v.get_type(), "WiXMSIBuilder");

        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn test_build() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        env.eval("msi = WiXMSIBuilder('prefix', 'name', '0.1', 'manufacturer')")?;
        env.eval("msi.build('test_build')")?;

        let context_value = get_context_value(&env.type_values).unwrap();
        let context = context_value.downcast_ref::<EnvironmentContext>().unwrap();

        let build_path = context.target_build_path("test_build");
        let msi_path = build_path.join("name-0.1.msi");

        assert!(msi_path.exists());

        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn test_set_msi_filename() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        env.eval("msi = WiXMSIBuilder('prefix', 'name', '0.1', 'manufacturer')")?;
        env.eval("msi.msi_filename = 'myapp.msi'")?;
        env.eval("msi.build('test_set_msi_filename')")?;

        let context_value = get_context_value(&env.type_values).unwrap();
        let context = context_value.downcast_ref::<EnvironmentContext>().unwrap();

        let build_path = context.target_build_path("test_set_msi_filename");
        let msi_path = build_path.join("myapp.msi");

        assert!(msi_path.exists());

        Ok(())
    }

    #[test]
    fn test_add_visual_cpp_redistributable() -> Result<()> {
        if tugger_windows::find_visual_cpp_redistributable("14", VcRedistributablePlatform::X64)
            .is_err()
        {
            eprintln!("skipping test because Visual C++ Redistributable files not found");
            return Ok(());
        }

        let mut env = StarlarkEnvironment::new()?;
        env.eval("msi = WiXMSIBuilder('prefix', 'name', '0.1', 'manufacturer')")?;
        env.eval("msi.add_visual_cpp_redistributable('14', 'x64')")?;

        Ok(())
    }
}
