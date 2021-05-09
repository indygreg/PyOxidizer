// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::starlark::{
        code_signing::{handle_signable_event, SigningAction, SigningContext},
        file_content::FileContentValue,
        file_manifest::FileManifestValue,
    },
    anyhow::{anyhow, Context},
    starlark::{
        environment::TypeValues,
        eval::call_stack::CallStack,
        values::{
            error::{RuntimeError, ValueError, INCORRECT_PARAMETER_TYPE_ERROR_CODE},
            none::NoneType,
            {Mutable, TypedValue, Value, ValueResult},
        },
        {
            starlark_fun, starlark_module, starlark_parse_param_type, starlark_signature,
            starlark_signature_extraction, starlark_signatures,
        },
    },
    starlark_dialect_build_targets::{
        get_context_value, optional_str_arg, EnvironmentContext, ResolvedTarget,
        ResolvedTargetValue, RunMode,
    },
    std::path::PathBuf,
    tugger_apple_bundle::MacOsApplicationBundleBuilder,
    tugger_code_signing::SigningDestination,
    tugger_file_manifest::FileData,
};

fn error_context<F, T>(label: &str, f: F) -> Result<T, ValueError>
where
    F: FnOnce() -> anyhow::Result<T>,
{
    f().map_err(|e| {
        ValueError::Runtime(RuntimeError {
            code: "TUGGER_MAC_OS_APPLICATION_BUNDLE_BUILDER",
            message: format!("{:?}", e),
            label: label.to_string(),
        })
    })
}

#[derive(Clone, Debug)]
pub struct MacOsApplicationBundleBuilderValue {
    pub inner: MacOsApplicationBundleBuilder,
}

impl TypedValue for MacOsApplicationBundleBuilderValue {
    type Holder = Mutable<MacOsApplicationBundleBuilderValue>;
    const TYPE: &'static str = "MacOsApplicationBundleBuilder";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }
}

impl MacOsApplicationBundleBuilderValue {
    pub fn new_from_args(bundle_name: String) -> ValueResult {
        let inner = error_context("MacOsApplicationBundleBuilder()", || {
            MacOsApplicationBundleBuilder::new(bundle_name)
        })?;

        Ok(Value::new(MacOsApplicationBundleBuilderValue { inner }))
    }

    pub fn add_icon(&mut self, path: String) -> ValueResult {
        error_context("MacOsApplicationBundleBuilder.add_icon()", || {
            self.inner.add_icon(FileData::from(PathBuf::from(path)))
        })?;

        Ok(Value::new(NoneType::None))
    }

    pub fn add_manifest(&mut self, manifest: FileManifestValue) -> ValueResult {
        error_context("MacOsApplicationBundleBuilder.add_manifest()", || {
            for (path, entry) in manifest.manifest.iter_entries() {
                self.inner
                    .add_file(PathBuf::from("Contents").join(path), entry.clone())
                    .with_context(|| format!("adding {}", path.display()))?;
            }

            Ok(())
        })?;

        Ok(Value::new(NoneType::None))
    }

    pub fn add_macos_file(&mut self, content: FileContentValue, path: Value) -> ValueResult {
        let path = optional_str_arg("path", &path)?;

        error_context("MacOsApplicationBundleBuilder.add_macos_file()", || {
            let path = if let Some(path) = path {
                PathBuf::from(path)
            } else {
                PathBuf::from(&content.filename)
            };

            self.inner
                .add_file_macos(path, content.content)
                .context("adding macOS file")
        })?;

        Ok(Value::new(NoneType::None))
    }

    pub fn add_macos_manifest(&mut self, manifest: FileManifestValue) -> ValueResult {
        error_context("MacOsApplicationBundleBuilder.add_macos_manifest()", || {
            for (path, entry) in manifest.manifest.iter_entries() {
                self.inner
                    .add_file_macos(path, entry.clone())
                    .with_context(|| format!("adding {}", path.display()))?;
            }

            Ok(())
        })?;

        Ok(Value::new(NoneType::None))
    }

    pub fn add_resources_file(&mut self, content: FileContentValue, path: Value) -> ValueResult {
        let path = optional_str_arg("path", &path)?;

        error_context("MacOsApplicationBundleBuilder.add_resources_file()", || {
            let path = if let Some(path) = path {
                PathBuf::from(path)
            } else {
                PathBuf::from(&content.filename)
            };

            self.inner
                .add_file_resources(path, content.content)
                .context("adding resources file")
        })?;

        Ok(Value::new(NoneType::None))
    }

    pub fn add_resources_manifest(&mut self, manifest: FileManifestValue) -> ValueResult {
        error_context(
            "MacOsApplicationBundleBuilder.add_resources_manifest()",
            || {
                for (path, entry) in manifest.manifest.iter_entries() {
                    self.inner
                        .add_file_resources(path, entry.clone())
                        .with_context(|| format!("adding {}", path.display()))?;
                }

                Ok(())
            },
        )?;

        Ok(Value::new(NoneType::None))
    }

    pub fn set_info_plist_key(&mut self, key: String, value: Value) -> ValueResult {
        let value: plist::Value = match value.get_type() {
            "bool" => value.to_bool().into(),
            "int" => value.to_int()?.into(),
            "string" => value.to_string().into(),
            t => {
                return Err(ValueError::from(RuntimeError {
                    code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                    message: format!("function expects a bool, int, or string; got {}", t),
                    label: "set_info_plist_key()".to_string(),
                }))
            }
        };

        error_context("MacOsApplicationBundleBuilder.set_info_plist_key()", || {
            self.inner
                .set_info_plist_key(key, value)
                .context("setting info plist key")
        })?;

        Ok(Value::new(NoneType::None))
    }

    pub fn set_info_plist_required_keys(
        &mut self,
        display_name: String,
        identifier: String,
        version: String,
        signature: String,
        executable: String,
    ) -> ValueResult {
        error_context(
            "MacOsApplicationBundleBuilder.set_info_plist_required_keys()",
            || {
                self.inner
                    .set_info_plist_required_keys(
                        display_name,
                        identifier,
                        version,
                        signature,
                        executable,
                    )
                    .context("setting info plist required keys")
            },
        )?;

        Ok(Value::new(NoneType::None))
    }

    pub fn build(
        &self,
        type_values: &TypeValues,
        call_stack: &mut CallStack,
        target: String,
    ) -> ValueResult {
        const LABEL: &str = "MacOsApplicationBundleBuilder.build()";

        let context_value = get_context_value(type_values)?;
        let context = context_value
            .downcast_ref::<EnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        let output_path = context.target_build_path(&target);

        let (bundle_path, filename) = error_context(LABEL, || {
            let bundle_path = self
                .inner
                .materialize_bundle(&output_path)
                .context("materializing bundle")?;

            let filename = bundle_path
                .file_name()
                .ok_or_else(|| anyhow!("unable to resolve bundle file name"))?
                .to_os_string();

            Ok((bundle_path, filename))
        })?;

        let candidate = bundle_path.as_path().into();
        let mut context = SigningContext::new(
            LABEL,
            SigningAction::MacOsApplicationBunderCreation,
            filename,
            &candidate,
        );
        context.set_path(&bundle_path);
        context.set_signing_destination(SigningDestination::Directory(bundle_path.clone()));

        handle_signable_event(type_values, call_stack, context)?;

        Ok(Value::new(ResolvedTargetValue {
            inner: ResolvedTarget {
                run_mode: RunMode::Path { path: bundle_path },
                output_path,
            },
        }))
    }
}

starlark_module! { macos_application_bundle_builder_module =>
    #[allow(non_snake_case)]
    MacOsApplicationBundleBuilder(bundle_name: String) {
        MacOsApplicationBundleBuilderValue::new_from_args(bundle_name)
    }

    MacOsApplicationBundleBuilder.add_icon(this, path: String) {
        let mut this = this.downcast_mut::<MacOsApplicationBundleBuilderValue>().unwrap().unwrap();
        this.add_icon(path)
    }

    MacOsApplicationBundleBuilder.add_manifest(this, manifest: FileManifestValue) {
        let mut this = this.downcast_mut::<MacOsApplicationBundleBuilderValue>().unwrap().unwrap();
        this.add_manifest(manifest)
    }

    MacOsApplicationBundleBuilder.add_macos_file(
        this,
        content: FileContentValue,
        path = NoneType::None
    )
    {
        let mut this = this.downcast_mut::<MacOsApplicationBundleBuilderValue>().unwrap().unwrap();
        this.add_macos_file(content, path)
    }

    MacOsApplicationBundleBuilder.add_macos_manifest(this, manifest: FileManifestValue) {
        let mut this = this.downcast_mut::<MacOsApplicationBundleBuilderValue>().unwrap().unwrap();
        this.add_macos_manifest(manifest)
    }

    MacOsApplicationBundleBuilder.add_resources_file(
        this,
        content: FileContentValue,
        path = NoneType::None
    ) {
        let mut this = this.downcast_mut::<MacOsApplicationBundleBuilderValue>().unwrap().unwrap();
        this.add_resources_file(content, path)
    }

    MacOsApplicationBundleBuilder.add_resources_manifest(this, manifest: FileManifestValue) {
        let mut this = this.downcast_mut::<MacOsApplicationBundleBuilderValue>().unwrap().unwrap();
        this.add_resources_manifest(manifest)
    }

    MacOsApplicationBundleBuilder.set_info_plist_key(this, key: String, value: Value) {
        let mut this = this.downcast_mut::<MacOsApplicationBundleBuilderValue>().unwrap().unwrap();
        this.set_info_plist_key(key, value)
    }

    MacOsApplicationBundleBuilder.set_info_plist_required_keys(
        this,
        display_name: String,
        identifier: String,
        version: String,
        signature: String,
        executable: String
    ) {
        let mut this = this.downcast_mut::<MacOsApplicationBundleBuilderValue>().unwrap().unwrap();
        this.set_info_plist_required_keys(display_name, identifier, version, signature, executable)
    }

    MacOsApplicationBundleBuilder.build(env env, call_stack cs, this, target: String) {
        let this = this.downcast_ref::<MacOsApplicationBundleBuilderValue>().unwrap();
        this.build(env, cs, target)
    }
}

#[cfg(test)]
mod tests {
    use {super::*, crate::starlark::testutil::*, anyhow::Result};

    #[test]
    fn constructor() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        let builder = env.eval("MacOsApplicationBundleBuilder('myapp')")?;
        assert_eq!(builder.get_type(), MacOsApplicationBundleBuilderValue::TYPE);

        Ok(())
    }

    #[test]
    fn set_info_plist_required_keys() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        env.eval("builder = MacOsApplicationBundleBuilder('myapp')")?;
        env.eval("builder.set_info_plist_required_keys('My App', 'com.example.my_app', '0.1', 'myap', 'myapp')")?;

        let builder_value = env.eval("builder")?;
        let builder = builder_value
            .downcast_ref::<MacOsApplicationBundleBuilderValue>()
            .unwrap();

        assert_eq!(
            builder.inner.get_info_plist_key("CFBundleDisplayName")?,
            Some("My App".into())
        );
        assert_eq!(
            builder.inner.get_info_plist_key("CFBundleIdentifier")?,
            Some("com.example.my_app".into())
        );
        assert_eq!(
            builder.inner.get_info_plist_key("CFBundleVersion")?,
            Some("0.1".into())
        );
        assert_eq!(
            builder.inner.get_info_plist_key("CFBundleSignature")?,
            Some("myap".into())
        );
        assert_eq!(
            builder.inner.get_info_plist_key("CFBundleExecutable")?,
            Some("myapp".into())
        );

        Ok(())
    }

    #[test]
    fn add_macos_file() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        env.eval("builder = MacOsApplicationBundleBuilder('myapp')")?;
        env.eval("builder.add_macos_file(FileContent(filename = 'file', content = 'content'))")?;

        let value = env.eval("builder")?;
        let builder = value
            .downcast_ref::<MacOsApplicationBundleBuilderValue>()
            .unwrap();
        assert!(builder.inner.files().get("Contents/MacOS/file").is_some());

        Ok(())
    }

    #[test]
    fn add_resources_file() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        env.eval("builder = MacOsApplicationBundleBuilder('myapp')")?;
        env.eval(
            "builder.add_resources_file(FileContent(filename = 'file', content = 'content'))",
        )?;

        let value = env.eval("builder")?;
        let builder = value
            .downcast_ref::<MacOsApplicationBundleBuilderValue>()
            .unwrap();
        assert!(builder
            .inner
            .files()
            .get("Contents/Resources/file")
            .is_some());

        Ok(())
    }
}
