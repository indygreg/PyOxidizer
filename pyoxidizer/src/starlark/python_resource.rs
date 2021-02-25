// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    super::python_packaging_policy::PythonPackagingPolicyValue,
    python_packaging::{
        location::ConcreteResourceLocation,
        resource::{
            PythonExtensionModule, PythonModuleSource, PythonPackageDistributionResource,
            PythonPackageResource, PythonResource,
        },
        resource_collection::PythonResourceAddCollectionContext,
    },
    starlark::{
        environment::TypeValues,
        eval::call_stack::CallStack,
        values::{
            error::{
                RuntimeError, UnsupportedOperation, ValueError, INCORRECT_PARAMETER_TYPE_ERROR_CODE,
            },
            none::NoneType,
            {Mutable, TypedValue, Value, ValueResult},
        },
    },
    std::convert::{TryFrom, TryInto},
    tugger_file_manifest::File,
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

impl From<OptionalResourceLocation> for Option<ConcreteResourceLocation> {
    fn from(location: OptionalResourceLocation) -> Self {
        location.inner
    }
}

/// Defines functionality for exposing `PythonResourceAddCollectionContext` from a type.
pub trait ResourceCollectionContext {
    /// Obtain the `PythonResourceAddCollectionContext` associated with this instance, if available.
    fn add_collection_context(&self) -> &Option<PythonResourceAddCollectionContext>;

    /// Obtain the mutable `PythonResourceAddCollectionContext` associated with this instance, if available.
    fn add_collection_context_mut(&mut self) -> &mut Option<PythonResourceAddCollectionContext>;

    /// Cast this instance to a `PythonResource`.
    fn as_python_resource(&self) -> PythonResource;

    /// Obtains the Starlark object attributes that are defined by the add collection context.
    fn add_collection_context_attrs(&self) -> Vec<&'static str> {
        vec![
            "add_include",
            "add_location",
            "add_location_fallback",
            "add_source",
            "add_bytecode_optimization_level_zero",
            "add_bytecode_optimization_level_one",
            "add_bytecode_optimization_level_two",
        ]
    }

    /// Obtain the attribute value for an add collection context.
    ///
    /// The caller should verify the attribute should be serviced by us
    /// before calling.
    fn get_attr_add_collection_context(&self, attribute: &str) -> ValueResult {
        if !self.add_collection_context_attrs().contains(&attribute) {
            panic!(
                "get_attr_add_collection_context({}) called when it shouldn't have been",
                attribute
            );
        }

        let context = self.add_collection_context();

        Ok(match context {
            Some(context) => match attribute {
                "add_bytecode_optimization_level_zero" => Value::new(context.optimize_level_zero),
                "add_bytecode_optimization_level_one" => Value::new(context.optimize_level_one),
                "add_bytecode_optimization_level_two" => Value::new(context.optimize_level_two),
                "add_include" => Value::new(context.include),
                "add_location" => Value::new::<String>(context.location.clone().into()),
                "add_location_fallback" => match context.location_fallback.as_ref() {
                    Some(location) => Value::new::<String>(location.clone().into()),
                    None => Value::from(NoneType::None),
                },
                "add_source" => Value::new(context.store_source),
                _ => panic!("this should not happen"),
            },
            None => Value::from(NoneType::None),
        })
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
                    "add_bytecode_optimization_level_zero" => {
                        context.optimize_level_zero = value.to_bool();
                        Ok(())
                    }
                    "add_bytecode_optimization_level_one" => {
                        context.optimize_level_one = value.to_bool();
                        Ok(())
                    }
                    "add_bytecode_optimization_level_two" => {
                        context.optimize_level_two = value.to_bool();
                        Ok(())
                    }
                    "add_include" => {
                        context.include = value.to_bool();
                        Ok(())
                    }
                    "add_location" => {
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
                    }
                    "add_location_fallback" => {
                        let location: OptionalResourceLocation = (&value).try_into()?;

                        match location.inner {
                            Some(location) => {
                                context.location_fallback = Some(location);
                                Ok(())
                            }
                            None => {
                                context.location_fallback = None;
                                Ok(())
                            }
                        }
                    }
                    "add_source" => {
                        context.store_source = value.to_bool();
                        Ok(())
                    }
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
    type Holder = Mutable<PythonPackageDistributionResourceValue>;
    const TYPE: &'static str = "PythonPackageDistributionResource";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }

    fn to_str(&self) -> String {
        format!(
            "{}<package={}, name={}>",
            Self::TYPE,
            self.inner.package,
            self.inner.name
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
            "is_stdlib" => Value::from(false),
            "package" => Value::new(self.inner.package.clone()),
            "name" => Value::new(self.inner.name.clone()),
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

/// Starlark `Value` wrapper for `PythonExtensionModule`.
#[derive(Debug, Clone)]
pub struct PythonExtensionModuleValue {
    pub inner: PythonExtensionModule,
    pub add_context: Option<PythonResourceAddCollectionContext>,
}

impl PythonExtensionModuleValue {
    pub fn new(em: PythonExtensionModule) -> Self {
        Self {
            inner: em,
            add_context: None,
        }
    }
}

impl ResourceCollectionContext for PythonExtensionModuleValue {
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

impl TypedValue for PythonExtensionModuleValue {
    type Holder = Mutable<PythonExtensionModuleValue>;
    const TYPE: &'static str = "PythonExtensionModule";

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
            "name" => true,
            attr => self.add_collection_context_attrs().contains(&attr),
        })
    }

    fn set_attr(&mut self, attribute: &str, value: Value) -> Result<(), ValueError> {
        self.set_attr_add_collection_context(attribute, value)
    }
}

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

/// Whether a `PythonResource` can be converted to a Starlark value.
pub fn is_resource_starlark_compatible(resource: &PythonResource) -> bool {
    match resource {
        PythonResource::ModuleSource(_) => true,
        PythonResource::PackageResource(_) => true,
        PythonResource::PackageDistributionResource(_) => true,
        PythonResource::ExtensionModule(_) => true,
        PythonResource::ModuleBytecode(_) => false,
        PythonResource::ModuleBytecodeRequest(_) => false,
        PythonResource::EggFile(_) => false,
        PythonResource::PathExtension(_) => false,
        PythonResource::File(_) => true,
    }
}

pub fn python_resource_to_value(
    type_values: &TypeValues,
    call_stack: &mut CallStack,
    resource: &PythonResource,
    policy: &PythonPackagingPolicyValue,
) -> ValueResult {
    match resource {
        PythonResource::ModuleSource(sm) => {
            let mut m = PythonModuleSourceValue::new(sm.clone().into_owned());
            policy.apply_to_resource(type_values, call_stack, &mut m)?;

            Ok(Value::new(m))
        }

        PythonResource::PackageResource(data) => {
            let mut r = PythonPackageResourceValue::new(data.clone().into_owned());
            policy.apply_to_resource(type_values, call_stack, &mut r)?;

            Ok(Value::new(r))
        }

        PythonResource::PackageDistributionResource(resource) => {
            let mut r = PythonPackageDistributionResourceValue::new(resource.clone().into_owned());
            policy.apply_to_resource(type_values, call_stack, &mut r)?;

            Ok(Value::new(r))
        }

        PythonResource::ExtensionModule(em) => {
            let mut em = PythonExtensionModuleValue::new(em.clone().into_owned());
            policy.apply_to_resource(type_values, call_stack, &mut em)?;

            Ok(Value::new(em))
        }

        PythonResource::File(f) => {
            let mut value = FileValue::new(f.clone().into_owned());
            policy.apply_to_resource(type_values, call_stack, &mut value)?;

            Ok(Value::new(value))
        }

        _ => {
            panic!("incompatible PythonResource variant passed; did you forget to filter through is_resource_starlark_compatible()?")
        }
    }
}

/// Attempt to resolve the `PythonResourceAddCollectionContext` for a Value.
pub fn add_context_for_value(
    value: &Value,
    label: &str,
) -> Result<Option<PythonResourceAddCollectionContext>, ValueError> {
    match value.get_type() {
        "PythonModuleSource" => Ok(value
            .downcast_ref::<PythonModuleSourceValue>()
            .unwrap()
            .add_collection_context()
            .clone()),
        "PythonPackageResource" => Ok(value
            .downcast_ref::<PythonPackageResourceValue>()
            .unwrap()
            .add_collection_context()
            .clone()),
        "PythonPackageDistributionResource" => Ok(value
            .downcast_ref::<PythonPackageDistributionResourceValue>()
            .unwrap()
            .add_collection_context()
            .clone()),
        "PythonExtensionModule" => Ok(value
            .downcast_ref::<PythonExtensionModuleValue>()
            .unwrap()
            .add_collection_context()
            .clone()),
        "File" => Ok(value
            .downcast_ref::<FileValue>()
            .unwrap()
            .add_collection_context()
            .clone()),
        t => Err(ValueError::from(RuntimeError {
            code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
            message: format!("unable to obtain add collection context from {}", t),
            label: label.to_string(),
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::super::python_distribution::PythonDistributionValue;
    use super::super::testutil::*;
    use super::*;
    use anyhow::Result;

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
