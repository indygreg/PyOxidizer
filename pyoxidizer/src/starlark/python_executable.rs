// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::starlark::python_resource::ResourceCollectionContext;
use {
    super::env::{get_context, EnvironmentContext},
    super::python_embedded_resources::PythonEmbeddedResources,
    super::python_resource::{
        python_resource_to_value, OptionalResourceLocation, PythonExtensionModuleValue,
        PythonPackageDistributionResourceValue, PythonPackageResourceValue,
        PythonSourceModuleValue,
    },
    super::target::{BuildContext, BuildTarget, ResolvedTarget, RunMode},
    super::util::{
        optional_dict_arg, optional_list_arg, required_bool_arg, required_list_arg,
        required_str_arg, required_type_arg,
    },
    crate::project_building::build_python_executable,
    crate::py_packaging::binary::PythonBinaryBuilder,
    anyhow::{Context, Result},
    python_packaging::resource::{
        BytecodeOptimizationLevel, DataLocation, PythonModuleBytecodeFromSource, PythonModuleSource,
    },
    slog::{info, warn},
    starlark::environment::TypeValues,
    starlark::values::error::{RuntimeError, ValueError, INCORRECT_PARAMETER_TYPE_ERROR_CODE},
    starlark::values::none::NoneType,
    starlark::values::{Mutable, TypedValue, Value, ValueResult},
    starlark::{
        starlark_fun, starlark_module, starlark_parse_param_type, starlark_signature,
        starlark_signature_extraction, starlark_signatures,
    },
    std::collections::HashMap,
    std::convert::TryFrom,
    std::io::Write,
    std::ops::Deref,
    std::path::{Path, PathBuf},
};

/// Represents a builder for a Python executable.
pub struct PythonExecutable {
    pub exe: Box<dyn PythonBinaryBuilder>,
}

impl TypedValue for PythonExecutable {
    type Holder = Mutable<PythonExecutable>;
    const TYPE: &'static str = "PythonExecutable";

    fn values_for_descendant_check_and_freeze(&self) -> Box<dyn Iterator<Item = Value>> {
        Box::new(std::iter::empty())
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
    /// PythonExecutable.make_python_source_module(name, source, is_package=false)
    pub fn starlark_make_python_source_module(
        &self,
        name: &Value,
        source: &Value,
        is_package: &Value,
    ) -> ValueResult {
        let name = required_str_arg("name", &name)?;
        let source = required_str_arg("source", &source)?;
        let is_package = required_bool_arg("is_package", &is_package)?;

        let module = PythonModuleSource {
            name,
            source: DataLocation::Memory(source.into_bytes()),
            is_package,
            cache_tag: self.exe.cache_tag().to_string(),
            is_stdlib: false,
            is_test: false,
        };

        let mut value = PythonSourceModuleValue::new(module);
        value.apply_packaging_policy(self.exe.python_packaging_policy());

        Ok(Value::new(value))
    }

    /// PythonExecutable.pip_install(args, extra_envs=None)
    pub fn starlark_pip_install(
        &self,
        type_values: &TypeValues,
        args: &Value,
        extra_envs: &Value,
    ) -> ValueResult {
        required_list_arg("args", "string", &args)?;
        optional_dict_arg("extra_envs", "string", "string", &extra_envs)?;

        let args: Vec<String> = args.iter()?.iter().map(|x| x.to_string()).collect();

        let extra_envs = match extra_envs.get_type() {
            "dict" => extra_envs
                .iter()?
                .iter()
                .map(|key| {
                    let k = key.to_string();
                    let v = extra_envs.at(key).unwrap().to_string();
                    (k, v)
                })
                .collect(),
            "NoneType" => HashMap::new(),
            _ => panic!("should have validated type above"),
        };

        let raw_context = get_context(type_values)?;
        let context = raw_context
            .downcast_ref::<EnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        let resources = self
            .exe
            .pip_install(&context.logger, context.verbose, &args, &extra_envs)
            .map_err(|e| {
                ValueError::from(RuntimeError {
                    code: "PIP_INSTALL_ERROR",
                    message: format!("error running pip install: {}", e),
                    label: "pip_install()".to_string(),
                })
            })?;

        Ok(Value::from(
            resources
                .iter()
                .map(|r| python_resource_to_value(r, self.exe.python_packaging_policy()))
                .collect::<Vec<Value>>(),
        ))
    }

    /// PythonExecutable.read_package_root(path, packages)
    pub fn starlark_read_package_root(
        &self,
        type_values: &TypeValues,
        path: &Value,
        packages: &Value,
    ) -> ValueResult {
        let path = required_str_arg("path", &path)?;
        required_list_arg("packages", "string", &packages)?;

        let packages = packages
            .iter()?
            .iter()
            .map(|x| x.to_string())
            .collect::<Vec<String>>();

        let raw_context = get_context(type_values)?;
        let context = raw_context
            .downcast_ref::<EnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        let resources = self
            .exe
            .read_package_root(&context.logger, Path::new(&path), &packages)
            .map_err(|e| {
                ValueError::from(RuntimeError {
                    code: "PACKAGE_ROOT_ERROR",
                    message: format!("could not find resources: {}", e),
                    label: "read_package_root()".to_string(),
                })
            })?;

        Ok(Value::from(
            resources
                .iter()
                .map(|r| python_resource_to_value(r, self.exe.python_packaging_policy()))
                .collect::<Vec<Value>>(),
        ))
    }

    /// PythonExecutable.read_virtualenv(path)
    pub fn starlark_read_virtualenv(&self, type_values: &TypeValues, path: &Value) -> ValueResult {
        let path = required_str_arg("path", &path)?;

        let raw_context = get_context(type_values)?;
        let context = raw_context
            .downcast_ref::<EnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        let resources = self
            .exe
            .read_virtualenv(&context.logger, &Path::new(&path))
            .map_err(|e| {
                ValueError::from(RuntimeError {
                    code: "VIRTUALENV_ERROR",
                    message: format!("could not find resources: {}", e),
                    label: "read_virtualenv()".to_string(),
                })
            })?;

        Ok(Value::from(
            resources
                .iter()
                .map(|r| python_resource_to_value(r, self.exe.python_packaging_policy()))
                .collect::<Vec<Value>>(),
        ))
    }

    /// PythonExecutable.setup_py_install(package_path, extra_envs=None, extra_global_arguments=None)
    pub fn starlark_setup_py_install(
        &self,
        type_values: &TypeValues,
        package_path: &Value,
        extra_envs: &Value,
        extra_global_arguments: &Value,
    ) -> ValueResult {
        let package_path = required_str_arg("package_path", &package_path)?;
        optional_dict_arg("extra_envs", "string", "string", &extra_envs)?;
        optional_list_arg("extra_global_arguments", "string", &extra_global_arguments)?;

        let extra_envs = match extra_envs.get_type() {
            "dict" => extra_envs
                .iter()?
                .iter()
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
                .iter()?
                .iter()
                .map(|x| x.to_string())
                .collect(),
            "NoneType" => Vec::new(),
            _ => panic!("should have validated type above"),
        };

        let package_path = PathBuf::from(package_path);

        let raw_context = get_context(type_values)?;
        let context = raw_context
            .downcast_ref::<EnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        let package_path = if package_path.is_absolute() {
            package_path
        } else {
            PathBuf::from(&context.cwd).join(package_path)
        };

        let resources = self
            .exe
            .setup_py_install(
                &context.logger,
                &package_path,
                context.verbose,
                &extra_envs,
                &extra_global_arguments,
            )
            .map_err(|e| {
                ValueError::from(RuntimeError {
                    code: "SETUP_PY_ERROR",
                    message: e.to_string(),
                    label: "setup_py_install()".to_string(),
                })
            })?;

        warn!(
            &context.logger,
            "collected {} resources from setup.py install",
            resources.len()
        );

        Ok(Value::from(
            resources
                .iter()
                .map(|r| python_resource_to_value(r, self.exe.python_packaging_policy()))
                .collect::<Vec<Value>>(),
        ))
    }

    /// PythonExecutable.add_python_module_source(module, location=None)
    pub fn starlark_add_python_module_source(
        &mut self,
        type_values: &TypeValues,
        module: &Value,
        location: &Value,
    ) -> ValueResult {
        required_type_arg("module", "PythonSourceModule", &module)?;
        let location = OptionalResourceLocation::try_from(location)?;

        let raw_context = get_context(type_values)?;
        let context = raw_context
            .downcast_ref::<EnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        let m = match module.downcast_ref::<PythonSourceModuleValue>() {
            Some(m) => Ok(m.inner.clone()),
            None => Err(ValueError::IncorrectParameterType),
        }?;
        info!(&context.logger, "adding Python source module {}", m.name);
        self.exe
            .add_python_module_source(&m, location.into())
            .map_err(|e| {
                ValueError::from(RuntimeError {
                    code: "PYOXIDIZER_BUILD",
                    message: e.to_string(),
                    label: "add_python_module_source".to_string(),
                })
            })?;

        Ok(Value::new(NoneType::None))
    }

    /// PythonExecutable.add_python_module_bytecode(module, optimize_level=0, location=None)
    pub fn starlark_add_python_module_bytecode(
        &mut self,
        type_values: &TypeValues,
        module: &Value,
        optimize_level: &Value,
        location: &Value,
    ) -> ValueResult {
        required_type_arg("module", "PythonSourceModule", &module)?;
        required_type_arg("optimize_level", "int", &optimize_level)?;
        let location = OptionalResourceLocation::try_from(location)?;

        let raw_context = get_context(type_values)?;
        let context = raw_context
            .downcast_ref::<EnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        let optimize_level = optimize_level.to_int().unwrap();

        let optimize_level = match optimize_level {
            0 => BytecodeOptimizationLevel::Zero,
            1 => BytecodeOptimizationLevel::One,
            2 => BytecodeOptimizationLevel::Two,
            i => {
                return Err(ValueError::from(RuntimeError {
                    code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                    message: format!("optimize_level must be 0, 1, or 2: got {}", i),
                    label: "invalid optimize_level value".to_string(),
                }));
            }
        };

        let m = match module.downcast_ref::<PythonSourceModuleValue>() {
            Some(m) => Ok(m.inner.clone()),
            None => Err(ValueError::IncorrectParameterType),
        }?;
        info!(&context.logger, "adding bytecode module {}", m.name);
        self.exe
            .add_python_module_bytecode_from_source(
                &PythonModuleBytecodeFromSource {
                    name: m.name.clone(),
                    source: m.source.clone(),
                    optimize_level,
                    is_package: m.is_package,
                    cache_tag: m.cache_tag,
                    is_stdlib: m.is_stdlib,
                    is_test: m.is_test,
                },
                location.into(),
            )
            .map_err(|e| {
                ValueError::from(RuntimeError {
                    code: "PYOXIDIZER_BUILD",
                    message: e.to_string(),
                    label: "add_python_module_bytecode".to_string(),
                })
            })?;

        Ok(Value::new(NoneType::None))
    }

    /// PythonExecutable.add_python_package_resource(resource, location=None)
    pub fn starlark_add_python_package_resource(
        &mut self,
        type_values: &TypeValues,
        resource: &Value,
        location: &Value,
    ) -> ValueResult {
        required_type_arg("resource", "PythonPackageResource", &resource)?;
        let location = OptionalResourceLocation::try_from(location)?;

        let raw_context = get_context(type_values)?;
        let context = raw_context
            .downcast_ref::<EnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        let r = match resource.downcast_ref::<PythonPackageResourceValue>() {
            Some(r) => Ok(r.inner.clone()),
            None => Err(ValueError::IncorrectParameterType),
        }?;
        info!(
            &context.logger,
            "adding resource data {}",
            r.symbolic_name()
        );
        self.exe
            .add_python_package_resource(&r, location.into())
            .map_err(|e| {
                ValueError::from(RuntimeError {
                    code: "PYOXIDIZER_BUILD",
                    message: e.to_string(),
                    label: "add_python_package_resource".to_string(),
                })
            })?;

        Ok(Value::new(NoneType::None))
    }

    /// PythonExecutable.add_python_package_distribution_resource(resource, location=None)
    pub fn starlark_add_python_package_distribution_resource(
        &mut self,
        type_values: &TypeValues,
        resource: &Value,
        location: &Value,
    ) -> ValueResult {
        required_type_arg("resource", "PythonPackageDistributionResource", &resource)?;
        let location = OptionalResourceLocation::try_from(location)?;

        let raw_context = get_context(type_values)?;
        let context = raw_context
            .downcast_ref::<EnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        let r = match resource.downcast_ref::<PythonPackageDistributionResourceValue>() {
            Some(r) => Ok(r.inner.clone()),
            None => Err(ValueError::IncorrectParameterType),
        }?;
        info!(
            &context.logger,
            "adding package distribution resource {}:{}", r.package, r.name
        );
        self.exe
            .add_python_package_distribution_resource(&r, location.into())
            .map_err(|e| {
                ValueError::from(RuntimeError {
                    code: "PYOXIDIZER_BUILD",
                    message: e.to_string(),
                    label: "add_python_package_distribution_resource".to_string(),
                })
            })?;

        Ok(Value::new(NoneType::None))
    }

    /// PythonExecutable.add_python_extension_module(module)
    pub fn starlark_add_python_extension_module(
        &mut self,
        type_values: &TypeValues,
        module: &Value,
        location: &Value,
    ) -> ValueResult {
        required_type_arg("module", "PythonExtensionModule", &module)?;
        let location = OptionalResourceLocation::try_from(location)?;

        let raw_context = get_context(type_values)?;
        let context = raw_context
            .downcast_ref::<EnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        let m = match module.downcast_ref::<PythonExtensionModuleValue>() {
            Some(m) => Ok(m.inner.clone()),
            None => Err(ValueError::IncorrectParameterType),
        }?;
        info!(&context.logger, "adding extension module {}", m.name);
        self.exe
            .add_python_extension_module(&m, location.into())
            .map_err(|e| {
                ValueError::from(RuntimeError {
                    code: "PYOXIDIZER_BUILD",
                    message: e.to_string(),
                    label: "add_python_extension_module".to_string(),
                })
            })?;

        Ok(Value::new(NoneType::None))
    }

    /// PythonExecutable.add_python_resource(resource, add_source_module=true, add_bytecode_module=true, optimize_level=0, location=None)
    pub fn starlark_add_python_resource(
        &mut self,
        type_values: &TypeValues,
        resource: &Value,
        add_source_module: &Value,
        add_bytecode_module: &Value,
        optimize_level: &Value,
        location: &Value,
    ) -> ValueResult {
        let add_source_module = required_bool_arg("add_source_module", &add_source_module)?;
        let add_bytecode_module = required_bool_arg("add_bytecode_module", &add_bytecode_module)?;
        required_type_arg("optimize_level", "int", &optimize_level)?;
        let location = OptionalResourceLocation::try_from(location)?;

        match resource.get_type() {
            "PythonSourceModule" => {
                if add_source_module {
                    self.starlark_add_python_module_source(
                        type_values,
                        resource,
                        &Value::from(&location),
                    )?;
                }
                if add_bytecode_module {
                    self.starlark_add_python_module_bytecode(
                        type_values,
                        resource,
                        optimize_level,
                        &Value::from(&location),
                    )?;
                }

                Ok(Value::new(NoneType::None))
            }
            "PythonBytecodeModule" => self.starlark_add_python_module_bytecode(
                type_values,
                resource,
                optimize_level,
                &Value::from(&location),
            ),
            "PythonPackageResource" => self.starlark_add_python_package_resource(
                type_values,
                resource,
                &Value::from(&location),
            ),
            "PythonPackageDistributionResource" => self
                .starlark_add_python_package_distribution_resource(
                    type_values,
                    resource,
                    &Value::from(&location),
                ),
            "PythonExtensionModule" => self.starlark_add_python_extension_module(
                type_values,
                resource,
                &Value::from(&location),
            ),
            _ => Err(ValueError::from(RuntimeError {
                code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                message: "resource argument must be a Python resource type".to_string(),
                label: ".add_python_resource()".to_string(),
            })),
        }
    }

    /// PythonExecutable.add_python_resources(resources, add_source_module=true, add_bytecode_module=true, optimize_level=0, location=None)
    pub fn starlark_add_python_resources(
        &mut self,
        type_values: &TypeValues,
        resources: &Value,
        add_source_module: &Value,
        add_bytecode_module: &Value,
        optimize_level: &Value,
        location: &Value,
    ) -> ValueResult {
        required_bool_arg("add_source_module", &add_source_module)?;
        required_bool_arg("add_bytecode_module", &add_bytecode_module)?;
        required_type_arg("optimize_level", "int", &optimize_level)?;

        for resource in &resources.iter()? {
            self.starlark_add_python_resource(
                type_values,
                &resource,
                add_source_module,
                add_bytecode_module,
                optimize_level,
                &location,
            )?;
        }

        Ok(Value::new(NoneType::None))
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
        type_values: &TypeValues,
        files: &Value,
        glob_files: &Value,
    ) -> ValueResult {
        optional_list_arg("files", "string", &files)?;
        optional_list_arg("glob_files", "string", &glob_files)?;

        let files = match files.get_type() {
            "list" => files
                .iter()?
                .iter()
                .map(|x| PathBuf::from(x.to_string()))
                .collect(),
            "NoneType" => Vec::new(),
            _ => panic!("type should have been validated above"),
        };

        let glob_files = match glob_files.get_type() {
            "list" => glob_files.iter()?.iter().map(|x| x.to_string()).collect(),
            "NoneType" => Vec::new(),
            _ => panic!("type should have been validated above"),
        };

        let files_refs = files.iter().map(|x| x.as_ref()).collect::<Vec<&Path>>();
        let glob_files_refs = glob_files.iter().map(|x| x.as_ref()).collect::<Vec<&str>>();

        let raw_context = get_context(type_values)?;
        let context = raw_context
            .downcast_ref::<EnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        self.exe
            .filter_resources_from_files(&context.logger, &files_refs, &glob_files_refs)
            .map_err(|e| {
                ValueError::from(RuntimeError {
                    code: "RUNTIME_ERROR",
                    message: e.to_string(),
                    label: "filter_from_files()".to_string(),
                })
            })?;

        Ok(Value::new(NoneType::None))
    }
}

starlark_module! { python_executable_env =>
    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.make_python_source_module(this, name, source, is_package=false) {
        match this.clone().downcast_ref::<PythonExecutable>() {
            Some(exe) => exe.starlark_make_python_source_module(&name, &source, &is_package),
            None => Err(ValueError::IncorrectParameterType),
        }
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.pip_install(env env, this, args, extra_envs=NoneType::None) {
        match this.clone().downcast_ref::<PythonExecutable>() {
            Some(exe) => exe.starlark_pip_install(&env, &args, &extra_envs),
            None => Err(ValueError::IncorrectParameterType),
        }
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.read_package_root(
        env env,
        this,
        path,
        packages
    ) {
        match this.clone().downcast_ref::<PythonExecutable>() {
            Some(exe) => exe.starlark_read_package_root(&env, &path, &packages),
            None => Err(ValueError::IncorrectParameterType),
        }
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutabvle.read_virtualenv(
        env env,
        this,
        path
    ) {
        match this.clone().downcast_ref::<PythonExecutable>() {
            Some(exe) => exe.starlark_read_virtualenv(&env, &path),
            None => Err(ValueError::IncorrectParameterType),
        }
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.setup_py_install(
        env env,
        this,
        package_path,
        extra_envs=NoneType::None,
        extra_global_arguments=NoneType::None
    ) {
        match this.clone().downcast_ref::<PythonExecutable>() {
            Some(exe) => exe.starlark_setup_py_install(&env, &package_path, &extra_envs, &extra_global_arguments),
            None => Err(ValueError::IncorrectParameterType),
        }
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.add_python_module_source(env env, this, module, location=NoneType::None) {
        match this.clone().downcast_mut::<PythonExecutable>()? {
            Some(mut exe) => exe.starlark_add_python_module_source(&env, &module, &location),
            None => Err(ValueError::IncorrectParameterType),
        }
    }

    // TODO consider unifying with add_python_module_source() so there only needs to be
    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.add_python_module_bytecode(env env, this, module, optimize_level=0, location=NoneType::None) {
        match this.clone().downcast_mut::<PythonExecutable>()? {
            Some(mut exe) => exe.starlark_add_python_module_bytecode(&env, &module, &optimize_level, &location),
            None => Err(ValueError::IncorrectParameterType),
        }
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.add_python_package_resource(env env, this, resource, location=NoneType::None) {
        match this.clone().downcast_mut::<PythonExecutable>()? {
            Some(mut exe) => exe.starlark_add_python_package_resource(&env, &resource, &location),
            None => Err(ValueError::IncorrectParameterType),
        }
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.add_python_package_distribution_resource(env env, this, resource, location=NoneType::None) {
        match this.clone().downcast_mut::<PythonExecutable>()? {
            Some(mut exe) => exe.starlark_add_python_package_distribution_resource(&env, &resource, &location),
            None => Err(ValueError::IncorrectParameterType),
        }
    }

    #[allow(clippy::ptr_arg)]
    PythonExecutable.add_python_extension_module(env env, this, module, location=NoneType::None) {
        match this.clone().downcast_mut::<PythonExecutable>()? {
            Some(mut exe) => exe.starlark_add_python_extension_module(&env, &module, &location),
            None => Err(ValueError::IncorrectParameterType),
        }
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.add_python_resource(
        env env,
        this,
        resource,
        add_source_module=true,
        add_bytecode_module=true,
        optimize_level=0,
        location=NoneType::None
    ) {
        match this.clone().downcast_mut::<PythonExecutable>()? {
            Some(mut exe) => exe.starlark_add_python_resource(
                &env,
                &resource,
                &add_source_module,
                &add_bytecode_module,
                &optimize_level,
                &location,
            ),
            None => Err(ValueError::IncorrectParameterType),
        }
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.add_python_resources(
        env env,
        this,
        resources,
        add_source_module=true,
        add_bytecode_module=true,
        optimize_level=0,
        location=NoneType::None
    ) {
        match this.clone().downcast_mut::<PythonExecutable>()? {
            Some(mut exe) => exe.starlark_add_python_resources(
                &env,
                &resources,
                &add_source_module,
                &add_bytecode_module,
                &optimize_level,
                &location,
            ),
            None => Err(ValueError::IncorrectParameterType),
        }
    }

    #[allow(clippy::ptr_arg)]
    PythonExecutable.filter_resources_from_files(
        env env,
        this,
        files=NoneType::None,
        glob_files=NoneType::None)
    {
        match this.clone().downcast_mut::<PythonExecutable>()? {
            Some(mut exe) => exe.starlark_filter_resources_from_files(&env, &files, &glob_files),
            None => Err(ValueError::IncorrectParameterType),
        }
    }

    #[allow(clippy::ptr_arg)]
    PythonExecutable.to_embedded_resources(this) {
        match this.clone().downcast_ref::<PythonExecutable>() {
            Some(exe) => exe.starlark_to_embedded_resources(),
            None => Err(ValueError::IncorrectParameterType),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::testutil::*;
    use super::*;

    #[test]
    fn test_default_values() {
        let (mut env, type_values) = starlark_env();

        starlark_eval_in_env(
            &mut env,
            &type_values,
            "dist = default_python_distribution()",
        )
        .unwrap();

        let exe = starlark_eval_in_env(
            &mut env,
            &type_values,
            "dist.to_python_executable('testapp')",
        )
        .unwrap();

        assert_eq!(exe.get_type(), "PythonExecutable");

        let exe = exe.downcast_ref::<PythonExecutable>().unwrap();
        assert!(exe
            .exe
            .iter_resources()
            .any(|(_, r)| r.in_memory_source.is_some()));
        assert!(exe
            .exe
            .iter_resources()
            .all(|(_, r)| r.in_memory_resources.is_none()));
    }

    #[test]
    fn test_no_sources() {
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

        starlark_eval_in_env(
            &mut env,
            &type_values,
            "policy.include_distribution_sources = False",
        )
        .unwrap();

        let exe = starlark_eval_in_env(
            &mut env,
            &type_values,
            "dist.to_python_executable('testapp', packaging_policy=policy)",
        )
        .unwrap();

        assert_eq!(exe.get_type(), "PythonExecutable");

        let exe = exe.downcast_ref::<PythonExecutable>().unwrap();
        assert!(exe
            .exe
            .iter_resources()
            .all(|(_, r)| r.in_memory_source.is_none()));
    }

    #[test]
    fn test_make_python_source_module() {
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
            "exe = dist.to_python_executable('testapp')",
        )
        .unwrap();

        let m = starlark_eval_in_env(
            &mut env,
            &type_values,
            "exe.make_python_source_module('foo', 'import bar')",
        )
        .unwrap();

        assert_eq!(m.get_type(), "PythonSourceModule");
        assert_eq!(m.get_attr("name").unwrap().to_str(), "foo");
        assert_eq!(m.get_attr("source").unwrap().to_str(), "import bar");
        assert_eq!(m.get_attr("is_package").unwrap().to_bool(), false);
    }

    #[test]
    fn test_pip_install_simple() {
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

        starlark_eval_in_env(
            &mut env,
            &type_values,
            "policy.include_distribution_sources = False",
        )
        .unwrap();

        starlark_eval_in_env(
            &mut env,
            &type_values,
            "exe = dist.to_python_executable('testapp', packaging_policy = policy)",
        )
        .unwrap();

        let resources = starlark_eval_in_env(
            &mut env,
            &type_values,
            "exe.pip_install(['pyflakes==2.1.1'])",
        )
        .unwrap();
        assert_eq!(resources.get_type(), "list");

        let raw_it = resources.iter().unwrap();
        let mut it = raw_it.iter();

        let v = it.next().unwrap();
        assert_eq!(v.get_type(), "PythonSourceModule");
        let x = v.downcast_ref::<PythonSourceModuleValue>().unwrap();
        assert_eq!(x.inner.name, "pyflakes");
        assert!(x.inner.is_package);
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
        starlark_eval_in_env(
            &mut env,
            &type_values,
            "policy.include_distribution_sources = False",
        )
        .unwrap();
        starlark_eval_in_env(
            &mut env,
            &type_values,
            "exe = dist.to_python_executable('testapp', packaging_policy = policy)",
        )
        .unwrap();

        let resources = starlark_eval_in_env(
            &mut env,
            &type_values,
            &format!(
                "exe.read_package_root(\"{}\", packages=['foo', 'bar'])",
                root.display()
            ),
        )
        .unwrap();

        assert_eq!(resources.get_type(), "list");
        assert_eq!(resources.length().unwrap(), 2);

        let raw_it = resources.iter().unwrap();
        let mut it = raw_it.iter();

        let v = it.next().unwrap();
        assert_eq!(v.get_type(), "PythonSourceModule");
        let x = v.downcast_ref::<PythonSourceModuleValue>().unwrap();
        assert_eq!(x.inner.name, "bar");
        assert!(x.inner.is_package);
        assert_eq!(x.inner.source.resolve().unwrap(), b"# bar");

        let v = it.next().unwrap();
        assert_eq!(v.get_type(), "PythonSourceModule");
        let x = v.downcast_ref::<PythonSourceModuleValue>().unwrap();
        assert_eq!(x.inner.name, "foo");
        assert!(!x.inner.is_package);
        assert_eq!(x.inner.source.resolve().unwrap(), b"# foo");

        Ok(())
    }
}
