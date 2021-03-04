// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    super::{
        env::{get_context, PyOxidizerEnvironmentContext},
        file_resource::file_manifest_add_python_executable,
        python_embedded_resources::PythonEmbeddedResourcesValue,
        python_packaging_policy::PythonPackagingPolicyValue,
        python_resource::{
            is_resource_starlark_compatible, python_resource_to_value, FileValue,
            PythonExtensionModuleValue, PythonModuleSourceValue,
            PythonPackageDistributionResourceValue, PythonPackageResourceValue,
            ResourceCollectionContext,
        },
    },
    crate::{
        project_building::build_python_executable,
        py_packaging::binary::PythonBinaryBuilder,
        py_packaging::binary::{PackedResourcesLoadMode, WindowsRuntimeDllsMode},
    },
    anyhow::{anyhow, Context, Result},
    linked_hash_map::LinkedHashMap,
    python_packaging::resource::PythonModuleSource,
    slog::{info, warn},
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
        {
            starlark_fun, starlark_module, starlark_parse_param_type, starlark_signature,
            starlark_signature_extraction, starlark_signatures,
        },
    },
    starlark_dialect_build_targets::{
        optional_dict_arg, optional_list_arg, optional_type_arg, required_list_arg, ResolvedTarget,
        ResolvedTargetValue, RunMode, ToOptional,
    },
    std::{
        collections::HashMap,
        convert::TryFrom,
        io::Write,
        ops::Deref,
        path::{Path, PathBuf},
    },
    tugger::starlark::{
        file_resource::FileManifestValue, wix_bundle_builder::WiXBundleBuilderValue,
        wix_msi_builder::WiXMsiBuilderValue,
    },
    tugger_file_manifest::FileData,
};

/// Represents a builder for a Python executable.
pub struct PythonExecutableValue {
    pub exe: Box<dyn PythonBinaryBuilder>,

    /// The Starlark Value for the Python packaging policy.
    // This is stored as a Vec because I couldn't figure out how to implement
    // values_for_descendant_check_and_freeze() without the borrow checker
    // complaining due to a temporary vec/array.
    policy: Vec<Value>,
}

impl PythonExecutableValue {
    pub fn new(exe: Box<dyn PythonBinaryBuilder>, policy: PythonPackagingPolicyValue) -> Self {
        Self {
            exe,
            policy: vec![Value::new(policy)],
        }
    }

    /// Obtains a copy of the `PythonPackagingPolicyValue` stored internally.
    pub fn python_packaging_policy(&self) -> PythonPackagingPolicyValue {
        self.policy[0]
            .downcast_ref::<PythonPackagingPolicyValue>()
            .unwrap()
            .clone()
    }

    pub fn build_internal(
        &self,
        type_values: &TypeValues,
        target: &str,
        context: &PyOxidizerEnvironmentContext,
    ) -> Result<ResolvedTarget> {
        // Build an executable by writing out a temporary Rust project
        // and building it.
        let build = build_python_executable(
            context.logger(),
            &self.exe.name(),
            self.exe.deref(),
            &context.build_target_triple,
            &context.build_opt_level,
            context.build_release,
        )?;

        let output_path = context
            .get_output_path(type_values, target)
            .map_err(|_| anyhow!("unable to resolve output path"))?;
        let dest_path = output_path.join(build.exe_name);
        warn!(
            context.logger(),
            "writing executable to {}",
            dest_path.display()
        );
        let mut fh = std::fs::File::create(&dest_path)
            .context(format!("creating {}", dest_path.display()))?;
        fh.write_all(&build.exe_data)
            .context(format!("writing {}", dest_path.display()))?;

        tugger_file_manifest::set_executable(&mut fh).context("making binary executable")?;

        Ok(ResolvedTarget {
            run_mode: RunMode::Path { path: dest_path },
            output_path,
        })
    }
}

impl TypedValue for PythonExecutableValue {
    type Holder = Mutable<PythonExecutableValue>;
    const TYPE: &'static str = "PythonExecutable";

    fn values_for_descendant_check_and_freeze<'a>(
        &'a self,
    ) -> Box<dyn Iterator<Item = Value> + 'a> {
        Box::new(self.policy.iter().cloned())
    }

    fn get_attr(&self, attribute: &str) -> ValueResult {
        match attribute {
            "packed_resources_load_mode" => Ok(Value::from(
                self.exe.packed_resources_load_mode().to_string(),
            )),
            "tcl_files_path" => match self.exe.tcl_files_path() {
                Some(value) => Ok(Value::from(value.to_string())),
                None => Ok(Value::from(NoneType::None)),
            },
            "windows_runtime_dlls_mode" => Ok(Value::from(
                self.exe.windows_runtime_dlls_mode().to_string(),
            )),
            "windows_subsystem" => Ok(Value::from(self.exe.windows_subsystem())),
            _ => Err(ValueError::OperationNotSupported {
                op: UnsupportedOperation::GetAttr(attribute.to_string()),
                left: Self::TYPE.to_string(),
                right: None,
            }),
        }
    }

    fn has_attr(&self, attribute: &str) -> Result<bool, ValueError> {
        Ok(matches!(
            attribute,
            "packed_resources_load_mode"
                | "tcl_files_path"
                | "windows_runtime_dlls_mode"
                | "windows_subsystem"
        ))
    }

    fn set_attr(&mut self, attribute: &str, value: Value) -> Result<(), ValueError> {
        match attribute {
            "packed_resources_load_mode" => {
                self.exe.set_packed_resources_load_mode(
                    PackedResourcesLoadMode::try_from(value.to_string().as_str()).map_err(|e| {
                        ValueError::from(RuntimeError {
                            code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                            message: e,
                            label: format!("{}.{}", Self::TYPE, attribute),
                        })
                    })?,
                );

                Ok(())
            }
            "tcl_files_path" => {
                self.exe.set_tcl_files_path(value.to_optional());

                Ok(())
            }
            "windows_runtime_dlls_mode" => {
                self.exe.set_windows_runtime_dlls_mode(
                    WindowsRuntimeDllsMode::try_from(value.to_string().as_str()).map_err(|e| {
                        ValueError::from(RuntimeError {
                            code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                            message: e,
                            label: format!("{}.{}", Self::TYPE, attribute),
                        })
                    })?,
                );

                Ok(())
            }
            "windows_subsystem" => {
                self.exe
                    .set_windows_subsystem(value.to_string().as_str())
                    .map_err(|e| {
                        ValueError::from(RuntimeError {
                            code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                            message: format!("{:?}", e),
                            label: format!("{}.{}", Self::TYPE, attribute),
                        })
                    })?;

                Ok(())
            }
            _ => Err(ValueError::OperationNotSupported {
                op: UnsupportedOperation::SetAttr(attribute.to_string()),
                left: Self::TYPE.to_string(),
                right: None,
            }),
        }
    }
}

// Starlark functions.
impl PythonExecutableValue {
    fn build(&self, type_values: &TypeValues, target: String) -> ValueResult {
        let pyoxidizer_context_value = get_context(type_values)?;
        let pyoxidizer_context = pyoxidizer_context_value
            .downcast_ref::<PyOxidizerEnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        Ok(Value::new(ResolvedTargetValue {
            inner: self
                .build_internal(type_values, &target, &pyoxidizer_context)
                .map_err(|e| {
                    ValueError::from(RuntimeError {
                        code: "PYOXIDIZER",
                        message: e.to_string(),
                        label: "build()".to_string(),
                    })
                })?,
        }))
    }

    /// PythonExecutable.make_python_module_source(name, source, is_package=false)
    pub fn make_python_module_source(
        &self,
        type_values: &TypeValues,
        call_stack: &mut CallStack,
        name: String,
        source: String,
        is_package: bool,
    ) -> ValueResult {
        let module = PythonModuleSource {
            name,
            source: FileData::Memory(source.into_bytes()),
            is_package,
            cache_tag: self.exe.cache_tag().to_string(),
            is_stdlib: false,
            is_test: false,
        };

        let mut value = PythonModuleSourceValue::new(module);
        self.python_packaging_policy()
            .apply_to_resource(type_values, call_stack, &mut value)?;

        Ok(Value::new(value))
    }

    /// PythonExecutable.pip_download(args)
    pub fn pip_download(
        &mut self,
        type_values: &TypeValues,
        call_stack: &mut CallStack,
        args: &Value,
    ) -> ValueResult {
        required_list_arg("args", "string", &args)?;

        let args: Vec<String> = args.iter()?.iter().map(|x| x.to_string()).collect();

        let pyoxidizer_context_value = get_context(type_values)?;
        let pyoxidizer_context = pyoxidizer_context_value
            .downcast_ref::<PyOxidizerEnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        let python_packaging_policy = self.python_packaging_policy();

        let resources = self
            .exe
            .pip_download(
                pyoxidizer_context.logger(),
                pyoxidizer_context.verbose,
                &args,
            )
            .map_err(|e| {
                ValueError::from(RuntimeError {
                    code: "PIP_INSTALL_ERROR",
                    message: format!("error running pip install: {}", e),
                    label: "pip_install()".to_string(),
                })
            })?
            .iter()
            .filter(|r| is_resource_starlark_compatible(r))
            .map(|r| python_resource_to_value(type_values, call_stack, r, &python_packaging_policy))
            .collect::<Result<Vec<Value>, ValueError>>()?;

        Ok(Value::from(resources))
    }

    /// PythonExecutable.pip_install(args, extra_envs=None)
    pub fn pip_install(
        &mut self,
        type_values: &TypeValues,
        call_stack: &mut CallStack,
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

        let pyoxidizer_context_value = get_context(type_values)?;
        let pyoxidizer_context = pyoxidizer_context_value
            .downcast_ref::<PyOxidizerEnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        let python_packaging_policy = self.python_packaging_policy();

        let resources = self
            .exe
            .pip_install(
                pyoxidizer_context.logger(),
                pyoxidizer_context.verbose,
                &args,
                &extra_envs,
            )
            .map_err(|e| {
                ValueError::from(RuntimeError {
                    code: "PIP_INSTALL_ERROR",
                    message: format!("error running pip install: {}", e),
                    label: "pip_install()".to_string(),
                })
            })?
            .iter()
            .filter(|r| is_resource_starlark_compatible(r))
            .map(|r| python_resource_to_value(type_values, call_stack, r, &python_packaging_policy))
            .collect::<Result<Vec<Value>, ValueError>>()?;

        Ok(Value::from(resources))
    }

    /// PythonExecutable.read_package_root(path, packages)
    pub fn read_package_root(
        &mut self,
        type_values: &TypeValues,
        call_stack: &mut CallStack,
        path: String,
        packages: &Value,
    ) -> ValueResult {
        required_list_arg("packages", "string", &packages)?;

        let packages = packages
            .iter()?
            .iter()
            .map(|x| x.to_string())
            .collect::<Vec<String>>();

        let pyoxidizer_context_value = get_context(type_values)?;
        let pyoxidizer_context = pyoxidizer_context_value
            .downcast_ref::<PyOxidizerEnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        let python_packaging_policy = self.python_packaging_policy();

        let resources = self
            .exe
            .read_package_root(pyoxidizer_context.logger(), Path::new(&path), &packages)
            .map_err(|e| {
                ValueError::from(RuntimeError {
                    code: "PACKAGE_ROOT_ERROR",
                    message: format!("could not find resources: {}", e),
                    label: "read_package_root()".to_string(),
                })
            })?
            .iter()
            .filter(|r| is_resource_starlark_compatible(r))
            .map(|r| python_resource_to_value(type_values, call_stack, r, &python_packaging_policy))
            .collect::<Result<Vec<Value>, ValueError>>()?;

        Ok(Value::from(resources))
    }

    /// PythonExecutable.read_virtualenv(path)
    pub fn read_virtualenv(
        &mut self,
        type_values: &TypeValues,
        call_stack: &mut CallStack,
        path: String,
    ) -> ValueResult {
        let pyoxidizer_context_value = get_context(type_values)?;
        let pyoxidizer_context = pyoxidizer_context_value
            .downcast_ref::<PyOxidizerEnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        let python_packaging_policy = self.python_packaging_policy();

        let resources = self
            .exe
            .read_virtualenv(pyoxidizer_context.logger(), &Path::new(&path))
            .map_err(|e| {
                ValueError::from(RuntimeError {
                    code: "VIRTUALENV_ERROR",
                    message: format!("could not find resources: {}", e),
                    label: "read_virtualenv()".to_string(),
                })
            })?
            .iter()
            .filter(|r| is_resource_starlark_compatible(r))
            .map(|r| python_resource_to_value(type_values, call_stack, r, &python_packaging_policy))
            .collect::<Result<Vec<Value>, ValueError>>()?;

        Ok(Value::from(resources))
    }

    /// PythonExecutable.setup_py_install(package_path, extra_envs=None, extra_global_arguments=None)
    pub fn setup_py_install(
        &mut self,
        type_values: &TypeValues,
        call_stack: &mut CallStack,
        package_path: String,
        extra_envs: &Value,
        extra_global_arguments: &Value,
    ) -> ValueResult {
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

        let pyoxidizer_context_value = get_context(type_values)?;
        let pyoxidizer_context = pyoxidizer_context_value
            .downcast_ref::<PyOxidizerEnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        let package_path = if package_path.is_absolute() {
            package_path
        } else {
            PathBuf::from(&pyoxidizer_context.cwd).join(package_path)
        };

        let python_packaging_policy = self.python_packaging_policy();

        let resources = self
            .exe
            .setup_py_install(
                pyoxidizer_context.logger(),
                &package_path,
                pyoxidizer_context.verbose,
                &extra_envs,
                &extra_global_arguments,
            )
            .map_err(|e| {
                ValueError::from(RuntimeError {
                    code: "SETUP_PY_ERROR",
                    message: format!("{:?}", e),
                    label: "setup_py_install()".to_string(),
                })
            })?
            .iter()
            .filter(|r| is_resource_starlark_compatible(r))
            .map(|r| python_resource_to_value(type_values, call_stack, r, &python_packaging_policy))
            .collect::<Result<Vec<Value>, ValueError>>()?;

        warn!(
            pyoxidizer_context.logger(),
            "collected {} resources from setup.py install",
            resources.len()
        );

        Ok(Value::from(resources))
    }

    pub fn add_python_module_source(
        &mut self,
        context: &PyOxidizerEnvironmentContext,
        label: &str,
        module: &PythonModuleSourceValue,
    ) -> ValueResult {
        info!(
            context.logger(),
            "adding Python source module {}", module.inner.name;
        );
        self.exe
            .add_python_module_source(&module.inner, module.add_collection_context().clone())
            .with_context(|| format!("adding {}", module.to_repr()))
            .map_err(|e| {
                ValueError::from(RuntimeError {
                    code: "PYOXIDIZER_BUILD",
                    message: format!("{:?}", e),
                    label: label.to_string(),
                })
            })?;

        Ok(Value::new(NoneType::None))
    }

    pub fn add_python_package_resource(
        &mut self,
        context: &PyOxidizerEnvironmentContext,
        label: &str,
        resource: &PythonPackageResourceValue,
    ) -> ValueResult {
        info!(
            context.logger(),
            "adding Python package resource {}",
            resource.inner.symbolic_name()
        );
        self.exe
            .add_python_package_resource(&resource.inner, resource.add_collection_context().clone())
            .with_context(|| format!("adding {}", resource.to_repr()))
            .map_err(|e| {
                ValueError::from(RuntimeError {
                    code: "PYOXIDIZER_BUILD",
                    message: format!("{:?}", e),
                    label: label.to_string(),
                })
            })?;

        Ok(Value::new(NoneType::None))
    }

    pub fn add_python_package_distribution_resource(
        &mut self,
        context: &PyOxidizerEnvironmentContext,
        label: &str,
        resource: &PythonPackageDistributionResourceValue,
    ) -> ValueResult {
        info!(
            context.logger(),
            "adding package distribution resource {}:{}",
            resource.inner.package,
            resource.inner.name
        );
        self.exe
            .add_python_package_distribution_resource(
                &resource.inner,
                resource.add_collection_context().clone(),
            )
            .with_context(|| format!("adding {}", resource.to_repr()))
            .map_err(|e| {
                ValueError::from(RuntimeError {
                    code: "PYOXIDIZER_BUILD",
                    message: format!("{:?}", e),
                    label: label.to_string(),
                })
            })?;

        Ok(Value::new(NoneType::None))
    }

    pub fn add_python_extension_module(
        &mut self,
        context: &PyOxidizerEnvironmentContext,
        label: &str,
        module: &PythonExtensionModuleValue,
    ) -> ValueResult {
        info!(
            context.logger(),
            "adding extension module {}", module.inner.name
        );
        self.exe
            .add_python_extension_module(&module.inner, module.add_collection_context().clone())
            .with_context(|| format!("adding {}", module.to_repr()))
            .map_err(|e| {
                ValueError::from(RuntimeError {
                    code: "PYOXIDIZER_BUILD",
                    message: format!("{:?}", e),
                    label: label.to_string(),
                })
            })?;

        Ok(Value::new(NoneType::None))
    }

    pub fn add_file_data(
        &mut self,
        context: &PyOxidizerEnvironmentContext,
        label: &str,
        file: &FileValue,
    ) -> ValueResult {
        info!(
            context.logger(),
            "adding file data {}", file.inner.path.display();
        );
        self.exe
            .add_file_data(&file.inner, file.add_collection_context().clone())
            .with_context(|| format!("adding {}", file.to_repr()))
            .map_err(|e| {
                ValueError::from(RuntimeError {
                    code: "PYOXIDIZER_BUILD",
                    message: format!("{:?}", e),
                    label: label.to_string(),
                })
            })?;

        Ok(Value::new(NoneType::None))
    }

    /// PythonExecutable.add_python_resource(resource)
    pub fn add_python_resource(
        &mut self,
        type_values: &TypeValues,
        resource: &Value,
        label: &str,
    ) -> ValueResult {
        let pyoxidizer_context_value = get_context(type_values)?;
        let pyoxidizer_context = pyoxidizer_context_value
            .downcast_ref::<PyOxidizerEnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        match resource.get_type() {
            FileValue::TYPE => {
                let file = resource.downcast_ref::<FileValue>().unwrap();
                self.add_file_data(pyoxidizer_context.deref(), label, file.deref())
            }
            PythonModuleSourceValue::TYPE => {
                let module = resource.downcast_ref::<PythonModuleSourceValue>().unwrap();
                self.add_python_module_source(pyoxidizer_context.deref(), label, module.deref())
            }
            PythonPackageResourceValue::TYPE => {
                let r = resource
                    .downcast_ref::<PythonPackageResourceValue>()
                    .unwrap();
                self.add_python_package_resource(pyoxidizer_context.deref(), label, r.deref())
            }
            PythonPackageDistributionResourceValue::TYPE => {
                let r = resource
                    .downcast_ref::<PythonPackageDistributionResourceValue>()
                    .unwrap();
                self.add_python_package_distribution_resource(
                    pyoxidizer_context.deref(),
                    label,
                    r.deref(),
                )
            }
            PythonExtensionModuleValue::TYPE => {
                let module = resource
                    .downcast_ref::<PythonExtensionModuleValue>()
                    .unwrap();
                self.add_python_extension_module(pyoxidizer_context.deref(), label, module.deref())
            }
            _ => Err(ValueError::from(RuntimeError {
                code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                message: "resource argument must be a Python resource type".to_string(),
                label: ".add_python_resource()".to_string(),
            })),
        }
    }

    /// PythonExecutable.add_python_resources(resources)
    pub fn add_python_resources(
        &mut self,
        type_values: &TypeValues,
        resources: &Value,
    ) -> ValueResult {
        for resource in &resources.iter()? {
            self.add_python_resource(type_values, &resource, "add_python_resources()")?;
        }

        Ok(Value::new(NoneType::None))
    }

    /// PythonExecutable.to_embedded_resources()
    pub fn to_embedded_resources(&self) -> ValueResult {
        Ok(Value::new(PythonEmbeddedResourcesValue {
            exe: self.exe.clone_trait(),
        }))
    }

    /// PythonExecutable.to_file_manifest(prefix)
    pub fn to_file_manifest(&self, type_values: &TypeValues, prefix: String) -> ValueResult {
        let pyoxidizer_context_value = get_context(type_values)?;
        let pyoxidizer_context = pyoxidizer_context_value
            .downcast_ref::<PyOxidizerEnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        let manifest_value = FileManifestValue::new_from_args()?;
        let mut manifest = manifest_value
            .downcast_mut::<FileManifestValue>()
            .unwrap()
            .unwrap();

        file_manifest_add_python_executable(
            &mut manifest,
            pyoxidizer_context.logger(),
            &prefix,
            self.exe.deref(),
            &pyoxidizer_context.build_target_triple,
            pyoxidizer_context.build_release,
            &pyoxidizer_context.build_opt_level,
        )
        .map_err(|e| {
            ValueError::from(RuntimeError {
                code: "PYOXIDIZER_PYTHON_EXECUTABLE",
                message: format!("{:?}", e),
                label: "to_file_manifest()".to_string(),
            })
        })?;

        Ok(manifest_value.clone())
    }

    /// PythonExecutable.to_wix_bundle_builder(id_prefix, name, version, manufacturer, msi_builder_callback)
    #[allow(clippy::too_many_arguments)]
    pub fn to_wix_bundle_builder(
        &self,
        type_values: &TypeValues,
        call_stack: &mut CallStack,
        id_prefix: String,
        product_name: String,
        product_version: String,
        product_manufacturer: String,
        msi_builder_callback: Value,
    ) -> ValueResult {
        optional_type_arg("msi_builder_callback", "function", &msi_builder_callback)?;

        let msi_builder_value = self.to_wix_msi_builder(
            type_values,
            id_prefix.clone(),
            product_name.clone(),
            product_version.clone(),
            product_manufacturer.clone(),
        )?;

        if msi_builder_callback.get_type() == "function" {
            msi_builder_callback.call(
                call_stack,
                type_values,
                vec![msi_builder_value.clone()],
                LinkedHashMap::new(),
                None,
                None,
            )?;
        }

        let msi_builder = msi_builder_value
            .downcast_ref::<WiXMsiBuilderValue>()
            .unwrap();

        let bundle_builder_value = WiXBundleBuilderValue::new_from_args(
            id_prefix,
            product_name,
            product_version,
            product_manufacturer,
        )?;
        let mut bundle_builder = bundle_builder_value
            .downcast_mut::<WiXBundleBuilderValue>()
            .unwrap()
            .unwrap();

        // Add the VC++ Redistributable for the target platform.
        match self.exe.target_triple() {
            "i686-pc-windows-msvc" => {
                bundle_builder.add_vc_redistributable(type_values, "x86".to_string())?;
            }
            "x86_64-pc-windows-msvc" => {
                bundle_builder.add_vc_redistributable(type_values, "x64".to_string())?;
            }
            _ => {}
        }

        bundle_builder.add_wix_msi_builder(
            msi_builder.deref().clone(),
            false,
            Value::new(NoneType::None),
        )?;

        Ok(bundle_builder_value.clone())
    }

    /// PythonExecutable.to_wix_msi_builder(id_prefix, product_name, product_version, product_manufacturer)
    pub fn to_wix_msi_builder(
        &self,
        type_values: &TypeValues,
        id_prefix: String,
        product_name: String,
        product_version: String,
        product_manufacturer: String,
    ) -> ValueResult {
        let manifest_value = self.to_file_manifest(type_values, ".".to_string())?;
        let manifest = manifest_value.downcast_ref::<FileManifestValue>().unwrap();

        let builder_value = WiXMsiBuilderValue::new_from_args(
            id_prefix,
            product_name,
            product_version,
            product_manufacturer,
        )?;
        let mut builder = builder_value
            .downcast_mut::<WiXMsiBuilderValue>()
            .unwrap()
            .unwrap();

        builder.add_program_files_manifest(manifest.deref().clone())?;

        Ok(builder_value.clone())
    }

    /// PythonExecutable.filter_resources_from_files(files=None, glob_files=None)
    pub fn filter_resources_from_files(
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

        let pyoxidizer_context_value = get_context(type_values)?;
        let pyoxidizer_context = pyoxidizer_context_value
            .downcast_ref::<PyOxidizerEnvironmentContext>()
            .ok_or(ValueError::IncorrectParameterType)?;

        self.exe
            .filter_resources_from_files(pyoxidizer_context.logger(), &files_refs, &glob_files_refs)
            .map_err(|e| {
                ValueError::from(RuntimeError {
                    code: "RUNTIME_ERROR",
                    message: format!("{:?}", e),
                    label: "filter_from_files()".to_string(),
                })
            })?;

        Ok(Value::new(NoneType::None))
    }
}

starlark_module! { python_executable_env =>
    PythonExecutable.build(env env, this, target: String) {
        let this = this.downcast_ref::<PythonExecutableValue>().unwrap();
        this.build(env, target)
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.make_python_module_source(
        env env,
        call_stack cs,
        this,
        name: String,
        source: String,
        is_package: bool = false
    ) {
        let this = this.downcast_ref::<PythonExecutableValue>().unwrap();
        this.make_python_module_source(&env, cs, name, source, is_package)
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.pip_download(
        env env,
        call_stack cs,
        this,
        args
    ) {
        let mut this = this.downcast_mut::<PythonExecutableValue>().unwrap().unwrap();
        this.pip_download(&env, cs, &args)
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.pip_install(
        env env,
        call_stack cs,
        this,
        args,
        extra_envs=NoneType::None
    ) {
        let mut this = this.downcast_mut::<PythonExecutableValue>().unwrap().unwrap();
        this.pip_install(&env, cs, &args, &extra_envs)
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.read_package_root(
        env env,
        call_stack cs,
        this,
        path: String,
        packages
    ) {
        let mut this = this.downcast_mut::<PythonExecutableValue>().unwrap().unwrap();
        this.read_package_root(&env, cs, path, &packages)
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.read_virtualenv(
        env env,
        call_stack cs,
        this,
        path: String
    ) {
        let mut this = this.downcast_mut::<PythonExecutableValue>().unwrap().unwrap();
        this.read_virtualenv(&env, cs, path)
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.setup_py_install(
        env env,
        call_stack cs,
        this,
        package_path: String,
        extra_envs=NoneType::None,
        extra_global_arguments=NoneType::None
    ) {
        let mut this = this.downcast_mut::<PythonExecutableValue>().unwrap().unwrap();
        this.setup_py_install(&env, cs, package_path, &extra_envs, &extra_global_arguments)
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.add_python_resource(
        env env,
        this,
        resource
    ) {
        let mut this = this.downcast_mut::<PythonExecutableValue>().unwrap().unwrap();
        this.add_python_resource(
            &env,
            &resource,
            "add_python_resource",
        )
    }

    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonExecutable.add_python_resources(
        env env,
        this,
        resources
    ) {
        let mut this = this.downcast_mut::<PythonExecutableValue>().unwrap().unwrap();
        this.add_python_resources(
            &env,
            &resources,
        )
    }

    #[allow(clippy::ptr_arg)]
    PythonExecutable.filter_resources_from_files(
        env env,
        this,
        files=NoneType::None,
        glob_files=NoneType::None)
    {
        let mut this = this.downcast_mut::<PythonExecutableValue>().unwrap().unwrap();
        this.filter_resources_from_files(&env, &files, &glob_files)
    }

    #[allow(clippy::ptr_arg)]
    PythonExecutable.to_embedded_resources(this) {
        let this = this.downcast_ref::<PythonExecutableValue>().unwrap();
        this.to_embedded_resources()
    }

    PythonExecutable.to_file_manifest(env env, this, prefix: String) {
        let this = this.downcast_ref::<PythonExecutableValue>().unwrap();
        this.to_file_manifest(&env, prefix)
    }

    PythonExecutable.to_wix_bundle_builder(
        env env,
        call_stack cs,
        this,
        id_prefix: String,
        product_name: String,
        product_version: String,
        product_manufacturer: String,
        msi_builder_callback = NoneType::None
    ) {
        let this = this.downcast_ref::<PythonExecutableValue>().unwrap();
        this.to_wix_bundle_builder(
            env,
            cs,
            id_prefix,
            product_name,
            product_version,
            product_manufacturer,
            msi_builder_callback
        )
    }

    PythonExecutable.to_wix_msi_builder(
        env env,
        this,
        id_prefix: String,
        product_name: String,
        product_version: String,
        product_manufacturer: String
    ) {
        let this = this.downcast_ref::<PythonExecutableValue>().unwrap();
        this.to_wix_msi_builder(&env, id_prefix, product_name, product_version, product_manufacturer)
    }
}

#[cfg(test)]
mod tests {
    use {super::super::testutil::*, super::*, crate::python_distributions::PYTHON_DISTRIBUTIONS};

    #[test]
    fn test_default_values() -> Result<()> {
        let mut env = test_evaluation_context_builder()?.into_context()?;
        add_exe(&mut env)?;
        let exe = env.eval("exe")?;

        assert_eq!(exe.get_type(), "PythonExecutable");

        let exe = exe.downcast_ref::<PythonExecutableValue>().unwrap();
        assert!(exe
            .exe
            .iter_resources()
            .any(|(_, r)| r.in_memory_source.is_some()));
        assert!(exe
            .exe
            .iter_resources()
            .all(|(_, r)| r.in_memory_resources.is_none()));

        Ok(())
    }

    #[test]
    fn test_no_sources() -> Result<()> {
        let mut env = test_evaluation_context_builder()?.into_context()?;

        env.eval("dist = default_python_distribution()")?;
        env.eval("policy = dist.make_python_packaging_policy()")?;
        env.eval("policy.include_distribution_sources = False")?;

        let exe = env.eval("dist.to_python_executable('testapp', packaging_policy=policy)")?;

        assert_eq!(exe.get_type(), "PythonExecutable");

        let exe = exe.downcast_ref::<PythonExecutableValue>().unwrap();
        assert!(exe
            .exe
            .iter_resources()
            .all(|(_, r)| r.in_memory_source.is_none()));

        Ok(())
    }

    #[test]
    fn test_make_python_module_source() -> Result<()> {
        let mut env = test_evaluation_context_builder()?.into_context()?;
        add_exe(&mut env)?;
        let m = env.eval("exe.make_python_module_source('foo', 'import bar')")?;

        assert_eq!(m.get_type(), PythonModuleSourceValue::TYPE);
        assert_eq!(m.get_attr("name").unwrap().to_str(), "foo");
        assert_eq!(m.get_attr("source").unwrap().to_str(), "import bar");
        assert_eq!(m.get_attr("is_package").unwrap().to_bool(), false);

        Ok(())
    }

    #[test]
    fn test_make_python_module_source_callback() -> Result<()> {
        let mut env = test_evaluation_context_builder()?.into_context()?;
        env.eval("dist = default_python_distribution()")?;
        env.eval("policy = dist.make_python_packaging_policy()")?;
        env.eval(
            "def my_func(policy, resource):\n    resource.add_source = True\n    resource.add_bytecode_optimization_level_two = True\n",
        )?;
        env.eval("policy.register_resource_callback(my_func)")?;
        env.eval("exe = dist.to_python_executable('testapp', packaging_policy = policy)")?;

        let m = env.eval("exe.make_python_module_source('foo', 'import bar')")?;

        assert_eq!(m.get_type(), PythonModuleSourceValue::TYPE);
        assert_eq!(m.get_attr("name").unwrap().to_str(), "foo");
        assert_eq!(m.get_attr("source").unwrap().to_str(), "import bar");
        assert_eq!(m.get_attr("is_package").unwrap().to_bool(), false);
        assert_eq!(m.get_attr("add_source").unwrap().to_bool(), true);
        assert_eq!(
            m.get_attr("add_bytecode_optimization_level_two")
                .unwrap()
                .to_bool(),
            true
        );

        Ok(())
    }

    #[test]
    fn test_pip_download_pyflakes() -> Result<()> {
        for target_triple in PYTHON_DISTRIBUTIONS.all_target_triples() {
            let mut env = test_evaluation_context_builder()?
                .build_target_triple(target_triple)
                .into_context()?;

            env.eval("dist = default_python_distribution()")?;
            env.eval("exe = dist.to_python_executable('testapp')")?;

            let resources = env.eval("exe.pip_download(['pyflakes==2.2.0'])")?;

            assert_eq!(resources.get_type(), "list");

            let raw_it = resources.iter().unwrap();
            let mut it = raw_it.iter();

            let v = it.next().unwrap();
            assert_eq!(v.get_type(), PythonModuleSourceValue::TYPE);
            let x = v.downcast_ref::<PythonModuleSourceValue>().unwrap();
            assert!(x.inner.package().starts_with("pyflakes"));
        }

        Ok(())
    }

    #[test]
    fn test_pip_install_simple() -> Result<()> {
        let mut env = test_evaluation_context_builder()?.into_context()?;

        env.eval("dist = default_python_distribution()")?;
        env.eval("policy = dist.make_python_packaging_policy()")?;
        env.eval("policy.include_distribution_sources = False")?;
        env.eval("exe = dist.to_python_executable('testapp', packaging_policy = policy)")?;

        let resources = env.eval("exe.pip_install(['pyflakes==2.1.1'])")?;
        assert_eq!(resources.get_type(), "list");

        let raw_it = resources.iter().unwrap();
        let mut it = raw_it.iter();

        let v = it.next().unwrap();
        assert_eq!(v.get_type(), PythonModuleSourceValue::TYPE);
        let x = v.downcast_ref::<PythonModuleSourceValue>().unwrap();
        assert_eq!(x.inner.name, "pyflakes");
        assert!(x.inner.is_package);

        Ok(())
    }

    #[test]
    fn test_read_package_root_simple() -> Result<()> {
        let temp_dir = tempfile::Builder::new()
            .prefix("pyoxidizer-test")
            .tempdir()?;

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

        let mut env = test_evaluation_context_builder()?.into_context()?;
        env.eval("dist = default_python_distribution()")?;
        env.eval("policy = dist.make_python_packaging_policy()")?;
        env.eval("policy.include_distribution_sources = False")?;
        env.eval("exe = dist.to_python_executable('testapp', packaging_policy = policy)")?;

        let resources = env.eval(&format!(
            "exe.read_package_root(\"{}\", packages=['foo', 'bar'])",
            root.display()
        ))?;

        assert_eq!(resources.get_type(), "list");
        assert_eq!(resources.length().unwrap(), 2);

        let raw_it = resources.iter().unwrap();
        let mut it = raw_it.iter();

        let v = it.next().unwrap();
        assert_eq!(v.get_type(), PythonModuleSourceValue::TYPE);
        let x = v.downcast_ref::<PythonModuleSourceValue>().unwrap();
        assert_eq!(x.inner.name, "bar");
        assert!(x.inner.is_package);
        assert_eq!(x.inner.source.resolve().unwrap(), b"# bar");

        let v = it.next().unwrap();
        assert_eq!(v.get_type(), PythonModuleSourceValue::TYPE);
        let x = v.downcast_ref::<PythonModuleSourceValue>().unwrap();
        assert_eq!(x.inner.name, "foo");
        assert!(!x.inner.is_package);
        assert_eq!(x.inner.source.resolve().unwrap(), b"# foo");

        Ok(())
    }

    #[test]
    fn test_windows_runtime_dlls_mode() -> Result<()> {
        let mut env = test_evaluation_context_builder()?.into_context()?;
        add_exe(&mut env)?;

        let value = env.eval("exe.windows_runtime_dlls_mode")?;
        assert_eq!(value.get_type(), "string");
        assert_eq!(value.to_string(), "when-present");

        let value =
            env.eval("exe.windows_runtime_dlls_mode = 'never'; exe.windows_runtime_dlls_mode")?;
        assert_eq!(value.to_string(), "never");

        let value =
            env.eval("exe.windows_runtime_dlls_mode = 'always'; exe.windows_runtime_dlls_mode")?;
        assert_eq!(value.to_string(), "always");

        assert!(env.eval("exe.windows_runtime_dlls_mode = 'bad'").is_err());

        let value = env.eval(
            "exe.windows_runtime_dlls_mode = 'when-present'; exe.windows_runtime_dlls_mode",
        )?;
        assert_eq!(value.to_string(), "when-present");

        Ok(())
    }

    #[test]
    fn test_packed_resources_load_mode() -> Result<()> {
        let mut env = test_evaluation_context_builder()?.into_context()?;
        add_exe(&mut env)?;

        let value = env.eval("exe.packed_resources_load_mode")?;
        assert_eq!(value.get_type(), "string");
        assert_eq!(value.to_string(), "embedded:packed-resources");

        let value =
            env.eval("exe.packed_resources_load_mode = 'none'; exe.packed_resources_load_mode")?;
        assert_eq!(value.get_type(), "string");
        assert_eq!(value.to_string(), "none");

        Ok(())
    }

    #[test]
    fn test_windows_subsystem() -> Result<()> {
        let mut env = test_evaluation_context_builder()?.into_context()?;
        add_exe(&mut env)?;

        let value = env.eval("exe.windows_subsystem")?;
        assert_eq!(value.get_type(), "string");
        assert_eq!(value.to_string(), "console");

        let value = env.eval("exe.windows_subsystem = 'windows'; exe.windows_subsystem")?;
        assert_eq!(value.get_type(), "string");
        assert_eq!(value.to_string(), "windows");

        Ok(())
    }

    #[test]
    fn test_tcl_files_path() -> Result<()> {
        let mut env = test_evaluation_context_builder()?.into_context()?;
        add_exe(&mut env)?;

        let value = env.eval("exe.tcl_files_path")?;
        assert_eq!(value.get_type(), "NoneType");

        let value = env.eval("exe.tcl_files_path = 'lib'; exe.tcl_files_path")?;
        assert_eq!(value.get_type(), "string");
        assert_eq!(value.to_string(), "lib");

        let value = env.eval("exe.tcl_files_path = None; exe.tcl_files_path")?;
        assert_eq!(value.get_type(), "NoneType");

        Ok(())
    }

    #[test]
    fn test_to_wix_bundle_builder_callback() -> Result<()> {
        let mut env = test_evaluation_context_builder()?.into_context()?;
        add_exe(&mut env)?;
        env.eval("def modify(msi):\n msi.package_description = 'description'\n")?;
        let builder_value = env.eval("exe.to_wix_bundle_builder('id_prefix', 'product_name', '0.1', 'manufacturer', msi_builder_callback = modify)")?;
        let builder = builder_value
            .downcast_ref::<WiXBundleBuilderValue>()
            .unwrap();

        assert_eq!(builder.build_msis.len(), 1);
        let mut writer = xml::EventWriter::new(vec![]);
        builder.build_msis[0].inner.write_xml(&mut writer)?;

        let xml = String::from_utf8(writer.into_inner())?;
        assert!(xml.find("Description=\"description\"").is_some());

        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn test_to_wix_bundle_builder() -> Result<()> {
        let mut env = test_evaluation_context_builder()?.into_context()?;
        add_exe(&mut env)?;
        env.eval("bundle = exe.to_wix_bundle_builder('id_prefix', 'product_name', '0.1', 'product_manufacturer')")?;
        env.eval("bundle.build('test_to_wix_bundle_builder')")?;

        let exe_path = env
            .target_build_path("test_to_wix_bundle_builder")
            .unwrap()
            .join("product_name-0.1.exe");

        assert!(
            exe_path.exists(),
            format!("exe exists: {}", exe_path.display())
        );

        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn test_to_wix_msi_builder() -> Result<()> {
        let mut env = test_evaluation_context_builder()?.into_context()?;
        add_exe(&mut env)?;
        env.eval("msi = exe.to_wix_msi_builder('id_prefix', 'product_name', '0.1', 'product_manufacturer')")?;
        env.eval("msi.build('test_to_wix_msi_builder')")?;

        let msi_path = env
            .target_build_path("test_to_wix_msi_builder")
            .unwrap()
            .join("product_name-0.1.msi");

        assert!(
            msi_path.exists(),
            format!("msi exists: {}", msi_path.display())
        );

        Ok(())
    }
}
