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
};

/// Starlark value wrapper for `PythonModuleSource`.
#[derive(Debug, Clone)]
pub struct PythonModuleSourceValue {
    pub inner: PythonModuleSource,
    pub add_context: Option<PythonResourceAddCollectionContext>,
}

impl PythonModuleSourceValue {
    pub fn new(module: PythonModuleSource) -> Self {
        Self {
            inner: module,
            add_context: None,
        }
    }
}

impl ResourceCollectionContext for PythonModuleSourceValue {
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

impl TypedValue for PythonModuleSourceValue {
    type Holder = Mutable<PythonModuleSourceValue>;
    const TYPE: &'static str = "PythonModuleSource";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }

    fn to_str(&self) -> String {
        format!("{}<name={}>", Self::TYPE, self.inner.name)
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_attr(&self, attribute: &str) -> ValueResult {
        let v = match attribute {
            "is_stdlib" => Value::from(self.inner.is_stdlib),
            "name" => Value::new(self.inner.name.clone()),
            "source" => {
                let source = self.inner.source.resolve().map_err(|e| {
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
            "is_package" => Value::new(self.inner.is_package),
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
        assert_eq!(m.get_attr("is_package").unwrap().to_bool(), false);

        assert!(m.has_attr("add_include").unwrap());
        assert_eq!(m.get_attr("add_include").unwrap().get_type(), "bool");
        assert_eq!(m.get_attr("add_include").unwrap().to_bool(), true);
        m.set_attr("add_include", Value::new(false)).unwrap();
        assert_eq!(m.get_attr("add_include").unwrap().to_bool(), false);

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
        assert_eq!(m.get_attr("add_source").unwrap().to_bool(), true);
        m.set_attr("add_source", Value::new(false)).unwrap();
        assert_eq!(m.get_attr("add_source").unwrap().to_bool(), false);

        assert!(m.has_attr("add_bytecode_optimization_level_zero").unwrap());
        assert_eq!(
            m.get_attr("add_bytecode_optimization_level_zero")
                .unwrap()
                .get_type(),
            "bool"
        );
        assert_eq!(
            m.get_attr("add_bytecode_optimization_level_zero")
                .unwrap()
                .to_bool(),
            true
        );
        m.set_attr("add_bytecode_optimization_level_zero", Value::new(false))
            .unwrap();
        assert_eq!(
            m.get_attr("add_bytecode_optimization_level_zero")
                .unwrap()
                .to_bool(),
            false
        );

        assert!(m.has_attr("add_bytecode_optimization_level_one").unwrap());
        assert_eq!(
            m.get_attr("add_bytecode_optimization_level_one")
                .unwrap()
                .get_type(),
            "bool"
        );
        assert_eq!(
            m.get_attr("add_bytecode_optimization_level_one")
                .unwrap()
                .to_bool(),
            false
        );
        m.set_attr("add_bytecode_optimization_level_one", Value::new(true))
            .unwrap();
        assert_eq!(
            m.get_attr("add_bytecode_optimization_level_one")
                .unwrap()
                .to_bool(),
            true
        );

        assert!(m.has_attr("add_bytecode_optimization_level_two").unwrap());
        assert_eq!(
            m.get_attr("add_bytecode_optimization_level_two")
                .unwrap()
                .get_type(),
            "bool"
        );
        assert_eq!(
            m.get_attr("add_bytecode_optimization_level_two")
                .unwrap()
                .to_bool(),
            false
        );
        m.set_attr("add_bytecode_optimization_level_two", Value::new(true))
            .unwrap();
        assert_eq!(
            m.get_attr("add_bytecode_optimization_level_two")
                .unwrap()
                .to_bool(),
            true
        );

        Ok(())
    }
}
