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
    starlark::values::error::{RuntimeError, ValueError, INCORRECT_PARAMETER_TYPE_ERROR_CODE},
    starlark::values::{Immutable, Mutable, TypedValue, Value, ValueResult},
    std::convert::{TryFrom, TryInto},
};

/// Where a resource should be loaded from.
#[derive(Clone, Debug)]
pub enum ResourceLocation {
    /// Use default load semantics for the target binary.
    Default,
    /// Load the resource from memory.
    InMemory,
    /// Load the resource from a filesystem path relative to the binary.
    RelativePath(String),
}

impl From<ResourceLocation> for Value {
    fn from(location: ResourceLocation) -> Self {
        Value::new(match location {
            ResourceLocation::Default => "default".to_string(),
            ResourceLocation::InMemory => "in-memory".to_string(),
            ResourceLocation::RelativePath(prefix) => format!("filesystem-relative:{}", prefix),
        })
    }
}

impl TryFrom<Value> for ResourceLocation {
    type Error = ValueError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let s = value.to_str();

        if s == "default" {
            Ok(ResourceLocation::Default)
        } else if s == "in-memory" {
            Ok(ResourceLocation::InMemory)
        } else if s.starts_with("filesystem-relative:") {
            let prefix = s.split_at("filesystem-relative:".len()).1;
            Ok(ResourceLocation::RelativePath(prefix.to_string()))
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

#[derive(Debug, Clone)]
pub struct PythonSourceModule {
    pub module: RawSourceModule,
    pub location: ResourceLocation,
}

impl PythonSourceModule {
    pub fn new(module: RawSourceModule) -> Self {
        Self {
            module,
            location: ResourceLocation::Default,
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
        format!("PythonSourceModule<name={}>", self.module.name)
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_attr(&self, attribute: &str) -> ValueResult {
        let v = match attribute {
            "name" => Value::new(self.module.name.clone()),
            "source" => {
                let source = self.module.source.resolve().map_err(|e| {
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
            "is_package" => Value::new(self.module.is_package),
            "location" => self.location.clone().into(),
            attr => {
                return Err(ValueError::OperationNotSupported {
                    op: format!(".{}", attr),
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
                self.location = value.try_into()?;

                Ok(())
            }
            _ => Err(ValueError::OperationNotSupported {
                op: format!(".{} =", attribute),
                left: Self::TYPE.to_owned(),
                right: None,
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PythonBytecodeModule {
    pub module: PythonModuleBytecodeFromSource,
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
            self.module.name, self.module.optimize_level
        )
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_attr(&self, attribute: &str) -> ValueResult {
        let v = match attribute {
            "name" => Value::new(self.module.name.clone()),
            // TODO expose source
            // "source" => Value::new(self.module.source),
            "optimize_level" => Value::new(match self.module.optimize_level {
                BytecodeOptimizationLevel::Zero => 0,
                BytecodeOptimizationLevel::One => 1,
                BytecodeOptimizationLevel::Two => 2,
            }),
            "is_package" => Value::new(self.module.is_package),
            attr => {
                return Err(ValueError::OperationNotSupported {
                    op: format!(".{}", attr),
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

#[derive(Debug, Clone)]
pub struct PythonPackageResource {
    pub data: RawPackageResource,
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
            self.data.leaf_package, self.data.relative_name
        )
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_attr(&self, attribute: &str) -> ValueResult {
        let v = match attribute {
            "package" => Value::new(self.data.leaf_package.clone()),
            "name" => Value::new(self.data.relative_name.clone()),
            // TODO expose raw data
            attr => {
                return Err(ValueError::OperationNotSupported {
                    op: format!(".{}", attr),
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

#[derive(Debug, Clone)]
pub struct PythonPackageDistributionResource {
    pub resource: RawDistributionResource,
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
            self.resource.package, self.resource.name
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
            "package" => Value::new(self.resource.package.clone()),
            "name" => Value::new(self.resource.name.clone()),
            // TODO expose raw data
            attr => {
                return Err(ValueError::OperationNotSupported {
                    op: format!(".{}", attr),
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

#[derive(Debug, Clone)]
pub struct PythonExtensionModule {
    pub em: RawPythonExtensionModule,
}

impl TypedValue for PythonExtensionModule {
    type Holder = Immutable<PythonExtensionModule>;
    const TYPE: &'static str = "PythonExtensionModule";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }

    fn to_str(&self) -> String {
        format!("PythonExtensionModule<name={}>", self.em.name)
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_attr(&self, attribute: &str) -> ValueResult {
        let v = match attribute {
            "name" => Value::new(self.em.name.clone()),
            attr => {
                return Err(ValueError::OperationNotSupported {
                    op: format!(".{}", attr),
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
            Value::new(PythonBytecodeModule { module: m.clone() })
        }

        PythonResource::ModuleBytecode { .. } => {
            panic!("not yet implemented");
        }

        PythonResource::Resource(data) => Value::new(PythonPackageResource { data: data.clone() }),

        PythonResource::DistributionResource(resource) => {
            Value::new(PythonPackageDistributionResource {
                resource: resource.clone(),
            })
        }

        PythonResource::ExtensionModuleDynamicLibrary(em) => {
            Value::new(PythonExtensionModule { em: em.clone() })
        }

        PythonResource::ExtensionModuleStaticallyLinked(em) => {
            Value::new(PythonExtensionModule { em: em.clone() })
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
        assert_eq!(m.get_attr("location").unwrap().to_str(), "default");

        m.set_attr("location", Value::from("in-memory")).unwrap();
        assert_eq!(m.get_attr("location").unwrap().to_str(), "in-memory");

        m.set_attr("location", Value::from("default")).unwrap();
        assert_eq!(m.get_attr("location").unwrap().to_str(), "default");

        m.set_attr("location", Value::from("filesystem-relative:lib"))
            .unwrap();
        assert_eq!(
            m.get_attr("location").unwrap().to_str(),
            "filesystem-relative:lib"
        );
    }
}
