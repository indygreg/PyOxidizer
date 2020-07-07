// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    super::env::EnvironmentContext,
    super::python_embedded_resources::PythonEmbeddedResources,
    super::python_resource::{
        python_resource_to_value, PythonExtensionModule, PythonExtensionModuleFlavor,
        PythonPackageDistributionResource, PythonPackageResource, PythonSourceModule,
    },
    super::target::{BuildContext, BuildTarget, ResolvedTarget, RunMode},
    super::util::{
        optional_dict_arg, optional_list_arg, required_bool_arg, required_list_arg,
        required_str_arg, required_type_arg,
    },
    crate::project_building::build_python_executable,
    crate::py_packaging::binary::PythonBinaryBuilder,
    anyhow::{anyhow, Context, Result},
    python_packaging::resource::{BytecodeOptimizationLevel, PythonModuleBytecodeFromSource},
    slog::{info, warn},
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
    std::io::Write,
    std::ops::Deref,
    std::path::{Path, PathBuf},
};

/// Represents a builder for a Python executable.
pub struct PythonExecutable {
    pub exe: Box<dyn PythonBinaryBuilder>,
}

impl TypedValue for PythonExecutable {
    immutable!();
    any!();
    not_supported!(binop, container, function, get_hash, to_int);

    fn to_str(&self) -> String {
        "PythonExecutable<>".to_string()
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_type(&self) -> &'static str {
        "PythonExecutable"
    }

    fn to_bool(&self) -> bool {
        true
    }

    fn compare(&self, other: &dyn TypedValue, _recursion: u32) -> Result<Ordering, ValueError> {
        default_compare(self, other)
    }
}

impl BuildTarget for PythonExecutable {
    fn build(&mut self, context: &BuildContext) -> Result<ResolvedTarget> {
        // Build an executable by writing out a temporary Rust project
        // and building it.
        let build = build_python_executable(
            &context.logger,
            &self.exe.name(),
            self.exe.deref(),
            &context.target_triple,
            &context.opt_level,
            context.release,
        )?;

        let dest_path = context.output_path.join(build.exe_name);
        warn!(
            &context.logger,
            "writing executable to {}",
            dest_path.display()
        );
        let mut fh = std::fs::File::create(&dest_path)
            .context(format!("creating {}", dest_path.display()))?;
        fh.write_all(&build.exe_data)
            .context(format!("writing {}", dest_path.display()))?;

        crate::app_packaging::resource::set_executable(&mut fh)
            .context("making binary executable")?;

        Ok(ResolvedTarget {
            run_mode: RunMode::Path { path: dest_path },
            output_path: context.output_path.clone(),
        })
    }
}

// Starlark functions.
impl PythonExecutable {
    /// PythonExecutable.pip_install(args, extra_envs=None)
    pub fn starlark_pip_install(
        &self,
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
        let (logger, verbose) =
            context.downcast_apply(|x: &EnvironmentContext| (x.logger.clone(), x.verbose));

        let resources = self
            .exe
            .pip_install(&logger, verbose, &args, &extra_envs)
            .or_else(|e| {
                Err(RuntimeError {
                    code: "PIP_INSTALL_ERROR",
                    message: format!("error running pip install: {}", e),
                    label: "pip_install()".to_string(),
                }
                .into())
            })?;

        Ok(Value::from(
            resources
                .iter()
                .map(python_resource_to_value)
                .collect::<Vec<Value>>(),
        ))
    }

    /// PythonExecutable.read_package_root(path, packages)
    pub fn starlark_read_package_root(
        &self,
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

        let resources = self
            .exe
            .read_package_root(&logger, Path::new(&path), &packages)
            .or_else(|e| {
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
                .map(python_resource_to_value)
                .collect::<Vec<Value>>(),
        ))
    }

    /// PythonExecutable.read_virtualenv(path)
    pub fn starlark_read_virtualenv(&self, env: &Environment, path: &Value) -> ValueResult {
        let path = required_str_arg("path", &path)?;

        let context = env.get("CONTEXT").expect("CONTEXT not defined");
        let logger = context.downcast_apply(|x: &EnvironmentContext| x.logger.clone());

        let resources = self
            .exe
            .read_virtualenv(&logger, &Path::new(&path))
            .or_else(|e| {
                Err(RuntimeError {
                    code: "VIRTUALENV_ERROR",
                    message: format!("could not find resources: {}", e),
                    label: "read_virtualenv()".to_string(),
                }
                .into())
            })?;

        Ok(Value::from(
            resources
                .iter()
                .map(python_resource_to_value)
                .collect::<Vec<Value>>(),
        ))
    }

    /// PythonExecutable.setup_py_install(package_path, extra_envs=None, extra_global_arguments=None)
    pub fn starlark_setup_py_install(
        &self,
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

        let resources = self
            .exe
            .setup_py_install(
                &logger,
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
            resources
                .iter()
                .map(python_resource_to_value)
                .collect::<Vec<Value>>(),
        ))
    }

    /// PythonExecutable.add_in_memory_module_source(module)
    pub fn starlark_add_in_memory_module_source(
        &mut self,
        env: &Environment,
        module: &Value,
    ) -> ValueResult {
        required_type_arg("module", "PythonSourceModule", &module)?;

        let context = env.get("CONTEXT").expect("CONTEXT not set");
        let logger = context.downcast_apply(|x: &EnvironmentContext| x.logger.clone());

        let m = module.downcast_apply(|m: &PythonSourceModule| m.module.clone());
        info!(&logger, "adding in-memory source module {}", m.name);
        self.exe.add_in_memory_module_source(&m).or_else(|e| {
            {
                Err(RuntimeError {
                    code: "PYOXIDIZER_BUILD",
                    message: e.to_string(),
                    label: "add_in_memory_module_source".to_string(),
                }
                .into())
            }
        })?;

        Ok(Value::new(None))
    }

    /// PythonExecutable.add_filesystem_relative_module_source(module, prefix="")
    pub fn starlark_add_filesystem_relative_module_source(
        &mut self,
        env: &Environment,
        prefix: &Value,
        module: &Value,
    ) -> ValueResult {
        let prefix = required_str_arg("prefix", &prefix)?;
        required_type_arg("module", "PythonSourceModule", &module)?;

        let context = env.get("CONTEXT").expect("CONTEXT not set");
        let logger = context.downcast_apply(|x: &EnvironmentContext| x.logger.clone());

        let m = module.downcast_apply(|m: &PythonSourceModule| m.module.clone());
        info!(
            &logger,
            "adding executable relative source module {}", m.name
        );
        self.exe
            .add_relative_path_module_source(&prefix, &m)
            .or_else(|e| {
                Err(RuntimeError {
                    code: "PYOXIDIZER_BUILD",
                    message: e.to_string(),
                    label: "add_filesystem_relative_module_source".to_string(),
                }
                .into())
            })?;

        Ok(Value::new(None))
    }

    /// PythonExecutable.add_module_source(module)
    pub fn starlark_add_module_source(&mut self, env: &Environment, module: &Value) -> ValueResult {
        required_type_arg("module", "PythonSourceModule", &module)?;

        let context = env.get("CONTEXT").expect("CONTEXT not set");
        let logger = context.downcast_apply(|x: &EnvironmentContext| x.logger.clone());

        let m = module.downcast_apply(|m: &PythonSourceModule| m.module.clone());
        info!(&logger, "adding source module {}", m.name);
        self.exe.add_module_source(&m).or_else(|e| {
            {
                Err(RuntimeError {
                    code: "PYOXIDIZER_BUILD",
                    message: e.to_string(),
                    label: "add_module_source".to_string(),
                }
                .into())
            }
        })?;

        Ok(Value::new(None))
    }

    /// PythonExecutable.add_in_memory_module_bytecode(module, optimize_level=0)
    pub fn starlark_add_in_memory_module_bytecode(
        &mut self,
        env: &Environment,
        module: &Value,
        optimize_level: &Value,
    ) -> ValueResult {
        required_type_arg("module", "PythonSourceModule", &module)?;
        required_type_arg("optimize_level", "int", &optimize_level)?;

        let context = env.get("CONTEXT").expect("CONTEXT not set");
        let logger = context.downcast_apply(|x: &EnvironmentContext| x.logger.clone());

        let optimize_level = optimize_level.to_int().unwrap();

        let optimize_level = match optimize_level {
            0 => BytecodeOptimizationLevel::Zero,
            1 => BytecodeOptimizationLevel::One,
            2 => BytecodeOptimizationLevel::Two,
            i => {
                return Err(RuntimeError {
                    code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                    message: format!("optimize_level must be 0, 1, or 2: got {}", i),
                    label: "invalid optimize_level value".to_string(),
                }
                .into());
            }
        };

        let m = module.downcast_apply(|m: &PythonSourceModule| m.module.clone());
        info!(&logger, "adding in-memory bytecode module {}", m.name);
        self.exe
            .add_in_memory_module_bytecode(&PythonModuleBytecodeFromSource {
                name: m.name.clone(),
                source: m.source.clone(),
                optimize_level,
                is_package: m.is_package,
                cache_tag: m.cache_tag,
            })
            .or_else(|e| {
                {
                    Err(RuntimeError {
                        code: "PYOXIDIZER_BUILD",
                        message: e.to_string(),
                        label: "add_in_memory_module_bytecode".to_string(),
                    }
                    .into())
                }
            })?;

        Ok(Value::new(None))
    }

    /// PythonExecutable.add_filesystem_relative_module_bytecode(prefix, module, optimize_level=0)
    pub fn starlark_add_filesystem_relative_module_bytecode(
        &mut self,
        env: &Environment,
        prefix: &Value,
        module: &Value,
        optimize_level: &Value,
    ) -> ValueResult {
        let prefix = required_str_arg("prefix", &prefix)?;
        required_type_arg("module", "PythonSourceModule", &module)?;
        required_type_arg("optimize_level", "int", &optimize_level)?;

        let context = env.get("CONTEXT").expect("CONTEXT not set");
        let logger = context.downcast_apply(|x: &EnvironmentContext| x.logger.clone());

        let optimize_level = optimize_level.to_int().unwrap();

        let optimize_level = match optimize_level {
            0 => BytecodeOptimizationLevel::Zero,
            1 => BytecodeOptimizationLevel::One,
            2 => BytecodeOptimizationLevel::Two,
            i => {
                return Err(RuntimeError {
                    code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                    message: format!("optimize_level must be 0, 1, or 2: got {}", i),
                    label: "invalid optimize_level value".to_string(),
                }
                .into());
            }
        };

        let m = module.downcast_apply(|m: &PythonSourceModule| m.module.clone());
        info!(
            &logger,
            "adding executable relative bytecode module {}", m.name
        );
        self.exe
            .add_relative_path_module_bytecode(
                &prefix,
                &PythonModuleBytecodeFromSource {
                    name: m.name.clone(),
                    source: m.source.clone(),
                    optimize_level,
                    is_package: m.is_package,
                    cache_tag: m.cache_tag,
                },
            )
            .or_else(|e| {
                Err(RuntimeError {
                    code: "PYOXIDIZER_BUILD",
                    message: e.to_string(),
                    label: "add_filesystem_relative_module_bytecode".to_string(),
                }
                .into())
            })?;

        Ok(Value::new(None))
    }

    /// PythonExecutable.add_module_bytecode(module, optimize_level=0)
    pub fn starlark_add_module_bytecode(
        &mut self,
        env: &Environment,
        module: &Value,
        optimize_level: &Value,
    ) -> ValueResult {
        required_type_arg("module", "PythonSourceModule", &module)?;
        required_type_arg("optimize_level", "int", &optimize_level)?;

        let context = env.get("CONTEXT").expect("CONTEXT not set");
        let logger = context.downcast_apply(|x: &EnvironmentContext| x.logger.clone());

        let optimize_level = optimize_level.to_int().unwrap();

        let optimize_level = match optimize_level {
            0 => BytecodeOptimizationLevel::Zero,
            1 => BytecodeOptimizationLevel::One,
            2 => BytecodeOptimizationLevel::Two,
            i => {
                return Err(RuntimeError {
                    code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                    message: format!("optimize_level must be 0, 1, or 2: got {}", i),
                    label: "invalid optimize_level value".to_string(),
                }
                .into());
            }
        };

        let m = module.downcast_apply(|m: &PythonSourceModule| m.module.clone());
        info!(&logger, "adding bytecode module {}", m.name);
        self.exe
            .add_module_bytecode(&PythonModuleBytecodeFromSource {
                name: m.name.clone(),
                source: m.source.clone(),
                optimize_level,
                is_package: m.is_package,
                cache_tag: m.cache_tag,
            })
            .or_else(|e| {
                {
                    Err(RuntimeError {
                        code: "PYOXIDIZER_BUILD",
                        message: e.to_string(),
                        label: "add_module_bytecode".to_string(),
                    }
                    .into())
                }
            })?;

        Ok(Value::new(None))
    }

    /// PythonExecutable.add_in_memory_package_resource(resource)
    pub fn starlark_add_in_memory_package_resource(
        &mut self,
        env: &Environment,
        resource: &Value,
    ) -> ValueResult {
        required_type_arg("resource", "PythonPackageResource", &resource)?;

        let context = env.get("CONTEXT").expect("CONTEXT not set");
        let logger = context.downcast_apply(|x: &EnvironmentContext| x.logger.clone());

        let r = resource.downcast_apply(|r: &PythonPackageResource| r.data.clone());
        info!(
            &logger,
            "adding in-memory resource data {}",
            r.symbolic_name()
        );
        self.exe.add_in_memory_package_resource(&r).or_else(|e| {
            {
                Err(RuntimeError {
                    code: "PYOXIDIZER_BUILD",
                    message: e.to_string(),
                    label: "add_in_memory_package_resource".to_string(),
                }
                .into())
            }
        })?;

        Ok(Value::new(None))
    }

    /// PythonExecutable.add_package_resource(resource)
    pub fn starlark_add_package_resource(
        &mut self,
        env: &Environment,
        resource: &Value,
    ) -> ValueResult {
        required_type_arg("resource", "PythonPackageResource", &resource)?;

        let context = env.get("CONTEXT").expect("CONTEXT not set");
        let logger = context.downcast_apply(|x: &EnvironmentContext| x.logger.clone());

        let r = resource.downcast_apply(|r: &PythonPackageResource| r.data.clone());
        info!(&logger, "adding resource data {}", r.symbolic_name());
        self.exe.add_package_resource(&r).or_else(|e| {
            {
                Err(RuntimeError {
                    code: "PYOXIDIZER_BUILD",
                    message: e.to_string(),
                    label: "add_package_resource".to_string(),
                }
                .into())
            }
        })?;

        Ok(Value::new(None))
    }

    /// PythonExecutable.add_filesystem_relative_package_resource(prefix, resource)
    pub fn starlark_add_filesystem_relative_package_resource(
        &mut self,
        env: &Environment,
        prefix: &Value,
        resource: &Value,
    ) -> ValueResult {
        let prefix = required_str_arg("prefix", &prefix)?;
        required_type_arg("resource", "PythonPackageResource", &resource)?;

        let context = env.get("CONTEXT").expect("CONTEXT not set");
        let logger = context.downcast_apply(|x: &EnvironmentContext| x.logger.clone());

        let r = resource.downcast_apply(|r: &PythonPackageResource| r.data.clone());
        info!(
            &logger,
            "adding executable relative resource data {}",
            r.symbolic_name()
        );
        self.exe
            .add_relative_path_package_resource(&prefix, &r)
            .or_else(|e| {
                Err(RuntimeError {
                    code: "PYOXIDIZER_BUILD",
                    message: e.to_string(),
                    label: "add_filesystem_relative_package_resource".to_string(),
                }
                .into())
            })?;

        Ok(Value::new(None))
    }

    /// PythonExecutable.add_in_memory_package_distribution_resource(resource)
    pub fn starlark_add_in_memory_package_distribution_resource(
        &mut self,
        env: &Environment,
        resource: &Value,
    ) -> ValueResult {
        required_type_arg("resource", "PythonPackageDistributionResource", &resource)?;

        let context = env.get("CONTEXT").expect("CONTEXT not set");
        let logger = context.downcast_apply(|x: &EnvironmentContext| x.logger.clone());

        let r = resource.downcast_apply(|r: &PythonPackageDistributionResource| r.resource.clone());
        info!(
            &logger,
            "adding in-memory package distribution resource {}:{}", r.package, r.name
        );
        self.exe
            .add_in_memory_package_distribution_resource(&r)
            .or_else(|e| {
                {
                    Err(RuntimeError {
                        code: "PYOXIDIZER_BUILD",
                        message: e.to_string(),
                        label: "add_in_memory_package_distribution_resource".to_string(),
                    }
                    .into())
                }
            })?;

        Ok(Value::new(None))
    }

    /// PythonExecutable.add_filesystem_relative_package_distribution_resource(prefix, resource)
    pub fn starlark_add_filesystem_relative_package_distribution_resource(
        &mut self,
        env: &Environment,
        prefix: &Value,
        resource: &Value,
    ) -> ValueResult {
        let prefix = required_str_arg("prefix", &prefix)?;
        required_type_arg("resource", "PythonPackageDistributionResource", &resource)?;

        let context = env.get("CONTEXT").expect("CONTEXT not set");
        let logger = context.downcast_apply(|x: &EnvironmentContext| x.logger.clone());

        let r = resource.downcast_apply(|r: &PythonPackageDistributionResource| r.resource.clone());
        info!(
            &logger,
            "adding executable relative package distribution resource {}:{}", r.package, r.name
        );
        self.exe
            .add_relative_path_package_distribution_resource(&prefix, &r)
            .or_else(|e| {
                Err(RuntimeError {
                    code: "PYOXIDIZER_BUILD",
                    message: e.to_string(),
                    label: "add_filesystem_relative_package_distribution_resource".to_string(),
                }
                .into())
            })?;

        Ok(Value::new(None))
    }

    /// PythonExecutable.add_package_distribution_resource(resource)
    pub fn starlark_add_package_distribution_resource(
        &mut self,
        env: &Environment,
        resource: &Value,
    ) -> ValueResult {
        required_type_arg("resource", "PythonPackageDistributionResource", &resource)?;

        let context = env.get("CONTEXT").expect("CONTEXT not set");
        let logger = context.downcast_apply(|x: &EnvironmentContext| x.logger.clone());

        let r = resource.downcast_apply(|r: &PythonPackageDistributionResource| r.resource.clone());
        info!(
            &logger,
            "adding package distribution resource {}:{}", r.package, r.name
        );
        self.exe
            .add_package_distribution_resource(&r)
            .or_else(|e| {
                {
                    Err(RuntimeError {
                        code: "PYOXIDIZER_BUILD",
                        message: e.to_string(),
                        label: "add_package_distribution_resource".to_string(),
                    }
                    .into())
                }
            })?;

        Ok(Value::new(None))
    }

    /// PythonExecutable.add_in_memory_extension_module(module)
    pub fn starlark_add_in_memory_extension_module(
        &mut self,
        env: &Environment,
        module: &Value,
    ) -> ValueResult {
        required_type_arg("module", "PythonExtensionModule", &module)?;

        let context = env.get("CONTEXT").expect("CONTEXT not set");
        let logger = context.downcast_apply(|x: &EnvironmentContext| x.logger.clone());

        let m = module.downcast_apply(|m: &PythonExtensionModule| m.em.clone());
        info!(&logger, "adding in-memory extension module {}", m.name());

        match m {
            PythonExtensionModuleFlavor::Distribution(m) => {
                self.exe.add_in_memory_distribution_extension_module(&m)
            }
            PythonExtensionModuleFlavor::StaticallyLinked(m) => {
                self.exe.add_static_extension_module(&m)
            }
            PythonExtensionModuleFlavor::DynamicLibrary(m) => {
                self.exe.add_in_memory_dynamic_extension_module(&m)
            }
        }
        .or_else(|e| {
            Err(RuntimeError {
                code: "PYOXIDIZER_BUILD",
                message: e.to_string(),
                label: "add_in_memory_extension_module".to_string(),
            }
            .into())
        })?;

        Ok(Value::new(None))
    }

    /// PythonExecutable.add_filesystem_relative_extension_module(module)
    pub fn starlark_add_filesystem_relative_extension_module(
        &mut self,
        env: &Environment,
        prefix: &Value,
        module: &Value,
    ) -> ValueResult {
        let prefix = required_str_arg("prefix", &prefix)?;
        required_type_arg("module", "PythonExtensionModule", &module)?;

        let context = env.get("CONTEXT").expect("CONTEXT not set");
        let logger = context.downcast_apply(|x: &EnvironmentContext| x.logger.clone());

        let m = module.downcast_apply(|m: &PythonExtensionModule| m.em.clone());
        info!(&logger, "adding in-extension module {}", m.name());

        match m {
            PythonExtensionModuleFlavor::Distribution(m) => self
                .exe
                .add_relative_path_distribution_extension_module(&prefix, &m),
            PythonExtensionModuleFlavor::StaticallyLinked(_) => Err(anyhow!(
                "statically linked extension modules cannot be added as filesystem relative"
            )),
            PythonExtensionModuleFlavor::DynamicLibrary(m) => self
                .exe
                .add_relative_path_dynamic_extension_module(&prefix, &m),
        }
        .or_else(|e| {
            Err(RuntimeError {
                code: "PYOXIDIZER_BUILD",
                message: e.to_string(),
                label: "add_filesystem_relative_extension_module".to_string(),
            }
            .into())
        })?;

        Ok(Value::new(None))
    }

    /// PythonExecutable.add_extension_module(module)
    pub fn starlark_add_extension_module(
        &mut self,
        env: &Environment,
        module: &Value,
    ) -> ValueResult {
        required_type_arg("module", "PythonExtensionModule", &module)?;

        let context = env.get("CONTEXT").expect("CONTEXT not set");
        let logger = context.downcast_apply(|x: &EnvironmentContext| x.logger.clone());

        let m = module.downcast_apply(|m: &PythonExtensionModule| m.em.clone());

        match m {
            PythonExtensionModuleFlavor::Distribution(m) => {
                info!(logger, "adding extension module {}", m.module);
                self.exe.add_distribution_extension_module(&m)
            }
            PythonExtensionModuleFlavor::StaticallyLinked(m) => {
                info!(
                    logger,
                    "adding statically linked extension module {}", m.name
                );
                self.exe.add_static_extension_module(&m)
            }
            PythonExtensionModuleFlavor::DynamicLibrary(m) => {
                info!(
                    logger,
                    "adding dynamically linked extension module {}", m.name
                );
                self.exe.add_dynamic_extension_module(&m)
            }
        }
        .or_else(|e| {
            {
                Err(RuntimeError {
                    code: "PYOXIDIZER_BUILD",
                    message: e.to_string(),
                    label: "add_extension_module".to_string(),
                }
                .into())
            }
        })?;

        Ok(Value::new(None))
    }

    /// PythonExecutable.add_in_memory_python_resource(resource, add_source_module=true, add_bytecode_module=true, optimize_level=0)
    pub fn starlark_add_in_memory_python_resource(
        &mut self,
        env: &Environment,
        resource: &Value,
        add_source_module: &Value,
        add_bytecode_module: &Value,
        optimize_level: &Value,
    ) -> ValueResult {
        let add_source_module = required_bool_arg("add_source_module", &add_source_module)?;
        let add_bytecode_module = required_bool_arg("add_bytecode_module", &add_bytecode_module)?;
        required_type_arg("optimize_level", "int", &optimize_level)?;

        match resource.get_type() {
            "PythonSourceModule" => {
                if add_source_module {
                    self.starlark_add_in_memory_module_source(env, resource)?;
                }
                if add_bytecode_module {
                    self.starlark_add_in_memory_module_bytecode(env, resource, optimize_level)?;
                }

                Ok(Value::new(None))
            }
            "PythonBytecodeModule" => {
                self.starlark_add_in_memory_module_bytecode(env, resource, optimize_level)
            }
            "PythonPackageResource" => self.starlark_add_in_memory_package_resource(env, resource),
            "PythonPackageDistributionResource" => {
                self.starlark_add_package_distribution_resource(env, resource)
            }
            "PythonExtensionModule" => self.starlark_add_extension_module(env, resource),
            _ => Err(RuntimeError {
                code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                message: "resource argument must be a Python resource type".to_string(),
                label: ".add_in_memory_python_resource()".to_string(),
            }
            .into()),
        }
    }

    /// PythonExecutable.add_filesystem_relative_python_resource(prefix, resource, add_source_module=true, add_bytecode_module=true, optimize_level=0)
    pub fn starlark_add_filesystem_relative_python_resource(
        &mut self,
        env: &Environment,
        prefix: &Value,
        resource: &Value,
        add_source_module: &Value,
        add_bytecode_module: &Value,
        optimize_level: &Value,
    ) -> ValueResult {
        required_str_arg("prefix", &prefix)?;
        let add_source_module = required_bool_arg("add_source_module", &add_source_module)?;
        let add_bytecode_module = required_bool_arg("add_bytecode_module", &add_bytecode_module)?;
        required_type_arg("optimize_level", "int", &optimize_level)?;

        match resource.get_type() {
            "PythonSourceModule" => {
                if add_source_module {
                    self.starlark_add_filesystem_relative_module_source(env, prefix, resource)?;
                }
                if add_bytecode_module {
                    self.starlark_add_filesystem_relative_module_bytecode(
                        env,
                        prefix,
                        resource,
                        optimize_level,
                    )?;
                }

                Ok(Value::new(None))
            }
            "PythonBytecodeModule" => self.starlark_add_filesystem_relative_module_bytecode(
                env,
                prefix,
                resource,
                optimize_level,
            ),
            "PythonPackageResource" => {
                self.starlark_add_filesystem_relative_package_resource(env, prefix, resource)
            }
            "PythonPackageDistributionResource" => self
                .starlark_add_filesystem_relative_package_distribution_resource(
                    env, prefix, resource,
                ),
            "PythonExtensionModule" => self.starlark_add_extension_module(env, resource),
            _ => Err(RuntimeError {
                code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                message: "resource argument must be a Python resource type".to_string(),
                label: ".add_in_memory_python_resource()".to_string(),
            }
            .into()),
        }
    }

    /// PythonExecutable.add_python_resource(resource, add_source_module=true, add_bytecode_module=true, optimize_level=0)
    pub fn starlark_add_python_resource(
        &mut self,
        env: &Environment,
        resource: &Value,
        add_source_module: &Value,
        add_bytecode_module: &Value,
        optimize_level: &Value,
    ) -> ValueResult {
        let add_source_module = required_bool_arg("add_source_module", &add_source_module)?;
        let add_bytecode_module = required_bool_arg("add_bytecode_module", &add_bytecode_module)?;
        required_type_arg("optimize_level", "int", &optimize_level)?;

        match resource.get_type() {
            "PythonSourceModule" => {
                if add_source_module {
                    self.starlark_add_module_source(env, resource)?;
                }
                if add_bytecode_module {
                    self.starlark_add_module_bytecode(env, resource, optimize_level)?;
                }

                Ok(Value::new(None))
            }
            "PythonBytecodeModule" => {
                self.starlark_add_module_bytecode(env, resource, optimize_level)
            }
            "PythonPackageResource" => self.starlark_add_package_resource(env, resource),
            "PythonPackageDistributionResource" => {
                self.starlark_add_package_distribution_resource(env, resource)
            }
            "PythonExtensionModule" => self.starlark_add_extension_module(env, resource),
            _ => Err(RuntimeError {
                code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                message: "resource argument must be a Python resource type".to_string(),
                label: ".add_python_resource()".to_string(),
            }
            .into()),
        }
    }

    /// PythonExecutable.add_in_memory_python_resources(resources, add_source_module=true, add_bytecode_module=true, optimize_level=0)
    pub fn starlark_add_in_memory_python_resources(
        &mut self,
        env: &Environment,
        resources: &Value,
        add_source_module: &Value,
        add_bytecode_module: &Value,
        optimize_level: &Value,
    ) -> ValueResult {
        required_bool_arg("add_source_module", &add_source_module)?;
        required_bool_arg("add_bytecode_module", &add_bytecode_module)?;
        required_type_arg("optimize_level", "int", &optimize_level)?;

        for resource in resources.into_iter()? {
            self.starlark_add_in_memory_python_resource(
                env,
                &resource,
                add_source_module,
                add_bytecode_module,
                optimize_level,
            )?;
        }

        Ok(Value::new(None))
    }

    /// PythonExecutable.add_filesystem_relative_python_resources(prefix, resources, add_source_module=true, add_bytecode_module=true, optimize_level=0)
    pub fn starlark_add_filesystem_relative_python_resources(
        &mut self,
        env: &Environment,
        prefix: &Value,
        resources: &Value,
        add_source_module: &Value,
        add_bytecode_module: &Value,
        optimize_level: &Value,
    ) -> ValueResult {
        required_str_arg("prefix", &prefix)?;
        required_bool_arg("add_source_module", &add_source_module)?;
        required_bool_arg("add_bytecode_module", &add_bytecode_module)?;
        required_type_arg("optimize_level", "int", &optimize_level)?;

        for resource in resources.into_iter()? {
            self.starlark_add_filesystem_relative_python_resource(
                env,
                prefix,
                &resource,
                add_source_module,
                add_bytecode_module,
                optimize_level,
            )?;
        }

        Ok(Value::new(None))
    }

    /// PythonExecutable.add_python_resources(resources, add_source_module=true, add_bytecode_module=true, optimize_level=0)
    pub fn starlark_add_python_resources(
        &mut self,
        env: &Environment,
        resources: &Value,
        add_source_module: &Value,
        add_bytecode_module: &Value,
        optimize_level: &Value,
    ) -> ValueResult {
        required_bool_arg("add_source_module", &add_source_module)?;
        required_bool_arg("add_bytecode_module", &add_bytecode_module)?;
        required_type_arg("optimize_level", "int", &optimize_level)?;

        for resource in resources.into_iter()? {
            self.starlark_add_python_resource(
                env,
                &resource,
                add_source_module,
                add_bytecode_module,
                optimize_level,
            )?;
        }

        Ok(Value::new(None))
    }

    /// PythonExecutable.to_embedded_resources()
    pub fn starlark_to_embedded_resources(&self) -> ValueResult {
        Ok(Value::new(PythonEmbeddedResources {
            exe: self.exe.clone_box(),
        }))
    }

    /// PythonExecutable.filter_resources_from_files(files=None, glob_files=None)
    pub fn starlark_filter_resources_from_files(
        &mut self,
        env: &Environment,
        files: &Value,
        glob_files: &Value,
    ) -> ValueResult {
        optional_list_arg("files", "string", &files)?;
        optional_list_arg("glob_files", "string", &glob_files)?;

        let files = match files.get_type() {
            "list" => files
                .into_iter()?
                .map(|x| PathBuf::from(x.to_string()))
                .collect(),
            "NoneType" => Vec::new(),
            _ => panic!("type should have been validated above"),
        };

        let glob_files = match glob_files.get_type() {
            "list" => glob_files.into_iter()?.map(|x| x.to_string()).collect(),
            "NoneType" => Vec::new(),
            _ => panic!("type should have been validated above"),
        };

        let files_refs = files.iter().map(|x| x.as_ref()).collect::<Vec<&Path>>();
        let glob_files_refs = glob_files.iter().map(|x| x.as_ref()).collect::<Vec<&str>>();

        let context = env.get("CONTEXT").expect("CONTEXT not defined");
        let logger = context.downcast_apply(|x: &EnvironmentContext| x.logger.clone());

        self.exe
            .filter_resources_from_files(&logger, &files_refs, &glob_files_refs)
            .or_else(|e| {
                Err(RuntimeError {
                    code: "RUNTIME_ERROR",
                    message: e.to_string(),
                    label: "filter_from_files()".to_string(),
                }
                .into())
            })?;

        Ok(Value::new(None))
    }
}

starlark_module! { python_executable_env =>
    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.pip_install(env env, this, args, extra_envs=None) {
        this.downcast_apply(|exe: &PythonExecutable| {
            exe.starlark_pip_install(&env, &args, &extra_envs)
        })
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.read_package_root(
        env env,
        this,
        path,
        packages
    ) {
        this.downcast_apply(|exe: &PythonExecutable| {
            exe.starlark_read_package_root(&env, &path, &packages)
        })
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutabvle.read_virtualenv(
        env env,
        this,
        path
    ) {
        this.downcast_apply(|exe: &PythonExecutable| {
            exe.starlark_read_virtualenv(&env, &path)
        })
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.setup_py_install(
        env env,
        this,
        package_path,
        extra_envs=None,
        extra_global_arguments=None
    ) {
        this.downcast_apply(|exe: &PythonExecutable| {
            exe.starlark_setup_py_install(&env, &package_path, &extra_envs, &extra_global_arguments)
        })
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.add_in_memory_module_source(env env, this, module) {
        this.downcast_apply_mut(|exe: &mut PythonExecutable| {
            exe.starlark_add_in_memory_module_source(&env, &module)
        })
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.add_filesystem_relative_module_source(env env, this, prefix, module) {
        this.downcast_apply_mut(|exe: &mut PythonExecutable| {
            exe.starlark_add_filesystem_relative_module_source(&env, &prefix, &module)
        })
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.add_module_source(env env, this, module) {
        this.downcast_apply_mut(|exe: &mut PythonExecutable| {
            exe.starlark_add_module_source(&env, &module)
        })
    }

    // TODO consider unifying with add_module_source() so there only needs to be
    // a single function call.
    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.add_in_memory_module_bytecode(env env, this, module, optimize_level=0) {
        this.downcast_apply_mut(|exe: &mut PythonExecutable| {
            exe.starlark_add_in_memory_module_bytecode(&env, &module, &optimize_level)
        })
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.add_filesystem_relative_module_bytecode(env env, this, prefix, module, optimize_level=0) {
        this.downcast_apply_mut(|exe: &mut PythonExecutable| {
            exe.starlark_add_filesystem_relative_module_bytecode(&env, &prefix, &module, &optimize_level)
        })
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.add_module_bytecode(env env, this, module, optimize_level=0) {
        this.downcast_apply_mut(|exe: &mut PythonExecutable| {
            exe.starlark_add_module_bytecode(&env, &module, &optimize_level)
        })
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.add_in_memory_package_resource(env env, this, resource) {
        this.downcast_apply_mut(|exe: &mut PythonExecutable| {
            exe.starlark_add_in_memory_package_resource(&env, &resource)
        })
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.add_filesystem_relative_package_resource(env env, this, prefix, resource) {
        this.downcast_apply_mut(|exe: &mut PythonExecutable| {
            exe.starlark_add_filesystem_relative_package_resource(&env, &prefix, &resource)
        })
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.add_package_resource(env env, this, resource) {
        this.downcast_apply_mut(|exe: &mut PythonExecutable| {
            exe.starlark_add_package_resource(&env, &resource)
        })
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.add_in_memory_package_distribution_resource(env env, this, resource) {
        this.downcast_apply_mut(|exe: &mut PythonExecutable| {
            exe.starlark_add_in_memory_package_distribution_resource(&env, &resource)
        })
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.add_filesystem_relative_package_distribution_resource(env env, this, prefix, resource) {
        this.downcast_apply_mut(|exe: &mut PythonExecutable| {
            exe.starlark_add_filesystem_relative_package_distribution_resource(&env, &prefix, &resource)
        })
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.add_package_distribution_resource(env env, this, resource) {
        this.downcast_apply_mut(|exe: &mut PythonExecutable| {
            exe.starlark_add_package_distribution_resource(&env, &resource)
        })
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.add_in_memory_extension_module(env env, this, module) {
        this.downcast_apply_mut(|exe: &mut PythonExecutable| {
            exe.starlark_add_in_memory_extension_module(&env, &module)
        })
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.add_filesystem_relative_extension_module(env env, this, prefix, module) {
        this.downcast_apply_mut(|exe: &mut PythonExecutable| {
            exe.starlark_add_filesystem_relative_extension_module(&env, &prefix, &module)
        })
    }

    #[allow(clippy::ptr_arg)]
    PythonExecutable.add_extension_module(env env, this, module) {
        this.downcast_apply_mut(|exe: &mut PythonExecutable| {
            exe.starlark_add_extension_module(&env, &module)
        })
    }

    #[allow(clippy::ptr_arg)]
    PythonExecutable.add_in_memory_python_resource(
        env env,
        this,
        resource,
        add_source_module=true,
        add_bytecode_module=true,
        optimize_level=0
        )
    {
        this.downcast_apply_mut(|exe: &mut PythonExecutable| {
            exe.starlark_add_in_memory_python_resource(
                &env,
                &resource,
                &add_source_module,
                &add_bytecode_module,
                &optimize_level,
            )
        })
    }

    #[allow(clippy::ptr_arg)]
    PythonExecutable.add_filesystem_relative_python_resource(
        env env,
        this,
        prefix,
        resource,
        add_source_module=true,
        add_bytecode_module=true,
        optimize_level=0
        )
    {
        this.downcast_apply_mut(|exe: &mut PythonExecutable| {
            exe.starlark_add_filesystem_relative_python_resource(
                &env,
                &prefix,
                &resource,
                &add_source_module,
                &add_bytecode_module,
                &optimize_level,
            )
        })
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.add_python_resource(
        env env,
        this,
        resource,
        add_source_module=true,
        add_bytecode_module=true,
        optimize_level=0
    ) {
        this.downcast_apply_mut(|exe: &mut PythonExecutable| {
            exe.starlark_add_python_resource(
                &env,
                &resource,
                &add_source_module,
                &add_bytecode_module,
                &optimize_level
            )
        })
    }

    #[allow(clippy::ptr_arg)]
    PythonExecutable.add_in_memory_python_resources(
        env env,
        this,
        resources,
        add_source_module=true,
        add_bytecode_module=true,
        optimize_level=0
    ) {
        this.downcast_apply_mut(|exe: &mut PythonExecutable| {
            exe.starlark_add_in_memory_python_resources(
                &env,
                &resources,
                &add_source_module,
                &add_bytecode_module,
                &optimize_level,
            )
        })
    }

    #[allow(clippy::ptr_arg)]
    PythonExecutable.add_filesystem_relative_python_resources(
        env env,
        this,
        prefix,
        resources,
        add_source_module=true,
        add_bytecode_module=true,
        optimize_level=0
    ) {
        this.downcast_apply_mut(|exe: &mut PythonExecutable| {
            exe.starlark_add_filesystem_relative_python_resources(
                &env,
                &prefix,
                &resources,
                &add_source_module,
                &add_bytecode_module,
                &optimize_level,
            )
        })
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.add_python_resources(
        env env,
        this,
        resources,
        add_source_module=true,
        add_bytecode_module=true,
        optimize_level=0
    ) {
        this.downcast_apply_mut(|exe: &mut PythonExecutable| {
            exe.starlark_add_python_resources(
                &env,
                &resources,
                &add_source_module,
                &add_bytecode_module,
                &optimize_level,
            )
        })
    }

    #[allow(clippy::ptr_arg)]
    PythonExecutable.filter_resources_from_files(
        env env,
        this,
        files=None,
        glob_files=None)
    {
        this.downcast_apply_mut(|exe: &mut PythonExecutable| {
            exe.starlark_filter_resources_from_files(&env, &files, &glob_files)
        })
    }

    #[allow(clippy::ptr_arg)]
    PythonExecutable.to_embedded_resources(this) {
        this.downcast_apply(|exe: &PythonExecutable| {
            exe.starlark_to_embedded_resources()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::testutil::*;
    use super::*;

    #[test]
    fn test_default_values() {
        let mut env = starlark_env();

        starlark_eval_in_env(&mut env, "dist = default_python_distribution()").unwrap();

        let exe = starlark_eval_in_env(&mut env, "dist.to_python_executable('testapp')").unwrap();

        assert_eq!(exe.get_type(), "PythonExecutable");

        exe.downcast_apply(|exe: &PythonExecutable| {
            assert!(!exe.exe.in_memory_module_sources().is_empty());
            assert!(exe.exe.in_memory_package_resources().is_empty());
        });
    }

    #[test]
    fn test_no_sources() {
        let mut env = starlark_env();

        starlark_eval_in_env(&mut env, "dist = default_python_distribution()").unwrap();

        let exe = starlark_eval_in_env(
            &mut env,
            "dist.to_python_executable('testapp', include_sources=False)",
        )
        .unwrap();

        assert_eq!(exe.get_type(), "PythonExecutable");

        exe.downcast_apply(|exe: &PythonExecutable| {
            assert!(exe.exe.in_memory_module_sources().is_empty());
        });
    }

    #[test]
    fn test_pip_install_simple() {
        let mut env = starlark_env();

        starlark_eval_in_env(&mut env, "dist = default_python_distribution()").unwrap();

        starlark_eval_in_env(
            &mut env,
            "exe = dist.to_python_executable('testapp', include_sources=False)",
        )
        .unwrap();

        let resources =
            starlark_eval_in_env(&mut env, "exe.pip_install(['pyflakes==2.1.1'])").unwrap();
        assert_eq!(resources.get_type(), "list");

        let mut it = resources.into_iter().unwrap();

        let v = it.next().unwrap();
        assert_eq!(v.get_type(), "PythonSourceModule");
        v.downcast_apply(|x: &PythonSourceModule| {
            assert_eq!(x.module.name, "pyflakes");
            assert!(x.module.is_package);
        });
    }

    #[test]
    fn test_read_package_root_simple() -> Result<()> {
        let temp_dir = tempdir::TempDir::new("pyoxidizer-test")?;

        let root = temp_dir.path();
        std::fs::create_dir(root.join("bar"))?;
        let bar_init = root.join("bar").join("__init__.py");
        std::fs::write(&bar_init, "# bar")?;

        let foo_path = root.join("foo.py");
        std::fs::write(&foo_path, "# foo")?;

        let baz_path = root.join("baz.py");
        std::fs::write(&baz_path, "# baz")?;

        std::fs::create_dir(root.join("extra"))?;
        let extra_path = root.join("extra").join("__init__.py");
        std::fs::write(&extra_path, "# extra")?;

        let mut env = starlark_env();
        starlark_eval_in_env(&mut env, "dist = default_python_distribution()").unwrap();
        starlark_eval_in_env(
            &mut env,
            "exe = dist.to_python_executable('testapp', include_sources=False)",
        )
        .unwrap();

        let resources = starlark_eval_in_env(
            &mut env,
            &format!(
                "exe.read_package_root(\"{}\", packages=['foo', 'bar'])",
                root.display()
            ),
        )
        .unwrap();

        assert_eq!(resources.get_type(), "list");
        assert_eq!(resources.length().unwrap(), 2);

        let mut it = resources.into_iter().unwrap();

        let v = it.next().unwrap();
        assert_eq!(v.get_type(), "PythonSourceModule");
        v.downcast_apply(|x: &PythonSourceModule| {
            assert_eq!(x.module.name, "bar");
            assert!(x.module.is_package);
            assert_eq!(x.module.source.resolve().unwrap(), b"# bar");
        });

        let v = it.next().unwrap();
        assert_eq!(v.get_type(), "PythonSourceModule");
        v.downcast_apply(|x: &PythonSourceModule| {
            assert_eq!(x.module.name, "foo");
            assert!(!x.module.is_package);
            assert_eq!(x.module.source.resolve().unwrap(), b"# foo");
        });

        Ok(())
    }
}
