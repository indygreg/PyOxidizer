// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::starlark::file_content::{FileContentValue, FileContentWrapper},
    anyhow::{anyhow, Context},
    simple_file_manifest::FileEntry,
    starlark::{
        environment::TypeValues,
        values::{
            error::{RuntimeError, ValueError},
            none::NoneType,
            Mutable, TypedValue, Value, ValueResult,
        },
        {
            starlark_fun, starlark_module, starlark_parse_param_type, starlark_signature,
            starlark_signature_extraction, starlark_signatures,
        },
    },
    starlark_dialect_build_targets::{get_context_value, EnvironmentContext},
    std::{
        path::PathBuf,
        sync::{Arc, Mutex},
    },
    tugger_apple::UniversalBinaryBuilder,
};

fn error_context<F, T>(label: &str, f: F) -> Result<T, ValueError>
where
    F: FnOnce() -> anyhow::Result<T>,
{
    f().map_err(|e| {
        ValueError::Runtime(RuntimeError {
            code: "TUGGER_APPLE_UNIVERSAL_BINARY",
            message: format!("{:?}", e),
            label: label.to_string(),
        })
    })
}

pub struct AppleUniversalBinaryValue {
    pub filename: String,
    pub builder: Arc<Mutex<UniversalBinaryBuilder>>,
}

impl TypedValue for AppleUniversalBinaryValue {
    type Holder = Mutable<AppleUniversalBinaryValue>;
    const TYPE: &'static str = "AppleUniversalBinary";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }
}

impl AppleUniversalBinaryValue {
    pub fn new_from_args(filename: String) -> ValueResult {
        Ok(Value::new(Self {
            filename,
            builder: Arc::new(Mutex::new(UniversalBinaryBuilder::default())),
        }))
    }

    pub fn add_path(&mut self, type_values: &TypeValues, path: String) -> ValueResult {
        const LABEL: &str = "AppleUniversalBinary.add_path()";

        let cwd = {
            let context_value = get_context_value(type_values)?;
            let context = context_value
                .downcast_ref::<EnvironmentContext>()
                .ok_or(ValueError::IncorrectParameterType)?;

            context.cwd().to_path_buf()
        };

        error_context(LABEL, || {
            let path = PathBuf::from(path);

            let path = if path.is_absolute() {
                path
            } else {
                cwd.join(path)
            };

            self.builder
                .try_lock()
                .map_err(|e| anyhow!("could not acquire lock: {}", e))?
                .add_binary(
                    std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?,
                )
                .with_context(|| format!("adding binary from {}", path.display()))?;

            Ok(())
        })?;

        Ok(Value::new(NoneType::None))
    }

    pub fn add_file(&mut self, content: FileContentValue) -> ValueResult {
        const LABEL: &str = "AppleUniversalBinary.add_file()";

        let inner = content.inner(LABEL)?;

        error_context(LABEL, || {
            self.builder
                .try_lock()
                .map_err(|e| anyhow!("could not acquire lock: {}", e))?
                .add_binary(
                    inner
                        .content
                        .resolve_content()
                        .context("resolving FileContent data")?,
                )
                .context("adding binary from FileContent")?;

            Ok(())
        })?;

        Ok(Value::new(NoneType::None))
    }

    pub fn to_file_content(&self) -> ValueResult {
        const LABEL: &str = "AppleUniversalBinary.to_file_content()";

        let v = error_context(LABEL, || {
            let mut data = Vec::<u8>::new();

            self.builder
                .try_lock()
                .map_err(|e| anyhow!("could not acquire lock: {}", e))?
                .write(&mut data)
                .context("writing universal binary")?;

            Ok(FileEntry::new_from_data(data, true))
        })?;

        Ok(FileContentWrapper {
            content: v,
            filename: self.filename.clone(),
        }
        .into())
    }

    pub fn write_to_directory(&self, type_values: &TypeValues, path: String) -> ValueResult {
        const LABEL: &str = "AppleUniversalBinary.write_to_directory()";

        let value = self.to_file_content()?;
        let file_content = value
            .downcast_ref::<FileContentValue>()
            .expect("expected FileContentValue");

        let context_value = get_context_value(type_values)?;
        let context = context_value
            .downcast_ref::<EnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        let dest_path = context.resolve_path(path).join(&self.filename);
        let inner = file_content.inner(LABEL)?;

        error_context(LABEL, || {
            inner
                .content
                .write_to_path(&dest_path)
                .with_context(|| format!("writing {}", dest_path.display()))?;

            Ok(())
        })?;

        Ok(Value::from(format!("{}", dest_path.display())))
    }
}

starlark_module! { apple_universal_binary_module =>
    #[allow(non_snake_case)]
    AppleUniversalBinary(filename: String) {
        AppleUniversalBinaryValue::new_from_args(filename)
    }

    AppleUniversalBinary.add_path(env env, this, path: String) {
        let mut this = this.downcast_mut::<AppleUniversalBinaryValue>().unwrap().unwrap();
        this.add_path(env, path)
    }

    AppleUniversalBinary.add_file(this, content: FileContentValue) {
        let mut this = this.downcast_mut::<AppleUniversalBinaryValue>().unwrap().unwrap();
        this.add_file(content)
    }

    AppleUniversalBinary.to_file_content(this) {
        let this = this.downcast_ref::<AppleUniversalBinaryValue>().unwrap();
        this.to_file_content()
    }

    AppleUniversalBinary.write_to_directory(env env, this, path: String) {
        let this = this.downcast_ref::<AppleUniversalBinaryValue>().unwrap();
        this.write_to_directory(env, path)
    }
}

#[cfg(test)]
mod tests {
    use {super::*, crate::starlark::testutil::*, anyhow::Result, tugger_common::testutil::*};

    #[test]
    fn new() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        assert!(env.eval("AppleUniversalBinary()").is_err());

        let raw = env.eval("AppleUniversalBinary('binary')")?;
        assert_eq!(raw.get_type(), AppleUniversalBinaryValue::TYPE);

        let v = raw.downcast_ref::<AppleUniversalBinaryValue>().unwrap();
        assert_eq!(v.filename, "binary");

        Ok(())
    }

    #[test]
    fn basic_operations() -> Result<()> {
        let macho_path = PathBuf::from("/Applications/Safari.app/Contents/MacOS/Safari");

        if !macho_path.exists() {
            eprintln!("skipping test because {} not found", macho_path.display());
            return Ok(());
        }

        let mut env = StarlarkEnvironment::new()?;

        env.eval("b = AppleUniversalBinary('Safari')")?;
        env.eval(&format!("b.add_path('{}')", macho_path.display()))?;
        env.eval(&format!(
            "b.add_file(FileContent(path = '{}'))",
            macho_path.display()
        ))?;

        let value = env.eval("b.to_file_content()")?;
        assert_eq!(value.get_type(), FileContentValue::TYPE);

        let dest_dir = DEFAULT_TEMP_DIR
            .path()
            .join("apple-universal-binary")
            .to_string_lossy()
            .replace('\\', "/");

        let path = env.eval(&format!("b.write_to_directory('{}')", dest_dir))?;
        assert_eq!(path.get_type(), "string");

        Ok(())
    }
}
