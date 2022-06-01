// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    super::{
        env::{get_context, PyOxidizerEnvironmentContext},
        python_executable::PythonExecutableValue,
        python_interpreter_config::PythonInterpreterConfigValue,
        python_packaging_policy::PythonPackagingPolicyValue,
        python_resource::{add_context_for_value, python_resource_to_value},
    },
    crate::py_packaging::{
        distribution::BinaryLibpythonLinkMode,
        distribution::{
            default_distribution_location, DistributionFlavor, PythonDistribution,
            PythonDistributionLocation,
        },
    },
    anyhow::{anyhow, Result},
    log::{info, warn},
    python_packaging::{
        policy::PythonPackagingPolicy, resource::PythonResource,
        resource_collection::PythonResourceAddCollectionContext,
    },
    starlark::{
        environment::TypeValues,
        eval::call_stack::CallStack,
        values::{
            error::{RuntimeError, ValueError, INCORRECT_PARAMETER_TYPE_ERROR_CODE},
            none::NoneType,
            {Mutable, TypedValue, Value, ValueResult},
        },
        {
            starlark_fun, starlark_module, starlark_parse_param_type, starlark_signature,
            starlark_signature_extraction, starlark_signatures,
        },
    },
    starlark_dialect_build_targets::{optional_str_arg, optional_type_arg},
    std::{ops::Deref, sync::Arc},
};

/// A Starlark Value wrapper for `PythonDistribution` traits.
pub struct PythonDistributionValue {
    /// Where the distribution should be obtained from.
    pub source: PythonDistributionLocation,

    /// The actual distribution.
    ///
    /// Populated on first read.
    pub distribution: Option<Arc<dyn PythonDistribution>>,
}

impl PythonDistributionValue {
    fn from_location(location: PythonDistributionLocation) -> PythonDistributionValue {
        PythonDistributionValue {
            source: location,
            distribution: None,
        }
    }

    pub fn resolve_distribution(
        &mut self,
        type_values: &TypeValues,
        label: &str,
    ) -> Result<Arc<dyn PythonDistribution>, ValueError> {
        if self.distribution.is_none() {
            let pyoxidizer_context_value = get_context(type_values)?;
            let pyoxidizer_context = pyoxidizer_context_value
                .downcast_mut::<PyOxidizerEnvironmentContext>()?
                .ok_or(ValueError::IncorrectParameterType)?;

            let dest_dir = pyoxidizer_context.python_distributions_path()?;

            self.distribution = Some(
                pyoxidizer_context
                    .distribution_cache
                    .resolve_distribution(&self.source, Some(&dest_dir))
                    .map_err(|e| {
                        ValueError::from(RuntimeError {
                            code: "PYOXIDIZER_BUILD",
                            message: format!("{:?}", e),
                            label: label.to_string(),
                        })
                    })?
                    .clone_trait(),
            );
        }

        Ok(self.distribution.as_ref().unwrap().clone())
    }
}

impl TypedValue for PythonDistributionValue {
    type Holder = Mutable<PythonDistributionValue>;
    const TYPE: &'static str = "PythonDistribution";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }

    fn to_str(&self) -> String {
        format!("PythonDistribution<{:#?}>", self.source)
    }
}

// Starlark functions.
impl PythonDistributionValue {
    /// default_python_distribution(flavor, build_target=None, python_version=None)
    fn default_python_distribution(
        type_values: &TypeValues,
        flavor: String,
        build_target: &Value,
        python_version: &Value,
    ) -> ValueResult {
        let build_target = optional_str_arg("build_target", build_target)?;
        let python_version = optional_str_arg("python_version", python_version)?;

        let pyoxidizer_context_value = get_context(type_values)?;
        let pyoxidizer_context = pyoxidizer_context_value
            .downcast_ref::<PyOxidizerEnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        let build_target = match build_target {
            Some(t) => t,
            None => pyoxidizer_context.build_target_triple.clone(),
        };

        let flavor = DistributionFlavor::try_from(flavor.as_str()).map_err(|e| {
            ValueError::from(RuntimeError {
                code: "PYOXIDIZER_BUILD",
                message: e,
                label: "default_python_distribution()".to_string(),
            })
        })?;

        let python_version_str = python_version.as_deref();

        let location = default_distribution_location(&flavor, &build_target, python_version_str)
            .map_err(|e| {
                ValueError::from(RuntimeError {
                    code: "PYOXIDIZER_BUILD",
                    message: format!("{:?}", e),
                    label: "default_python_distribution()".to_string(),
                })
            })?;

        warn!(
            "target Python distribution for {} resolves to: {}",
            build_target, location
        );

        Ok(Value::new(PythonDistributionValue::from_location(location)))
    }

    /// PythonDistribution()
    fn from_args(sha256: String, local_path: &Value, url: &Value, flavor: String) -> ValueResult {
        optional_str_arg("local_path", local_path)?;
        optional_str_arg("url", url)?;

        if local_path.get_type() != "NoneType" && url.get_type() != "NoneType" {
            return Err(ValueError::from(RuntimeError {
                code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                message: "cannot define both local_path and url".to_string(),
                label: "cannot define both local_path and url".to_string(),
            }));
        }

        let distribution = if local_path.get_type() != "NoneType" {
            PythonDistributionLocation::Local {
                local_path: local_path.to_string(),
                sha256,
            }
        } else {
            PythonDistributionLocation::Url {
                url: url.to_string(),
                sha256,
            }
        };

        match flavor.as_ref() {
            "standalone" => (),
            v => {
                return Err(ValueError::from(RuntimeError {
                    code: "PYOXIDIZER_BUILD",
                    message: format!("invalid distribution flavor {}", v),
                    label: "PythonDistribution()".to_string(),
                }))
            }
        }

        Ok(Value::new(PythonDistributionValue::from_location(
            distribution,
        )))
    }

    /// PythonDistribution.make_python_packaging_policy()
    fn make_python_packaging_policy_starlark(&mut self, type_values: &TypeValues) -> ValueResult {
        let dist = self.resolve_distribution(type_values, "resolve_distribution")?;

        let policy = dist.create_packaging_policy().map_err(|e| {
            ValueError::from(RuntimeError {
                code: "PYOXIDIZER_BUILD",
                message: format!("{:?}", e),
                label: "make_python_packaging_policy()".to_string(),
            })
        })?;

        Ok(Value::new(PythonPackagingPolicyValue::new(policy)))
    }

    /// PythonDistribution.make_python_interpreter_config()
    fn make_python_interpreter_config_starlark(&mut self, type_values: &TypeValues) -> ValueResult {
        let dist = self.resolve_distribution(type_values, "resolve_distribution()")?;

        let config = dist.create_python_interpreter_config().map_err(|e| {
            ValueError::from(RuntimeError {
                code: "PYOXIDIZER_BUILD",
                message: format!("{:?}", e),
                label: "make_python_packaging_policy()".to_string(),
            })
        })?;

        Ok(Value::new(PythonInterpreterConfigValue::new(config)))
    }

    /// PythonDistribution.to_python_executable(
    ///     name,
    ///     packaging_policy=None,
    ///     config=None,
    /// )
    #[allow(clippy::too_many_arguments, clippy::wrong_self_convention)]
    fn to_python_executable_starlark(
        &mut self,
        type_values: &TypeValues,
        call_stack: &mut CallStack,
        name: String,
        packaging_policy: &Value,
        config: &Value,
    ) -> ValueResult {
        const LABEL: &str = "PythonDistribution.to_python_executable()";

        optional_type_arg(
            "packaging_policy",
            "PythonPackagingPolicy",
            packaging_policy,
        )?;
        optional_type_arg("config", "PythonInterpreterConfig", config)?;

        let dist = self.resolve_distribution(type_values, "resolve_distribution()")?;

        let policy = if packaging_policy.get_type() == "NoneType" {
            Ok(PythonPackagingPolicyValue::new(
                dist.create_packaging_policy().map_err(|e| {
                    ValueError::from(RuntimeError {
                        code: "PYOXIDIZER_BUILD",
                        message: format!("{:?}", e),
                        label: "to_python_executable_starlark()".to_string(),
                    })
                })?,
            ))
        } else {
            match packaging_policy.downcast_ref::<PythonPackagingPolicyValue>() {
                Some(policy) => Ok(policy.clone()),
                None => Err(ValueError::IncorrectParameterType),
            }
        }?;

        let config = if config.get_type() == "NoneType" {
            Ok(PythonInterpreterConfigValue::new(
                dist.create_python_interpreter_config().map_err(|e| {
                    ValueError::from(RuntimeError {
                        code: "PYOXIDIZER_BUILD",
                        message: format!("{:?}", e),
                        label: "to_python_executable_starlark()".to_string(),
                    })
                })?,
            ))
        } else {
            match config.downcast_ref::<PythonInterpreterConfigValue>() {
                Some(c) => Ok(c.clone()),
                None => Err(ValueError::IncorrectParameterType),
            }
        }?;

        let pyoxidizer_context_value = get_context(type_values)?;
        let pyoxidizer_context = pyoxidizer_context_value
            .downcast_ref::<PyOxidizerEnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        let python_distributions_path = pyoxidizer_context.python_distributions_path()?;

        let host_distribution = if dist
            .compatible_host_triples()
            .contains(&pyoxidizer_context.build_host_triple)
        {
            warn!("reusing target Python distribution for host execution");
            Some(dist.clone())
        } else {
            info!(
                "searching for host Python {} distribution",
                dist.python_major_minor_version()
            );
            let host_dist = pyoxidizer_context
                .distribution_cache
                .host_distribution(
                    Some(dist.python_major_minor_version().as_str()),
                    Some(&python_distributions_path),
                )
                .map_err(|e| {
                    ValueError::from(RuntimeError {
                        code: "PYOXIDIZER_BUILD",
                        message: format!("{:?}", e),
                        label: "to_python_executable()".to_string(),
                    })
                })?;

            Some(host_dist.clone_trait())
        };

        let mut builder = dist
            .as_python_executable_builder(
                &pyoxidizer_context.build_host_triple,
                &pyoxidizer_context.build_target_triple,
                &name,
                // TODO make configurable
                BinaryLibpythonLinkMode::Default,
                policy.inner(LABEL)?.deref(),
                config.inner(LABEL)?.deref(),
                host_distribution,
            )
            .map_err(|e| {
                ValueError::from(RuntimeError {
                    code: "PYOXIDIZER_BUILD",
                    message: format!("{:?}", e),
                    label: "to_python_executable()".to_string(),
                })
            })?;

        let callback = Box::new(
            |_policy: &PythonPackagingPolicy,
             resource: &PythonResource,
             add_context: &mut PythonResourceAddCollectionContext|
             -> Result<()> {
                // Callback is declared Fn, so we can't take a mutable reference.
                // A copy should be fine.
                let mut cs = call_stack.clone();

                // There is a PythonPackagingPolicy passed into this callback
                // and one passed into the outer function as a &Value. The
                // former is derived from the latter. And the latter has Starlark
                // callbacks registered on it.
                //
                // When we call python_resource_to_value(), the Starlark
                // callbacks are automatically called.

                let value =
                    python_resource_to_value(LABEL, type_values, &mut cs, resource, &policy)
                        .map_err(|e| {
                            anyhow!("error converting PythonResource to Value: {:?}", e)
                        })?;

                let new_add_context = add_context_for_value(&value, "to_python_executable")
                    .map_err(|e| anyhow!("error obtaining add context from Value: {:?}", e))?
                    .expect("add context should have been populated as part of Value conversion");

                add_context.replace(&new_add_context);

                Ok(())
            },
        );

        for action in builder
            .add_distribution_resources(Some(callback))
            .map_err(|e| {
                ValueError::from(RuntimeError {
                    code: "PYOXIDIZER_BUILD",
                    message: format!("{:?}", e),
                    label: "to_python_executable()".to_string(),
                })
            })?
        {
            info!("{}", action.to_string());
        }

        Ok(Value::new(PythonExecutableValue::new(builder, policy)))
    }

    pub fn python_resources_starlark(
        &mut self,
        type_values: &TypeValues,
        call_stack: &mut CallStack,
    ) -> ValueResult {
        const LABEL: &str = "PythonDistribution.python_resources()";

        let dist = self.resolve_distribution(type_values, "resolve_distribution")?;
        let policy =
            PythonPackagingPolicyValue::new(dist.create_packaging_policy().map_err(|e| {
                ValueError::from(RuntimeError {
                    code: "PYTHON_DISTRIBUTION",
                    message: format!("{:?}", e),
                    label: LABEL.to_string(),
                })
            })?);

        let values = dist
            .python_resources()
            .iter()
            .map(|resource| {
                python_resource_to_value(LABEL, type_values, call_stack, resource, &policy)
            })
            .collect::<Result<Vec<Value>, ValueError>>()?;

        Ok(Value::from(values))
    }
}

starlark_module! { python_distribution_module =>
    #[allow(non_snake_case)]
    PythonDistribution(sha256: String, local_path=NoneType::None, url=NoneType::None, flavor: String = "standalone".to_string()) {
        PythonDistributionValue::from_args(sha256, &local_path, &url, flavor)
    }

    PythonDistribution.make_python_packaging_policy(env env, this) {
        let mut this = this.downcast_mut::<PythonDistributionValue>().unwrap().unwrap();
        this.make_python_packaging_policy_starlark(env)
    }

    PythonDistribution.make_python_interpreter_config(env env, this) {
        let mut this = this.downcast_mut::<PythonDistributionValue>().unwrap().unwrap();
        this.make_python_interpreter_config_starlark(env)
    }

    PythonDistribution.python_resources(env env, call_stack cs, this) {
        let mut this = this.downcast_mut::<PythonDistributionValue>().unwrap().unwrap();
        this.python_resources_starlark(env, cs)
    }

    PythonDistribution.to_python_executable(
        env env,
        call_stack cs,
        this,
        name: String,
        packaging_policy=NoneType::None,
        config=NoneType::None
    ) {
        let mut this = this.downcast_mut::<PythonDistributionValue>().unwrap().unwrap();
        this.to_python_executable_starlark(
            env,
            cs,
            name,
            &packaging_policy,
            &config,
        )
    }

    default_python_distribution(
        env env,
        flavor: String = "standalone".to_string(),
        build_target=NoneType::None,
        python_version=NoneType::None
    ) {
        PythonDistributionValue::default_python_distribution(env, flavor, &build_target, &python_version)
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::{
            environment::default_target_triple,
            py_packaging::distribution::DistributionFlavor,
            python_distributions::PYTHON_DISTRIBUTIONS,
            starlark::{
                python_extension_module::PythonExtensionModuleValue,
                python_module_source::PythonModuleSourceValue,
                python_package_resource::PythonPackageResourceValue, testutil::*,
            },
        },
    };

    #[test]
    fn test_default_python_distribution() {
        let dist = starlark_ok("default_python_distribution()");
        assert_eq!(dist.get_type(), "PythonDistribution");

        let host_distribution = PYTHON_DISTRIBUTIONS
            .find_distribution(
                default_target_triple(),
                &DistributionFlavor::Standalone,
                None,
            )
            .unwrap();

        let x = dist.downcast_ref::<PythonDistributionValue>().unwrap();
        assert_eq!(x.source, host_distribution.location)
    }

    #[test]
    // Python 3.8 not supported on aarch64.
    #[cfg(not(target_arch = "aarch64"))]
    fn test_default_python_distribution_python_38() -> Result<()> {
        let mut env = test_evaluation_context_builder()?.into_context()?;

        let dist = env.eval("default_python_distribution(python_version='3.8')")?;
        assert_eq!(dist.get_type(), "PythonDistribution");

        let wanted = PYTHON_DISTRIBUTIONS
            .find_distribution(
                default_target_triple(),
                &DistributionFlavor::Standalone,
                Some("3.8"),
            )
            .unwrap();

        let x = dist.downcast_ref::<PythonDistributionValue>().unwrap();
        assert_eq!(x.source, wanted.location);

        Ok(())
    }

    #[test]
    fn test_default_python_distribution_python_39() -> Result<()> {
        let mut env = test_evaluation_context_builder()?.into_context()?;

        let dist = env.eval("default_python_distribution(python_version='3.9')")?;
        assert_eq!(dist.get_type(), "PythonDistribution");

        let wanted = PYTHON_DISTRIBUTIONS
            .find_distribution(
                default_target_triple(),
                &DistributionFlavor::Standalone,
                Some("3.9"),
            )
            .unwrap();

        let x = dist.downcast_ref::<PythonDistributionValue>().unwrap();
        assert_eq!(x.source, wanted.location);

        Ok(())
    }

    #[test]
    fn test_default_python_distribution_python_310() -> Result<()> {
        let mut env = test_evaluation_context_builder()?.into_context()?;

        let dist = env.eval("default_python_distribution(python_version='3.10')")?;
        assert_eq!(dist.get_type(), "PythonDistribution");

        let wanted = PYTHON_DISTRIBUTIONS
            .find_distribution(
                default_target_triple(),
                &DistributionFlavor::Standalone,
                Some("3.10"),
            )
            .unwrap();

        let x = dist.downcast_ref::<PythonDistributionValue>().unwrap();
        assert_eq!(x.source, wanted.location);

        Ok(())
    }

    #[test]
    #[cfg(windows)]
    fn test_default_python_distribution_dynamic_windows() {
        let dist = starlark_ok("default_python_distribution(flavor='standalone_dynamic')");
        assert_eq!(dist.get_type(), "PythonDistribution");

        let host_distribution = PYTHON_DISTRIBUTIONS
            .find_distribution(
                default_target_triple(),
                &DistributionFlavor::StandaloneDynamic,
                None,
            )
            .unwrap();

        let x = dist.downcast_ref::<PythonDistributionValue>().unwrap();
        assert_eq!(x.source, host_distribution.location)
    }

    #[test]
    fn test_python_distribution_no_args() {
        let err = starlark_nok("PythonDistribution()");
        assert!(err.message.starts_with("Missing parameter sha256"));
    }

    #[test]
    fn test_python_distribution_multiple_args() {
        let err = starlark_nok(
            "PythonDistribution('sha256', url='url_value', local_path='local_path_value')",
        );
        assert_eq!(err.message, "cannot define both local_path and url");
    }

    #[test]
    fn test_python_distribution_url() {
        let dist = starlark_ok("PythonDistribution('sha256', url='some_url')");
        let wanted = PythonDistributionLocation::Url {
            url: "some_url".to_string(),
            sha256: "sha256".to_string(),
        };

        let x = dist.downcast_ref::<PythonDistributionValue>().unwrap();
        assert_eq!(x.source, wanted);
    }

    #[test]
    fn test_python_distribution_local_path() {
        let dist = starlark_ok("PythonDistribution('sha256', local_path='some_path')");
        let wanted = PythonDistributionLocation::Local {
            local_path: "some_path".to_string(),
            sha256: "sha256".to_string(),
        };

        let x = dist.downcast_ref::<PythonDistributionValue>().unwrap();
        assert_eq!(x.source, wanted);
    }

    #[test]
    fn test_make_python_packaging_policy() {
        let policy = starlark_ok("default_python_distribution().make_python_packaging_policy()");
        assert_eq!(policy.get_type(), "PythonPackagingPolicy");
    }

    #[test]
    fn test_make_python_interpreter_config() {
        let config = starlark_ok("default_python_distribution().make_python_interpreter_config()");
        assert_eq!(config.get_type(), "PythonInterpreterConfig");
    }

    #[test]
    fn test_python_resources() {
        let resources = starlark_ok("default_python_distribution().python_resources()");
        assert_eq!(resources.get_type(), "list");

        let values = resources.iter().unwrap().to_vec();

        assert!(values.len() > 100);

        assert!(values
            .iter()
            .any(|v| v.get_type() == PythonModuleSourceValue::TYPE));
        assert!(values
            .iter()
            .any(|v| v.get_type() == PythonExtensionModuleValue::TYPE));
        assert!(values
            .iter()
            .any(|v| v.get_type() == PythonPackageResourceValue::TYPE));

        assert!(values
            .iter()
            .filter(|v| v.get_type() == PythonModuleSourceValue::TYPE)
            .all(|v| v.get_attr("is_stdlib").unwrap().to_bool()));
        assert!(values
            .iter()
            .filter(|v| v.get_type() == PythonExtensionModuleValue::TYPE)
            .all(|v| v.get_attr("is_stdlib").unwrap().to_bool()));
        assert!(values
            .iter()
            .filter(|v| v.get_type() == PythonPackageResourceValue::TYPE)
            .all(|v| v.get_attr("is_stdlib").unwrap().to_bool()));
    }
}
