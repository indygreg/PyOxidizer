// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    python_packaging::policy::{
        ExtensionModuleFilter, PythonPackagingPolicy, PythonResourcesPolicy,
    },
    starlark::values::{
        error::{RuntimeError, UnsupportedOperation, ValueError},
        Mutable, TypedValue, Value, ValueResult,
    },
    std::convert::TryFrom,
};

#[derive(Debug, Clone)]
pub struct PythonPackagingPolicyValue {
    pub inner: PythonPackagingPolicy,
}

impl TypedValue for PythonPackagingPolicyValue {
    type Holder = Mutable<PythonPackagingPolicyValue>;
    const TYPE: &'static str = "PythonPackagingPolicy";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }

    fn get_attr(&self, attribute: &str) -> ValueResult {
        let v = match attribute {
            "extension_module_filter" => {
                Value::from(self.inner.get_extension_module_filter().as_ref())
            }
            "include_distribution_sources" => {
                Value::from(self.inner.include_distribution_sources())
            }
            "include_distribution_resources" => {
                Value::from(self.inner.include_distribution_resources())
            }
            "resources_policy" => Value::new::<String>(self.inner.get_resources_policy().into()),
            attr => {
                return Err(ValueError::OperationNotSupported {
                    op: UnsupportedOperation::GetAttr(attr.to_string()),
                    left: "PythonPackagingPolicy".to_string(),
                    right: None,
                })
            }
        };

        Ok(v)
    }

    fn has_attr(&self, attribute: &str) -> Result<bool, ValueError> {
        Ok(match attribute {
            "extension_module_filter" => true,
            "include_distribution_sources" => true,
            "include_distribution_resources" => true,
            "resources_policy" => true,
            _ => false,
        })
    }

    fn set_attr(&mut self, attribute: &str, value: Value) -> Result<(), ValueError> {
        match attribute {
            "extension_module_filter" => {
                let filter =
                    ExtensionModuleFilter::try_from(value.to_string().as_str()).map_err(|e| {
                        ValueError::from(RuntimeError {
                            code: "PYOXIDIZER_BUILD",
                            message: e,
                            label: format!("{}.{} = {}", Self::TYPE, attribute, value.to_string()),
                        })
                    })?;

                self.inner.set_extension_module_filter(filter);
            }
            "include_distribution_sources" => {
                self.inner.set_include_distribution_sources(value.to_bool());
            }
            "include_distribution_resources" => {
                self.inner
                    .set_include_distribution_resources(value.to_bool());
            }
            "resources_policy" => {
                let policy =
                    PythonResourcesPolicy::try_from(value.to_string().as_str()).map_err(|e| {
                        ValueError::from(RuntimeError {
                            code: "PYOXIDIZER_BUILD",
                            message: e.to_string(),
                            label: format!("{}.{} = {}", Self::TYPE, attribute, value.to_string()),
                        })
                    })?;

                self.inner.set_resources_policy(policy);
            }
            attr => {
                return Err(ValueError::OperationNotSupported {
                    op: UnsupportedOperation::SetAttr(attr.to_string()),
                    left: Self::TYPE.to_owned(),
                    right: None,
                });
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use {
        super::super::python_distribution::PythonDistribution, super::super::testutil::*, super::*,
    };

    #[test]
    fn test_basic() {
        let (mut env, type_values) = starlark_env();

        starlark_eval_in_env(
            &mut env,
            &type_values,
            "dist = default_python_distribution()",
        )
        .unwrap();
        starlark_eval_in_env(
            &mut env,
            &type_values,
            "policy = dist.make_python_packaging_policy()",
        )
        .unwrap();

        let dist_value = starlark_eval_in_env(&mut env, &type_values, "dist").unwrap();
        let dist = dist_value.downcast_ref::<PythonDistribution>().unwrap();

        let policy = dist
            .distribution
            .as_ref()
            .unwrap()
            .create_packaging_policy()
            .unwrap();

        // Need value to go out of scope to avoid double borrow.
        {
            let policy_value = starlark_eval_in_env(&mut env, &type_values, "policy").unwrap();
            assert_eq!(policy_value.get_type(), "PythonPackagingPolicy");

            let x = policy_value
                .downcast_ref::<PythonPackagingPolicyValue>()
                .unwrap();

            // Distribution method should return a policy equivalent to what Starlark gives us.
            assert_eq!(policy, x.inner);
        }

        // attributes work
        let value =
            starlark_eval_in_env(&mut env, &type_values, "policy.extension_module_filter").unwrap();
        assert_eq!(value.get_type(), "string");
        assert_eq!(
            value.to_string(),
            policy.get_extension_module_filter().as_ref()
        );

        let value = starlark_eval_in_env(
            &mut env,
            &type_values,
            "policy.extension_module_filter = 'minimal'; policy.extension_module_filter",
        )
        .unwrap();
        assert_eq!(value.to_string(), "minimal");

        let value = starlark_eval_in_env(
            &mut env,
            &type_values,
            "policy.include_distribution_sources",
        )
        .unwrap();
        assert_eq!(value.get_type(), "bool");
        assert_eq!(value.to_bool(), policy.include_distribution_sources());

        let value = starlark_eval_in_env(
            &mut env,
            &type_values,
            "policy.include_distribution_sources = False; policy.include_distribution_sources",
        )
        .unwrap();
        assert!(!value.to_bool());

        let value = starlark_eval_in_env(
            &mut env,
            &type_values,
            "policy.include_distribution_sources = True; policy.include_distribution_sources",
        )
        .unwrap();
        assert!(value.to_bool());

        let value = starlark_eval_in_env(
            &mut env,
            &type_values,
            "policy.include_distribution_resources",
        )
        .unwrap();
        assert_eq!(value.get_type(), "bool");
        assert_eq!(value.to_bool(), policy.include_distribution_resources());

        let value = starlark_eval_in_env(
            &mut env,
            &type_values,
            "policy.include_distribution_resources = False; policy.include_distribution_resources",
        )
        .unwrap();
        assert!(!value.to_bool());

        let value = starlark_eval_in_env(
            &mut env,
            &type_values,
            "policy.include_distribution_resources = True; policy.include_distribution_resources",
        )
        .unwrap();
        assert!(value.to_bool());

        let value =
            starlark_eval_in_env(&mut env, &type_values, "policy.resources_policy").unwrap();
        assert_eq!(value.get_type(), "string");
        assert_eq!(
            &PythonResourcesPolicy::try_from(value.to_string().as_str()).unwrap(),
            policy.get_resources_policy()
        );
    }
}
