// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::starlark::{
        code_signing::{
            handle_file_manifest_signable_events, handle_signable_event, SigningAction,
            SigningContext,
        },
        file_manifest::FileManifestValue,
        wix_msi_builder::WiXMsiBuilderValue,
    },
    anyhow::{Context, Result},
    starlark::{
        environment::TypeValues,
        eval::call_stack::CallStack,
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
        get_context_value, optional_dict_arg, optional_str_arg, EnvironmentContext, ResolvedTarget,
        ResolvedTargetValue, RunMode,
    },
    std::{convert::TryFrom, path::Path},
    tugger_code_signing::SigningDestination,
    tugger_file_manifest::{FileEntry, FileManifest},
    tugger_wix::{WiXInstallerBuilder, WiXSimpleMsiBuilder, WxsBuilder},
};

fn error_context<F, T>(label: &str, f: F) -> Result<T, ValueError>
where
    F: FnOnce() -> anyhow::Result<T>,
{
    f().map_err(|e| {
        ValueError::Runtime(RuntimeError {
            code: "TUGGER_WIX_INSTALLER",
            message: format!("{:?}", e),
            label: label.to_string(),
        })
    })
}

pub struct WiXInstallerValue {
    pub inner: WiXInstallerBuilder,
    pub filename: String,
}

impl TypedValue for WiXInstallerValue {
    type Holder = Mutable<WiXInstallerValue>;
    const TYPE: &'static str = "WiXInstaller";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }
}

impl WiXInstallerValue {
    fn new_from_args(type_values: &TypeValues, id: String, filename: String) -> ValueResult {
        let build_context_value = get_context_value(type_values)?;
        let context = build_context_value
            .downcast_ref::<EnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        // TODO grab target triple properly.
        let builder = WiXInstallerBuilder::new(id, env!("HOST").to_string(), context.build_path());

        Ok(Value::new(WiXInstallerValue {
            inner: builder,
            filename,
        }))
    }

    fn add_build_files(&mut self, manifest: FileManifestValue) -> ValueResult {
        error_context("WiXInstaller.add_build_files()", || {
            self.inner
                .add_extra_build_files(&manifest.manifest)
                .context("adding extra build files from FileManifest")
        })?;

        Ok(Value::new(NoneType::None))
    }

    fn resolve_file_entry(&self, path: impl AsRef<Path>, force_read: bool) -> Result<FileEntry> {
        let entry = FileEntry::try_from(path.as_ref())?;

        Ok(if force_read {
            entry.to_memory()?
        } else {
            entry
        })
    }

    fn add_build_file(
        &mut self,
        install_path: String,
        filesystem_path: String,
        force_read: bool,
    ) -> ValueResult {
        error_context("WiXInstaller.add_build_file()", || {
            let entry = self
                .resolve_file_entry(&filesystem_path, force_read)
                .with_context(|| format!("resolving file entry: {}", filesystem_path))?;

            self.inner
                .add_extra_build_file(&install_path, entry)
                .with_context(|| format!("adding extra build file to {}", install_path))
        })?;

        Ok(Value::new(NoneType::None))
    }

    fn add_install_file(
        &mut self,
        type_values: &TypeValues,
        call_stack: &mut CallStack,
        install_path: String,
        filesystem_path: String,
        force_read: bool,
    ) -> ValueResult {
        const LABEL: &str = "WiXInstaller.add_install_file()";

        let manifest = error_context(LABEL, || {
            let mut manifest = FileManifest::default();

            let entry = self
                .resolve_file_entry(&filesystem_path, force_read)
                .with_context(|| format!("resolving file entry from path {}", filesystem_path))?;

            manifest
                .add_file_entry(&install_path, entry)
                .context("adding FileEntry to InstallManifest")?;

            Ok(manifest)
        })?;

        self.add_install_files_from_manifest(type_values, call_stack, LABEL, &manifest)
    }

    fn add_install_files(
        &mut self,
        type_values: &TypeValues,
        call_stack: &mut CallStack,
        manifest: FileManifestValue,
    ) -> ValueResult {
        self.add_install_files_from_manifest(
            type_values,
            call_stack,
            "WiXInstaller.add_install_files()",
            &manifest.manifest,
        )
    }

    fn add_install_files_from_manifest(
        &mut self,
        type_values: &TypeValues,
        call_stack: &mut CallStack,
        label: &'static str,
        manifest: &FileManifest,
    ) -> ValueResult {
        error_context(label, || {
            let manifest = handle_file_manifest_signable_events(
                type_values,
                call_stack,
                manifest,
                label,
                SigningAction::WindowsInstallerFileAdded,
            )
            .context("running code signing checks for FileManifest")?;

            self.inner
                .install_files_mut()
                .add_manifest(&manifest)
                .context("adding install files from FileManifest")
        })?;

        Ok(Value::new(NoneType::None))
    }

    fn add_msi_builder(&mut self, builder: WiXMsiBuilderValue) -> ValueResult {
        error_context("WiXInstaller.add_msi_builder()", || {
            builder
                .inner
                .add_to_installer_builder(&mut self.inner)
                .context("adding WiXInstallerBuilder")
        })?;

        Ok(Value::new(NoneType::None))
    }

    fn add_simple_installer(
        &mut self,
        id_prefix: String,
        product_name: String,
        product_version: String,
        product_manufacturer: String,
        program_files: FileManifestValue,
    ) -> ValueResult {
        error_context("WiXInstaller.add_simple_installer()", || {
            let mut builder = WiXSimpleMsiBuilder::new(
                &id_prefix,
                &product_name,
                &product_version,
                &product_manufacturer,
            );
            builder
                .add_program_files_manifest(&program_files.manifest)
                .context("adding program files manifest")?;

            builder
                .add_to_installer_builder(&mut self.inner)
                .context("adding to WiXInstallerBuilder")
        })?;

        Ok(Value::new(NoneType::None))
    }

    fn add_wxs_file(&mut self, path: String, preprocessor_parameters: Value) -> ValueResult {
        optional_dict_arg(
            "preprocessor_parameters",
            "string",
            "string",
            &preprocessor_parameters,
        )?;

        let mut builder = error_context("WiXInstaller.add_wxs_file()", || {
            WxsBuilder::from_path(path).context("constructing WxsBuilder from path")
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

    fn build(
        &mut self,
        type_values: &TypeValues,
        call_stack: &mut CallStack,
        target: String,
    ) -> ValueResult {
        let context_value = get_context_value(type_values)?;
        let context = context_value
            .downcast_ref::<EnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        let output_path = context.target_build_path(&target);
        let installer_path = output_path.join(&self.filename);

        error_context("WiXInstaller.build()", || {
            self.inner
                .add_files_manifest_wxs()
                .context("generating install_files manifest wxs")?;

            self.inner
                .build(context.logger(), &installer_path)
                .context("building")
        })?;

        let candidate = installer_path.as_path().into();
        let mut context = SigningContext::new(
            "build()",
            SigningAction::WindowsInstallerCreation,
            &self.filename,
            &candidate,
        );
        context.set_path(&installer_path);
        context.set_signing_destination(SigningDestination::File(installer_path.clone()));

        handle_signable_event(type_values, call_stack, context)?;

        Ok(Value::new(ResolvedTargetValue {
            inner: ResolvedTarget {
                run_mode: RunMode::Path {
                    path: installer_path,
                },
                output_path,
            },
        }))
    }

    fn set_variable(&mut self, key: String, value: Value) -> ValueResult {
        let value = optional_str_arg("value", &value)?;
        self.inner.set_variable(key, value);

        Ok(Value::new(NoneType::None))
    }
}

starlark_module! { wix_installer_module =>
    #[allow(non_snake_case)]
    WiXInstaller(env env, id: String, filename: String) {
        WiXInstallerValue::new_from_args(env, id, filename)
    }

    WiXInstaller.add_build_file(
        this,
        build_path: String,
        filesystem_path: String,
        force_read: bool = false
    ) {
        let mut this = this.downcast_mut::<WiXInstallerValue>().unwrap().unwrap();
        this.add_build_file(build_path, filesystem_path, force_read)
    }

    WiXInstaller.add_build_files(this, manifest: FileManifestValue) {
        let mut this = this.downcast_mut::<WiXInstallerValue>().unwrap().unwrap();
        this.add_build_files(manifest)
    }

    WiXInstaller.add_install_file(
        env env,
        call_stack cs,
        this,
        install_path: String,
        filesystem_path: String,
        force_read: bool = false
    ) {
        let mut this = this.downcast_mut::<WiXInstallerValue>().unwrap().unwrap();
        this.add_install_file(env, cs, install_path, filesystem_path, force_read)
    }

    WiXInstaller.add_install_files(
        env env, call_stack cs, this, manifest: FileManifestValue) {
        let mut this = this.downcast_mut::<WiXInstallerValue>().unwrap().unwrap();
        this.add_install_files(env, cs, manifest)
    }

    WiXInstaller.add_msi_builder(this, builder: WiXMsiBuilderValue) {
        let mut this = this.downcast_mut::<WiXInstallerValue>().unwrap().unwrap();
        this.add_msi_builder(builder)
    }

    WiXInstaller.add_simple_installer(
        this,
        id_prefix: String,
        product_name: String,
        product_version: String,
        product_manufacturer: String,
        program_files: FileManifestValue
    ) {
        let mut this = this.downcast_mut::<WiXInstallerValue>().unwrap().unwrap();
        this.add_simple_installer(
            id_prefix,
            product_name,
            product_version,
            product_manufacturer,
            program_files,
        )
    }

    WiXInstaller.add_wxs_file(this, path: String, preprocessor_parameters = NoneType::None) {
        let mut this = this.downcast_mut::<WiXInstallerValue>().unwrap().unwrap();
        this.add_wxs_file(path, preprocessor_parameters)
    }

    WiXInstaller.build(env env, call_stack cs, this, target: String) {
        let mut this = this.downcast_mut::<WiXInstallerValue>().unwrap().unwrap();
        this.build(env, cs, target)
    }

    WiXInstaller.set_variable(this, key: String, value) {
        let mut this = this.downcast_mut::<WiXInstallerValue>().unwrap().unwrap();
        this.set_variable(key, value)
    }
}

#[cfg(test)]
mod tests {
    use {super::*, crate::starlark::testutil::*, anyhow::Result};

    #[test]
    fn test_constructor() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        let installer = env.eval("WiXInstaller('myapp', 'ignored')")?;
        assert_eq!(installer.get_type(), WiXInstallerValue::TYPE);

        Ok(())
    }

    #[test]
    fn test_add_missing_file() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        env.eval("installer = WiXInstaller('myapp', 'ignored')")?;
        assert!(env
            .eval("installer.add_wxs_file('does-not-exist')")
            .is_err());

        Ok(())
    }

    #[test]
    fn test_set_variable() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        env.eval("installer = WiXInstaller('myapp', 'ignored')")?;
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

        env.eval("installer = WiXInstaller('myapp', 'ignored')")?;
        env.eval(
            "installer.add_simple_installer('myapp', 'myapp', '0.1', 'author', FileManifest())",
        )?;

        Ok(())
    }

    #[test]
    fn test_add_build_files() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        env.eval("installer = WiXInstaller('myapp', 'ignored')")?;
        env.eval("m = FileManifest()")?;
        env.eval("installer.add_build_files(m)")?;

        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn test_build_simple_installer() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        env.eval("installer = WiXInstaller('myapp', 'myapp.msi')")?;
        env.eval(
            "installer.add_simple_installer('myapp', 'myapp', '0.1', 'author', FileManifest())",
        )?;
        let resolved_value = env.eval("installer.build('test_build_simple_installer')")?;

        assert_eq!(resolved_value.get_type(), "ResolvedTarget");

        let resolved = resolved_value
            .downcast_ref::<ResolvedTargetValue>()
            .unwrap();

        let context_value = get_context_value(&env.type_values).unwrap();
        let context = context_value.downcast_ref::<EnvironmentContext>().unwrap();

        let build_path = context.target_build_path("test_build_simple_installer");
        let msi_path = build_path.join("myapp.msi");

        assert_eq!(
            resolved.inner.run_mode,
            RunMode::Path {
                path: msi_path.clone()
            }
        );
        assert!(msi_path.exists());

        Ok(())
    }
}
