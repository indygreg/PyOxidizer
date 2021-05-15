// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    super::python_resource::ResourceCollectionContext,
    python_packaging::{
        resource::PythonResource, resource_collection::PythonResourceAddCollectionContext,
    },
    starlark::values::{
        error::{UnsupportedOperation, ValueError},
        {Mutable, TypedValue, Value, ValueResult},
    },
    tugger_file_manifest::File,
};

/// Starlark value wrapper for `File`.
#[derive(Clone, Debug)]
pub struct FileValue {
    pub inner: File,
    pub add_context: Option<PythonResourceAddCollectionContext>,
}

impl FileValue {
    pub fn new(file: File) -> Self {
        Self {
            inner: file,
            add_context: None,
        }
    }
}

impl ResourceCollectionContext for FileValue {
    fn add_collection_context(&self) -> &Option<PythonResourceAddCollectionContext> {
        &self.add_context
    }

    fn add_collection_context_mut(&mut self) -> &mut Option<PythonResourceAddCollectionContext> {
        &mut self.add_context
    }

    fn as_python_resource(&self) -> PythonResource<'_> {
        PythonResource::from(&self.inner)
    }
}

impl TypedValue for FileValue {
    type Holder = Mutable<FileValue>;
    const TYPE: &'static str = "File";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }

    fn to_str(&self) -> String {
        format!(
            "{}<path={}, is_executable={}>",
            Self::TYPE,
            self.inner.path_string(),
            self.inner.entry.executable
        )
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_attr(&self, attribute: &str) -> ValueResult {
        let v = match attribute {
            "path" => Value::from(self.inner.path_string()),
            "is_executable" => Value::from(self.inner.entry.executable),
            attr => {
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
