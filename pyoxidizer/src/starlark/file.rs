// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    super::python_resource::ResourceCollectionContext,
    python_packaging::{
        resource::PythonResource, resource_collection::PythonResourceAddCollectionContext,
    },
    simple_file_manifest::File,
    starlark::values::{
        error::{RuntimeError, UnsupportedOperation, ValueError},
        {Mutable, TypedValue, Value, ValueResult},
    },
    std::sync::{Arc, Mutex, MutexGuard},
};

#[derive(Debug)]
pub struct FileWrapper {
    pub file: File,
    pub add_context: Option<PythonResourceAddCollectionContext>,
}

/// Starlark value wrapper for `File`.
#[derive(Clone, Debug)]
pub struct FileValue {
    inner: Arc<Mutex<FileWrapper>>,
    path: String,
}

impl FileValue {
    pub fn new(file: File) -> Self {
        let path = file.path_string();

        Self {
            inner: Arc::new(Mutex::new(FileWrapper {
                file,
                add_context: None,
            })),
            path,
        }
    }

    pub fn inner(&self, label: &str) -> Result<MutexGuard<FileWrapper>, ValueError> {
        self.inner.try_lock().map_err(|e| {
            ValueError::Runtime(RuntimeError {
                code: "FILE",
                message: format!("failed to acquire lock: {}", e),
                label: label.to_string(),
            })
        })
    }
}

impl ResourceCollectionContext for FileValue {
    fn add_collection_context(
        &self,
    ) -> Result<Option<PythonResourceAddCollectionContext>, ValueError> {
        Ok(self
            .inner("File.add_collection_context()")?
            .add_context
            .clone())
    }

    fn replace_add_collection_context(
        &mut self,
        context: PythonResourceAddCollectionContext,
    ) -> Result<Option<PythonResourceAddCollectionContext>, ValueError> {
        Ok(self
            .inner("File.replace_add_collection_context()")?
            .add_context
            .replace(context))
    }

    fn as_python_resource(&self) -> Result<PythonResource<'_>, ValueError> {
        Ok(PythonResource::from(
            self.inner("File.as_python_resource()")?.file.clone(),
        ))
    }
}

impl TypedValue for FileValue {
    type Holder = Mutable<FileValue>;
    const TYPE: &'static str = "File";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }

    fn to_str(&self) -> String {
        format!("{}<path={}>", Self::TYPE, self.path,)
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_attr(&self, attribute: &str) -> ValueResult {
        let inner = self.inner(&format!("File.{}", attribute))?;

        let v = match attribute {
            "path" => Value::from(inner.file.path_string()),
            "is_executable" => Value::from(inner.file.entry().is_executable()),
            attr => {
                drop(inner);

                return if self.add_collection_context_attrs().contains(&attr) {
                    self.get_attr_add_collection_context(attr)
                } else {
                    Err(ValueError::OperationNotSupported {
                        op: UnsupportedOperation::GetAttr(attr.to_string()),
                        left: Self::TYPE.to_string(),
                        right: None,
                    })
                };
            }
        };

        Ok(v)
    }

    fn has_attr(&self, attribute: &str) -> Result<bool, ValueError> {
        Ok(match attribute {
            "path" => true,
            "is_executable" => true,
            attr => self.add_collection_context_attrs().contains(&attr),
        })
    }

    fn set_attr(&mut self, attribute: &str, value: Value) -> Result<(), ValueError> {
        self.set_attr_add_collection_context(attribute, value)
    }
}
