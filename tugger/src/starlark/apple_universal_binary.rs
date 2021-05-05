// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::starlark::file_content::FileContentValue,
    anyhow::{anyhow, Context},
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
    tugger_file_manifest::FileEntry,
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

#[derive(Clone)]
pub struct AppleUniversalBinary {
    pub filename: String,
    pub builder: Arc<Mutex<UniversalBinaryBuilder>>,
}

impl TypedValue for AppleUniversalBinary {
    type Holder = Mutable<AppleUniversalBinary>;
    const TYPE: &'static str = "AppleUniversalBinary";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }
}

impl AppleUniversalBinary {
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
                .lock()
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

        error_context(LABEL, || {
            self.builder
                .lock()
                .map_err(|e| anyhow!("could not acquire lock: {}", e))?
                .add_binary(
                    content
                        .content
                        .data
                        .resolve()
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
                .lock()
                .map_err(|e| anyhow!("could not acquire lock: {}", e))?
                .write(&mut data)
                .context("writing universal binary")?;

            Ok(FileEntry {
                data: data.into(),
                executable: true,
            })
        })?;

        Ok(Value::new(FileContentValue {
            content: v,
            filename: self.filename.clone(),
        }))
    }
}

starlark_module! { apple_universal_binary_module =>
    #[allow(non_snake_case)]
    AppleUniversalBinary(filename: String) {
        AppleUniversalBinary::new_from_args(filename)
    }

    AppleUniversalBinary.add_path(env env, this, path: String) {
        let mut this = this.downcast_mut::<AppleUniversalBinary>().unwrap().unwrap();
        this.add_path(env, path)
    }

    AppleUniversalBinary.add_file(this, content: FileContentValue) {
        let mut this = this.downcast_mut::<AppleUniversalBinary>().unwrap().unwrap();
        this.add_file(content)
    }

    AppleUniversalBinary.to_file_content(this) {
        let this = this.downcast_ref::<AppleUniversalBinary>().unwrap();
        this.to_file_content()
    }
}

#[cfg(test)]
mod tests {
    use {super::*, crate::starlark::testutil::*, anyhow::Result};

    #[test]
    fn new() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        assert!(env.eval("AppleUniversalBinary()").is_err());

        let raw = env.eval("AppleUniversalBinary('binary')")?;
        assert_eq!(raw.get_type(), AppleUniversalBinary::TYPE);

        let v = raw.downcast_ref::<AppleUniversalBinary>().unwrap();
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

        Ok(())
    }
}
