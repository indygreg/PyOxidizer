// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    super::python_resource::ResourceCollectionContext,
    python_packaging::{
        resource::{PythonPackageDistributionResource, PythonResource},
        resource_collection::PythonResourceAddCollectionContext,
    },
    starlark::values::{
        error::{RuntimeError, UnsupportedOperation, ValueError},
        {Mutable, TypedValue, Value, ValueResult},
    },
    std::sync::{Arc, Mutex, MutexGuard},
};

#[derive(Debug)]
pub struct PythonPackageDistributionResourceWrapper {
    pub r: PythonPackageDistributionResource,
    pub add_context: Option<PythonResourceAddCollectionContext>,
}

/// Starlark `Value` wrapper for `PythonPackageDistributionResource`.
#[derive(Debug, Clone)]
pub struct PythonPackageDistributionResourceValue {
    inner: Arc<Mutex<PythonPackageDistributionResourceWrapper>>,
    package: String,
    name: String,
}

impl PythonPackageDistributionResourceValue {
    pub fn new(resource: PythonPackageDistributionResource) -> Self {
        let package = resource.package.clone();
        let name = resource.name.clone();

        Self {
            inner: Arc::new(Mutex::new(PythonPackageDistributionResourceWrapper {
                r: resource,
                add_context: None,
            })),
            package,
            name,
        }
    }

    pub fn inner(
        &self,
        label: &str,
    ) -> Result<MutexGuard<PythonPackageDistributionResourceWrapper>, ValueError> {
        self.inner.try_lock().map_err(|e| {
            ValueError::Runtime(RuntimeError {
                code: "PYTHON_PACKAGE_DISTRIBUTION_RESOURCE",
                message: format!("failed to acquire lock: {}", e),
                label: label.to_string(),
            })
        })
    }
}

impl ResourceCollectionContext for PythonPackageDistributionResourceValue {
    fn add_collection_context(
        &self,
    ) -> Result<Option<PythonResourceAddCollectionContext>, ValueError> {
        Ok(self
            .inner("PythonPackageDistributionResource.add_collection_context()")?
            .add_context
            .clone())
    }

    fn replace_add_collection_context(
        &mut self,
        context: PythonResourceAddCollectionContext,
    ) -> Result<Option<PythonResourceAddCollectionContext>, ValueError> {
        Ok(self
            .inner("PythonPackageDistributionResource.replace_add_collection_context()")?
            .add_context
            .replace(context))
    }

    fn as_python_resource(&self) -> Result<PythonResource<'_>, ValueError> {
        Ok(PythonResource::from(
            self.inner("PythonPackageDistributionResource.as_python_resource()")?
                .r
                .clone(),
        ))
    }
}

impl TypedValue for PythonPackageDistributionResourceValue {
    type Holder = Mutable<PythonPackageDistributionResourceValue>;
    const TYPE: &'static str = "PythonPackageDistributionResource";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }

    fn to_str(&self) -> String {
        format!(
            "{}<package={}, name={}>",
            Self::TYPE,
            self.package,
            self.name
        )
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn to_bool(&self) -> bool {
        true
    }

    fn get_attr(&self, attribute: &str) -> ValueResult {
        let inner = self.inner(&format!("PythonPackageDistributionResource.{}", attribute))?;

        let v = match attribute {
            "is_stdlib" => Value::from(false),
            "package" => Value::new(inner.r.package.clone()),
            "name" => Value::new(inner.r.name.clone()),
            // TODO expose raw data
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
            "is_stdlib" => true,
            "package" => true,
            "name" => true,
            // TODO expose raw data
            attr => self.add_collection_context_attrs().contains(&attr),
        })
    }

    fn set_attr(&mut self, attribute: &str, value: Value) -> Result<(), ValueError> {
        if self.add_collection_context_attrs().contains(&attribute) {
            self.set_attr_add_collection_context(attribute, value)
        } else {
            Err(ValueError::OperationNotSupported {
                op: UnsupportedOperation::SetAttr(attribute.to_string()),
                left: Self::TYPE.to_owned(),
                right: None,
            })
        }
    }
}
