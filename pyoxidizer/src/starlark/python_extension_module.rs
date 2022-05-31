// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    super::python_resource::ResourceCollectionContext,
    python_packaging::{
        resource::{PythonExtensionModule, PythonResource},
        resource_collection::PythonResourceAddCollectionContext,
    },
    starlark::values::{
        error::{RuntimeError, UnsupportedOperation, ValueError},
        {Mutable, TypedValue, Value, ValueResult},
    },
    std::sync::{Arc, Mutex, MutexGuard},
};

#[derive(Debug)]
pub struct PythonExtensionModuleWrapper {
    pub em: PythonExtensionModule,
    pub add_context: Option<PythonResourceAddCollectionContext>,
}

/// Starlark `Value` wrapper for `PythonExtensionModule`.
#[derive(Debug, Clone)]
pub struct PythonExtensionModuleValue {
    inner: Arc<Mutex<PythonExtensionModuleWrapper>>,
    name: String,
}

impl PythonExtensionModuleValue {
    pub fn new(em: PythonExtensionModule) -> Self {
        let name = em.name.clone();

        Self {
            inner: Arc::new(Mutex::new(PythonExtensionModuleWrapper {
                em,
                add_context: None,
            })),
            name,
        }
    }

    pub fn inner(
        &self,
        label: &str,
    ) -> Result<MutexGuard<PythonExtensionModuleWrapper>, ValueError> {
        self.inner.try_lock().map_err(|e| {
            ValueError::Runtime(RuntimeError {
                code: "PYTHON_EXTENSION_MODULE",
                message: format!("failed to acquire lock: {}", e),
                label: label.to_string(),
            })
        })
    }
}

impl ResourceCollectionContext for PythonExtensionModuleValue {
    fn add_collection_context(
        &self,
    ) -> Result<Option<PythonResourceAddCollectionContext>, ValueError> {
        Ok(self
            .inner("PythonExtensionModule.add_collection_context()")?
            .add_context
            .clone())
    }

    fn replace_add_collection_context(
        &mut self,
        context: PythonResourceAddCollectionContext,
    ) -> Result<Option<PythonResourceAddCollectionContext>, ValueError> {
        Ok(self
            .inner("PythonExtensionModule.replace_add_collection_context()")?
            .add_context
            .replace(context))
    }

    fn as_python_resource(&self) -> Result<PythonResource<'_>, ValueError> {
        Ok(PythonResource::from(
            self.inner("PythonExtensionModule.as_python_resource()")?
                .em
                .clone(),
        ))
    }
}

impl TypedValue for PythonExtensionModuleValue {
    type Holder = Mutable<PythonExtensionModuleValue>;
    const TYPE: &'static str = "PythonExtensionModule";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }

    fn to_str(&self) -> String {
        format!("{}<name={}>", Self::TYPE, self.name)
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_attr(&self, attribute: &str) -> ValueResult {
        let inner = self.inner(&format!("PythonExtensionModule.{}", attribute))?;

        let v = match attribute {
            "is_stdlib" => Value::from(inner.em.is_stdlib),
            "name" => Value::new(inner.em.name.clone()),
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
            "name" => true,
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
