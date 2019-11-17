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

use super::env::required_type_arg;
use crate::py_packaging::embedded_resource::EmbeddedPythonResourcesPrePackaged;
use crate::py_packaging::resource::{
    BytecodeModule, BytecodeOptimizationLevel, ResourceData, SourceModule,
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

starlark_module! { python_resource_env =>
    #[allow(non_snake_case)]
    PythonEmbeddedResources(env _env) {
        let embedded = EmbeddedPythonResourcesPrePackaged::default();

        Ok(Value::new(PythonEmbeddedResources { embedded }))
    }

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

    PythonEmbeddedResources.add_resource_data(this, resource) {
        required_type_arg("resource", "PythonResourceData", &resource)?;

        this.downcast_apply_mut(|embedded: &mut PythonEmbeddedResources| {
            let r = resource.downcast_apply(|r: &PythonResourceData| r.data.clone());
            embedded.embedded.add_resource(&r);
        });

        Ok(Value::new(None))
    }
}
