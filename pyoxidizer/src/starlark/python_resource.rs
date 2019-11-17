// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use starlark::environment::Environment;
use starlark::values::{default_compare, TypedValue, Value, ValueError, ValueResult};
use starlark::{any, immutable, not_supported};
use std::any::Any;
use std::cmp::Ordering;
use std::collections::HashMap;

use crate::py_packaging::embedded_resource::EmbeddedPythonResources;
use crate::py_packaging::resource::{BytecodeModule, BytecodeOptimizationLevel, SourceModule};

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
pub struct PythonEmbeddedResources {
    pub embedded: EmbeddedPythonResources,
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
