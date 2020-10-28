// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{
        starlark::file_resource::FileManifestValue,
        wix::{WiXInstallerBuilder, WxsBuilder},
    },
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
        get_context_value, optional_dict_arg, optional_str_arg, EnvironmentContext,
    },
};

pub struct WiXInstallerValue {
    pub inner: WiXInstallerBuilder,
}

impl TypedValue for WiXInstallerValue {
    type Holder = Mutable<WiXInstallerValue>;
    const TYPE: &'static str = "WiXInstaller";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }
}

impl WiXInstallerValue {
    fn new_from_args(type_values: &TypeValues, id: String) -> ValueResult {
        let build_context_value = get_context_value(type_values)?;
        let context = build_context_value
            .downcast_ref::<EnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        // TODO grab target triple properly.
        let builder = WiXInstallerBuilder::new(id, env!("HOST").to_string(), context.build_path());

        Ok(Value::new(WiXInstallerValue { inner: builder }))
    }

    fn add_build_files_starlark(&mut self, manifest: FileManifestValue) -> ValueResult {
        self.inner
            .add_extra_build_files(&manifest.manifest)
            .map_err(|e| {
                ValueError::from(RuntimeError {
                    code: "TUGGER",
                    message: e.to_string(),
                    label: "add_build_files()".to_string(),
                })
            })?;

        Ok(Value::new(NoneType::None))
    }

    fn add_simple_installer_starlark(
        &mut self,
        product_name: String,
        product_version: String,
        product_manufacturer: String,
        program_files: FileManifestValue,
    ) -> ValueResult {
        self.inner
            .add_install_files_manifest(&program_files.manifest)
            .map_err(|e| {
                ValueError::from(RuntimeError {
                    code: "TUGGER",
                    message: e.to_string(),
                    label: "add_simple_installer()".to_string(),
                })
            })?;

        self.inner
            .add_simple_wxs(&product_name, &product_version, &product_manufacturer)
            .map_err(|e| {
                ValueError::from(RuntimeError {
                    code: "TUGGER",
                    message: e.to_string(),
                    label: "add_simple_installer()".to_string(),
                })
            })?;

        Ok(Value::new(NoneType::None))
    }

    fn add_wxs_file_starlark(
        &mut self,
        path: String,
        preprocessor_parameters: Value,
    ) -> ValueResult {
        optional_dict_arg(
            "preprocessor_parameters",
            "string",
            "string",
            &preprocessor_parameters,
        )?;

        let mut builder = WxsBuilder::from_path(path).map_err(|e| {
            ValueError::from(RuntimeError {
                code: "TUGGER",
                message: e.to_string(),
                label: "add_wxs_file()".to_string(),
            })
        })?;

        match preprocessor_parameters.get_type() {
            "dict" => {
                for key in preprocessor_parameters.iter()?.iter() {
                    let k = key.to_string();
                    let v = preprocessor_parameters.at(key).unwrap().to_string();

                    builder.set_preprocessor_parameter(k, v);
                }
            }
            "NoneType" => (),
            _ => panic!("should have validated type above"),
        }

        self.inner.add_wxs(builder);

        Ok(Value::new(NoneType::None))
    }

    fn set_variable_starlark(&mut self, key: String, value: Value) -> ValueResult {
        let value = optional_str_arg("value", &value)?;
        self.inner.set_variable(key, value);

        Ok(Value::new(NoneType::None))
    }
}

starlark_module! { wix_installer_module =>
    #[allow(non_snake_case)]
    WiXInstaller(env env, id: String) {
        WiXInstallerValue::new_from_args(env, id)
    }

    WiXInstaller.add_build_files(this, manifest: FileManifestValue) {
        match this.clone().downcast_mut::<WiXInstallerValue>()? {
            Some(mut installer) => installer.add_build_files_starlark(manifest),
            None => Err(ValueError::IncorrectParameterType),
        }
    }

    WiXInstaller.add_simple_installer(
        this,
        product_name: String,
        product_version: String,
        product_manufacturer: String,
        program_files: FileManifestValue
    ) {
        match this.clone().downcast_mut::<WiXInstallerValue>()? {
            Some(mut installer) => installer.add_simple_installer_starlark(
                product_name,
                product_version,
                product_manufacturer,
                program_files,
            ),
            None => Err(ValueError::IncorrectParameterType),
        }
    }

    WiXInstaller.add_wxs_file(this, path: String, preprocessor_parameters = NoneType::None) {
        match this.clone().downcast_mut::<WiXInstallerValue>()? {
            Some(mut installer) => installer.add_wxs_file_starlark(path, preprocessor_parameters),
            None => Err(ValueError::IncorrectParameterType),
        }
    }

    WiXInstaller.set_variable(this, key: String, value) {
        match this.clone().downcast_mut::<WiXInstallerValue>()? {
            Some(mut installer) => installer.set_variable_starlark(key, value),
            None => Err(ValueError::IncorrectParameterType),
        }
    }
}

#[cfg(test)]
mod tests {
    use {super::*, crate::starlark::testutil::*, anyhow::Result};

    #[test]
    fn test_constructor() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        let installer = env.eval("WiXInstaller('myapp')")?;
        assert_eq!(installer.get_type(), WiXInstallerValue::TYPE);

        Ok(())
    }

    #[test]
    fn test_add_missing_file() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        env.eval("installer = WiXInstaller('myapp')")?;
        assert!(env
            .eval("installer.add_wxs_file('does-not-exist')")
            .is_err());

        Ok(())
    }

    #[test]
    fn test_set_variable() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        env.eval("installer = WiXInstaller('myapp')")?;
        env.eval("installer.set_variable('foo', None)")?;
        env.eval("installer.set_variable('bar', 'baz')")?;
        let installer_value = env.eval("installer")?;
        let installer = installer_value.downcast_ref::<WiXInstallerValue>().unwrap();

        let variables = installer.inner.variables().collect::<Vec<_>>();
        assert_eq!(
            variables,
            vec![
                (&"bar".to_string(), &Some("baz".to_string())),
                (&"foo".to_string(), &None),
            ]
        );

        Ok(())
    }

    #[test]
    fn test_add_simple_installer() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        env.eval("installer = WiXInstaller('myapp')")?;
        env.eval("installer.add_simple_installer('myapp', '0.1', 'author', FileManifest())")?;

        Ok(())
    }

    #[test]
    fn test_add_build_files() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        env.eval("installer = WiXInstaller('myapp')")?;
        env.eval("m = FileManifest()")?;
        env.eval("installer.add_build_files(m)")?;

        Ok(())
    }
}
