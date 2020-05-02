// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::py_packaging::standalone_distribution::DistributionExtensionModule,
    python_packaging::resource::{
        BytecodeOptimizationLevel, PythonExtensionModule as RawExtensionModule,
        PythonModuleBytecodeFromSource, PythonModuleSource as RawSourceModule,
        PythonPackageDistributionResource as RawDistributionResource,
        PythonPackageResource as RawPackageResource, PythonResource,
    },
    starlark::environment::Environment,
    starlark::values::{default_compare, TypedValue, Value, ValueError, ValueResult},
    starlark::{any, immutable, not_supported},
    std::any::Any,
    std::cmp::Ordering,
    std::collections::HashMap,
};

#[derive(Debug, Clone)]
pub struct PythonSourceModule {
    pub module: RawSourceModule,
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
    pub module: PythonModuleBytecodeFromSource,
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
pub struct PythonPackageResource {
    pub data: RawPackageResource,
}

impl TypedValue for PythonPackageResource {
    immutable!();
    any!();
    not_supported!(
        binop, dir_attr, function, get_hash, indexable, iterable, sequence, set_attr, to_int
    );

    fn to_str(&self) -> String {
        format!(
            "PythonPackageResource<package={}, name={}>",
            self.data.leaf_package, self.data.relative_name
        )
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_type(&self) -> &'static str {
        "PythonPackageResource"
    }

    fn to_bool(&self) -> bool {
        true
    }

    fn compare(&self, other: &dyn TypedValue, _recursion: u32) -> Result<Ordering, ValueError> {
        default_compare(self, other)
    }

    fn get_attr(&self, attribute: &str) -> ValueResult {
        let v = match attribute {
            "package" => Value::new(self.data.leaf_package.clone()),
            "name" => Value::new(self.data.relative_name.clone()),
            // TODO expose raw data
            attr => {
                return Err(ValueError::OperationNotSupported {
                    op: format!(".{}", attr),
                    left: "PythonPackageResource".to_string(),
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
pub struct PythonPackageDistributionResource {
    pub resource: RawDistributionResource,
}

impl TypedValue for PythonPackageDistributionResource {
    immutable!();
    any!();
    not_supported!(
        binop, dir_attr, function, get_hash, indexable, iterable, sequence, set_attr, to_int
    );

    fn to_str(&self) -> String {
        format!(
            "PythonPackageDistributionResource<package={}, name={}>",
            self.resource.package, self.resource.name
        )
    }

    fn to_repr(&self) -> String {
        self.to_str()
    }

    fn get_type(&self) -> &'static str {
        "PythonPackageDistributionResource"
    }

    fn to_bool(&self) -> bool {
        true
    }

    fn compare(&self, other: &dyn TypedValue, _recursion: u32) -> Result<Ordering, ValueError> {
        default_compare(self, other)
    }

    fn get_attr(&self, attribute: &str) -> ValueResult {
        let v = match attribute {
            "package" => Value::new(self.resource.package.clone()),
            "name" => Value::new(self.resource.name.clone()),
            // TODO expose raw data
            attr => {
                return Err(ValueError::OperationNotSupported {
                    op: format!(".{}", attr),
                    left: "PythonPackageDistributionResource".to_string(),
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

/// Represents an extension module flavor.
#[derive(Debug, Clone)]
pub enum PythonExtensionModuleFlavor {
    /// An extension module from a Python distribution.
    Distribution(DistributionExtensionModule),

    /// An extension module that can be statically linked.
    StaticallyLinked(RawExtensionModule),

    /// An extension module that exists as a dynamic library.
    DynamicLibrary(RawExtensionModule),
}

impl PythonExtensionModuleFlavor {
    pub fn name(&self) -> String {
        match self {
            PythonExtensionModuleFlavor::Distribution(m) => m.module.clone(),
            PythonExtensionModuleFlavor::StaticallyLinked(m) => m.name.clone(),
            PythonExtensionModuleFlavor::DynamicLibrary(m) => m.name.clone(),
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

pub fn python_resource_to_value(resource: &PythonResource) -> Value {
    match resource {
        PythonResource::ModuleSource(sm) => Value::new(PythonSourceModule { module: sm.clone() }),

        PythonResource::ModuleBytecodeRequest(m) => {
            Value::new(PythonBytecodeModule { module: m.clone() })
        }

        PythonResource::ModuleBytecode { .. } => {
            panic!("not yet implemented");
        }

        PythonResource::Resource(data) => Value::new(PythonPackageResource { data: data.clone() }),

        PythonResource::DistributionResource(resource) => {
            Value::new(PythonPackageDistributionResource {
                resource: resource.clone(),
            })
        }

        PythonResource::ExtensionModuleDynamicLibrary(em) => Value::new(PythonExtensionModule {
            em: PythonExtensionModuleFlavor::DynamicLibrary(em.clone()),
        }),

        PythonResource::ExtensionModuleStaticallyLinked(em) => Value::new(PythonExtensionModule {
            em: PythonExtensionModuleFlavor::StaticallyLinked(em.clone()),
        }),

        PythonResource::EggFile(_) => {
            panic!("egg files not supported");
        }

        PythonResource::PathExtension(_) => {
            panic!("path extensions not supported");
        }
    }
}
