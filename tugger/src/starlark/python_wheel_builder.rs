// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::starlark::file_content::{FileContentValue, FileContentWrapper},
    anyhow::{anyhow, Context},
    log::warn,
    python_packaging::wheel_builder::WheelBuilder,
    starlark::{
        environment::TypeValues,
        values::{
            error::{RuntimeError, UnsupportedOperation, ValueError},
            none::NoneType,
            Mutable, TypedValue, Value, ValueResult,
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
    std::{
        path::PathBuf,
        sync::{Arc, Mutex, MutexGuard},
    },
};

fn error_context<F, T>(label: &str, f: F) -> Result<T, ValueError>
where
    F: FnOnce() -> anyhow::Result<T>,
{
    f().map_err(|e| {
        ValueError::Runtime(RuntimeError {
            code: "PYTHON_WHEEL_BUILDER",
            message: format!("{:?}", e),
            label: label.to_string(),
        })
    })
}

#[derive(Clone)]
pub struct PythonWheelBuilderValue {
    inner: Arc<Mutex<WheelBuilder>>,
}

impl TypedValue for PythonWheelBuilderValue {
    type Holder = Mutable<PythonWheelBuilderValue>;
    const TYPE: &'static str = "PythonWheelBuilder";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }

    fn get_attr(&self, attribute: &str) -> ValueResult {
        let builder = self.inner()?;

        Ok(match attribute {
            "abi_tag" => Value::from(builder.abi_tag()),
            "build_tag" => {
                if let Some(v) = builder.build_tag() {
                    Value::from(v)
                } else {
                    Value::from(NoneType::None)
                }
            }
            "generator" => Value::from(builder.generator()),
            "modified_time" => Value::from(builder.modified_time().unix_timestamp()),
            "platform_tag" => Value::from(builder.platform_tag()),
            "python_tag" => Value::from(builder.python_tag()),
            "root_is_purelib" => Value::from(builder.root_is_purelib()),
            "tag" => Value::from(builder.tag()),
            "wheel_file_name" => Value::from(builder.wheel_file_name()),
            _ => {
                return Err(ValueError::OperationNotSupported {
                    op: UnsupportedOperation::GetAttr(attribute.to_string()),
                    left: Self::TYPE.to_string(),
                    right: None,
                })
            }
        })
    }

    fn has_attr(&self, attribute: &str) -> Result<bool, ValueError> {
        Ok(matches!(
            attribute,
            "abi_tag"
                | "build_tag"
                | "generator"
                | "modified_time"
                | "platform_tag"
                | "python_tag"
                | "root_is_purelib"
                | "tag"
                | "wheel_file_name"
        ))
    }

    fn set_attr(&mut self, attribute: &str, value: Value) -> Result<(), ValueError> {
        let mut builder = self.inner()?;

        match attribute {
            "abi_tag" => {
                builder.set_abi_tag(value.to_string());
            }
            "build_tag" => {
                builder.set_build_tag(value.to_string());
            }
            "generator" => {
                builder.set_generator(value.to_string());
            }
            "modified_time" => {
                builder.set_modified_time(
                    time::OffsetDateTime::from_unix_timestamp(value.to_int()?).map_err(|e| {
                        ValueError::Runtime(RuntimeError {
                            code: "PYTHON_WHEEL_BUILDER",
                            message: format!("unable to parse time: {}", e),
                            label: format!("{}.modified_time", Self::TYPE),
                        })
                    })?,
                );
            }
            "platform_tag" => {
                builder.set_platform_tag(value.to_string());
            }
            "python_tag" => {
                builder.set_python_tag(value.to_string());
            }
            "root_is_purelib" => {
                builder.set_root_is_purelib(value.to_bool());
            }
            "tag" => {
                error_context("PythonWheelBuilder.tag = ", || {
                    builder.set_tag(value.to_string())
                })?;
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

impl PythonWheelBuilderValue {
    fn inner(&self) -> Result<MutexGuard<'_, WheelBuilder>, ValueError> {
        self.inner.try_lock().map_err(|e| {
            ValueError::Runtime(RuntimeError {
                code: "PYTHON_WHEEL_BUILDER",
                message: format!("unable to obtain wheel builder lock: {:?}", e),
                label: "PythonWheelBuilder".to_string(),
            })
        })
    }

    pub fn new_from_args(distribution: String, version: String) -> ValueResult {
        Ok(Value::new(Self {
            inner: Arc::new(Mutex::new(WheelBuilder::new(distribution, version))),
        }))
    }

    pub fn add_file_dist_info(
        &self,
        content: FileContentValue,
        path: Value,
        directory: Value,
    ) -> ValueResult {
        const LABEL: &str = "PythonWheelBuilder.add_file_dist_info()";

        let path = optional_str_arg("path", &path)?;
        let directory = optional_str_arg("directory", &directory)?;

        let mut inner = self.inner()?;
        let content_inner = content.inner(LABEL)?;

        error_context(LABEL, || {
            if path.is_some() && directory.is_some() {
                return Err(anyhow!(
                    "at most 1 of `path` and `directory` must be specified"
                ));
            }

            let path = if let Some(path) = path {
                PathBuf::from(path)
            } else if let Some(directory) = directory {
                PathBuf::from(directory).join(&content_inner.filename)
            } else {
                PathBuf::from(&content_inner.filename)
            };

            inner.add_file_dist_info(path, content_inner.content.clone())
        })?;

        Ok(Value::new(NoneType::None))
    }

    pub fn add_file_data(
        &self,
        destination: String,
        content: FileContentValue,
        path: Value,
        directory: Value,
    ) -> ValueResult {
        const LABEL: &str = "PythonWheelBuilder.add_file_data()";

        let path = optional_str_arg("path", &path)?;
        let directory = optional_str_arg("directory", &directory)?;

        let mut inner = self.inner()?;
        let content_inner = content.inner(LABEL)?;

        error_context(LABEL, || {
            if path.is_some() && directory.is_some() {
                return Err(anyhow!(
                    "at most 1 of `path` and `directory` must be specified"
                ));
            }

            let path = if let Some(path) = path {
                PathBuf::from(path)
            } else if let Some(directory) = directory {
                PathBuf::from(directory).join(&content_inner.filename)
            } else {
                PathBuf::from(&content_inner.filename)
            };

            inner.add_file_data(destination, path, content_inner.content.clone())
        })?;

        Ok(Value::new(NoneType::None))
    }

    pub fn add_file(
        &self,
        content: FileContentValue,
        path: Value,
        directory: Value,
    ) -> ValueResult {
        const LABEL: &str = "PythonWheelBuilder.add_file()";

        let path = optional_str_arg("path", &path)?;
        let directory = optional_str_arg("directory", &directory)?;

        let mut inner = self.inner()?;
        let content_inner = content.inner(LABEL)?;

        error_context(LABEL, || {
            if path.is_some() && directory.is_some() {
                return Err(anyhow!(
                    "at most 1 of `path` and `directory` must be specified"
                ));
            }

            let path = if let Some(path) = path {
                PathBuf::from(path)
            } else if let Some(directory) = directory {
                PathBuf::from(directory).join(&content_inner.filename)
            } else {
                PathBuf::from(&content_inner.filename)
            };

            inner.add_file(path, content_inner.content.clone())
        })?;

        Ok(Value::new(NoneType::None))
    }

    pub fn to_file_content(&self) -> ValueResult {
        const LABEL: &str = "PythonWheelBuilder.to_file_content()";

        let inner = self.inner()?;

        let data = error_context(LABEL, || {
            let mut cursor = std::io::Cursor::new(Vec::<u8>::new());
            inner
                .write_wheel_data(&mut cursor)
                .context("wring wheel data")?;

            Ok(cursor.into_inner())
        })?;

        Ok(FileContentWrapper {
            content: data.into(),
            filename: inner.wheel_file_name(),
        }
        .into())
    }

    pub fn write_to_directory(&self, type_values: &TypeValues, path: String) -> ValueResult {
        const LABEL: &str = "PythonWheelBuilder.write_to_directory()";

        let inner = self.inner()?;

        let context_value = get_context_value(type_values)?;
        let context = context_value
            .downcast_ref::<EnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        let path = context.resolve_path(path);

        let wheel_path = error_context(LABEL, || {
            std::fs::create_dir_all(&path)
                .with_context(|| format!("creating directory {}", path.display()))?;

            inner
                .write_wheel_into_directory(&path)
                .context("writing wheel to directory")
        })?;

        Ok(Value::from(format!("{}", wheel_path.display())))
    }

    fn build(&self, type_values: &TypeValues, target: String) -> ValueResult {
        const LABEL: &str = "PythonWheelBuilder.build()";

        let inner = self.inner()?;

        let context_value = get_context_value(type_values)?;
        let context = context_value
            .downcast_ref::<EnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        let output_directory = context.target_build_path(&target);

        let wheel_path = error_context(LABEL, || {
            warn!("writing wheel to {}", output_directory.display());

            std::fs::create_dir_all(&output_directory)
                .with_context(|| format!("creating directory {}", output_directory.display()))?;

            inner
                .write_wheel_into_directory(&output_directory)
                .context("writing wheel to directory")
        })?;

        warn!("wrote wheel {}", wheel_path.display());

        Ok(Value::new(ResolvedTargetValue {
            inner: ResolvedTarget {
                run_mode: RunMode::None,
                output_path: output_directory,
            },
        }))
    }
}

starlark_module! { python_wheel_builder_module =>
    #[allow(non_snake_case)]
    PythonWheelBuilder(distribution: String, version: String) {
        PythonWheelBuilderValue::new_from_args(distribution, version)
    }

    PythonWheelBuilder.add_file_dist_info(
        this,
        file: FileContentValue,
        path = NoneType::None,
        directory = NoneType::None
    ) {
        let this = this.downcast_ref::<PythonWheelBuilderValue>().unwrap();
        this.add_file_dist_info(file, path, directory)
    }

    PythonWheelBuilder.add_file_data(
        this,
        destination: String,
        file: FileContentValue,
        path = NoneType::None,
        directory = NoneType::None
    ) {
        let this = this.downcast_ref::<PythonWheelBuilderValue>().unwrap();
        this.add_file_data(destination, file, path, directory)
    }

    PythonWheelBuilder.add_file(
        this,
        file: FileContentValue,
        path = NoneType::None,
        directory = NoneType::None
    ) {
        let this = this.downcast_ref::<PythonWheelBuilderValue>().unwrap();
        this.add_file(file, path, directory)
    }

    PythonWheelBuilder.to_file_content(this) {
        let this = this.downcast_ref::<PythonWheelBuilderValue>().unwrap();
        this.to_file_content()
    }

    PythonWheelBuilder.write_to_directory(env env, this, path: String) {
        let this = this.downcast_ref::<PythonWheelBuilderValue>().unwrap();
        this.write_to_directory(env, path)
    }

    PythonWheelBuilder.build(env env, this, target: String) {
        let this = this.downcast_ref::<PythonWheelBuilderValue>().unwrap();
        this.build(env, target)
    }

}

#[cfg(test)]
mod tests {
    use {super::*, crate::starlark::testutil::*, anyhow::Result, tugger_common::testutil::*};

    #[test]
    fn type_info() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        let raw = env.eval("PythonWheelBuilder('package', '0.1')")?;
        assert_eq!(raw.get_type(), PythonWheelBuilderValue::TYPE);

        Ok(())
    }

    #[test]
    fn attributes() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        env.eval("w = PythonWheelBuilder('package', '0.1')")?;

        let v = env.eval("w.build_tag")?;
        assert_eq!(v.get_type(), "NoneType");

        let v = env.eval("w.tag")?;
        assert_eq!(v.to_string(), "py3-none-any");

        let v = env.eval("w.python_tag")?;
        assert_eq!(v.to_string(), "py3");

        let v = env.eval("w.abi_tag")?;
        assert_eq!(v.to_string(), "none");

        let v = env.eval("w.platform_tag")?;
        assert_eq!(v.to_string(), "any");

        let v = env.eval("w.root_is_purelib")?;
        assert_eq!(v.get_type(), "bool");

        let v = env.eval("w.modified_time")?;
        assert_eq!(v.get_type(), "int");

        let v = env.eval("w.wheel_file_name")?;
        assert_eq!(v.to_string(), "package-0.1-py3-none-any.whl");

        Ok(())
    }

    #[test]
    fn to_file_content() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        env.eval("w = PythonWheelBuilder('package', '0.1')")?;
        let f = env.eval("w.to_file_content()")?;

        assert_eq!(f.get_type(), "FileContent");
        let value = f.downcast_ref::<FileContentValue>().unwrap();
        let inner = value.inner("ignored").unwrap();
        assert_eq!(inner.filename, "package-0.1-py3-none-any.whl");

        Ok(())
    }

    #[test]
    fn write_to_directory() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        let temp_dir_s = format!("{}", DEFAULT_TEMP_DIR.path().display()).replace('\\', "/");

        env.eval("w = PythonWheelBuilder('package', '0.1')")?;
        let path = env.eval(&format!("w.write_to_directory('{}')", temp_dir_s))?;

        assert_eq!(path.get_type(), "string");
        let path = PathBuf::from(path.to_string());
        assert!(path.exists());

        Ok(())
    }
}
