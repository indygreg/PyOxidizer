// Copyright 2022 Gregory Szorc.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::{borrow::Cow, collections::HashMap, path::Path};

/// Represents an indexed resource.
///
/// The resource has a name and type affinity via various `is_*` fields.
///
/// The data for the resource may be present in the instance or referenced
/// via an external filesystem path.
///
/// Data fields are `Cow<T>` and can either hold a borrowed reference or
/// owned data. This allows the use of a single type to both hold
/// data or reference it from some other location.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resource<'a, X: 'a>
where
    [X]: ToOwned<Owned = Vec<X>>,
{
    /// The resource name.
    pub name: Cow<'a, str>,

    /// Whether this resource defines a Python module/package.
    pub is_python_module: bool,

    /// Whether this resource defines a builtin extension module.
    pub is_python_builtin_extension_module: bool,

    /// Whether this resource defines a frozen Python module.
    pub is_python_frozen_module: bool,

    /// Whether this resource defines a Python extension module.
    pub is_python_extension_module: bool,

    /// Whether this resource defines a shared library.
    pub is_shared_library: bool,

    /// Whether this resource defines data for an arbitrary file.
    ///
    /// If set, `name` is the UTF-8 encoded filename being represented.
    ///
    /// The file data should exist in one of the `file_data_*` fields.
    pub is_utf8_filename_data: bool,

    /// Whether the Python module is a package.
    pub is_python_package: bool,

    /// Whether the Python module is a namespace package.
    pub is_python_namespace_package: bool,

    /// Python module source code to use to import module from memory.
    pub in_memory_source: Option<Cow<'a, [X]>>,

    /// Python module bytecode to use to import module from memory.
    pub in_memory_bytecode: Option<Cow<'a, [X]>>,

    /// Python module bytecode at optimized level 1 to use to import from memory.
    pub in_memory_bytecode_opt1: Option<Cow<'a, [X]>>,

    /// Python module bytecode at optimized level 2 to use to import from memory.
    pub in_memory_bytecode_opt2: Option<Cow<'a, [X]>>,

    /// Native machine code constituting a shared library for an extension module
    /// which can be imported from memory. (Not supported on all platforms.)
    pub in_memory_extension_module_shared_library: Option<Cow<'a, [X]>>,

    /// Mapping of virtual filename to data for resources to expose to Python's
    /// `importlib.resources` API via in-memory data access.
    pub in_memory_package_resources: Option<HashMap<Cow<'a, str>, Cow<'a, [X]>>>,

    /// Mapping of virtual filename to data for package distribution metadata
    /// to expose to Python's `importlib.metadata` API via in-memory data access.
    pub in_memory_distribution_resources: Option<HashMap<Cow<'a, str>, Cow<'a, [X]>>>,

    /// Native machine code constituting a shared library which can be imported from memory.
    ///
    /// In-memory loading of shared libraries is not supported on all platforms.
    pub in_memory_shared_library: Option<Cow<'a, [X]>>,

    /// Sequence of names of shared libraries this resource depends on.
    pub shared_library_dependency_names: Option<Vec<Cow<'a, str>>>,

    /// Relative path to file containing Python module source code.
    pub relative_path_module_source: Option<Cow<'a, Path>>,

    /// Relative path to file containing Python module bytecode.
    pub relative_path_module_bytecode: Option<Cow<'a, Path>>,

    /// Relative path to file containing Python module bytecode at optimization level 1.
    pub relative_path_module_bytecode_opt1: Option<Cow<'a, Path>>,

    /// Relative path to file containing Python module bytecode at optimization level 2.
    pub relative_path_module_bytecode_opt2: Option<Cow<'a, Path>>,

    /// Relative path to file containing Python extension module loadable as a shared library.
    pub relative_path_extension_module_shared_library: Option<Cow<'a, Path>>,

    /// Mapping of Python package resource names to relative filesystem paths for those resources.
    pub relative_path_package_resources: Option<HashMap<Cow<'a, str>, Cow<'a, Path>>>,

    /// Mapping of Python package distribution files to relative filesystem paths for those resources.
    pub relative_path_distribution_resources: Option<HashMap<Cow<'a, str>, Cow<'a, Path>>>,

    /// Whether this resource's file data should be executable.
    pub file_executable: bool,

    /// Holds arbitrary file data in memory.
    pub file_data_embedded: Option<Cow<'a, [X]>>,

    /// Holds arbitrary file data in a relative path encoded in UTF-8.
    pub file_data_utf8_relative_path: Option<Cow<'a, str>>,
}

impl<'a, X> Default for Resource<'a, X>
where
    [X]: ToOwned<Owned = Vec<X>>,
{
    fn default() -> Self {
        Resource {
            name: Cow::Borrowed(""),
            is_python_module: false,
            is_python_builtin_extension_module: false,
            is_python_frozen_module: false,
            is_python_extension_module: false,
            is_shared_library: false,
            is_utf8_filename_data: false,
            is_python_package: false,
            is_python_namespace_package: false,
            in_memory_source: None,
            in_memory_bytecode: None,
            in_memory_bytecode_opt1: None,
            in_memory_bytecode_opt2: None,
            in_memory_extension_module_shared_library: None,
            in_memory_package_resources: None,
            in_memory_distribution_resources: None,
            in_memory_shared_library: None,
            shared_library_dependency_names: None,
            relative_path_module_source: None,
            relative_path_module_bytecode: None,
            relative_path_module_bytecode_opt1: None,
            relative_path_module_bytecode_opt2: None,
            relative_path_extension_module_shared_library: None,
            relative_path_package_resources: None,
            relative_path_distribution_resources: None,
            file_executable: false,
            file_data_embedded: None,
            file_data_utf8_relative_path: None,
        }
    }
}

impl<'a, X> AsRef<Resource<'a, X>> for Resource<'a, X>
where
    [X]: ToOwned<Owned = Vec<X>>,
{
    fn as_ref(&self) -> &Resource<'a, X> {
        self
    }
}

impl<'a, X> Resource<'a, X>
where
    [X]: ToOwned<Owned = Vec<X>>,
{
    /// Merge another resource into this one.
    ///
    /// Fields from other will overwrite fields from self.
    pub fn merge_from(&mut self, other: Resource<'a, X>) -> Result<(), &'static str> {
        if self.name != other.name {
            return Err("resource names must be identical to perform a merge");
        }

        self.is_python_module |= other.is_python_module;
        self.is_python_builtin_extension_module |= other.is_python_builtin_extension_module;
        self.is_python_frozen_module |= other.is_python_frozen_module;
        self.is_python_extension_module |= other.is_python_extension_module;
        self.is_shared_library |= other.is_shared_library;
        self.is_utf8_filename_data |= other.is_utf8_filename_data;
        self.is_python_package |= other.is_python_package;
        self.is_python_namespace_package |= other.is_python_namespace_package;
        if let Some(value) = other.in_memory_source {
            self.in_memory_source.replace(value);
        }
        if let Some(value) = other.in_memory_bytecode {
            self.in_memory_bytecode.replace(value);
        }
        if let Some(value) = other.in_memory_bytecode_opt1 {
            self.in_memory_bytecode_opt1.replace(value);
        }
        if let Some(value) = other.in_memory_bytecode_opt2 {
            self.in_memory_bytecode_opt2.replace(value);
        }
        if let Some(value) = other.in_memory_extension_module_shared_library {
            self.in_memory_extension_module_shared_library
                .replace(value);
        }
        if let Some(value) = other.in_memory_package_resources {
            self.in_memory_package_resources.replace(value);
        }
        if let Some(value) = other.in_memory_distribution_resources {
            self.in_memory_distribution_resources.replace(value);
        }
        if let Some(value) = other.in_memory_shared_library {
            self.in_memory_shared_library.replace(value);
        }
        if let Some(value) = other.shared_library_dependency_names {
            self.shared_library_dependency_names.replace(value);
        }
        if let Some(value) = other.relative_path_module_source {
            self.relative_path_module_source.replace(value);
        }
        if let Some(value) = other.relative_path_module_bytecode {
            self.relative_path_module_bytecode.replace(value);
        }
        if let Some(value) = other.relative_path_module_bytecode_opt1 {
            self.relative_path_module_bytecode_opt1.replace(value);
        }
        if let Some(value) = other.relative_path_module_bytecode_opt2 {
            self.relative_path_module_bytecode_opt2.replace(value);
        }
        if let Some(value) = other.relative_path_extension_module_shared_library {
            self.relative_path_extension_module_shared_library
                .replace(value);
        }
        if let Some(value) = other.relative_path_package_resources {
            self.relative_path_package_resources.replace(value);
        }
        if let Some(value) = other.relative_path_distribution_resources {
            self.relative_path_distribution_resources.replace(value);
        }
        // TODO we should probably store an Option<bool> here so this assignment is
        // unambiguous.
        self.file_executable |= other.file_executable;
        if let Some(value) = other.file_data_embedded {
            self.file_data_embedded.replace(value);
        }
        if let Some(value) = other.file_data_utf8_relative_path {
            self.file_data_utf8_relative_path.replace(value);
        }

        Ok(())
    }

    pub fn to_owned(&self) -> Resource<'static, X> {
        Resource {
            name: Cow::Owned(self.name.clone().into_owned()),
            is_python_module: self.is_python_module,
            is_python_builtin_extension_module: self.is_python_builtin_extension_module,
            is_python_frozen_module: self.is_python_frozen_module,
            is_python_extension_module: self.is_python_extension_module,
            is_shared_library: self.is_shared_library,
            is_utf8_filename_data: self.is_utf8_filename_data,
            is_python_package: self.is_python_package,
            is_python_namespace_package: self.is_python_namespace_package,
            in_memory_source: self
                .in_memory_source
                .as_ref()
                .map(|value| Cow::Owned(value.clone().into_owned())),
            in_memory_bytecode: self
                .in_memory_bytecode
                .as_ref()
                .map(|value| Cow::Owned(value.clone().into_owned())),
            in_memory_bytecode_opt1: self
                .in_memory_bytecode_opt1
                .as_ref()
                .map(|value| Cow::Owned(value.clone().into_owned())),
            in_memory_bytecode_opt2: self
                .in_memory_bytecode_opt2
                .as_ref()
                .map(|value| Cow::Owned(value.clone().into_owned())),
            in_memory_extension_module_shared_library: self
                .in_memory_extension_module_shared_library
                .as_ref()
                .map(|value| Cow::Owned(value.clone().into_owned())),
            in_memory_package_resources: self.in_memory_package_resources.as_ref().map(|value| {
                value
                    .iter()
                    .map(|(k, v)| {
                        (
                            Cow::Owned(k.clone().into_owned()),
                            Cow::Owned(v.clone().into_owned()),
                        )
                    })
                    .collect()
            }),
            in_memory_distribution_resources: self.in_memory_distribution_resources.as_ref().map(
                |value| {
                    value
                        .iter()
                        .map(|(k, v)| {
                            (
                                Cow::Owned(k.clone().into_owned()),
                                Cow::Owned(v.clone().into_owned()),
                            )
                        })
                        .collect()
                },
            ),
            in_memory_shared_library: self
                .in_memory_source
                .as_ref()
                .map(|value| Cow::Owned(value.clone().into_owned())),
            shared_library_dependency_names: self.shared_library_dependency_names.as_ref().map(
                |value| {
                    value
                        .iter()
                        .map(|x| Cow::Owned(x.clone().into_owned()))
                        .collect()
                },
            ),
            relative_path_module_source: self
                .relative_path_module_source
                .as_ref()
                .map(|value| Cow::Owned(value.clone().into_owned())),
            relative_path_module_bytecode: self
                .relative_path_module_bytecode
                .as_ref()
                .map(|value| Cow::Owned(value.clone().into_owned())),
            relative_path_module_bytecode_opt1: self
                .relative_path_module_bytecode_opt1
                .as_ref()
                .map(|value| Cow::Owned(value.clone().into_owned())),
            relative_path_module_bytecode_opt2: self
                .relative_path_module_bytecode_opt2
                .as_ref()
                .map(|value| Cow::Owned(value.clone().into_owned())),
            relative_path_extension_module_shared_library: self
                .relative_path_extension_module_shared_library
                .as_ref()
                .map(|value| Cow::Owned(value.clone().into_owned())),
            relative_path_package_resources: self.relative_path_package_resources.as_ref().map(
                |value| {
                    value
                        .iter()
                        .map(|(k, v)| {
                            (
                                Cow::Owned(k.clone().into_owned()),
                                Cow::Owned(v.clone().into_owned()),
                            )
                        })
                        .collect()
                },
            ),
            relative_path_distribution_resources: self
                .relative_path_distribution_resources
                .as_ref()
                .map(|value| {
                    value
                        .iter()
                        .map(|(k, v)| {
                            (
                                Cow::Owned(k.clone().into_owned()),
                                Cow::Owned(v.clone().into_owned()),
                            )
                        })
                        .collect()
                }),
            file_executable: self.file_executable,
            file_data_embedded: self
                .file_data_embedded
                .as_ref()
                .map(|value| Cow::Owned(value.clone().into_owned())),
            file_data_utf8_relative_path: self
                .file_data_utf8_relative_path
                .as_ref()
                .map(|value| Cow::Owned(value.clone().into_owned())),
        }
    }
}
