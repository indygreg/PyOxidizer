// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    super::python_resource::ResourceCollectionContext,
    python_packaging::{
        resource::{PythonModuleSource, PythonResource},
        resource_collection::PythonResourceAddCollectionContext,
    },
    starlark::values::{
        error::{RuntimeError, UnsupportedOperation, ValueError},
        {Mutable, TypedValue, Value, ValueResult},
    },
    std::sync::{Arc, Mutex, MutexGuard},
};

#[derive(Debug)]
pub struct PythonModuleSourceWrapper {
    pub m: PythonModuleSource,
    pub add_context: Option<PythonResourceAddCollectionContext>,
}

/// Starlark value wrapper for `PythonModuleSource`.
#[derive(Debug, Clone)]
pub struct PythonModuleSourceValue {
    inner: Arc<Mutex<PythonModuleSourceWrapper>>,
    name: String,
}

impl PythonModuleSourceValue {
    pub fn new(module: PythonModuleSource) -> Self {
        let name = module.name.clone();

        Self {
            inner: Arc::new(Mutex::new(PythonModuleSourceWrapper {
                m: module,
                add_context: None,
            })),
            name,
        }
    }

    pub fn inner(&self, label: &str) -> Result<MutexGuard<PythonModuleSourceWrapper>, ValueError> {
        self.inner.try_lock().map_err(|e| {
            ValueError::Runtime(RuntimeError {
                code: "PYTHON_MODULE_SOURCE",
                message: format!("failed to acquire lock: {}", e),
                label: label.to_string(),
            })
        })
    }
}

impl ResourceCollectionContext for PythonModuleSourceValue {
    fn add_collection_context(
        &self,
    ) -> Result<Option<PythonResourceAddCollectionContext>, ValueError> {
        Ok(self
            .inner("PythonModuleSource.add_collection_context()")?
            .add_context
            .clone())
    }

    fn replace_add_collection_context(
        &mut self,
        context: PythonResourceAddCollectionContext,
    ) -> Result<Option<PythonResourceAddCollectionContext>, ValueError> {
        Ok(self
            .inner("PythonModuleSource.replace_add_collection_context()")?
            .add_context
            .replace(context))
    }

    fn as_python_resource(&self) -> Result<PythonResource<'_>, ValueError> {
        Ok(PythonResource::from(
            self.inner("PythonModuleSource.as_python_resource()")?
                .m
                .clone(),
        ))
    }
}

impl TypedValue for PythonModuleSourceValue {
    type Holder = Mutable<PythonModuleSourceValue>;
    const TYPE: &'static str = "PythonModuleSource";

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
        let inner = self.inner(&format!("PythonModuleSource.{}", attribute))?;

        let v = match attribute {
            "is_stdlib" => Value::from(inner.m.is_stdlib),
            "name" => Value::new(inner.m.name.clone()),
            "source" => {
                let source = inner.m.source.resolve_content().map_err(|e| {
                    ValueError::from(RuntimeError {
                        code: "PYOXIDIZER_SOURCE_ERROR",
                        message: format!("error resolving source code: {}", e),
                        label: "source".to_string(),
                    })
                })?;

                let source = String::from_utf8(source).map_err(|_| {
                    ValueError::from(RuntimeError {
                        code: "PYOXIDIZER_SOURCE_ERROR",
                        message: "error converting source code to UTF-8".to_string(),
                        label: "source".to_string(),
                    })
                })?;

                Value::new(source)
            }
            "is_package" => Value::new(inner.m.is_package),
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
            "name" => true,
            "source" => true,
            "is_package" => true,
            "is_stdlib" => true,
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

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::starlark::{python_distribution::PythonDistributionValue, testutil::*},
        anyhow::Result,
        starlark::values::none::NoneType,
    };

    #[test]
    fn test_source_module_attrs() -> Result<()> {
        let mut env = test_evaluation_context_builder()?.into_context()?;
        add_exe(&mut env)?;

        let dist_value = env.eval("dist")?;
        let dist_ref = dist_value
            .downcast_ref::<PythonDistributionValue>()
            .unwrap();
        let dist = dist_ref.distribution.as_ref().unwrap();

        let mut m = env.eval("exe.make_python_module_source('foo', 'import bar')")?;

        assert_eq!(m.get_type(), PythonModuleSourceValue::TYPE);
        assert!(m.has_attr("name").unwrap());
        assert_eq!(m.get_attr("name").unwrap().to_str(), "foo");

        assert!(m.has_attr("source").unwrap());
        assert_eq!(m.get_attr("source").unwrap().to_str(), "import bar");

        assert!(m.has_attr("is_package").unwrap());
        assert!(!m.get_attr("is_package").unwrap().to_bool());

        assert!(m.has_attr("add_include").unwrap());
        assert_eq!(m.get_attr("add_include").unwrap().get_type(), "bool");
        assert!(m.get_attr("add_include").unwrap().to_bool());
        m.set_attr("add_include", Value::new(false)).unwrap();
        assert!(!m.get_attr("add_include").unwrap().to_bool());

        assert!(m.has_attr("add_location").unwrap());
        assert_eq!(m.get_attr("add_location").unwrap().to_str(), "in-memory");

        m.set_attr("add_location", Value::from("in-memory"))
            .unwrap();
        assert_eq!(m.get_attr("add_location").unwrap().to_str(), "in-memory");

        m.set_attr("add_location", Value::from("filesystem-relative:lib"))
            .unwrap();
        assert_eq!(
            m.get_attr("add_location").unwrap().to_str(),
            "filesystem-relative:lib"
        );

        assert!(m.has_attr("add_location_fallback").unwrap());

        assert_eq!(
            m.get_attr("add_location_fallback").unwrap().get_type(),
            if dist.supports_in_memory_shared_library_loading() {
                "string"
            } else {
                "NoneType"
            }
        );

        m.set_attr("add_location_fallback", Value::from("in-memory"))
            .unwrap();
        assert_eq!(
            m.get_attr("add_location_fallback").unwrap().to_str(),
            "in-memory"
        );

        m.set_attr(
            "add_location_fallback",
            Value::from("filesystem-relative:lib"),
        )
        .unwrap();
        assert_eq!(
            m.get_attr("add_location_fallback").unwrap().to_str(),
            "filesystem-relative:lib"
        );

        m.set_attr("add_location_fallback", Value::from(NoneType::None))
            .unwrap();
        assert_eq!(
            m.get_attr("add_location_fallback").unwrap().get_type(),
            "NoneType"
        );

        assert!(m.has_attr("add_source").unwrap());
        assert_eq!(m.get_attr("add_source").unwrap().get_type(), "bool");
        assert!(m.get_attr("add_source").unwrap().to_bool());
        m.set_attr("add_source", Value::new(false)).unwrap();
        assert!(!m.get_attr("add_source").unwrap().to_bool());

        assert!(m.has_attr("add_bytecode_optimization_level_zero").unwrap());
        assert_eq!(
            m.get_attr("add_bytecode_optimization_level_zero")
                .unwrap()
                .get_type(),
            "bool"
        );
        assert!(m
            .get_attr("add_bytecode_optimization_level_zero")
            .unwrap()
            .to_bool(),);
        m.set_attr("add_bytecode_optimization_level_zero", Value::new(false))
            .unwrap();
        assert!(!m
            .get_attr("add_bytecode_optimization_level_zero")
            .unwrap()
            .to_bool());

        assert!(m.has_attr("add_bytecode_optimization_level_one").unwrap());
        assert_eq!(
            m.get_attr("add_bytecode_optimization_level_one")
                .unwrap()
                .get_type(),
            "bool"
        );
        assert!(!m
            .get_attr("add_bytecode_optimization_level_one")
            .unwrap()
            .to_bool());
        m.set_attr("add_bytecode_optimization_level_one", Value::new(true))
            .unwrap();
        assert!(m
            .get_attr("add_bytecode_optimization_level_one")
            .unwrap()
            .to_bool(),);

        assert!(m.has_attr("add_bytecode_optimization_level_two").unwrap());
        assert_eq!(
            m.get_attr("add_bytecode_optimization_level_two")
                .unwrap()
                .get_type(),
            "bool"
        );
        assert!(!m
            .get_attr("add_bytecode_optimization_level_two")
            .unwrap()
            .to_bool());
        m.set_attr("add_bytecode_optimization_level_two", Value::new(true))
            .unwrap();
        assert!(m
            .get_attr("add_bytecode_optimization_level_two")
            .unwrap()
            .to_bool());

        Ok(())
    }
}
