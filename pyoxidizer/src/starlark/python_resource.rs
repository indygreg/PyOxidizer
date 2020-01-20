// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use starlark::environment::Environment;
use starlark::values::{
    default_compare, RuntimeError, TypedValue, Value, ValueError, ValueResult,
    INCORRECT_PARAMETER_TYPE_ERROR_CODE,
};
use starlark::{
    any, immutable, not_supported, starlark_fun, starlark_module, starlark_signature,
    starlark_signature_extraction, starlark_signatures,
};
use std::any::Any;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::env::EnvironmentContext;
use super::util::{optional_list_arg, required_bool_arg, required_type_arg};
use crate::py_packaging::distribution::ExtensionModule;
use crate::py_packaging::embedded_resource::EmbeddedPythonResourcesPrePackaged;
use crate::py_packaging::resource::{
    BuiltExtensionModule, BytecodeModule, BytecodeOptimizationLevel, PythonResource, ResourceData,
    SourceModule,
};

#[derive(Debug, Clone)]
pub struct PythonSourceModule {
    pub module: SourceModule,
}

impl TypedValue for PythonSourceModule {
    immutable!();
    any!();
    not_supported!(
        binop, dir_attr, function, get_hash, indexable, iterable, sequence, set_attr, to_int
    );

    fn to_str(&self) -> String {
        format!("PythonSourceModule<name={}>", self.module.name)
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_type(&self) -> &'static str {
        "PythonSourceModule"
    }

    fn to_bool(&self) -> bool {
        true
    }

    fn compare(&self, other: &dyn TypedValue, _recursion: u32) -> Result<Ordering, ValueError> {
        default_compare(self, other)
    }

    fn get_attr(&self, attribute: &str) -> ValueResult {
        let v = match attribute {
            "name" => Value::new(self.module.name.clone()),
            // TODO expose source
            // "source" => Value::new(self.module.source),
            "is_package" => Value::new(self.module.is_package),
            attr => {
                return Err(ValueError::OperationNotSupported {
                    op: format!(".{}", attr),
                    left: "PythonSourceModule".to_string(),
                    right: None,
                })
            }
        };

        Ok(v)
    }

    fn has_attr(&self, attribute: &str) -> Result<bool, ValueError> {
        Ok(match attribute {
            "name" => true,
            // TODO expose source
            // "source" => true,
            "is_package" => true,
            _ => false,
        })
    }
}

#[derive(Debug, Clone)]
pub struct PythonBytecodeModule {
    pub module: BytecodeModule,
}

impl TypedValue for PythonBytecodeModule {
    immutable!();
    any!();
    not_supported!(
        binop, dir_attr, function, get_hash, indexable, iterable, sequence, set_attr, to_int
    );

    fn to_str(&self) -> String {
        format!(
            "PythonBytecodeModule<name={}; level={:?}>",
            self.module.name, self.module.optimize_level
        )
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_type(&self) -> &'static str {
        "PythonBytecodeModule"
    }

    fn to_bool(&self) -> bool {
        true
    }

    fn compare(&self, other: &dyn TypedValue, _recursion: u32) -> Result<Ordering, ValueError> {
        default_compare(self, other)
    }

    fn get_attr(&self, attribute: &str) -> ValueResult {
        let v = match attribute {
            "name" => Value::new(self.module.name.clone()),
            // TODO expose source
            // "source" => Value::new(self.module.source),
            "optimize_level" => Value::new(match self.module.optimize_level {
                BytecodeOptimizationLevel::Zero => 0,
                BytecodeOptimizationLevel::One => 1,
                BytecodeOptimizationLevel::Two => 2,
            }),
            "is_package" => Value::new(self.module.is_package),
            attr => {
                return Err(ValueError::OperationNotSupported {
                    op: format!(".{}", attr),
                    left: "PythonBytecodeModule".to_string(),
                    right: None,
                })
            }
        };

        Ok(v)
    }

    fn has_attr(&self, attribute: &str) -> Result<bool, ValueError> {
        Ok(match attribute {
            "name" => true,
            // TODO expose source
            // "source" => true,
            "optimize_level" => true,
            "is_package" => true,
            _ => false,
        })
    }
}

#[derive(Debug, Clone)]
pub struct PythonResourceData {
    pub data: ResourceData,
}

impl TypedValue for PythonResourceData {
    immutable!();
    any!();
    not_supported!(
        binop, dir_attr, function, get_hash, indexable, iterable, sequence, set_attr, to_int
    );

    fn to_str(&self) -> String {
        format!(
            "PythonResourceData<package={}, name={}>",
            self.data.package, self.data.name
        )
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_type(&self) -> &'static str {
        "PythonResourceData"
    }

    fn to_bool(&self) -> bool {
        true
    }

    fn compare(&self, other: &dyn TypedValue, _recursion: u32) -> Result<Ordering, ValueError> {
        default_compare(self, other)
    }

    fn get_attr(&self, attribute: &str) -> ValueResult {
        let v = match attribute {
            "package" => Value::new(self.data.package.clone()),
            "name" => Value::new(self.data.name.clone()),
            // TODO expose raw data
            attr => {
                return Err(ValueError::OperationNotSupported {
                    op: format!(".{}", attr),
                    left: "PythonResourceData".to_string(),
                    right: None,
                })
            }
        };

        Ok(v)
    }

    fn has_attr(&self, attribute: &str) -> Result<bool, ValueError> {
        Ok(match attribute {
            "package" => true,
            "name" => true,
            // TODO expose raw data
            _ => false,
        })
    }
}

#[derive(Debug, Clone)]
pub enum PythonExtensionModuleFlavor {
    Persisted(ExtensionModule),
    Built(BuiltExtensionModule),
}

impl PythonExtensionModuleFlavor {
    pub fn name(&self) -> String {
        match self {
            PythonExtensionModuleFlavor::Persisted(m) => m.module.clone(),
            PythonExtensionModuleFlavor::Built(m) => m.name.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PythonExtensionModule {
    pub em: PythonExtensionModuleFlavor,
}

impl TypedValue for PythonExtensionModule {
    immutable!();
    any!();
    not_supported!(
        binop, dir_attr, function, get_hash, indexable, iterable, sequence, set_attr, to_int
    );

    fn to_str(&self) -> String {
        format!("PythonExtensionModule<name={}>", self.em.name())
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_type(&self) -> &'static str {
        "PythonExtensionModule"
    }

    fn to_bool(&self) -> bool {
        true
    }

    fn compare(&self, other: &dyn TypedValue, _recursion: u32) -> Result<Ordering, ValueError> {
        default_compare(self, other)
    }

    fn get_attr(&self, attribute: &str) -> ValueResult {
        let v = match attribute {
            "name" => Value::new(self.em.name()),
            attr => {
                return Err(ValueError::OperationNotSupported {
                    op: format!(".{}", attr),
                    left: "PythonExtensionModule".to_string(),
                    right: None,
                })
            }
        };

        Ok(v)
    }

    fn has_attr(&self, attribute: &str) -> Result<bool, ValueError> {
        Ok(match attribute {
            "name" => true,
            _ => false,
        })
    }
}

#[derive(Debug, Clone)]
pub struct PythonEmbeddedResources {
    pub embedded: EmbeddedPythonResourcesPrePackaged,
}

impl TypedValue for PythonEmbeddedResources {
    immutable!();
    any!();
    not_supported!(
        binop, dir_attr, function, get_attr, get_hash, has_attr, indexable, iterable, sequence,
        set_attr, to_int
    );

    fn to_str(&self) -> String {
        "PythonEmbeddedResources<...>".to_string()
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_type(&self) -> &'static str {
        "PythonEmbeddedResources"
    }

    fn to_bool(&self) -> bool {
        true
    }

    fn compare(&self, other: &dyn TypedValue, _recursion: u32) -> Result<Ordering, ValueError> {
        default_compare(self, other)
    }
}

impl<'a> From<&'a PythonResource> for Value {
    fn from(resource: &'a PythonResource) -> Value {
        match resource {
            PythonResource::ModuleSource {
                name,
                source,
                is_package,
            } => Value::new(PythonSourceModule {
                module: SourceModule {
                    name: name.clone(),
                    source: source.clone(),
                    is_package: *is_package,
                },
            }),

            PythonResource::ModuleBytecodeRequest {
                name,
                source,
                optimize_level,
                is_package,
            } => Value::new(PythonBytecodeModule {
                module: BytecodeModule {
                    name: name.clone(),
                    source: source.clone(),
                    optimize_level: BytecodeOptimizationLevel::from(*optimize_level),
                    is_package: *is_package,
                },
            }),

            PythonResource::ModuleBytecode { .. } => {
                panic!("not yet implemented");
            }

            PythonResource::Resource {
                package,
                name,
                data,
            } => Value::new(PythonResourceData {
                data: ResourceData {
                    package: package.clone(),
                    name: name.clone(),
                    data: data.clone(),
                },
            }),

            PythonResource::ExtensionModule { .. } => {
                panic!("not yet implemented");
            }

            PythonResource::BuiltExtensionModule(_em) => {
                panic!("not yet implemented");
            }
        }
    }
}

starlark_module! { python_resource_env =>
    #[allow(non_snake_case, clippy::ptr_arg)]
    PythonEmbeddedResources(env _env) {
        let embedded = EmbeddedPythonResourcesPrePackaged::default();

        Ok(Value::new(PythonEmbeddedResources { embedded }))
    }

    #[allow(clippy::ptr_arg)]
    PythonEmbeddedResources.add_module_source(this, module) {
        required_type_arg("module", "PythonSourceModule", &module)?;

        this.downcast_apply_mut(|embedded: &mut PythonEmbeddedResources| {
            let m = module.downcast_apply(|m: &PythonSourceModule| m.module.clone());
            embedded.embedded.add_source_module(&m);
        });

        Ok(Value::new(None))
    }

    // TODO consider unifying with add_module_source() so there only needs to be
    // a single function call.
    #[allow(clippy::ptr_arg)]
    PythonEmbeddedResources.add_module_bytecode(this, module, optimize_level=0) {
        required_type_arg("module", "PythonSourceModule", &module)?;
        required_type_arg("optimize_level", "int", &optimize_level)?;

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
                }.into());
            }
        };

        this.downcast_apply_mut(|embedded: &mut PythonEmbeddedResources| {
            let m = module.downcast_apply(|m: &PythonSourceModule| m.module.clone());
            embedded.embedded.add_bytecode_module(&BytecodeModule {
                name: m.name.clone(),
                source: m.source.clone(),
                optimize_level,
                is_package: m.is_package,
            });
        });

        Ok(Value::new(None))
    }

    #[allow(clippy::ptr_arg)]
    PythonEmbeddedResources.add_resource_data(this, resource) {
        required_type_arg("resource", "PythonResourceData", &resource)?;

        this.downcast_apply_mut(|embedded: &mut PythonEmbeddedResources| {
            let r = resource.downcast_apply(|r: &PythonResourceData| r.data.clone());
            embedded.embedded.add_resource(&r);
        });

        Ok(Value::new(None))
    }

    #[allow(clippy::ptr_arg)]
    PythonEmbeddedResources.add_extension_module(this, module) {
        required_type_arg("resource", "PythonExtensionModule", &module)?;

        this.downcast_apply_mut(|embedded: &mut PythonEmbeddedResources| {
            let m = module.downcast_apply(|m: &PythonExtensionModule| m.em.clone());
            match m {
                PythonExtensionModuleFlavor::Persisted(m) => {
                    embedded.embedded.add_extension_module(&m);
                    Ok(())
                },
                PythonExtensionModuleFlavor::Built(_) => Err(RuntimeError {
                    code: "PYOXIDIZER_BUILD",
                    message: "support for built extension modules not yet implemented".to_string(),
                    label: "add_extension_module()".to_string(),
                }.into())
            }
        })?;

        Ok(Value::new(None))
    }

    #[allow(clippy::ptr_arg)]
    PythonEmbeddedResources.add_python_resource(
        call_stack call_stack,
        env env,
        this,
        resource,
        add_source_module=true,
        add_bytecode_module=true,
        optimize_level=0
        ) {
        let add_source_module = required_bool_arg("add_source_module", &add_source_module)?;
        let add_bytecode_module = required_bool_arg("add_bytecode_module", &add_bytecode_module)?;
        required_type_arg("optimize_level", "int", &optimize_level)?;

        match resource.get_type() {
            "PythonSourceModule" => {
                if add_source_module {
                    let f = env.get_type_value(&this, "add_module_source").unwrap();
                    f.call(call_stack, env.clone(), vec![this.clone(), resource.clone()], HashMap::new(), None, None)?;
                }
                if add_bytecode_module {
                    let f = env.get_type_value(&this, "add_module_bytecode").unwrap();
                    f.call(call_stack, env, vec![this, resource, optimize_level], HashMap::new(), None, None)?;
                }

                Ok(Value::new(None))
            }
            "PythonBytecodeModule" => {
                let f = env.get_type_value(&this, "add_module_bytecode").unwrap();
                f.call(call_stack, env, vec![this, resource, optimize_level], HashMap::new(), None, None)?;
                Ok(Value::new(None))
            }
            "PythonResourceData" => {
                let f = env.get_type_value(&this, "add_resource_data").unwrap();
                f.call(call_stack, env, vec![this, resource], HashMap::new(), None, None)?;
                Ok(Value::new(None))
            }
            "PythonExtensionModule" => {
                let f = env.get_type_value(&this, "add_extension_module").unwrap();
                f.call(call_stack, env, vec![this, resource], HashMap::new(), None, None)?;
                Ok(Value::new(None))
            }
            _ => Err(RuntimeError {
                code: INCORRECT_PARAMETER_TYPE_ERROR_CODE,
                message: "resource argument must be a Python resource type".to_string(),
                label: ".add_python_resource()".to_string(),
            }.into())
        }
    }

    #[allow(clippy::ptr_arg)]
    PythonEmbeddedResources.add_python_resources(
        call_stack call_stack,
        env env,
        this,
        resources,
        add_source_module=true,
        add_bytecode_module=true,
        optimize_level=0
    ) {
        required_bool_arg("add_source_module", &add_source_module)?;
        required_bool_arg("add_bytecode_module", &add_bytecode_module)?;
        required_type_arg("optimize_level", "int", &optimize_level)?;

        let f = env.get_type_value(&this, "add_python_resource").unwrap();

        for resource in resources.into_iter()? {
            let args = vec![
                this.clone(),
                resource,
                add_source_module.clone(),
                add_bytecode_module.clone(),
                optimize_level.clone(),
            ];
            f.call(call_stack, env.clone(), args, HashMap::new(), None, None)?;
        }

        Ok(Value::new(None))
    }

    #[allow(clippy::ptr_arg)]
    PythonEmbeddedResources.filter_from_files(
        env env,
        this,
        files=None,
        glob_files=None) {
        optional_list_arg("files", "string", &files)?;
        optional_list_arg("glob_files", "string", &glob_files)?;

        let files = match files.get_type() {
            "list" => files.into_iter()?.map(|x| PathBuf::from(x.to_string())).collect(),
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

        this.downcast_apply_mut(|embedded: &mut PythonEmbeddedResources| {
            embedded.embedded.filter_from_files(&logger, &files_refs, &glob_files_refs)
        }).or_else(|e| Err(
            RuntimeError {
                code: "RUNTIME_ERROR",
                message: e.to_string(),
                label: "filter_from_files()".to_string(),
            }.into()
        ))?;

        Ok(Value::new(None))
    }
}
