// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    super::python_resource::ResourceCollectionContext,
    python_packaging::{
        resource::{PythonPackageResource, PythonResource},
        resource_collection::PythonResourceAddCollectionContext,
    },
    starlark::values::{
        error::{UnsupportedOperation, ValueError},
        {Mutable, TypedValue, Value, ValueResult},
    },
};

/// Starlark `Value` wrapper for `PythonPackageResource`.
#[derive(Debug, Clone)]
pub struct PythonPackageResourceValue {
    pub inner: PythonPackageResource,
    pub add_context: Option<PythonResourceAddCollectionContext>,
}

impl PythonPackageResourceValue {
    pub fn new(resource: PythonPackageResource) -> Self {
        Self {
            inner: resource,
            add_context: None,
        }
    }
}

impl ResourceCollectionContext for PythonPackageResourceValue {
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

impl TypedValue for PythonPackageResourceValue {
    type Holder = Mutable<PythonPackageResourceValue>;
    const TYPE: &'static str = "PythonPackageResource";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }

    fn to_str(&self) -> String {
        format!(
            "{}<package={}, name={}>",
            Self::TYPE,
            self.inner.leaf_package,
            self.inner.relative_name
        )
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_attr(&self, attribute: &str) -> ValueResult {
        let v = match attribute {
            "is_stdlib" => Value::from(self.inner.is_stdlib),
            "package" => Value::new(self.inner.leaf_package.clone()),
            "name" => Value::new(self.inner.relative_name.clone()),
            // TODO expose raw data
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
            "is_stdlib" => true,
            "package" => true,
            "name" => true,
            // TODO expose raw data
            attr => self.add_collection_context_attrs().contains(&attr),
        })
    }

    fn set_attr(&mut self, attribute: &str, value: Value) -> Result<(), ValueError> {
        self.set_attr_add_collection_context(attribute, value)
    }
}
