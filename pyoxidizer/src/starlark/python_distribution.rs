// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    super::env::EnvironmentContext,
    super::python_resource::{
        PythonExtensionModule, PythonExtensionModuleFlavor, PythonResourceData, PythonSourceModule,
    },
    super::util::{
        optional_dict_arg, optional_list_arg, optional_str_arg, optional_type_arg,
        required_bool_arg, required_list_arg, required_str_arg,
    },
    crate::py_packaging::binary::{EmbeddedPythonBinaryData, PreBuiltPythonExecutable},
    crate::py_packaging::bytecode::{BytecodeCompiler, CompileMode},
    crate::py_packaging::config::EmbeddedPythonConfig,
    crate::py_packaging::distribution::{
        is_stdlib_test_package, resolve_parsed_distribution, ExtensionModuleFilter,
        ParsedPythonDistribution, PythonDistributionLocation,
    },
    crate::py_packaging::packaging_tool::{
        find_resources, pip_install as raw_pip_install, read_virtualenv as raw_read_virtualenv,
        setup_py_install as raw_setup_py_install,
    },
    crate::py_packaging::resource::BytecodeOptimizationLevel,
    crate::python_distributions::CPYTHON_BY_TRIPLE,
    anyhow::{anyhow, Result},
    itertools::Itertools,
    slog::warn,
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

#[derive(Debug)]
pub struct PythonDistribution {
    pub source: PythonDistributionLocation,

    dest_dir: PathBuf,

    pub distribution: Option<Arc<ParsedPythonDistribution>>,

    compiler: Option<BytecodeCompiler>,
}

impl PythonDistribution {
    fn from_location(location: PythonDistributionLocation, dest_dir: &Path) -> PythonDistribution {
        PythonDistribution {
            source: location,
            dest_dir: dest_dir.to_path_buf(),
            distribution: None,
            compiler: None,
        }
    }

    pub fn ensure_distribution_resolved(&mut self, logger: &slog::Logger) {
        if self.distribution.is_some() {
            return;
        }

        let dist = resolve_parsed_distribution(logger, &self.source, &self.dest_dir).unwrap();
        warn!(logger, "distribution info: {:#?}", dist.as_minimal_info());

        self.distribution = Some(Arc::new(dist));
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
        self.ensure_distribution_resolved(logger);

        if let Some(dist) = &self.distribution {
            if self.compiler.is_none() {
                let compiler = BytecodeCompiler::new(&dist.python_exe)?;
                self.compiler = Some(compiler);
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
    /// default_python_distribution(build_target=None)
    fn default_python_distribution(env: &Environment, build_target: &Value) -> ValueResult {
        let build_target = match build_target.get_type() {
            "NoneType" => env.get("BUILD_TARGET_TRIPLE").unwrap().to_string(),
            "string" => build_target.to_string(),
            t => {
                return Err(ValueError::TypeNotX {
                    object_type: t.to_string(),
                    op: "str".to_string(),
                })
            }
        };

        resolve_default_python_distribution(&env, &build_target)
    }

    /// PythonDistribution()
    fn from_args(
        env: &Environment,
        sha256: &Value,
        local_path: &Value,
        url: &Value,
    ) -> ValueResult {
        required_str_arg("sha256", &sha256)?;
        optional_str_arg("local_path", &local_path)?;
        optional_str_arg("url", &url)?;

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

        let context = env.get("CONTEXT").expect("CONTEXT not defined");
        let dest_dir =
            context.downcast_apply(|x: &EnvironmentContext| x.python_distributions_path.clone());

        Ok(Value::new(PythonDistribution::from_location(
            distribution,
            &dest_dir,
        )))
    }

    /// PythonDistribution.to_python_executable(
    ///     name,
    ///     config=None,
    ///     extension_module_filter="all",
    ///     preferred_extension_module_variants=None,
    ///     include_sources=true,
    ///     include_resources=true,
    ///     include_test=false,
    /// )
    #[allow(clippy::ptr_arg, clippy::too_many_arguments)]
    fn as_python_executable_starlark(
        &mut self,
        env: Environment,
        call_stack: &Vec<(String, String)>,
        name: &Value,
        config: &Value,
        extension_module_filter: &Value,
        preferred_extension_module_variants: &Value,
        include_sources: &Value,
        include_resources: &Value,
        include_test: &Value,
    ) -> ValueResult {
        let name = required_str_arg("name", &name)?;
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

        self.ensure_distribution_resolved(&logger);
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

        let pre_built = PreBuiltPythonExecutable::from_python_distribution(
            &logger,
            dist,
            &name,
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
        })?;

        context
            .downcast_apply(|context: &EnvironmentContext| -> Result<()> {
                if let Some(path) = &context.write_artifacts_path {
                    warn!(
                        &logger,
                        "writing PyOxidizer build artifacts to {}",
                        path.display()
                    );
                    let embedded = EmbeddedPythonBinaryData::from_pre_built_python_executable(
                        &pre_built,
                        &logger,
                        &context.build_host_triple,
                        &context.build_target_triple,
                        &context.build_opt_level,
                    )?;

                    embedded.write_files(path)?;

                    Ok(())
                } else {
                    Ok(())
                }
            })
            .or_else(|e| {
                Err(RuntimeError {
                    code: "PYOXIDIZER_BUILD",
                    message: e.to_string(),
                    label: "to_python_executable()".to_string(),
                }
                .into())
            })?;

        Ok(Value::new(pre_built))
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

        self.ensure_distribution_resolved(&logger);

        Ok(Value::from(
            self.distribution
                .as_ref()
                .unwrap()
                .filter_extension_modules(&logger, &filter, preferred_variants)
                .iter()
                .map(|em| {
                    Value::new(PythonExtensionModule {
                        em: PythonExtensionModuleFlavor::Persisted(em.clone()),
                    })
                })
                .collect_vec(),
        ))
    }

    /// PythonDistribution.pip_install(args, extra_envs=None)
    pub fn pip_install(
        &mut self,
        env: &Environment,
        args: &Value,
        extra_envs: &Value,
    ) -> ValueResult {
        required_list_arg("args", "string", &args)?;
        optional_dict_arg("extra_envs", "string", "string", &extra_envs)?;

        let args: Vec<String> = args.into_iter()?.map(|x| x.to_string()).collect();

        let extra_envs = match extra_envs.get_type() {
            "dict" => extra_envs
                .into_iter()?
                .map(|key| {
                    let k = key.to_string();
                    let v = extra_envs.at(key).unwrap().to_string();
                    (k, v)
                })
                .collect(),
            "NoneType" => HashMap::new(),
            _ => panic!("should have validated type above"),
        };

        let context = env.get("CONTEXT").expect("CONTEXT not defined");
        let logger = context.downcast_apply(|x: &EnvironmentContext| x.logger.clone());

        self.ensure_distribution_resolved(&logger);
        let dist = self.distribution.as_ref().unwrap();

        // TODO get verbose flag from context.
        let resources =
            raw_pip_install(&logger, &dist, false, &args, &extra_envs).or_else(|e| {
                Err(RuntimeError {
                    code: "PIP_INSTALL_ERROR",
                    message: format!("error running pip install: {}", e),
                    label: "pip_install()".to_string(),
                }
                .into())
            })?;

        Ok(Value::from(
            resources.iter().map(Value::from).collect::<Vec<Value>>(),
        ))
    }

    /// PythonDistribution.read_package_root(path, packages)
    pub fn read_package_root(
        &mut self,
        env: &Environment,
        path: &Value,
        packages: &Value,
    ) -> ValueResult {
        let path = required_str_arg("path", &path)?;
        required_list_arg("packages", "string", &packages)?;

        let packages = packages
            .into_iter()?
            .map(|x| x.to_string())
            .collect::<Vec<String>>();

        let context = env.get("CONTEXT").expect("CONTEXT not defined");
        let logger = context.downcast_apply(|x: &EnvironmentContext| x.logger.clone());

        self.ensure_distribution_resolved(&logger);

        let resources = find_resources(&Path::new(&path), None).or_else(|e| {
            Err(RuntimeError {
                code: "PACKAGE_ROOT_ERROR",
                message: format!("could not find resources: {}", e),
                label: "read_package_root()".to_string(),
            }
            .into())
        })?;

        Ok(Value::from(
            resources
                .iter()
                .filter(|x| x.is_in_packages(&packages))
                .map(Value::from)
                .collect::<Vec<Value>>(),
        ))
    }

    /// PythonDistribution.read_virtualenv(path)
    pub fn read_virtualenv(&mut self, env: &Environment, path: &Value) -> ValueResult {
        let path = required_str_arg("path", &path)?;

        let context = env.get("CONTEXT").expect("CONTEXT not defined");
        let logger = context.downcast_apply(|x: &EnvironmentContext| x.logger.clone());

        self.ensure_distribution_resolved(&logger);
        let dist = self.distribution.as_ref().unwrap();

        let resources = raw_read_virtualenv(&dist, &Path::new(&path)).or_else(|e| {
            Err(RuntimeError {
                code: "VIRTUALENV_ERROR",
                message: format!("could not find resources: {}", e),
                label: "read_virtualenv()".to_string(),
            }
            .into())
        })?;

        Ok(Value::from(
            resources.iter().map(Value::from).collect::<Vec<Value>>(),
        ))
    }

    /// PythonDistribution.resources_data(include_test=false)
    pub fn resources_data(&mut self, env: &Environment, include_test: &Value) -> ValueResult {
        let include_test = required_bool_arg("include_test", &include_test)?;

        let context = env.get("CONTEXT").expect("CONTEXT not defined");

        let logger = context.downcast_apply(|x: &EnvironmentContext| x.logger.clone());

        self.ensure_distribution_resolved(&logger);

        let resources = self
            .distribution
            .as_ref()
            .unwrap()
            .resources_data()
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
                    if !include_test && is_stdlib_test_package(&data.package) {
                        None
                    } else {
                        Some(Value::new(PythonResourceData { data: data.clone() }))
                    }
                })
                .collect_vec(),
        ))
    }

    /// PythonDistribution.setup_py_install(package_path, extra_envs=None, extra_global_arguments=None)
    pub fn setup_py_install(
        &mut self,
        env: &Environment,
        package_path: &Value,
        extra_envs: &Value,
        extra_global_arguments: &Value,
    ) -> ValueResult {
        let package_path = required_str_arg("package_path", &package_path)?;
        optional_dict_arg("extra_envs", "string", "string", &extra_envs)?;
        optional_list_arg("extra_global_arguments", "string", &extra_global_arguments)?;

        let extra_envs = match extra_envs.get_type() {
            "dict" => extra_envs
                .into_iter()?
                .map(|key| {
                    let k = key.to_string();
                    let v = extra_envs.at(key).unwrap().to_string();
                    (k, v)
                })
                .collect(),
            "NoneType" => HashMap::new(),
            _ => panic!("should have validated type above"),
        };
        let extra_global_arguments = match extra_global_arguments.get_type() {
            "list" => extra_global_arguments
                .into_iter()?
                .map(|x| x.to_string())
                .collect(),
            "NoneType" => Vec::new(),
            _ => panic!("should have validated type above"),
        };

        let package_path = PathBuf::from(package_path);

        let context = env.get("CONTEXT").expect("CONTEXT not defined");
        let cwd = env.get("CWD").expect("CWD not defined").to_string();
        let (logger, verbose) =
            context.downcast_apply(|x: &EnvironmentContext| (x.logger.clone(), x.verbose));

        let package_path = if package_path.is_absolute() {
            package_path
        } else {
            PathBuf::from(cwd).join(package_path)
        };

        self.ensure_distribution_resolved(&logger);
        let dist = self.distribution.as_ref().unwrap();

        let resources = raw_setup_py_install(
            &logger,
            dist,
            &package_path,
            verbose,
            &extra_envs,
            &extra_global_arguments,
        )
        .or_else(|e| {
            Err(RuntimeError {
                code: "SETUP_PY_ERROR",
                message: e.to_string(),
                label: "setup_py_install()".to_string(),
            }
            .into())
        })?;

        warn!(
            logger,
            "collected {} resources from setup.py install",
            resources.len()
        );

        Ok(Value::from(
            resources.iter().map(Value::from).collect::<Vec<Value>>(),
        ))
    }

    /// PythonDistribution.source_modules()
    pub fn source_modules(&mut self, env: &Environment) -> ValueResult {
        let context = env.get("CONTEXT").expect("CONTEXT not defined");

        let logger = context.downcast_apply(|x: &EnvironmentContext| x.logger.clone());

        self.ensure_distribution_resolved(&logger);

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

pub fn resolve_default_python_distribution(env: &Environment, build_target: &str) -> ValueResult {
    match CPYTHON_BY_TRIPLE.get(build_target) {
        Some(dist) => {
            let distribution = PythonDistributionLocation::Url {
                url: dist.url.clone(),
                sha256: dist.sha256.clone(),
            };

            let context = env.get("CONTEXT").expect("CONTEXT not defined");
            let dest_dir = context
                .downcast_apply(|x: &EnvironmentContext| x.python_distributions_path.clone());

            Ok(Value::new(PythonDistribution::from_location(
                distribution,
                &dest_dir,
            )))
        }
        None => Err(ValueError::Runtime(RuntimeError {
            code: "no_default_distribution",
            message: format!(
                "could not find default Python distribution for {}",
                build_target
            ),
            label: "build_target".to_string(),
        })),
    }
}

starlark_module! { python_distribution_module =>
    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonDistribution(env env, sha256, local_path=None, url=None) {
        PythonDistribution::from_args(&env, &sha256, &local_path, &url)
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
    PythonDistribution.resources_data(env env, this, include_test=false) {
        this.downcast_apply_mut(|dist: &mut PythonDistribution| {
            dist.resources_data(&env, &include_test)
        })
    }

    #[allow(clippy::ptr_arg)]
    PythonDistribution.pip_install(env env, this, args, extra_envs=None) {
        this.downcast_apply_mut(|dist: &mut PythonDistribution| {
            dist.pip_install(&env, &args, &extra_envs)
        })
    }

    #[allow(clippy::ptr_arg)]
    PythonDistribution.read_package_root(
        env env,
        this,
        path,
        packages
    ) {
        this.downcast_apply_mut(|dist: &mut PythonDistribution| {
            dist.read_package_root(&env, &path, &packages)
        })
    }

    #[allow(clippy::ptr_arg)]
    PythonDistribution.read_virtualenv(
        env env,
        this,
        path
    ) {
        this.downcast_apply_mut(|dist: &mut PythonDistribution| {
            dist.read_virtualenv(&env, &path)
        })
    }

    #[allow(clippy::ptr_arg)]
    PythonDistribution.setup_py_install(
        env env,
        this,
        package_path,
        extra_envs=None,
        extra_global_arguments=None
    ) {
        this.downcast_apply_mut(|dist: &mut PythonDistribution| {
            dist.setup_py_install(&env, &package_path, &extra_envs, &extra_global_arguments)
        })
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonDistribution.to_python_executable(
        env env,
        call_stack call_stack,
        this,
        name,
        config=None,
        extension_module_filter="all",
        preferred_extension_module_variants=None,
        include_sources=true,
        include_resources=false,
        include_test=false
    ) {
        this.downcast_apply_mut(|dist: &mut PythonDistribution| {
            dist.as_python_executable_starlark(
                env.clone(),
                call_stack,
                &name,
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
    default_python_distribution(env env, build_target=None) {
        PythonDistribution::default_python_distribution(&env, &build_target)
    }
}

#[cfg(test)]
mod tests {
    use super::super::testutil::*;
    use super::*;

    #[test]
    fn test_default_python_distribution() {
        let dist = starlark_ok("default_python_distribution()");
        assert_eq!(dist.get_type(), "PythonDistribution");

        let host_distribution = CPYTHON_BY_TRIPLE
            .get(crate::app_packaging::repackage::HOST)
            .unwrap();

        let wanted = PythonDistributionLocation::Url {
            url: host_distribution.url.clone(),
            sha256: host_distribution.sha256.clone(),
        };

        dist.downcast_apply(|x: &PythonDistribution| assert_eq!(x.source, wanted));
    }

    #[test]
    fn test_default_python_distribution_bad_arg() {
        let err = starlark_nok("default_python_distribution(False)");
        assert_eq!(err.message, "The type 'bool' is not str");
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

        dist.downcast_apply(|x: &PythonDistribution| assert_eq!(x.source, wanted));
    }

    #[test]
    fn test_python_distribution_local_path() {
        let dist = starlark_ok("PythonDistribution('sha256', local_path='some_path')");
        let wanted = PythonDistributionLocation::Local {
            local_path: "some_path".to_string(),
            sha256: "sha256".to_string(),
        };

        dist.downcast_apply(|x: &PythonDistribution| assert_eq!(x.source, wanted));
    }

    #[test]
    fn test_source_modules() {
        let mods = starlark_ok("default_python_distribution().source_modules()");
        assert_eq!(mods.get_type(), "list");
    }

    #[test]
    fn test_resources_data() {
        let data_default = starlark_ok("default_python_distribution().resources_data()");
        let data_tests =
            starlark_ok("default_python_distribution().resources_data(include_test=True)");

        let default_length = data_default.length().unwrap();
        let data_length = data_tests.length().unwrap();

        // TODO there is likely a bug in the Windows distribution or resource
        // detection logic.
        if cfg!(windows) {
            assert_eq!(default_length, data_length);
        } else {
            assert!(default_length < data_length);
        }
    }

    #[test]
    fn test_pip_install_simple() {
        let resources =
            starlark_ok("default_python_distribution().pip_install(['pyflakes==2.1.1'])");
        assert_eq!(resources.get_type(), "list");

        let mut it = resources.into_iter().unwrap();

        let v = it.next().unwrap();
        assert_eq!(v.get_type(), "PythonSourceModule");
        v.downcast_apply(|x: &PythonSourceModule| {
            assert_eq!(x.module.name, "pyflakes");
            assert!(x.module.is_package);
        });
    }
}
