// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::starlark::{
        code_signing::{handle_signable_event, SigningAction, SigningContext},
        file_content::FileContentWrapper,
        wix_msi_builder::WiXMsiBuilderValue,
    },
    anyhow::Context,
    simple_file_manifest::FileEntry,
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
        get_context_value, EnvironmentContext, ResolvedTarget, ResolvedTargetValue, RunMode,
    },
    std::path::{Path, PathBuf},
    tugger_code_signing::SigningDestination,
    tugger_windows::VcRedistributablePlatform,
    tugger_wix::{MsiPackage, WiXBundleInstallerBuilder},
};

fn error_context<F, T>(label: &str, f: F) -> Result<T, ValueError>
where
    F: FnOnce() -> anyhow::Result<T>,
{
    f().map_err(|e| {
        ValueError::Runtime(RuntimeError {
            code: "TUGGER_WIX_BUNDLE_BUILDER",
            message: format!("{:?}", e),
            label: label.to_string(),
        })
    })
}

pub struct WiXBundleBuilderValue<'a> {
    pub inner: WiXBundleInstallerBuilder<'a>,
    pub arch: String,
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
        arch: String,
    ) -> ValueResult {
        let inner = WiXBundleInstallerBuilder::new(name, version, manufacturer);

        Ok(Value::new(WiXBundleBuilderValue {
            inner,
            arch,
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
        let context_value = get_context_value(type_values)?;
        let context = context_value
            .downcast_ref::<EnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        error_context("WiXBundleBuilder.add_vc_redistributable()", || {
            let platform = VcRedistributablePlatform::try_from(platform.as_str())
                .context("obtaining VcRedistributablePlatform from str")?;

            self.inner
                .add_vc_redistributable(platform, context.build_path())
                .context("adding VC++ Redistributable to bundle builder")
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
        const LABEL: &str = "WiXBundleBuilder.add_wix_msi_builder()";

        let mut package = MsiPackage {
            source_file: Some(builder.msi_filename(LABEL)?.into()),
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

    fn materialize(
        &self,
        type_values: &TypeValues,
        call_stack: &mut CallStack,
        label: &'static str,
        dest_dir: &Path,
    ) -> Result<(PathBuf, String), ValueError> {
        // We need to ensure dependent MSIs are built.
        for builder in self.build_msis.iter() {
            builder.materialize(type_values, call_stack, label, dest_dir)?;
        }

        let builder = error_context(label, || {
            self.inner
                .to_installer_builder(&self.id_prefix, &self.arch, dest_dir)
                .context("converting to WiXInstallerBuilder")
        })?;

        let filename = self.inner.default_exe_filename();
        let exe_path = dest_dir.join(&filename);

        error_context(label, || {
            builder
                .build(&exe_path)
                .context("building WiXInstallerBuilder")
        })?;

        let candidate = exe_path.as_path().into();
        let mut context = SigningContext::new(
            label,
            SigningAction::WindowsInstallerCreation,
            filename.clone(),
            &candidate,
        );
        context.set_path(&exe_path);
        context.set_signing_destination(SigningDestination::File(exe_path.clone()));

        handle_signable_event(type_values, call_stack, context)?;

        Ok((exe_path, filename))
    }

    fn materialize_temp_dir(
        &self,
        type_values: &TypeValues,
        call_stack: &mut CallStack,
        label: &'static str,
    ) -> Result<(FileEntry, String), ValueError> {
        let build_path = {
            let context_value = get_context_value(type_values)?;
            let context = context_value
                .downcast_ref::<EnvironmentContext>()
                .ok_or(ValueError::IncorrectParameterType)?;

            context.build_path().to_path_buf()
        };

        let dest_dir = error_context(label, || {
            tempfile::Builder::new()
                .prefix("wix-bundle-builder-")
                .tempdir_in(&build_path)
                .context("creating temp directory")
        })?;

        let (installer_path, filename) =
            self.materialize(type_values, call_stack, label, dest_dir.path())?;

        let entry = FileEntry::new_from_path(&installer_path, false);

        let entry = error_context(label, || {
            entry
                .to_memory()
                .context("converting FileEntry to in-memory")
        })?;

        Ok((entry, filename))
    }

    /// WiXBundleBuilder.build(target)
    pub fn build(
        &self,
        type_values: &TypeValues,
        call_stack: &mut CallStack,
        target: String,
    ) -> ValueResult {
        const LABEL: &str = "WiXBundleBuilder.build()";

        let dest_dir = {
            let context_value = get_context_value(type_values)?;
            let context = context_value
                .downcast_ref::<EnvironmentContext>()
                .ok_or(ValueError::IncorrectParameterType)?;

            context.target_build_path(&target)
        };

        let exe_path = self
            .materialize(type_values, call_stack, LABEL, &dest_dir)?
            .0;

        Ok(Value::new(ResolvedTargetValue {
            inner: ResolvedTarget {
                run_mode: RunMode::Path { path: exe_path },
                output_path: dest_dir,
            },
        }))
    }

    pub fn to_file_content(
        &self,
        type_values: &TypeValues,
        call_stack: &mut CallStack,
    ) -> ValueResult {
        const LABEL: &str = "WiXBundleBuilder.to_file_content()";

        let (content, filename) = self.materialize_temp_dir(type_values, call_stack, LABEL)?;

        Ok(FileContentWrapper { content, filename }.into())
    }

    pub fn write_to_directory(
        &self,
        type_values: &TypeValues,
        call_stack: &mut CallStack,
        path: String,
    ) -> ValueResult {
        const LABEL: &str = "WiXBundleBuilder.write_to_directory()";

        let dest_dir = {
            let context_value = get_context_value(type_values)?;
            let context = context_value
                .downcast_ref::<EnvironmentContext>()
                .ok_or(ValueError::IncorrectParameterType)?;

            context.resolve_path(path)
        };

        let (content, filename) = self.materialize_temp_dir(type_values, call_stack, LABEL)?;

        let dest_path = dest_dir.join(&filename);

        error_context(LABEL, || {
            content
                .write_to_path(&dest_path)
                .with_context(|| format!("writing {}", dest_path.display()))
        })?;

        Ok(Value::from(format!("{}", dest_path.display())))
    }
}

starlark_module! { wix_bundle_builder_module =>
    #[allow(non_snake_case)]
    WiXBundleBuilder(
        id_prefix: String,
        name: String,
        version: String,
        manufacturer: String,
        arch: String = "x64".to_string()
    ) {
        WiXBundleBuilderValue::new_from_args(id_prefix, name, version, manufacturer, arch)
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

    WiXBundleBuilder.build(env env, call_stack cs, this, target: String) {
        let this = this.downcast_ref::<WiXBundleBuilderValue>().unwrap();
        this.build(env, cs, target)
    }

    WiXBundleBuilder.to_file_content(env env, call_stack cs, this) {
        let this = this.downcast_ref::<WiXBundleBuilderValue>().unwrap();
        this.to_file_content(env, cs)
    }

    WiXBundleBuilder.write_to_directory(env env, call_stack cs, this, path: String) {
        let this = this.downcast_ref::<WiXBundleBuilderValue>().unwrap();
        this.write_to_directory(env, cs, path)
    }
}

#[cfg(test)]
mod tests {
    use {super::*, crate::starlark::testutil::*, anyhow::Result};
    #[cfg(windows)]
    use {crate::starlark::file_content::FileContentValue, tugger_common::testutil::*};

    #[test]
    fn test_new() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        let v = env.eval("WiXBundleBuilder('prefix', 'name', '0.1', 'manufacturer')")?;
        assert_eq!(v.get_type(), "WiXBundleBuilder");

        let builder = v.downcast_ref::<WiXBundleBuilderValue>().unwrap();
        assert_eq!(builder.id_prefix, "prefix");
        assert_eq!(builder.arch, "x64");

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

        assert!(exe_path.exists(), "exe exist");

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

        assert!(exe_path.exists(), "exe exists");

        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn to_file_content() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        env.eval("builder = WiXBundleBuilder('prefix', 'name', '0.1', 'manufacturer')")?;
        env.eval("builder.add_vc_redistributable('x64')")?;
        let value = env.eval("builder.to_file_content()")?;

        assert_eq!(value.get_type(), FileContentValue::TYPE);

        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn write_to_directory() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        let dest_dir = DEFAULT_TEMP_DIR
            .path()
            .join("wix-bundle-builder-write-to-directory");
        let dest_dir_s = dest_dir.to_string_lossy().replace('\\', "/");

        env.eval("builder = WiXBundleBuilder('prefix', 'name', '0.1', 'manufacturer')")?;
        env.eval("builder.add_vc_redistributable('x64')")?;
        let value = env.eval(&format!("builder.write_to_directory('{}')", dest_dir_s))?;

        assert_eq!(value.get_type(), "string");
        let path = PathBuf::from(value.to_string());
        assert!(path.exists());

        Ok(())
    }
}
