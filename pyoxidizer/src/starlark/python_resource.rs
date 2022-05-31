// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    super::{
        file::FileValue, python_extension_module::PythonExtensionModuleValue,
        python_module_source::PythonModuleSourceValue,
        python_package_distribution_resource::PythonPackageDistributionResourceValue,
        python_package_resource::PythonPackageResourceValue,
        python_packaging_policy::PythonPackagingPolicyValue,
    },
    python_packaging::{
        location::ConcreteResourceLocation, resource::PythonResource,
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
            {Value, ValueResult},
        },
    },
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
    fn add_collection_context(
        &self,
    ) -> Result<Option<PythonResourceAddCollectionContext>, ValueError>;

    /// Set the [PythonResourceAddCollectionContext] value on this instance.
    ///
    /// Returns the old value.
    fn replace_add_collection_context(
        &mut self,
        add_context: PythonResourceAddCollectionContext,
    ) -> Result<Option<PythonResourceAddCollectionContext>, ValueError>;

    /// Cast this instance to a `PythonResource`.
    fn as_python_resource(&self) -> Result<PythonResource, ValueError>;

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

        let context = self.add_collection_context()?;

        Ok(match context {
            Some(context) => match attribute {
                "add_bytecode_optimization_level_zero" => Value::new(context.optimize_level_zero),
                "add_bytecode_optimization_level_one" => Value::new(context.optimize_level_one),
                "add_bytecode_optimization_level_two" => Value::new(context.optimize_level_two),
                "add_include" => Value::new(context.include),
                "add_location" => Value::new::<String>(context.location.into()),
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

    // The caller should validate that the attribute is an add collection attribute
    // before calling. Otherwise a panic can occur.
    fn set_attr_add_collection_context(
        &mut self,
        attribute: &str,
        value: Value,
    ) -> Result<(), ValueError> {
        let mut context = self.add_collection_context()?;

        match context {
            Some(ref mut context) => {
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
        }?;

        self.replace_add_collection_context(context.unwrap())?;

        Ok(())
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
    label: &str,
    type_values: &TypeValues,
    call_stack: &mut CallStack,
    resource: &PythonResource,
    policy: &PythonPackagingPolicyValue,
) -> ValueResult {
    match resource {
        PythonResource::ModuleSource(sm) => {
            let mut m = PythonModuleSourceValue::new(sm.clone().into_owned());
            policy.apply_to_resource(label, type_values, call_stack, &mut m)?;

            Ok(Value::new(m))
        }

        PythonResource::PackageResource(data) => {
            let mut r = PythonPackageResourceValue::new(data.clone().into_owned());
            policy.apply_to_resource(label, type_values, call_stack, &mut r)?;

            Ok(Value::new(r))
        }

        PythonResource::PackageDistributionResource(resource) => {
            let mut r = PythonPackageDistributionResourceValue::new(resource.clone().into_owned());
            policy.apply_to_resource(label, type_values, call_stack, &mut r)?;

            Ok(Value::new(r))
        }

        PythonResource::ExtensionModule(em) => {
            let mut em = PythonExtensionModuleValue::new(em.clone().into_owned());
            policy.apply_to_resource(label, type_values, call_stack, &mut em)?;

            Ok(Value::new(em))
        }

        PythonResource::File(f) => {
            let mut value = FileValue::new(f.clone().into_owned());
            policy.apply_to_resource(label, type_values, call_stack, &mut value)?;

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
            .add_collection_context()?),
        "PythonPackageResource" => Ok(value
            .downcast_ref::<PythonPackageResourceValue>()
            .unwrap()
            .add_collection_context()?),
        "PythonPackageDistributionResource" => Ok(value
            .downcast_ref::<PythonPackageDistributionResourceValue>()
            .unwrap()
            .add_collection_context()?),
        "PythonExtensionModule" => Ok(value
            .downcast_ref::<PythonExtensionModuleValue>()
            .unwrap()
            .add_collection_context()?),
        "File" => Ok(value
            .downcast_ref::<FileValue>()
            .unwrap()
            .add_collection_context()?),
        t => Err(ValueError::from(RuntimeError {
            code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
            message: format!("unable to obtain add collection context from {}", t),
            label: label.to_string(),
        })),
    }
}
