// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::starlark::{
        code_signing::{
            handle_file_manifest_signable_events, handle_signable_event, SigningAction,
            SigningContext,
        },
        file_content::FileContentWrapper,
        file_manifest::FileManifestValue,
    },
    anyhow::{anyhow, Context, Result},
    simple_file_manifest::FileEntry,
    starlark::{
        environment::TypeValues,
        eval::call_stack::CallStack,
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
    starlark_dialect_build_targets::{
        get_context_value, EnvironmentContext, ResolvedTarget, ResolvedTargetValue, RunMode,
    },
    std::{
        path::{Path, PathBuf},
        sync::{Arc, Mutex, MutexGuard},
    },
    tugger_code_signing::SigningDestination,
    tugger_windows::VcRedistributablePlatform,
    tugger_wix::WiXSimpleMsiBuilder,
};

fn error_context<F, T>(label: &str, f: F) -> Result<T, ValueError>
where
    F: FnOnce() -> anyhow::Result<T>,
{
    f().map_err(|e| {
        ValueError::Runtime(RuntimeError {
            code: "TUGGER_WIX_MSI_BUILDER",
            message: format!("{:?}", e),
            label: label.to_string(),
        })
    })
}

pub struct WiXMsiBuilderWrapper {
    pub builder: WiXSimpleMsiBuilder,
    /// Explicit filename to use for the built MSI.
    pub msi_filename: Option<String>,
    /// The target architecture we are building for.
    pub arch: String,
}

#[derive(Clone)]
pub struct WiXMsiBuilderValue {
    inner: Arc<Mutex<WiXMsiBuilderWrapper>>,
}

impl TypedValue for WiXMsiBuilderValue {
    type Holder = Mutable<WiXMsiBuilderValue>;
    const TYPE: &'static str = "WiXMSIBuilder";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }

    fn set_attr(&mut self, attribute: &str, value: Value) -> Result<(), ValueError> {
        let mut inner = self.inner(&format!("{}.{}", Self::TYPE, &attribute))?;

        match attribute {
            "arch" => {
                inner.arch = value.to_string();
            }
            "banner_bmp_path" => {
                inner.builder = inner.builder.clone().banner_bmp_path(value.to_string());
            }
            "dialog_bmp_path" => {
                inner.builder = inner.builder.clone().dialog_bmp_path(value.to_string());
            }
            "eula_rtf_path" => {
                inner.builder = inner.builder.clone().eula_rtf_path(value.to_string());
            }
            "help_url" => {
                inner.builder = inner.builder.clone().help_url(value.to_string());
            }
            "license_path" => {
                inner.builder = inner.builder.clone().license_path(value.to_string());
            }
            "msi_filename" => {
                inner.msi_filename = Some(value.to_string());
            }
            "package_description" => {
                inner.builder = inner.builder.clone().package_description(value.to_string());
            }
            "package_keywords" => {
                inner.builder = inner.builder.clone().package_keywords(value.to_string());
            }
            "product_icon_path" => {
                inner.builder = inner.builder.clone().product_icon_path(value.to_string());
            }
            "upgrade_code" => {
                inner.builder = inner.builder.clone().upgrade_code(value.to_string());
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
        arch: String,
    ) -> ValueResult {
        let builder = WiXSimpleMsiBuilder::new(
            &id_prefix,
            &product_name,
            &product_version,
            &product_manufacturer,
        );

        Ok(Value::new(WiXMsiBuilderValue {
            inner: Arc::new(Mutex::new(WiXMsiBuilderWrapper {
                builder,
                msi_filename: None,
                arch,
            })),
        }))
    }

    pub fn inner(&self, label: &str) -> Result<MutexGuard<WiXMsiBuilderWrapper>, ValueError> {
        self.inner.try_lock().map_err(|e| {
            ValueError::Runtime(RuntimeError {
                code: "TUGGER_WIX_MSI_BUILDER",
                message: format!("error obtaining lock: {}", e),
                label: label.to_string(),
            })
        })
    }

    pub fn add_program_files_manifest(
        &mut self,
        type_values: &TypeValues,
        call_stack: &mut CallStack,
        manifest: FileManifestValue,
    ) -> ValueResult {
        const LABEL: &str = "WiXMSIBuilder.add_program_files_manifest()";

        let mut inner = self.inner(LABEL)?;
        let manifest = manifest.inner(LABEL)?;

        error_context(LABEL, || {
            let manifest = handle_file_manifest_signable_events(
                type_values,
                call_stack,
                &manifest,
                LABEL,
                SigningAction::WindowsInstallerFileAdded,
            )?;

            inner
                .builder
                .add_program_files_manifest(&manifest)
                .context("adding program files manifest")
        })?;

        Ok(Value::new(NoneType::None))
    }

    pub fn add_visual_cpp_redistributable(
        &mut self,
        redist_version: String,
        platform: String,
    ) -> ValueResult {
        const LABEL: &str = "WiXMSIBuilder.add_visual_cpp_redistributable()";

        let mut inner = self.inner(LABEL)?;

        error_context(LABEL, || {
            let platform = VcRedistributablePlatform::try_from(platform.as_str())
                .context("obtaining VcRedistributablePlatform from str")?;

            inner
                .builder
                .add_visual_cpp_redistributable(&redist_version, platform)
                .context("adding Visual C++ redistributable")
        })?;

        Ok(Value::new(NoneType::None))
    }

    pub fn materialize(
        &self,
        type_values: &TypeValues,
        call_stack: &mut CallStack,
        label: &'static str,
        build_dir: &Path,
    ) -> Result<PathBuf, ValueError> {
        let msi_filename = self.msi_filename(label)?;
        let inner = self.inner(label)?;

        let msi_path = error_context(label, || {
            let builder = inner
                .builder
                .to_installer_builder(&inner.arch, build_dir)
                .context("converting WiXSimpleMSiBuilder to WiXInstallerBuilder")?;

            let msi_path = build_dir.join(&msi_filename);

            builder
                .build(&msi_path)
                .context("building WiXInstallerBuilder")?;

            Ok(msi_path)
        })?;

        let candidate = msi_path.as_path().into();
        let mut context = SigningContext::new(
            label,
            SigningAction::WindowsInstallerCreation,
            &msi_filename,
            &candidate,
        );
        context.set_path(&msi_path);
        context.set_signing_destination(SigningDestination::File(msi_path.clone()));

        handle_signable_event(type_values, call_stack, context)?;

        Ok(msi_path)
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
                .prefix("wix-msi-builder-")
                .tempdir_in(&build_path)
                .context("creating temp directory")
        })?;

        let installer_path = self.materialize(type_values, call_stack, label, dest_dir.path())?;

        let entry = FileEntry::new_from_path(&installer_path, false);

        let (entry, filename) = error_context(label, || {
            let entry = entry
                .to_memory()
                .context("converting FileEntry to in-memory")?;

            let filename = installer_path
                .file_name()
                .ok_or_else(|| anyhow!("unable to resolve file name of generated installer"))?;

            Ok((entry, filename.to_string_lossy().to_string()))
        })?;

        Ok((entry, filename))
    }

    pub fn build(
        &self,
        type_values: &TypeValues,
        call_stack: &mut CallStack,
        target: String,
    ) -> ValueResult {
        const LABEL: &str = "WiXMSIBuilder.build()";

        let dest_dir = {
            let context_value = get_context_value(type_values)?;
            let context = context_value
                .downcast_ref::<EnvironmentContext>()
                .ok_or(ValueError::IncorrectParameterType)?;

            context.target_build_path(&target)
        };

        let msi_path = self.materialize(type_values, call_stack, LABEL, &dest_dir)?;

        Ok(Value::new(ResolvedTargetValue {
            inner: ResolvedTarget {
                run_mode: RunMode::Path { path: msi_path },
                output_path: dest_dir,
            },
        }))
    }

    pub fn msi_filename(&self, label: &str) -> Result<String, ValueError> {
        let inner = self.inner(label)?;

        Ok(if let Some(filename) = &inner.msi_filename {
            filename.clone()
        } else {
            inner.builder.default_msi_filename()
        })
    }

    pub fn to_file_content(
        &self,
        type_values: &TypeValues,
        call_stack: &mut CallStack,
    ) -> ValueResult {
        const LABEL: &str = "WiXMSIBuilder.to_file_content()";

        let (entry, filename) = self.materialize_temp_dir(type_values, call_stack, LABEL)?;

        Ok(FileContentWrapper {
            content: entry,
            filename,
        }
        .into())
    }

    fn write_to_directory(
        &self,
        type_values: &TypeValues,
        call_stack: &mut CallStack,
        path: String,
    ) -> ValueResult {
        const LABEL: &str = "WiXMSIBuilder.write_to_directory()";

        let dest_dir = {
            let context_value = get_context_value(type_values)?;
            let context = context_value
                .downcast_ref::<EnvironmentContext>()
                .ok_or(ValueError::IncorrectParameterType)?;

            context.resolve_path(path)
        };

        let (entry, filename) = self.materialize_temp_dir(type_values, call_stack, LABEL)?;

        let installer_path = dest_dir.join(&filename);

        error_context(LABEL, || {
            entry
                .write_to_path(&installer_path)
                .with_context(|| format!("writing installer to {}", installer_path.display()))
        })?;

        Ok(Value::from(format!("{}", installer_path.display())))
    }
}

starlark_module! { wix_msi_builder_module =>
    #[allow(non_snake_case)]
    WiXMSIBuilder(
        id_prefix: String,
        product_name: String,
        product_version: String,
        product_manufacturer: String,
        arch: String = "x64".to_string()
    ) {
        WiXMsiBuilderValue::new_from_args(id_prefix, product_name, product_version, product_manufacturer, arch)
    }

    WiXMSIBuilder.add_program_files_manifest(env env, call_stack cs, this, manifest: FileManifestValue) {
        let mut this = this.downcast_mut::<WiXMsiBuilderValue>().unwrap().unwrap();
        this.add_program_files_manifest(env, cs, manifest)
    }

    WiXMSIBuilder.add_visual_cpp_redistributable(
        this,
        redist_version: String,
        platform: String
    ) {
        let mut this = this.downcast_mut::<WiXMsiBuilderValue>().unwrap().unwrap();
        this.add_visual_cpp_redistributable(redist_version, platform)
    }

    WiXMSIBuilder.build(env env, call_stack cs, this, target: String) {
        let this = this.downcast_ref::<WiXMsiBuilderValue>().unwrap();
        this.build(env, cs, target)
    }

    WiXMSIBuilder.to_file_content(env env, call_stack cs, this) {
        let this = this.downcast_ref::<WiXMsiBuilderValue>().unwrap();
        this.to_file_content(env, cs)
    }

    WiXMSIBuilder.write_to_directory(env env, call_stack cs, this, path: String) {
        let this = this.downcast_ref::<WiXMsiBuilderValue>().unwrap();
        this.write_to_directory(env, cs, path)
    }
}

#[cfg(test)]
mod tests {
    use {super::*, crate::starlark::testutil::*};
    #[cfg(windows)]
    use {crate::starlark::file_content::FileContentValue, tugger_common::testutil::*};

    #[test]
    fn test_new() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        let builder_value = env.eval("WiXMSIBuilder('prefix', 'name', '0.1', 'manufacturer')")?;
        assert_eq!(builder_value.get_type(), "WiXMSIBuilder");
        let builder = builder_value.downcast_ref::<WiXMsiBuilderValue>().unwrap();
        assert_eq!(builder.inner.lock().unwrap().arch, "x64");

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

    #[cfg(windows)]
    #[test]
    fn to_file_content() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        env.eval("msi = WiXMSIBuilder('prefix', 'name', '0.1', 'manufacturer')")?;
        let value = env.eval("msi.to_file_content()")?;

        assert_eq!(value.get_type(), FileContentValue::TYPE);

        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn write_to_directory() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        let dest_dir = DEFAULT_TEMP_DIR
            .path()
            .join("wix-installer-write-to-directory");
        let dest_dir_s = dest_dir.to_string_lossy().replace('\\', "/");

        env.eval("msi = WiXMSIBuilder('prefix', 'name', '0.1', 'manufacturer')")?;
        let value = env.eval(&format!("msi.write_to_directory('{}')", dest_dir_s))?;

        assert_eq!(value.get_type(), "string");
        let path = PathBuf::from(value.to_string());
        assert!(path.exists());

        Ok(())
    }
}
