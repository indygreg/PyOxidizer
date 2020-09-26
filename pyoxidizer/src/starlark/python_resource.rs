// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    python_packaging::policy::PythonPackagingPolicy,
    python_packaging::resource::{
        BytecodeOptimizationLevel, PythonExtensionModule, PythonModuleBytecodeFromSource,
        PythonModuleSource, PythonPackageDistributionResource, PythonPackageResource,
        PythonResource,
    },
    python_packaging::resource_collection::{
        ConcreteResourceLocation, PythonResourceAddCollectionContext,
    },
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

/// Defines functionality for exposing `PythonResourceAddCollectionContext` from a type.
pub trait ResourceCollectionContext {
    /// Obtain the `PythonResourceAddCollectionContext` associated with this instance, if available.
    fn add_collection_context(&self) -> &Option<PythonResourceAddCollectionContext>;

    /// Obtain the mutable `PythonResourceAddCollectionContext` associated with this instance, if available.
    fn add_collection_context_mut(&mut self) -> &mut Option<PythonResourceAddCollectionContext>;

    fn as_python_resource(&self) -> PythonResource;

    /// Apply a Python packaging policy to this instance.
    ///
    /// This has the effect of replacing the `PythonResourceAddCollectionContext`
    /// instance with a fresh one derived from the policy. If no context
    /// is currently defined on the instance, a new one will be created so
    /// there is.
    fn apply_packaging_policy(&mut self, policy: &PythonPackagingPolicy) {
        let new_context = policy.derive_collection_add_context(&self.as_python_resource());
        self.add_collection_context_mut().replace(new_context);
    }

    /// Obtains the Starlark object attributes that are defined by the add collection context.
    fn add_collection_context_attrs(&self) -> Vec<&'static str> {
        vec!["location"]
    }

    /// Obtain the attribute value for an add collection context.
    ///
    /// The caller should verify the attribute should be serviced by us
    /// before calling.
    fn get_attr_add_collection_context(&self, attribute: &str) -> ValueResult {
        let context = self.add_collection_context();

        match attribute {
            "location" => Ok(match context {
                Some(context) => Value::new::<String>(context.location.clone().into()),
                None => Value::from(NoneType::None),
            }),
            attr => panic!(
                "get_attr_add_collection_context({}) called when it shouldn't have been",
                attr
            ),
        }
    }

    fn set_attr_add_collection_context(
        &mut self,
        attribute: &str,
        value: Value,
    ) -> Result<(), ValueError> {
        let context = self.add_collection_context_mut();

        match context {
            Some(context) => {
                match attribute {
                    "location" => {
                        let location: OptionalResourceLocation = (&value).try_into()?;

                        match location.inner {
                            Some(location) => {
                                context.location = location;

                                Ok(())
                            }
                            None => {
                                Err(ValueError::OperationNotSupported {
                                    op: UnsupportedOperation::SetAttr(attribute.to_string()),
                                    left: "set_attr".to_string(),
                                    right: None,
                                })
                            }
                        }
                    },
                    attr => panic!("set_attr_add_collection_context({}) called when it shouldn't have been", attr)
                }
            },
            None => Err(ValueError::from(RuntimeError {
                code: "PYOXIDIZER",
                message: "attempting to set a collection context attribute on an object without a context".to_string(),
                label: "setattr()".to_string()
            }))
        }
    }
}

/// Starlark value wrapper for `PythonModuleSource`.
#[derive(Debug, Clone)]
pub struct PythonSourceModuleValue {
    pub inner: PythonModuleSource,
    pub add_context: Option<PythonResourceAddCollectionContext>,
}

impl PythonSourceModuleValue {
    pub fn new(module: PythonModuleSource) -> Self {
        Self {
            inner: module,
            add_context: None,
        }
    }
}

impl ResourceCollectionContext for PythonSourceModuleValue {
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

impl TypedValue for PythonSourceModuleValue {
    type Holder = Mutable<PythonSourceModuleValue>;
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
            attr => {
                return if self.add_collection_context_attrs().contains(&attr) {
                    self.get_attr_add_collection_context(attr)
                } else {
                    Err(ValueError::OperationNotSupported {
                        op: UnsupportedOperation::GetAttr(attr.to_string()),
                        left: "PythonSourceModule".to_string(),
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

/// Starlark `Value` wrapper for `PythonModuleBytecodeFromSource`.
#[derive(Debug, Clone)]
pub struct PythonBytecodeModuleValue {
    pub inner: PythonModuleBytecodeFromSource,
    pub add_context: Option<PythonResourceAddCollectionContext>,
}

impl PythonBytecodeModuleValue {
    pub fn new(module: PythonModuleBytecodeFromSource) -> Self {
        Self {
            inner: module,
            add_context: None,
        }
    }
}

impl ResourceCollectionContext for PythonBytecodeModuleValue {
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

impl TypedValue for PythonBytecodeModuleValue {
    type Holder = Immutable<PythonBytecodeModuleValue>;
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
                return if self.add_collection_context_attrs().contains(&attr) {
                    self.get_attr_add_collection_context(attr)
                } else {
                    Err(ValueError::OperationNotSupported {
                        op: UnsupportedOperation::GetAttr(attr.to_string()),
                        left: "PythonBytecodeModule".to_string(),
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
            // TODO expose source
            // "source" => true,
            "optimize_level" => true,
            "is_package" => true,
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
    type Holder = Immutable<PythonPackageResourceValue>;
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
pub struct PythonPackageDistributionResourceValue {
    pub inner: PythonPackageDistributionResource,
    pub add_context: Option<PythonResourceAddCollectionContext>,
}

impl PythonPackageDistributionResourceValue {
    pub fn new(resource: PythonPackageDistributionResource) -> Self {
        Self {
            inner: resource,
            add_context: None,
        }
    }
}

impl ResourceCollectionContext for PythonPackageDistributionResourceValue {
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

impl TypedValue for PythonPackageDistributionResourceValue {
    type Holder = Immutable<PythonPackageDistributionResourceValue>;
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
pub struct PythonExtensionModuleValue {
    pub inner: PythonExtensionModule,
}

impl TypedValue for PythonExtensionModuleValue {
    type Holder = Immutable<PythonExtensionModuleValue>;
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

/// Whether a `PythonResource` can be converted to a Starlark value.
pub fn is_resource_starlark_compatible(resource: &PythonResource) -> bool {
    match resource {
        PythonResource::ModuleSource(_) => true,
        PythonResource::ModuleBytecodeRequest(_) => true,
        PythonResource::Resource(_) => true,
        PythonResource::DistributionResource(_) => true,
        PythonResource::ExtensionModule(_) => true,
        _ => false,
    }
}

pub fn python_resource_to_value(
    resource: &PythonResource,
    policy: &PythonPackagingPolicy,
) -> Value {
    match resource {
        PythonResource::ModuleSource(sm) => {
            let mut m = PythonSourceModuleValue::new(sm.clone().into_owned());
            m.apply_packaging_policy(policy);

            Value::new(m)
        }

        PythonResource::ModuleBytecodeRequest(m) => {
            let mut m = PythonBytecodeModuleValue::new(m.clone().into_owned());
            m.apply_packaging_policy(policy);

            Value::new(m)
        }

        PythonResource::Resource(data) => {
            let mut r = PythonPackageResourceValue::new(data.clone().into_owned());
            r.apply_packaging_policy(policy);

            Value::new(r)
        }

        PythonResource::DistributionResource(resource) => {
            let mut r = PythonPackageDistributionResourceValue::new(resource.clone().into_owned());
            r.apply_packaging_policy(policy);

            Value::new(r)
        }

        PythonResource::ExtensionModule(em) => Value::new(PythonExtensionModuleValue {
            inner: em.clone().into_owned(),
        }),

        _ => {
            panic!("incompatible PythonResource variant passed; did you forget to filter through is_resource_starlark_compatible()?")
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
        assert_eq!(m.get_attr("location").unwrap().to_str(), "in-memory");

        m.set_attr("location", Value::from("in-memory")).unwrap();
        assert_eq!(m.get_attr("location").unwrap().to_str(), "in-memory");

        m.set_attr("location", Value::from("filesystem-relative:lib"))
            .unwrap();
        assert_eq!(
            m.get_attr("location").unwrap().to_str(),
            "filesystem-relative:lib"
        );
    }
}
