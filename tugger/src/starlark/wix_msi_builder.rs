// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{starlark::file_resource::FileManifestValue, wix::WiXSimpleMSIBuilder},
    anyhow::Result,
    starlark::{
        values::{
            error::{RuntimeError, UnsupportedOperation, ValueError},
            none::NoneType,
            {Mutable, TypedValue, Value, ValueResult},
        },
        {
            starlark_fun, starlark_module, starlark_parse_param_type, starlark_signature,
            starlark_signature_extraction, starlark_signatures,
        },
    },
};

#[derive(Clone)]
pub struct WiXMSIBuilderValue {
    pub inner: WiXSimpleMSIBuilder,
}

impl TypedValue for WiXMSIBuilderValue {
    type Holder = Mutable<WiXMSIBuilderValue>;
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
            "package_description" => {
                self.inner = self.inner.clone().package_description(value.to_string());
            }
            "package_keywords" => {
                self.inner = self.inner.clone().package_keywords(value.to_string());
            }
            "product_icon_path" => {
                self.inner = self.inner.clone().product_icon_path(value.to_string());
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

impl WiXMSIBuilderValue {
    pub fn new_from_args(
        id_prefix: String,
        product_name: String,
        product_version: String,
        product_manufacturer: String,
    ) -> ValueResult {
        let inner = WiXSimpleMSIBuilder::new(
            &id_prefix,
            &product_name,
            &product_version,
            &product_manufacturer,
        );

        Ok(Value::new(WiXMSIBuilderValue { inner }))
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
}

starlark_module! { wix_msi_builder_module =>
    #[allow(non_snake_case)]
    WiXMSIBuilder(
        id_prefix: String,
        product_name: String,
        product_version: String,
        product_manufacturer: String
    ) {
        WiXMSIBuilderValue::new_from_args(id_prefix, product_name, product_version, product_manufacturer)
    }

    #[allow(non_snake_case)]
    WiXMSIBuilder.add_program_files_manifest(this, manifest: FileManifestValue) {
        let mut this = this.downcast_mut::<WiXMSIBuilderValue>().unwrap().unwrap();
        this.add_program_files_manifest(manifest)
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
}
