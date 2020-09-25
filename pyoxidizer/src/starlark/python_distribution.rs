// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    super::env::{get_context, EnvironmentContext},
    super::python_executable::PythonExecutable,
    super::python_packaging_policy::PythonPackagingPolicyValue,
    super::python_resource::{
        PythonExtensionModuleValue, PythonPackageResourceValue, PythonSourceModuleValue,
    },
    super::util::{optional_str_arg, optional_type_arg, required_bool_arg, required_str_arg},
    crate::py_packaging::config::EmbeddedPythonConfig,
    crate::py_packaging::distribution::BinaryLibpythonLinkMode,
    crate::py_packaging::distribution::{
        default_distribution_location, is_stdlib_test_package, resolve_distribution,
        DistributionFlavor, PythonDistribution as PythonDistributionTrait,
        PythonDistributionLocation,
    },
    anyhow::{anyhow, Result},
    itertools::Itertools,
    python_packaging::bytecode::{CompileMode, PythonBytecodeCompiler},
    python_packaging::resource::BytecodeOptimizationLevel,
    starlark::environment::TypeValues,
    starlark::values::error::{RuntimeError, ValueError, INCORRECT_PARAMETER_TYPE_ERROR_CODE},
    starlark::values::none::NoneType,
    starlark::values::{Mutable, TypedValue, Value, ValueResult},
    starlark::{
        starlark_fun, starlark_module, starlark_parse_param_type, starlark_signature,
        starlark_signature_extraction, starlark_signatures,
    },
    std::path::{Path, PathBuf},
    std::sync::Arc,
};

pub struct PythonDistribution {
    flavor: DistributionFlavor,
    pub source: PythonDistributionLocation,

    dest_dir: PathBuf,

    pub distribution: Option<Arc<Box<dyn PythonDistributionTrait>>>,

    compiler: Option<Box<dyn PythonBytecodeCompiler>>,
}

impl PythonDistribution {
    fn from_location(
        flavor: DistributionFlavor,
        location: PythonDistributionLocation,
        dest_dir: &Path,
    ) -> PythonDistribution {
        PythonDistribution {
            flavor,
            source: location,
            dest_dir: dest_dir.to_path_buf(),
            distribution: None,
            compiler: None,
        }
    }

    pub fn ensure_distribution_resolved(&mut self, logger: &slog::Logger) -> Result<()> {
        if self.distribution.is_some() {
            return Ok(());
        }

        let dist = resolve_distribution(logger, &self.flavor, &self.source, &self.dest_dir)?;
        //warn!(logger, "distribution info: {:#?}", dist.as_minimal_info());

        self.distribution = Some(Arc::new(dist));

        Ok(())
    }

    /// Compile bytecode using this distribution.
    ///
    /// A bytecode compiler will be lazily instantiated and preserved for the
    /// lifetime of the instance. So calling multiple times does not pay a
    /// recurring performance penalty for instantiating the bytecode compiler.
    pub fn compile_bytecode(
        &mut self,
        logger: &slog::Logger,
        source: &[u8],
        filename: &str,
        optimize: BytecodeOptimizationLevel,
        output_mode: CompileMode,
    ) -> Result<Vec<u8>> {
        self.ensure_distribution_resolved(logger)?;

        if let Some(dist) = &self.distribution {
            if self.compiler.is_none() {
                self.compiler = Some(dist.create_bytecode_compiler()?);
            }
        }

        if let Some(compiler) = &mut self.compiler {
            compiler.compile(source, filename, optimize, output_mode)
        } else {
            Err(anyhow!("bytecode compiler should exist"))
        }
    }
}

impl TypedValue for PythonDistribution {
    type Holder = Mutable<PythonDistribution>;
    const TYPE: &'static str = "PythonDistribution";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
    }

    fn to_str(&self) -> String {
        format!("PythonDistribution<{:#?}>", self.source)
    }
}

// Starlark functions.
impl PythonDistribution {
    /// default_python_distribution(flavor, build_target=None)
    fn default_python_distribution(
        type_values: &TypeValues,
        flavor: &Value,
        build_target: &Value,
    ) -> ValueResult {
        let flavor = required_str_arg("flavor", flavor)?;
        let build_target = optional_str_arg("build_target", build_target)?;

        let raw_context = get_context(type_values)?;
        let context = raw_context
            .downcast_ref::<EnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        let build_target = match build_target {
            Some(t) => t,
            None => context.build_target_triple.clone(),
        };

        let flavor = match flavor.as_ref() {
            "standalone" => DistributionFlavor::Standalone,
            "standalone_static" => DistributionFlavor::StandaloneStatic,
            "standalone_dynamic" => DistributionFlavor::StandaloneDynamic,
            v => {
                return Err(ValueError::from(RuntimeError {
                    code: "PYOXIDIZER_BUILD",
                    message: format!("unknown distribution flavor {}", v),
                    label: "default_python_distribution()".to_string(),
                }))
            }
        };

        let location = default_distribution_location(&flavor, &build_target).map_err(|e| {
            ValueError::from(RuntimeError {
                code: "PYOXIDIZER_BUILD",
                message: e.to_string(),
                label: "default_python_distribution()".to_string(),
            })
        })?;

        let raw_context = get_context(type_values)?;
        let context = raw_context
            .downcast_ref::<EnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        Ok(Value::new(PythonDistribution::from_location(
            flavor,
            location,
            &context.python_distributions_path,
        )))
    }

    /// PythonDistribution()
    fn from_args(
        type_values: &TypeValues,
        sha256: &Value,
        local_path: &Value,
        url: &Value,
        flavor: &Value,
    ) -> ValueResult {
        required_str_arg("sha256", sha256)?;
        optional_str_arg("local_path", local_path)?;
        optional_str_arg("url", url)?;
        let flavor = required_str_arg("flavor", flavor)?;

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
                sha256: sha256.to_string(),
            }
        } else {
            PythonDistributionLocation::Url {
                url: url.to_string(),
                sha256: sha256.to_string(),
            }
        };

        let flavor = match flavor.as_ref() {
            "standalone" => DistributionFlavor::Standalone,
            v => {
                return Err(ValueError::from(RuntimeError {
                    code: "PYOXIDIZER_BUILD",
                    message: format!("invalid distribution flavor {}", v),
                    label: "PythonDistribution()".to_string(),
                }))
            }
        };

        let raw_context = get_context(type_values)?;
        let context = raw_context
            .downcast_ref::<EnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        Ok(Value::new(PythonDistribution::from_location(
            flavor,
            distribution,
            &context.python_distributions_path,
        )))
    }

    /// PythonDistribution.make_python_packaging_policy()
    fn make_python_packaging_policy_starlark(&mut self, type_values: &TypeValues) -> ValueResult {
        let raw_context = get_context(type_values)?;
        let context = raw_context
            .downcast_ref::<EnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        self.ensure_distribution_resolved(&context.logger)
            .map_err(|e| {
                ValueError::from(RuntimeError {
                    code: "PYOXIDIZER_BUILD",
                    message: e.to_string(),
                    label: "resolve_distribution()".to_string(),
                })
            })?;
        let dist = self.distribution.as_ref().unwrap().clone();

        let policy = dist.create_packaging_policy().map_err(|e| {
            ValueError::from(RuntimeError {
                code: "PYOXIDIZER_BUILD",
                message: e.to_string(),
                label: "make_python_packaging_policy()".to_string(),
            })
        })?;

        Ok(Value::new(PythonPackagingPolicyValue { inner: policy }))
    }

    /// PythonDistribution.to_python_executable(
    ///     name,
    ///     packaging_policy=None,
    ///     config=None,
    /// )
    #[allow(
        clippy::ptr_arg,
        clippy::too_many_arguments,
        clippy::clippy::wrong_self_convention
    )]
    fn to_python_executable_starlark(
        &mut self,
        type_values: &TypeValues,
        name: &Value,
        packaging_policy: &Value,
        config: &Value,
    ) -> ValueResult {
        let name = required_str_arg("name", &name)?;
        optional_type_arg(
            "packaging_policy",
            "PythonPackagingPolicy",
            &packaging_policy,
        )?;
        optional_type_arg("config", "PythonInterpreterConfig", &config)?;

        let raw_context = get_context(type_values)?;
        let context = raw_context
            .downcast_ref::<EnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        self.ensure_distribution_resolved(&context.logger)
            .map_err(|e| {
                ValueError::from(RuntimeError {
                    code: "PYOXIDIZER_BUILD",
                    message: e.to_string(),
                    label: "resolve_distribution()".to_string(),
                })
            })?;
        let dist = self.distribution.as_ref().unwrap().clone();

        let policy = if packaging_policy.get_type() == "NoneType" {
            Ok(dist.create_packaging_policy().map_err(|e| {
                ValueError::from(RuntimeError {
                    code: "PYOXIDIZER_BUILD",
                    message: e.to_string(),
                    label: "resolve_distribution()".to_string(),
                })
            })?)
        } else {
            match packaging_policy.downcast_ref::<PythonPackagingPolicyValue>() {
                Some(policy) => Ok(policy.inner.clone()),
                None => Err(ValueError::IncorrectParameterType),
            }
        }?;

        let config = if config.get_type() == "NoneType" {
            Ok(EmbeddedPythonConfig::default_starlark())
        } else {
            match config.downcast_ref::<EmbeddedPythonConfig>() {
                Some(c) => Ok(c.clone()),
                None => Err(ValueError::IncorrectParameterType),
            }
        }?;

        Ok(Value::new(PythonExecutable {
            exe: dist
                .as_python_executable_builder(
                    &context.logger,
                    &context.build_host_triple,
                    &context.build_target_triple,
                    &name,
                    // TODO make configurable
                    BinaryLibpythonLinkMode::Default,
                    &policy,
                    &config,
                )
                .map_err(|e| {
                    ValueError::from(RuntimeError {
                        code: "PYOXIDIZER_BUILD",
                        message: e.to_string(),
                        label: "to_python_executable()".to_string(),
                    })
                })?,
        }))
    }

    /// PythonDistribution.extension_modules()
    pub fn extension_modules(&mut self, type_values: &TypeValues) -> ValueResult {
        let raw_context = get_context(type_values)?;
        let context = raw_context
            .downcast_ref::<EnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        self.ensure_distribution_resolved(&context.logger)
            .map_err(|e| {
                ValueError::from(RuntimeError {
                    code: "PYOXIDIZER_BUILD",
                    message: e.to_string(),
                    label: "resolve_distribution()".to_string(),
                })
            })?;

        Ok(Value::from(
            self.distribution
                .as_ref()
                .unwrap()
                .iter_extension_modules()
                .map(|em| Value::new(PythonExtensionModuleValue { inner: em.clone() }))
                .collect_vec(),
        ))
    }

    /// PythonDistribution.package_resources(include_test=false)
    pub fn package_resources(
        &mut self,
        type_values: &TypeValues,
        include_test: &Value,
    ) -> ValueResult {
        let include_test = required_bool_arg("include_test", &include_test)?;

        let raw_context = get_context(type_values)?;
        let context = raw_context
            .downcast_ref::<EnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        self.ensure_distribution_resolved(&context.logger)
            .map_err(|e| {
                ValueError::from(RuntimeError {
                    code: "PYOXIDIZER_BUILD",
                    message: e.to_string(),
                    label: "resolve_distribution()".to_string(),
                })
            })?;

        let resources = self
            .distribution
            .as_ref()
            .unwrap()
            .resource_datas()
            .map_err(|e| {
                ValueError::from(RuntimeError {
                    code: "PYTHON_DISTRIBUTION",
                    message: e.to_string(),
                    label: e.to_string(),
                })
            })?;

        Ok(Value::from(
            resources
                .iter()
                .filter_map(|data| {
                    if !include_test && is_stdlib_test_package(&data.leaf_package) {
                        None
                    } else {
                        Some(Value::new(PythonPackageResourceValue {
                            inner: data.clone(),
                        }))
                    }
                })
                .collect_vec(),
        ))
    }

    /// PythonDistribution.source_modules()
    pub fn source_modules(&mut self, type_values: &TypeValues) -> ValueResult {
        let raw_context = get_context(type_values)?;
        let context = raw_context
            .downcast_ref::<EnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        self.ensure_distribution_resolved(&context.logger)
            .map_err(|e| {
                ValueError::from(RuntimeError {
                    code: "PYOXIDIZER_BUILD",
                    message: e.to_string(),
                    label: "resolve_distribution()".to_string(),
                })
            })?;

        let modules = self
            .distribution
            .as_ref()
            .unwrap()
            .source_modules()
            .map_err(|e| {
                ValueError::from(RuntimeError {
                    code: "PYTHON_DISTRIBUTION",
                    message: e.to_string(),
                    label: e.to_string(),
                })
            })?;

        Ok(Value::from(
            modules
                .iter()
                .map(|module| Value::new(PythonSourceModuleValue::new(module.clone())))
                .collect_vec(),
        ))
    }
}

starlark_module! { python_distribution_module =>
    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonDistribution(env env, sha256, local_path=NoneType::None, url=NoneType::None, flavor="standalone") {
        PythonDistribution::from_args(&env, &sha256, &local_path, &url, &flavor)
    }

    PythonDistribution.make_python_packaging_policy(env env, this) {
        match this.clone().downcast_mut::<PythonDistribution>()? {
            Some(mut dist) => dist.make_python_packaging_policy_starlark(&env),
            None => Err(ValueError::IncorrectParameterType),
        }
    }

    #[allow(clippy::ptr_arg)]
    PythonDistribution.extension_modules(env env, this) {
        match this.clone().downcast_mut::<PythonDistribution>()? {
            Some(mut dist) => dist.extension_modules(&env),
            None => Err(ValueError::IncorrectParameterType),
        }
    }

    #[allow(clippy::ptr_arg)]
    PythonDistribution.source_modules(env env, this) {
        match this.clone().downcast_mut::<PythonDistribution>()? {
            Some(mut dist) => dist.source_modules(&env),
            None => Err(ValueError::IncorrectParameterType),
        }
    }

    #[allow(clippy::ptr_arg)]
    PythonDistribution.package_resources(env env, this, include_test=false) {
        match this.clone().downcast_mut::<PythonDistribution>()? {
            Some(mut dist) => dist.package_resources(&env, &include_test),
            None => Err(ValueError::IncorrectParameterType),
        }
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonDistribution.to_python_executable(
        env env,
        this,
        name,
        packaging_policy=NoneType::None,
        config=NoneType::None
    ) {
        match this.clone().downcast_mut::<PythonDistribution>()? {
            Some(mut dist) =>dist.to_python_executable_starlark(
                &env,
                &name,
                &packaging_policy,
                &config,
            ),
            None => Err(ValueError::IncorrectParameterType),
        }
    }

    #[allow(clippy::ptr_arg)]
    default_python_distribution(env env, flavor="standalone", build_target=NoneType::None) {
        PythonDistribution::default_python_distribution(&env, &flavor, &build_target)
    }
}

#[cfg(test)]
mod tests {
    use {
        super::super::testutil::*, super::*, crate::py_packaging::distribution::DistributionFlavor,
        crate::python_distributions::PYTHON_DISTRIBUTIONS,
    };

    #[test]
    fn test_default_python_distribution() {
        let dist = starlark_ok("default_python_distribution()");
        assert_eq!(dist.get_type(), "PythonDistribution");

        let host_distribution = PYTHON_DISTRIBUTIONS
            .find_distribution(
                crate::project_building::HOST,
                &DistributionFlavor::Standalone,
            )
            .unwrap();

        let x = dist.downcast_ref::<PythonDistribution>().unwrap();
        assert_eq!(x.source, host_distribution.location)
    }

    #[test]
    fn test_default_python_distribution_bad_arg() {
        let err = starlark_nok("default_python_distribution(False)");
        assert_eq!(
            err.message,
            "function expects a string for flavor; got type bool"
        );
    }

    #[test]
    #[cfg(windows)]
    fn test_default_python_distribution_dynamic_windows() {
        let dist = starlark_ok("default_python_distribution(flavor='standalone_dynamic')");
        assert_eq!(dist.get_type(), "PythonDistribution");

        let host_distribution = PYTHON_DISTRIBUTIONS
            .find_distribution(
                crate::project_building::HOST,
                &DistributionFlavor::StandaloneDynamic,
            )
            .unwrap();

        let x = dist.downcast_ref::<PythonDistribution>().unwrap();
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

        let x = dist.downcast_ref::<PythonDistribution>().unwrap();
        assert_eq!(x.source, wanted);
        assert_eq!(x.flavor, DistributionFlavor::Standalone);
    }

    #[test]
    fn test_python_distribution_local_path() {
        let dist = starlark_ok("PythonDistribution('sha256', local_path='some_path')");
        let wanted = PythonDistributionLocation::Local {
            local_path: "some_path".to_string(),
            sha256: "sha256".to_string(),
        };

        let x = dist.downcast_ref::<PythonDistribution>().unwrap();
        assert_eq!(x.source, wanted);
        assert_eq!(x.flavor, DistributionFlavor::Standalone);
    }

    #[test]
    fn test_make_python_packaging_policy() {
        let policy = starlark_ok("default_python_distribution().make_python_packaging_policy()");
        assert_eq!(policy.get_type(), "PythonPackagingPolicy");
    }

    #[test]
    fn test_source_modules() {
        let mods = starlark_ok("default_python_distribution().source_modules()");
        assert_eq!(mods.get_type(), "list");
    }

    #[test]
    fn test_package_resources() {
        let data_default = starlark_ok("default_python_distribution().package_resources()");
        let data_tests =
            starlark_ok("default_python_distribution().package_resources(include_test=True)");

        let default_length = data_default.length().unwrap();
        let data_length = data_tests.length().unwrap();

        assert!(default_length < data_length);
    }
}
