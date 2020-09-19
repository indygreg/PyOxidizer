// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    python_packaging::resource::{
        BytecodeOptimizationLevel, PythonExtensionModule as RawPythonExtensionModule,
        PythonModuleBytecodeFromSource, PythonModuleSource as RawSourceModule,
        PythonPackageDistributionResource as RawDistributionResource,
        PythonPackageResource as RawPackageResource, PythonResource,
    },
    python_packaging::resource_collection::ConcreteResourceLocation,
    starlark::values::error::{
        RuntimeError, UnsupportedOperation, ValueError, INCORRECT_PARAMETER_TYPE_ERROR_CODE,
    },
    starlark::values::none::NoneType,
    starlark::values::{Immutable, Mutable, TypedValue, Value, ValueResult},
    std::convert::{TryFrom, TryInto},
};

#[derive(Clone, Debug)]
pub struct OptionalResourceLocation {
    inner: Option<ConcreteResourceLocation>,
}

impl From<&OptionalResourceLocation> for Value {
    fn from(location: &OptionalResourceLocation) -> Self {
        match &location.inner {
            Some(ConcreteResourceLocation::InMemory) => Value::from("in-memory"),
            Some(ConcreteResourceLocation::RelativePath(prefix)) => {
                Value::from(format!("filesystem-relative:{}", prefix))
            }
            None => Value::from(NoneType::None),
        }
    }
}

impl TryFrom<&str> for OptionalResourceLocation {
    type Error = ValueError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        if s == "default" {
            Ok(OptionalResourceLocation { inner: None })
        } else if s == "in-memory" {
            Ok(OptionalResourceLocation {
                inner: Some(ConcreteResourceLocation::InMemory),
            })
        } else if s.starts_with("filesystem-relative:") {
            let prefix = s.split_at("filesystem-relative:".len()).1;
            Ok(OptionalResourceLocation {
                inner: Some(ConcreteResourceLocation::RelativePath(prefix.to_string())),
            })
        } else {
            Err(ValueError::from(RuntimeError {
                code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                message: format!("unable to convert value {} to a resource location", s),
                label: format!(
                    "expected `default`, `in-memory`, or `filesystem-relative:*`; got {}",
                    s
                ),
            }))
        }
    }
}

impl TryFrom<&Value> for OptionalResourceLocation {
    type Error = ValueError;

    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        match value.get_type() {
            "NoneType" => Ok(OptionalResourceLocation { inner: None }),
            "string" => {
                let s = value.to_str();
                Ok(OptionalResourceLocation::try_from(s.as_str())?)
            }
            t => Err(ValueError::from(RuntimeError {
                code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                message: format!("unable to convert value {} to resource location", t),
                label: "resource location conversion".to_string(),
            })),
        }
    }
}

impl Into<Option<ConcreteResourceLocation>> for OptionalResourceLocation {
    fn into(self) -> Option<ConcreteResourceLocation> {
        self.inner
    }
}

/// Starlark value wrapper for `PythonSourceModule`.
#[derive(Debug, Clone)]
pub struct PythonSourceModule {
    pub inner: RawSourceModule,
    pub location: OptionalResourceLocation,
}

impl PythonSourceModule {
    pub fn new(module: RawSourceModule) -> Self {
        Self {
            inner: module,
            location: OptionalResourceLocation { inner: None },
        }
    }
}

impl TypedValue for PythonSourceModule {
    type Holder = Mutable<PythonSourceModule>;
    const TYPE: &'static str = "PythonSourceModule";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }

    fn to_str(&self) -> String {
        format!("PythonSourceModule<name={}>", self.inner.name)
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_attr(&self, attribute: &str) -> ValueResult {
        let v = match attribute {
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
            "location" => Value::from(&self.location),
            attr => {
                return Err(ValueError::OperationNotSupported {
                    op: UnsupportedOperation::GetAttr(attr.to_string()),
                    left: "PythonSourceModule".to_string(),
                    right: None,
                })
            }
        };

        Ok(v)
    }

    fn has_attr(&self, attribute: &str) -> Result<bool, ValueError> {
        Ok(match attribute {
            "name" => true,
            "source" => true,
            "is_package" => true,
            "location" => true,
            _ => false,
        })
    }

    fn set_attr(&mut self, attribute: &str, value: Value) -> Result<(), ValueError> {
        match attribute {
            "location" => {
                self.location = (&value).try_into()?;

                Ok(())
            }
            _ => Err(ValueError::OperationNotSupported {
                op: UnsupportedOperation::SetAttr(attribute.to_string()),
                left: Self::TYPE.to_owned(),
                right: None,
            }),
        }
    }
}

/// Starlark `Value` wrapper for `PythonModuleBytecodeFromSource`.
#[derive(Debug, Clone)]
pub struct PythonBytecodeModule {
    pub inner: PythonModuleBytecodeFromSource,
}

impl TypedValue for PythonBytecodeModule {
    type Holder = Immutable<PythonBytecodeModule>;
    const TYPE: &'static str = "PythonBytecodeModule";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }

    fn to_str(&self) -> String {
        format!(
            "PythonBytecodeModule<name={}; level={:?}>",
            self.inner.name, self.inner.optimize_level
        )
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_attr(&self, attribute: &str) -> ValueResult {
        let v = match attribute {
            "name" => Value::new(self.inner.name.clone()),
            // TODO expose source
            // "source" => Value::new(self.module.source),
            "optimize_level" => Value::new(match self.inner.optimize_level {
                BytecodeOptimizationLevel::Zero => 0,
                BytecodeOptimizationLevel::One => 1,
                BytecodeOptimizationLevel::Two => 2,
            }),
            "is_package" => Value::new(self.inner.is_package),
            attr => {
                return Err(ValueError::OperationNotSupported {
                    op: UnsupportedOperation::GetAttr(attr.to_string()),
                    left: "PythonBytecodeModule".to_string(),
                    right: None,
                })
            }
        };

        Ok(v)
    }

    fn has_attr(&self, attribute: &str) -> Result<bool, ValueError> {
        Ok(match attribute {
            "name" => true,
            // TODO expose source
            // "source" => true,
            "optimize_level" => true,
            "is_package" => true,
            _ => false,
        })
    }
}

/// Starlark `Value` wrapper for `PythonPackageResource`.
#[derive(Debug, Clone)]
pub struct PythonPackageResource {
    pub inner: RawPackageResource,
}

impl TypedValue for PythonPackageResource {
    type Holder = Immutable<PythonPackageResource>;
    const TYPE: &'static str = "PythonPackageResource";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }

    fn to_str(&self) -> String {
        format!(
            "PythonPackageResource<package={}, name={}>",
            self.inner.leaf_package, self.inner.relative_name
        )
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_attr(&self, attribute: &str) -> ValueResult {
        let v = match attribute {
            "package" => Value::new(self.inner.leaf_package.clone()),
            "name" => Value::new(self.inner.relative_name.clone()),
            // TODO expose raw data
            attr => {
                return Err(ValueError::OperationNotSupported {
                    op: UnsupportedOperation::GetAttr(attr.to_string()),
                    left: "PythonPackageResource".to_string(),
                    right: None,
                })
            }
        };

        Ok(v)
    }

    fn has_attr(&self, attribute: &str) -> Result<bool, ValueError> {
        Ok(match attribute {
            "package" => true,
            "name" => true,
            // TODO expose raw data
            _ => false,
        })
    }
}

/// Starlark `Value` wrapper for `PythonPackageDistributionResource`.
#[derive(Debug, Clone)]
pub struct PythonPackageDistributionResource {
    pub inner: RawDistributionResource,
}

impl TypedValue for PythonPackageDistributionResource {
    type Holder = Immutable<PythonPackageDistributionResource>;
    const TYPE: &'static str = "PythonPackageDistributionResource";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }

    fn to_str(&self) -> String {
        format!(
            "PythonPackageDistributionResource<package={}, name={}>",
            self.inner.package, self.inner.name
        )
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn to_bool(&self) -> bool {
        true
    }

    fn get_attr(&self, attribute: &str) -> ValueResult {
        let v = match attribute {
            "package" => Value::new(self.inner.package.clone()),
            "name" => Value::new(self.inner.name.clone()),
            // TODO expose raw data
            attr => {
                return Err(ValueError::OperationNotSupported {
                    op: UnsupportedOperation::GetAttr(attr.to_string()),
                    left: "PythonPackageDistributionResource".to_string(),
                    right: None,
                })
            }
        };

        Ok(v)
    }

    fn has_attr(&self, attribute: &str) -> Result<bool, ValueError> {
        Ok(match attribute {
            "package" => true,
            "name" => true,
            // TODO expose raw data
            _ => false,
        })
    }
}

/// Starlark `Value` wrapper for `PythonExtensionModule`.
#[derive(Debug, Clone)]
pub struct PythonExtensionModule {
    pub inner: RawPythonExtensionModule,
}

impl TypedValue for PythonExtensionModule {
    type Holder = Immutable<PythonExtensionModule>;
    const TYPE: &'static str = "PythonExtensionModule";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }

    fn to_str(&self) -> String {
        format!("PythonExtensionModule<name={}>", self.inner.name)
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_attr(&self, attribute: &str) -> ValueResult {
        let v = match attribute {
            "name" => Value::new(self.inner.name.clone()),
            attr => {
                return Err(ValueError::OperationNotSupported {
                    op: UnsupportedOperation::GetAttr(attr.to_string()),
                    left: "PythonExtensionModule".to_string(),
                    right: None,
                })
            }
        };

        Ok(v)
    }

    fn has_attr(&self, attribute: &str) -> Result<bool, ValueError> {
        Ok(match attribute {
            "name" => true,
            _ => false,
        })
    }
}

pub fn python_resource_to_value(resource: &PythonResource) -> Value {
    match resource {
        PythonResource::ModuleSource(sm) => Value::new(PythonSourceModule::new(sm.clone())),

        PythonResource::ModuleBytecodeRequest(m) => {
            Value::new(PythonBytecodeModule { inner: m.clone() })
        }

        PythonResource::ModuleBytecode { .. } => {
            panic!("not yet implemented");
        }

        PythonResource::Resource(data) => Value::new(PythonPackageResource {
            inner: data.clone(),
        }),

        PythonResource::DistributionResource(resource) => {
            Value::new(PythonPackageDistributionResource {
                inner: resource.clone(),
            })
        }

        PythonResource::ExtensionModuleDynamicLibrary(em) => {
            Value::new(PythonExtensionModule { inner: em.clone() })
        }

        PythonResource::ExtensionModuleStaticallyLinked(em) => {
            Value::new(PythonExtensionModule { inner: em.clone() })
        }

        PythonResource::EggFile(_) => {
            panic!("egg files not supported");
        }

        PythonResource::PathExtension(_) => {
            panic!("path extensions not supported");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::testutil::*;
    use super::*;

    #[test]
    fn test_source_module_attrs() {
        let (mut env, type_values) = starlark_make_exe().unwrap();

        let mut m = starlark_eval_in_env(
            &mut env,
            &type_values,
            "exe.make_python_source_module('foo', 'import bar')",
        )
        .unwrap();

        assert_eq!(m.get_type(), "PythonSourceModule");
        assert!(m.has_attr("name").unwrap());
        assert_eq!(m.get_attr("name").unwrap().to_str(), "foo");

        assert!(m.has_attr("source").unwrap());
        assert_eq!(m.get_attr("source").unwrap().to_str(), "import bar");

        assert!(m.has_attr("is_package").unwrap());
        assert_eq!(m.get_attr("is_package").unwrap().to_bool(), false);

        assert!(m.has_attr("location").unwrap());
        assert_eq!(m.get_attr("location").unwrap().get_type(), "NoneType");

        m.set_attr("location", Value::from("in-memory")).unwrap();
        assert_eq!(m.get_attr("location").unwrap().to_str(), "in-memory");

        m.set_attr("location", Value::from("default")).unwrap();
        assert_eq!(m.get_attr("location").unwrap().get_type(), "NoneType");

        m.set_attr("location", Value::from("filesystem-relative:lib"))
            .unwrap();
        assert_eq!(
            m.get_attr("location").unwrap().to_str(),
            "filesystem-relative:lib"
        );
    }
}
