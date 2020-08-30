// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::py_packaging::standalone_distribution::DistributionExtensionModule,
    python_packaging::resource::{
        BytecodeOptimizationLevel, PythonExtensionModule as RawPythonExtensionModule,
        PythonModuleBytecodeFromSource, PythonModuleSource as RawSourceModule,
        PythonPackageDistributionResource as RawDistributionResource,
        PythonPackageResource as RawPackageResource, PythonResource,
    },
    starlark::environment::Environment,
    starlark::values::{
        default_compare, RuntimeError, TypedValue, Value, ValueError, ValueResult,
        INCORRECT_PARAMETER_TYPE_ERROR_CODE,
    },
    starlark::{any, immutable, not_supported},
    std::any::Any,
    std::cmp::Ordering,
    std::collections::HashMap,
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
            Err(RuntimeError {
                code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                message: format!("unable to convert value {} to a resource location", s),
                label: format!(
                    "expected `default`, `in-memory`, or `filesystem-relative:*`; got {}",
                    s
                ),
            }
            .into())
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
    immutable!();
    any!();
    not_supported!(binop, dir_attr, function, get_hash, indexable, iterable, sequence, to_int);

    fn to_str(&self) -> String {
        format!("PythonSourceModule<name={}>", self.module.name)
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_type(&self) -> &'static str {
        "PythonSourceModule"
    }

    fn to_bool(&self) -> bool {
        true
    }

    fn compare(&self, other: &dyn TypedValue, _recursion: u32) -> Result<Ordering, ValueError> {
        default_compare(self, other)
    }

    fn get_attr(&self, attribute: &str) -> ValueResult {
        let v = match attribute {
            "name" => Value::new(self.module.name.clone()),
            "source" => {
                let source = self.module.source.resolve().map_err(|e| {
                    RuntimeError {
                        code: "PYOXIDIZER_SOURCE_ERROR",
                        message: format!("error resolving source code: {}", e),
                        label: "source".to_string(),
                    }
                    .into()
                })?;

                let source = String::from_utf8(source).map_err(|_| {
                    RuntimeError {
                        code: "PYOXIDIZER_SOURCE_ERROR",
                        message: "error converting source code to UTF-8".to_string(),
                        label: "source".to_string(),
                    }
                    .into()
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
                left: self.get_type().to_owned(),
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
    immutable!();
    any!();
    not_supported!(
        binop, dir_attr, function, get_hash, indexable, iterable, sequence, set_attr, to_int
    );

    fn to_str(&self) -> String {
        format!(
            "PythonBytecodeModule<name={}; level={:?}>",
            self.module.name, self.module.optimize_level
        )
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_type(&self) -> &'static str {
        "PythonBytecodeModule"
    }

    fn to_bool(&self) -> bool {
        true
    }

    fn compare(&self, other: &dyn TypedValue, _recursion: u32) -> Result<Ordering, ValueError> {
        default_compare(self, other)
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
    immutable!();
    any!();
    not_supported!(
        binop, dir_attr, function, get_hash, indexable, iterable, sequence, set_attr, to_int
    );

    fn to_str(&self) -> String {
        format!(
            "PythonPackageResource<package={}, name={}>",
            self.data.leaf_package, self.data.relative_name
        )
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_type(&self) -> &'static str {
        "PythonPackageResource"
    }

    fn to_bool(&self) -> bool {
        true
    }

    fn compare(&self, other: &dyn TypedValue, _recursion: u32) -> Result<Ordering, ValueError> {
        default_compare(self, other)
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
    immutable!();
    any!();
    not_supported!(
        binop, dir_attr, function, get_hash, indexable, iterable, sequence, set_attr, to_int
    );

    fn to_str(&self) -> String {
        format!(
            "PythonPackageDistributionResource<package={}, name={}>",
            self.resource.package, self.resource.name
        )
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_type(&self) -> &'static str {
        "PythonPackageDistributionResource"
    }

    fn to_bool(&self) -> bool {
        true
    }

    fn compare(&self, other: &dyn TypedValue, _recursion: u32) -> Result<Ordering, ValueError> {
        default_compare(self, other)
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

/// Represents an extension module flavor.
#[derive(Debug, Clone)]
pub enum PythonExtensionModuleFlavor {
    /// An extension module from a Python distribution.
    Distribution(DistributionExtensionModule),

    /// An extension module that can be statically linked.
    StaticallyLinked(RawPythonExtensionModule),

    /// An extension module that exists as a dynamic library.
    DynamicLibrary(RawPythonExtensionModule),
}

impl PythonExtensionModuleFlavor {
    pub fn name(&self) -> String {
        match self {
            PythonExtensionModuleFlavor::Distribution(m) => m.module.clone(),
            PythonExtensionModuleFlavor::StaticallyLinked(m) => m.name.clone(),
            PythonExtensionModuleFlavor::DynamicLibrary(m) => m.name.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PythonExtensionModule {
    pub em: PythonExtensionModuleFlavor,
}

impl TypedValue for PythonExtensionModule {
    immutable!();
    any!();
    not_supported!(
        binop, dir_attr, function, get_hash, indexable, iterable, sequence, set_attr, to_int
    );

    fn to_str(&self) -> String {
        format!("PythonExtensionModule<name={}>", self.em.name())
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_type(&self) -> &'static str {
        "PythonExtensionModule"
    }

    fn to_bool(&self) -> bool {
        true
    }

    fn compare(&self, other: &dyn TypedValue, _recursion: u32) -> Result<Ordering, ValueError> {
        default_compare(self, other)
    }

    fn get_attr(&self, attribute: &str) -> ValueResult {
        let v = match attribute {
            "name" => Value::new(self.em.name()),
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

        PythonResource::ExtensionModuleDynamicLibrary(em) => Value::new(PythonExtensionModule {
            em: PythonExtensionModuleFlavor::DynamicLibrary(em.clone()),
        }),

        PythonResource::ExtensionModuleStaticallyLinked(em) => Value::new(PythonExtensionModule {
            em: PythonExtensionModuleFlavor::StaticallyLinked(em.clone()),
        }),

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
        let mut env = starlark_make_exe().unwrap();

        let mut m = starlark_eval_in_env(
            &mut env,
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
