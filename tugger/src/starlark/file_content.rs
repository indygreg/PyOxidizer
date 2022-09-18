// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    anyhow::{anyhow, Context},
    simple_file_manifest::{FileData, FileEntry},
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
        get_context_value, optional_bool_arg, optional_str_arg, EnvironmentContext,
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
            code: "TUGGER_FILE_CONTENT",
            message: format!("{:?}", e),
            label: label.to_string(),
        })
    })
}

fn validate_filename(label: &str, v: &str) -> Result<(), ValueError> {
    if v.contains('/') || v.contains('\\') {
        Err(ValueError::Runtime(RuntimeError {
            code: "TUGGER_FILE_CONTENT",
            message: format!("directory separators aren't allowed in file names: {}", v),
            label: label.to_string(),
        }))
    } else {
        Ok(())
    }
}

#[derive(Debug)]
pub struct FileContentWrapper {
    pub content: FileEntry,
    pub filename: String,
}

impl From<FileContentWrapper> for Value {
    fn from(v: FileContentWrapper) -> Self {
        Value::new(FileContentValue {
            inner: Arc::new(Mutex::new(v)),
        })
    }
}

// TODO merge this into `FileValue`?
#[derive(Clone, Debug)]
pub struct FileContentValue {
    inner: Arc<Mutex<FileContentWrapper>>,
}

impl TypedValue for FileContentValue {
    type Holder = Mutable<FileContentValue>;
    const TYPE: &'static str = "FileContent";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }

    fn get_attr(&self, attribute: &str) -> ValueResult {
        let inner = self.inner(&format!("{}.{}", Self::TYPE, attribute))?;

        Ok(match attribute {
            "executable" => Value::from(inner.content.is_executable()),
            "filename" => Value::from(inner.filename.as_str()),
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
        Ok(matches!(attribute, "executable" | "filename"))
    }

    fn set_attr(&mut self, attribute: &str, value: Value) -> Result<(), ValueError> {
        let mut inner = self.inner(&format!("{}.{}", Self::TYPE, attribute))?;

        match attribute {
            "executable" => {
                inner.content.set_executable(value.to_bool());
            }
            "filename" => {
                validate_filename("FileContent.filename = ", &value.to_string())?;
                inner.filename = value.to_string();
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

impl FileContentValue {
    pub fn new_from_args(
        type_values: &TypeValues,
        path: Value,
        filename: Value,
        content: Value,
        executable: Value,
    ) -> ValueResult {
        const LABEL: &str = "FileContent()";

        let path = optional_str_arg("path", &path)?;
        let filename = optional_str_arg("filename", &filename)?;
        let content = optional_str_arg("content", &content)?;
        let executable = optional_bool_arg("executable", &executable)?;

        if let Some(filename) = &filename {
            validate_filename(LABEL, filename)?;
        }

        let cwd = {
            let context_value = get_context_value(type_values)?;
            let context = context_value
                .downcast_ref::<EnvironmentContext>()
                .ok_or(ValueError::IncorrectParameterType)?;

            context.cwd().to_path_buf()
        };

        let file_content = error_context(LABEL, || {
            if path.is_some() && content.is_some() {
                return Err(anyhow!(
                    "at most 1 of `path` and `content` arguments can be specified"
                ));
            }

            if let Some(path) = path {
                let path = PathBuf::from(path);

                let path = if path.is_relative() {
                    cwd.join(path)
                } else {
                    path
                };

                let filename = if let Some(filename) = filename {
                    filename
                } else {
                    path.file_name()
                        .ok_or_else(|| {
                            anyhow!("unable to resolve file name from path {}", path.display())
                        })?
                        .to_string_lossy()
                        .to_string()
                };

                let mut file_entry = FileEntry::try_from(path.as_path())?;

                if let Some(executable) = executable {
                    file_entry.set_executable(executable);
                }

                Ok(FileContentWrapper {
                    content: file_entry,
                    filename,
                })
            } else if let Some(content) = content {
                let filename = filename.ok_or_else(|| {
                    anyhow!("filename argument is required when content is specified")
                })?;

                let data = FileData::from(content.as_bytes());

                let file_entry = FileEntry::new_from_data(data, executable.unwrap_or(false));

                Ok(FileContentWrapper {
                    content: file_entry,
                    filename,
                })
            } else {
                Err(anyhow!(
                    "at least 1 of `path` or `content` arguments must be specified"
                ))
            }
        })?;

        Ok(Value::new(FileContentValue {
            inner: Arc::new(Mutex::new(file_content)),
        }))
    }

    pub fn inner(&self, label: &str) -> Result<MutexGuard<FileContentWrapper>, ValueError> {
        self.inner.try_lock().map_err(|e| {
            ValueError::Runtime(RuntimeError {
                code: "TUGGER_FILE_CONTENT",
                message: format!("error obtaining lock: {}", e),
                label: label.to_string(),
            })
        })
    }

    pub fn write_to_directory(&self, type_values: &TypeValues, path: String) -> ValueResult {
        const LABEL: &str = "FileContent.write_to_directory()";

        let context_value = get_context_value(type_values)?;
        let context = context_value
            .downcast_ref::<EnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        let inner = self.inner(LABEL)?;

        let dest_path = context.resolve_path(path).join(&inner.filename);

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

starlark_module! { file_content_module =>
    #[allow(non_snake_case)]
    FileContent(
        env env,
        path = NoneType::None,
        filename = NoneType::None,
        content = NoneType::None,
        executable = NoneType::None
    ) {
        FileContentValue::new_from_args(env, path, filename, content, executable)
    }

    FileContent.write_to_directory(env env, this, path: String) {
        let this = this.downcast_ref::<FileContentValue>().unwrap();
        this.write_to_directory(env, path)
    }
}

#[cfg(test)]
mod tests {
    use {super::*, crate::starlark::testutil::*, anyhow::Result, tugger_common::testutil::*};

    #[test]
    fn new_no_args() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;
        assert!(env.eval("FileContent()").is_err());

        Ok(())
    }

    #[test]
    fn new_bad_path() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;
        assert!(env.eval("FileContent(path = '/does/not/exist')").is_err());

        Ok(())
    }

    #[test]
    fn new_path() -> Result<()> {
        let temp_path = DEFAULT_TEMP_DIR.path().join("tugger_file_content_new_path");
        let temp_path_normalized = format!("{}", temp_path.display()).replace('\\', "/");

        std::fs::write(&temp_path, b"content")?;

        let mut env = StarlarkEnvironment::new()?;

        env.eval(&format!("FileContent(path = '{}')", temp_path_normalized))?;

        Ok(())
    }

    #[test]
    fn new_content_no_filename() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;
        assert!(env.eval("FileContent(content = 'foo')").is_err());

        Ok(())
    }

    #[test]
    fn new_filename_with_directory() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;
        assert!(env
            .eval("FileContent(filename = 'foo/bar', content = 'foo')")
            .is_err());
        assert!(env
            .eval("FileContent(filename = 'foo\\bar', content = 'foo')")
            .is_err());

        Ok(())
    }

    #[test]
    fn new_content() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        let raw = env.eval("FileContent(filename = 'file', content = 'foo')")?;

        assert_eq!(raw.get_type(), FileContentValue::TYPE);
        let v = raw.downcast_ref::<FileContentValue>().unwrap();

        let inner = v.inner("ignored").unwrap();

        assert_eq!(inner.filename, "file");
        assert!(!inner.content.is_executable());
        assert_eq!(inner.content.resolve_content()?, b"foo".to_vec());

        Ok(())
    }

    #[test]
    fn attributes() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        env.eval("c = FileContent(filename = 'file', content = 'foo')")?;

        assert!(!env.eval("c.executable")?.to_bool());
        assert_eq!(env.eval("c.filename")?.to_string(), "file");

        env.eval("c.executable = True")?;
        env.eval("c.filename = 'renamed'")?;

        assert!(env.eval("c.executable")?.to_bool());
        assert_eq!(env.eval("c.filename")?.to_string(), "renamed");

        Ok(())
    }

    #[test]
    fn write_to_directory() -> Result<()> {
        let mut env = StarlarkEnvironment::new()?;

        let dest_dir = DEFAULT_TEMP_DIR
            .path()
            .join("file-content-write-to-directory");
        let dest_dir_s = dest_dir.to_string_lossy().replace('\\', "/");

        env.eval("c = FileContent(filename = 'file.txt', content = 'foo')")?;

        let dest_path = env.eval(&format!("c.write_to_directory('{}')", dest_dir_s))?;
        assert_eq!(dest_path.get_type(), "string");

        let dest_path = PathBuf::from(dest_path.to_string());
        assert!(dest_path.exists());

        Ok(())
    }
}
