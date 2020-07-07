// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    super::env::EnvironmentContext,
    super::python_executable::PythonExecutable,
    super::python_resource::{
        PythonExtensionModule, PythonExtensionModuleFlavor, PythonPackageResource,
        PythonSourceModule,
    },
    super::util::{
        optional_dict_arg, optional_str_arg, optional_type_arg, required_bool_arg, required_str_arg,
    },
    crate::py_packaging::config::EmbeddedPythonConfig,
    crate::py_packaging::distribution::BinaryLibpythonLinkMode,
    crate::py_packaging::distribution::{
        default_distribution_location, is_stdlib_test_package, resolve_distribution,
        DistributionFlavor, ExtensionModuleFilter, PythonDistribution as PythonDistributionTrait,
        PythonDistributionLocation,
    },
    anyhow::{anyhow, Result},
    itertools::Itertools,
    python_packaging::bytecode::{BytecodeCompiler, CompileMode},
    python_packaging::resource::BytecodeOptimizationLevel,
    python_packaging::resource_collection::PythonResourcesPolicy,
    starlark::environment::Environment,
    starlark::values::{
        default_compare, RuntimeError, TypedValue, Value, ValueError, ValueResult,
        INCORRECT_PARAMETER_TYPE_ERROR_CODE,
    },
    starlark::{
        any, immutable, not_supported, starlark_fun, starlark_module, starlark_signature,
        starlark_signature_extraction, starlark_signatures,
    },
    std::any::Any,
    std::cmp::Ordering,
    std::collections::HashMap,
    std::convert::TryFrom,
    std::path::{Path, PathBuf},
    std::sync::Arc,
};

pub struct PythonDistribution {
    flavor: DistributionFlavor,
    pub source: PythonDistributionLocation,

    dest_dir: PathBuf,

    pub distribution: Option<Arc<Box<dyn PythonDistributionTrait>>>,

    compiler: Option<BytecodeCompiler>,
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
    immutable!();
    any!();
    not_supported!(binop);
    not_supported!(container);
    not_supported!(function);
    not_supported!(get_hash);
    not_supported!(to_int);

    fn to_str(&self) -> String {
        format!("PythonDistribution<{:#?}>", self.source)
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_type(&self) -> &'static str {
        "PythonDistribution"
    }

    fn to_bool(&self) -> bool {
        true
    }

    fn compare(&self, other: &dyn TypedValue, _recursion: u32) -> Result<Ordering, ValueError> {
        default_compare(self, other)
    }
}

// Starlark functions.
impl PythonDistribution {
    /// default_python_distribution(flavor, build_target=None)
    fn default_python_distribution(
        env: &Environment,
        flavor: &Value,
        build_target: &Value,
    ) -> ValueResult {
        let flavor = required_str_arg("flavor", flavor)?;
        let build_target = optional_str_arg("build_target", build_target)?;

        let build_target = match build_target {
            Some(t) => t,
            None => env.get("BUILD_TARGET_TRIPLE").unwrap().to_string(),
        };

        let flavor = match flavor.as_ref() {
            "standalone" => DistributionFlavor::Standalone,
            "standalone_static" => DistributionFlavor::StandaloneStatic,
            "standalone_dynamic" => DistributionFlavor::StandaloneDynamic,
            v => {
                return Err(RuntimeError {
                    code: "PYOXIDIZER_BUILD",
                    message: format!("unknown distribution flavor {}", v),
                    label: "default_python_distribution()".to_string(),
                }
                .into())
            }
        };

        let location = default_distribution_location(&flavor, &build_target).or_else(|e| {
            Err(RuntimeError {
                code: "PYOXIDIZER_BUILD",
                message: e.to_string(),
                label: "default_python_distribution()".to_string(),
            }
            .into())
        })?;

        let context = env.get("CONTEXT").expect("CONTEXT not defined");
        let dest_dir =
            context.downcast_apply(|x: &EnvironmentContext| x.python_distributions_path.clone());

        Ok(Value::new(PythonDistribution::from_location(
            flavor, location, &dest_dir,
        )))
    }

    /// PythonDistribution()
    fn from_args(
        env: &Environment,
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
            return Err(RuntimeError {
                code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                message: "cannot define both local_path and url".to_string(),
                label: "cannot define both local_path and url".to_string(),
            }
            .into());
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
                return Err(RuntimeError {
                    code: "PYOXIDIZER_BUILD",
                    message: format!("invalid distribution flavor {}", v),
                    label: "PythonDistribution()".to_string(),
                }
                .into())
            }
        };

        let context = env.get("CONTEXT").expect("CONTEXT not defined");
        let dest_dir =
            context.downcast_apply(|x: &EnvironmentContext| x.python_distributions_path.clone());

        Ok(Value::new(PythonDistribution::from_location(
            flavor,
            distribution,
            &dest_dir,
        )))
    }

    /// PythonDistribution.to_python_executable(
    ///     name,
    ///     resources_policy="in-memory-only",
    ///     config=None,
    ///     extension_module_filter="all",
    ///     preferred_extension_module_variants=None,
    ///     include_sources=true,
    ///     include_resources=true,
    ///     include_test=false,
    /// )
    #[allow(clippy::ptr_arg, clippy::too_many_arguments)]
    fn to_python_executable_starlark(
        &mut self,
        env: Environment,
        call_stack: &Vec<(String, String)>,
        name: &Value,
        resources_policy: &Value,
        config: &Value,
        extension_module_filter: &Value,
        preferred_extension_module_variants: &Value,
        include_sources: &Value,
        include_resources: &Value,
        include_test: &Value,
    ) -> ValueResult {
        let name = required_str_arg("name", &name)?;
        let resources_policy = required_str_arg("resources_policy", &resources_policy)?;
        optional_type_arg("config", "PythonInterpreterConfig", &config)?;
        let extension_module_filter =
            required_str_arg("extension_module_filter", &extension_module_filter)?;
        optional_dict_arg(
            "preferred_extension_module_variants",
            "string",
            "string",
            &preferred_extension_module_variants,
        )?;
        let include_sources = required_bool_arg("include_sources", &include_sources)?;
        let include_resources = required_bool_arg("include_resources", &include_resources)?;
        let include_test = required_bool_arg("include_test", &include_test)?;

        let context = env.get("CONTEXT").expect("CONTEXT not defined");
        let logger = context.downcast_apply(|x: &EnvironmentContext| x.logger.clone());
        let (host_triple, target_triple) = context.downcast_apply(|x: &EnvironmentContext| {
            (x.build_host_triple.clone(), x.build_target_triple.clone())
        });

        let resources_policy =
            PythonResourcesPolicy::try_from(resources_policy.as_str()).or_else(|e| {
                Err(RuntimeError {
                    code: "PYOXIDIZER_BUILD",
                    message: e.to_string(),
                    label: "resources_policy".to_string(),
                }
                .into())
            })?;

        let extension_module_filter =
            ExtensionModuleFilter::try_from(extension_module_filter.as_str()).or_else(|e| {
                Err(RuntimeError {
                    code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                    message: e,
                    label: "invalid policy value".to_string(),
                }
                .into())
            })?;

        let preferred_extension_module_variants =
            match preferred_extension_module_variants.get_type() {
                "NoneType" => None,
                "dict" => {
                    let mut m = HashMap::new();

                    for k in preferred_extension_module_variants.into_iter()? {
                        let v = preferred_extension_module_variants
                            .at(k.clone())?
                            .to_string();
                        m.insert(k.to_string(), v);
                    }

                    Some(m)
                }
                _ => panic!("type should have been validated above"),
            };

        self.ensure_distribution_resolved(&logger).or_else(|e| {
            Err(RuntimeError {
                code: "PYOXIDIZER_BUILD",
                message: e.to_string(),
                label: "resolve_distribution()".to_string(),
            }
            .into())
        })?;
        let dist = self.distribution.as_ref().unwrap().clone();

        let config = if config.get_type() == "NoneType" {
            let v = env
                .get("PythonInterpreterConfig")
                .expect("PythonInterpreterConfig not defined");
            v.call(call_stack, env, Vec::new(), HashMap::new(), None, None)?
                .downcast_apply(|c: &EmbeddedPythonConfig| c.clone())
        } else {
            config.downcast_apply(|c: &EmbeddedPythonConfig| c.clone())
        };

        Ok(Value::new(PythonExecutable {
            exe: dist
                .as_python_executable_builder(
                    &logger,
                    &host_triple,
                    &target_triple,
                    &name,
                    // TODO make configurable
                    BinaryLibpythonLinkMode::Default,
                    &resources_policy,
                    &config,
                    &extension_module_filter,
                    preferred_extension_module_variants,
                    include_sources,
                    include_resources,
                    include_test,
                )
                .or_else(|e| {
                    Err(RuntimeError {
                        code: "PYOXIDIZER_BUILD",
                        message: e.to_string(),
                        label: "to_python_executable()".to_string(),
                    }
                    .into())
                })?,
        }))
    }

    /// PythonDistribution.extension_modules(filter="all", preferred_variants=None)
    pub fn extension_modules(
        &mut self,
        env: &Environment,
        filter: &Value,
        preferred_variants: &Value,
    ) -> ValueResult {
        let filter = required_str_arg("filter", &filter)?;
        optional_dict_arg(
            "preferred_variants",
            "string",
            "string",
            &preferred_variants,
        )?;

        let filter = ExtensionModuleFilter::try_from(filter.as_str()).or_else(|e| {
            Err(RuntimeError {
                code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                message: e,
                label: "invalid policy value".to_string(),
            }
            .into())
        })?;

        let preferred_variants = match preferred_variants.get_type() {
            "NoneType" => None,
            "dict" => {
                let mut m = HashMap::new();

                for k in preferred_variants.into_iter()? {
                    let v = preferred_variants.at(k.clone())?.to_string();
                    m.insert(k.to_string(), v);
                }

                Some(m)
            }
            _ => panic!("type should have been validated above"),
        };

        let context = env.get("CONTEXT").expect("CONTEXT not defined");

        let logger = context.downcast_apply(|x: &EnvironmentContext| x.logger.clone());

        self.ensure_distribution_resolved(&logger).or_else(|e| {
            Err(RuntimeError {
                code: "PYOXIDIZER_BUILD",
                message: e.to_string(),
                label: "resolve_distribution()".to_string(),
            }
            .into())
        })?;

        Ok(Value::from(
            self.distribution
                .as_ref()
                .unwrap()
                .filter_extension_modules(&logger, &filter, preferred_variants)
                .or_else(|e| {
                    Err(RuntimeError {
                        code: "PYOXIDIZER_BUILD",
                        message: e.to_string(),
                        label: "extension_modules()".to_string(),
                    }
                    .into())
                })?
                .iter()
                .map(|em| {
                    Value::new(PythonExtensionModule {
                        em: PythonExtensionModuleFlavor::Distribution(em.clone()),
                    })
                })
                .collect_vec(),
        ))
    }

    /// PythonDistribution.package_resources(include_test=false)
    pub fn package_resources(&mut self, env: &Environment, include_test: &Value) -> ValueResult {
        let include_test = required_bool_arg("include_test", &include_test)?;

        let context = env.get("CONTEXT").expect("CONTEXT not defined");

        let logger = context.downcast_apply(|x: &EnvironmentContext| x.logger.clone());

        self.ensure_distribution_resolved(&logger).or_else(|e| {
            Err(RuntimeError {
                code: "PYOXIDIZER_BUILD",
                message: e.to_string(),
                label: "resolve_distribution()".to_string(),
            }
            .into())
        })?;

        let resources = self
            .distribution
            .as_ref()
            .unwrap()
            .resource_datas()
            .or_else(|e| {
                Err(RuntimeError {
                    code: "PYTHON_DISTRIBUTION",
                    message: e.to_string(),
                    label: e.to_string(),
                }
                .into())
            })?;

        Ok(Value::from(
            resources
                .iter()
                .filter_map(|data| {
                    if !include_test && is_stdlib_test_package(&data.leaf_package) {
                        None
                    } else {
                        Some(Value::new(PythonPackageResource { data: data.clone() }))
                    }
                })
                .collect_vec(),
        ))
    }

    /// PythonDistribution.source_modules()
    pub fn source_modules(&mut self, env: &Environment) -> ValueResult {
        let context = env.get("CONTEXT").expect("CONTEXT not defined");

        let logger = context.downcast_apply(|x: &EnvironmentContext| x.logger.clone());

        self.ensure_distribution_resolved(&logger).or_else(|e| {
            Err(RuntimeError {
                code: "PYOXIDIZER_BUILD",
                message: e.to_string(),
                label: "resolve_distribution()".to_string(),
            }
            .into())
        })?;

        let modules = self
            .distribution
            .as_ref()
            .unwrap()
            .source_modules()
            .or_else(|e| {
                Err(RuntimeError {
                    code: "PYTHON_DISTRIBUTION",
                    message: e.to_string(),
                    label: e.to_string(),
                }
                .into())
            })?;

        Ok(Value::from(
            modules
                .iter()
                .map(|module| {
                    Value::new(PythonSourceModule {
                        module: module.clone(),
                    })
                })
                .collect_vec(),
        ))
    }
}

starlark_module! { python_distribution_module =>
    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonDistribution(env env, sha256, local_path=None, url=None, flavor="standalone") {
        PythonDistribution::from_args(&env, &sha256, &local_path, &url, &flavor)
    }

    #[allow(clippy::ptr_arg)]
    PythonDistribution.extension_modules(env env, this, filter="all", preferred_variants=None) {
        this.downcast_apply_mut(|dist: &mut PythonDistribution| {
            dist.extension_modules(&env, &filter, &preferred_variants)
        })
    }

    #[allow(clippy::ptr_arg)]
    PythonDistribution.source_modules(env env, this) {
        this.downcast_apply_mut(|dist: &mut PythonDistribution| {
            dist.source_modules(&env)
        })
    }

    #[allow(clippy::ptr_arg)]
    PythonDistribution.package_resources(env env, this, include_test=false) {
        this.downcast_apply_mut(|dist: &mut PythonDistribution| {
            dist.package_resources(&env, &include_test)
        })
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonDistribution.to_python_executable(
        env env,
        call_stack call_stack,
        this,
        name,
        resources_policy="in-memory-only",
        config=None,
        extension_module_filter="all",
        preferred_extension_module_variants=None,
        include_sources=true,
        include_resources=false,
        include_test=false
    ) {
        this.downcast_apply_mut(|dist: &mut PythonDistribution| {
            dist.to_python_executable_starlark(
                env.clone(),
                call_stack,
                &name,
                &resources_policy,
                &config,
                &extension_module_filter,
                &preferred_extension_module_variants,
                &include_sources,
                &include_resources,
                &include_test,
            )
        })
    }

    #[allow(clippy::ptr_arg)]
    default_python_distribution(env env, flavor="standalone", build_target=None) {
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

        dist.downcast_apply(|x: &PythonDistribution| {
            assert_eq!(x.source, host_distribution.location)
        });
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

        dist.downcast_apply(|x: &PythonDistribution| {
            assert_eq!(x.source, host_distribution.location)
        });
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

        dist.downcast_apply(|x: &PythonDistribution| {
            assert_eq!(x.source, wanted);
            assert_eq!(x.flavor, DistributionFlavor::Standalone);
        });
    }

    #[test]
    fn test_python_distribution_local_path() {
        let dist = starlark_ok("PythonDistribution('sha256', local_path='some_path')");
        let wanted = PythonDistributionLocation::Local {
            local_path: "some_path".to_string(),
            sha256: "sha256".to_string(),
        };

        dist.downcast_apply(|x: &PythonDistribution| {
            assert_eq!(x.source, wanted);
            assert_eq!(x.flavor, DistributionFlavor::Standalone);
        });
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
