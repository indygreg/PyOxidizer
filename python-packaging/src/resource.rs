// Copyright 2022 Gregory Szorc.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*! Defines types representing Python resources. */

use {
    crate::{
        bytecode::{CompileMode, PythonBytecodeCompiler},
        licensing::LicensedComponent,
        module_util::{is_package_from_path, packages_from_module_name, resolve_path_for_module},
        python_source::has_dunder_file,
    },
    anyhow::{anyhow, Result},
    simple_file_manifest::{File, FileData},
    std::{
        borrow::Cow,
        collections::HashMap,
        hash::BuildHasher,
        path::{Path, PathBuf},
    },
};

#[cfg(feature = "serialization")]
use serde::{Deserialize, Serialize};

/// An optimization level for Python bytecode.
///
/// Serialization type: `int`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serialization", derive(Deserialize, Serialize))]
pub enum BytecodeOptimizationLevel {
    /// Optimization level 0.
    ///
    /// Serialized value: `0`
    #[cfg_attr(feature = "serialization", serde(rename = "0"))]
    Zero,

    /// Optimization level 1.
    ///
    /// Serialized value: `1`
    #[cfg_attr(feature = "serialization", serde(rename = "1"))]
    One,

    /// Optimization level 2.
    ///
    /// Serialized value: `2`
    #[cfg_attr(feature = "serialization", serde(rename = "2"))]
    Two,
}

impl BytecodeOptimizationLevel {
    /// Determine hte extra filename tag for bytecode files of this variant.
    pub fn to_extra_tag(&self) -> &'static str {
        match self {
            BytecodeOptimizationLevel::Zero => "",
            BytecodeOptimizationLevel::One => ".opt-1",
            BytecodeOptimizationLevel::Two => ".opt-2",
        }
    }
}

impl TryFrom<i32> for BytecodeOptimizationLevel {
    type Error = &'static str;

    fn try_from(i: i32) -> Result<Self, Self::Error> {
        match i {
            0 => Ok(BytecodeOptimizationLevel::Zero),
            1 => Ok(BytecodeOptimizationLevel::One),
            2 => Ok(BytecodeOptimizationLevel::Two),
            _ => Err("unsupported bytecode optimization level"),
        }
    }
}

impl From<BytecodeOptimizationLevel> for i32 {
    fn from(level: BytecodeOptimizationLevel) -> Self {
        match level {
            BytecodeOptimizationLevel::Zero => 0,
            BytecodeOptimizationLevel::One => 1,
            BytecodeOptimizationLevel::Two => 2,
        }
    }
}

/// A Python module defined via source code.
#[derive(Clone, Debug, PartialEq)]
pub struct PythonModuleSource {
    /// The fully qualified Python module name.
    pub name: String,
    /// Python source code.
    pub source: FileData,
    /// Whether this module is also a package.
    pub is_package: bool,
    /// Tag to apply to bytecode files.
    ///
    /// e.g. `cpython-39`.
    pub cache_tag: String,
    /// Whether this module belongs to the Python standard library.
    ///
    /// Modules with this set are distributed as part of Python itself.
    pub is_stdlib: bool,
    /// Whether this module is a test module.
    ///
    /// Test modules are those defining test code and aren't critical to
    /// run-time functionality of a package.
    pub is_test: bool,
}

impl PythonModuleSource {
    pub fn description(&self) -> String {
        format!("source code for Python module {}", self.name)
    }

    pub fn to_memory(&self) -> Result<Self> {
        Ok(Self {
            name: self.name.clone(),
            source: self.source.to_memory()?,
            is_package: self.is_package,
            cache_tag: self.cache_tag.clone(),
            is_stdlib: self.is_stdlib,
            is_test: self.is_test,
        })
    }

    /// Resolve the package containing this module.
    ///
    /// If this module is a package, returns the name of self.
    pub fn package(&self) -> String {
        if self.is_package {
            self.name.clone()
        } else if let Some(idx) = self.name.rfind('.') {
            self.name[0..idx].to_string()
        } else {
            self.name.clone()
        }
    }

    /// Obtain the top-level package name this module belongs to.
    pub fn top_level_package(&self) -> &str {
        if let Some(idx) = self.name.find('.') {
            &self.name[0..idx]
        } else {
            &self.name
        }
    }

    /// Convert the instance to a BytecodeModule.
    pub fn as_bytecode_module(
        &self,
        optimize_level: BytecodeOptimizationLevel,
    ) -> PythonModuleBytecodeFromSource {
        PythonModuleBytecodeFromSource {
            name: self.name.clone(),
            source: self.source.clone(),
            optimize_level,
            is_package: self.is_package,
            cache_tag: self.cache_tag.clone(),
            is_stdlib: self.is_stdlib,
            is_test: self.is_test,
        }
    }

    /// Resolve the filesystem path for this source module.
    pub fn resolve_path(&self, prefix: &str) -> PathBuf {
        resolve_path_for_module(prefix, &self.name, self.is_package, None)
    }

    /// Whether the source code for this module has __file__
    pub fn has_dunder_file(&self) -> Result<bool> {
        has_dunder_file(&self.source.resolve_content()?)
    }
}

/// Python module bytecode defined via source code.
///
/// This is essentially a request to generate bytecode from Python module
/// source code.
#[derive(Clone, Debug, PartialEq)]
pub struct PythonModuleBytecodeFromSource {
    pub name: String,
    pub source: FileData,
    pub optimize_level: BytecodeOptimizationLevel,
    pub is_package: bool,
    /// Tag to apply to bytecode files.
    ///
    /// e.g. `cpython-39`.
    pub cache_tag: String,
    /// Whether this module belongs to the Python standard library.
    ///
    /// Modules with this set are distributed as part of Python itself.
    pub is_stdlib: bool,
    /// Whether this module is a test module.
    ///
    /// Test modules are those defining test code and aren't critical to
    /// run-time functionality of a package.
    pub is_test: bool,
}

impl PythonModuleBytecodeFromSource {
    pub fn description(&self) -> String {
        format!(
            "bytecode for Python module {} at O{} (compiled from source)",
            self.name, self.optimize_level as i32
        )
    }

    pub fn to_memory(&self) -> Result<Self> {
        Ok(Self {
            name: self.name.clone(),
            source: self.source.to_memory()?,
            optimize_level: self.optimize_level,
            is_package: self.is_package,
            cache_tag: self.cache_tag.clone(),
            is_stdlib: self.is_stdlib,
            is_test: self.is_test,
        })
    }

    /// Compile source to bytecode using a compiler.
    pub fn compile(
        &self,
        compiler: &mut dyn PythonBytecodeCompiler,
        mode: CompileMode,
    ) -> Result<Vec<u8>> {
        compiler.compile(
            &self.source.resolve_content()?,
            &self.name,
            self.optimize_level,
            mode,
        )
    }

    /// Resolve filesystem path to this bytecode.
    pub fn resolve_path(&self, prefix: &str) -> PathBuf {
        let bytecode_tag = match self.optimize_level {
            BytecodeOptimizationLevel::Zero => self.cache_tag.clone(),
            BytecodeOptimizationLevel::One => format!("{}.opt-1", self.cache_tag),
            BytecodeOptimizationLevel::Two => format!("{}.opt-2", self.cache_tag),
        };

        resolve_path_for_module(prefix, &self.name, self.is_package, Some(&bytecode_tag))
    }

    /// Whether the source for this module has __file__.
    pub fn has_dunder_file(&self) -> Result<bool> {
        has_dunder_file(&self.source.resolve_content()?)
    }
}

/// Compiled Python module bytecode.
#[derive(Clone, Debug, PartialEq)]
pub struct PythonModuleBytecode {
    pub name: String,
    bytecode: FileData,
    pub optimize_level: BytecodeOptimizationLevel,
    pub is_package: bool,
    /// Tag to apply to bytecode files.
    ///
    /// e.g. `cpython-39`.
    pub cache_tag: String,
    /// Whether this module belongs to the Python standard library.
    ///
    /// Modules with this set are distributed as part of Python itself.
    pub is_stdlib: bool,
    /// Whether this module is a test module.
    ///
    /// Test modules are those defining test code and aren't critical to
    /// run-time functionality of a package.
    pub is_test: bool,
}

impl PythonModuleBytecode {
    pub fn new(
        name: &str,
        optimize_level: BytecodeOptimizationLevel,
        is_package: bool,
        cache_tag: &str,
        data: &[u8],
    ) -> Self {
        Self {
            name: name.to_string(),
            bytecode: FileData::Memory(data.to_vec()),
            optimize_level,
            is_package,
            cache_tag: cache_tag.to_string(),
            is_stdlib: false,
            is_test: false,
        }
    }

    pub fn from_path(
        name: &str,
        optimize_level: BytecodeOptimizationLevel,
        cache_tag: &str,
        path: &Path,
    ) -> Self {
        Self {
            name: name.to_string(),
            bytecode: FileData::Path(path.to_path_buf()),
            optimize_level,
            is_package: is_package_from_path(path),
            cache_tag: cache_tag.to_string(),
            is_stdlib: false,
            is_test: false,
        }
    }

    pub fn description(&self) -> String {
        format!(
            "bytecode for Python module {} at O{}",
            self.name, self.optimize_level as i32
        )
    }

    pub fn to_memory(&self) -> Result<Self> {
        Ok(Self {
            name: self.name.clone(),
            bytecode: FileData::Memory(self.resolve_bytecode()?),
            optimize_level: self.optimize_level,
            is_package: self.is_package,
            cache_tag: self.cache_tag.clone(),
            is_stdlib: self.is_stdlib,
            is_test: self.is_test,
        })
    }

    /// Resolve the bytecode data for this module.
    pub fn resolve_bytecode(&self) -> Result<Vec<u8>> {
        match &self.bytecode {
            FileData::Memory(data) => Ok(data.clone()),
            FileData::Path(path) => {
                let data = std::fs::read(path)?;

                if data.len() >= 16 {
                    Ok(data[16..data.len()].to_vec())
                } else {
                    Err(anyhow!("bytecode file is too short"))
                }
            }
        }
    }

    /// Sets the bytecode for this module.
    pub fn set_bytecode(&mut self, data: &[u8]) {
        self.bytecode = FileData::Memory(data.to_vec());
    }

    /// Resolve filesystem path to this bytecode.
    pub fn resolve_path(&self, prefix: &str) -> PathBuf {
        let bytecode_tag = match self.optimize_level {
            BytecodeOptimizationLevel::Zero => self.cache_tag.clone(),
            BytecodeOptimizationLevel::One => format!("{}.opt-1", self.cache_tag),
            BytecodeOptimizationLevel::Two => format!("{}.opt-2", self.cache_tag),
        };

        resolve_path_for_module(prefix, &self.name, self.is_package, Some(&bytecode_tag))
    }
}

/// Python package resource data, agnostic of storage location.
#[derive(Clone, Debug, PartialEq)]
pub struct PythonPackageResource {
    /// The leaf-most Python package this resource belongs to.
    pub leaf_package: String,
    /// The relative path within `leaf_package` to this resource.
    pub relative_name: String,
    /// Location of resource data.
    pub data: FileData,
    /// Whether this resource belongs to the Python standard library.
    ///
    /// Modules with this set are distributed as part of Python itself.
    pub is_stdlib: bool,
    /// Whether this resource belongs to a package that is a test.
    pub is_test: bool,
}

impl PythonPackageResource {
    pub fn description(&self) -> String {
        format!("Python package resource {}", self.symbolic_name())
    }

    pub fn to_memory(&self) -> Result<Self> {
        Ok(Self {
            leaf_package: self.leaf_package.clone(),
            relative_name: self.relative_name.clone(),
            data: self.data.to_memory()?,
            is_stdlib: self.is_stdlib,
            is_test: self.is_test,
        })
    }

    pub fn symbolic_name(&self) -> String {
        format!("{}:{}", self.leaf_package, self.relative_name)
    }

    /// Resolve filesystem path to this bytecode.
    pub fn resolve_path(&self, prefix: &str) -> PathBuf {
        let mut path = PathBuf::from(prefix);

        for p in self.leaf_package.split('.') {
            path = path.join(p);
        }

        path = path.join(&self.relative_name);

        path
    }
}

/// Represents where a Python package distribution resource is materialized.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PythonPackageDistributionResourceFlavor {
    /// In a .dist-info directory.
    DistInfo,

    /// In a .egg-info directory.
    EggInfo,
}

/// Represents a file defining Python package metadata.
///
/// Instances of this correspond to files in a `<package>-<version>.dist-info`
/// or `.egg-info` directory.
///
/// In terms of `importlib.metadata` terminology, instances correspond to
/// files in a `Distribution`.
#[derive(Clone, Debug, PartialEq)]
pub struct PythonPackageDistributionResource {
    /// Where the resource is materialized.
    pub location: PythonPackageDistributionResourceFlavor,

    /// The name of the Python package this resource is associated with.
    pub package: String,

    /// Version string of Python package.
    pub version: String,

    /// Name of this resource within the distribution.
    ///
    /// Corresponds to the file name in the `.dist-info` directory for this
    /// package distribution.
    pub name: String,

    /// The raw content of the distribution resource.
    pub data: FileData,
}

impl PythonPackageDistributionResource {
    pub fn description(&self) -> String {
        format!(
            "Python package distribution resource {}:{}",
            self.package, self.name
        )
    }

    pub fn to_memory(&self) -> Result<Self> {
        Ok(Self {
            location: self.location.clone(),
            package: self.package.clone(),
            version: self.version.clone(),
            name: self.name.clone(),
            data: self.data.to_memory()?,
        })
    }

    /// Resolve filesystem path to this resource file.
    pub fn resolve_path(&self, prefix: &str) -> PathBuf {
        // The package name has hyphens normalized to underscores when
        // materialized on the filesystem.
        let normalized_package = self.package.to_lowercase().replace('-', "_");

        let p = match self.location {
            PythonPackageDistributionResourceFlavor::DistInfo => {
                format!("{}-{}.dist-info", normalized_package, self.version)
            }
            PythonPackageDistributionResourceFlavor::EggInfo => {
                format!("{}-{}.egg-info", normalized_package, self.version)
            }
        };

        PathBuf::from(prefix).join(p).join(&self.name)
    }
}

/// Represents a dependency on a library.
///
/// The library can be defined a number of ways and multiple variants may be
/// present.
#[derive(Clone, Debug, PartialEq)]
pub struct LibraryDependency {
    /// Name of the library.
    ///
    /// This will be used to tell the linker what to link.
    pub name: String,

    /// Static library version of library.
    pub static_library: Option<FileData>,

    /// The filename the static library should be materialized as.
    pub static_filename: Option<PathBuf>,

    /// Shared library version of library.
    pub dynamic_library: Option<FileData>,

    /// The filename the dynamic library should be materialized as.
    pub dynamic_filename: Option<PathBuf>,

    /// Whether this is a system framework (macOS).
    pub framework: bool,

    /// Whether this is a system library.
    pub system: bool,
}

impl LibraryDependency {
    pub fn to_memory(&self) -> Result<Self> {
        Ok(Self {
            name: self.name.clone(),
            static_library: if let Some(data) = &self.static_library {
                Some(data.to_memory()?)
            } else {
                None
            },
            static_filename: self.static_filename.clone(),
            dynamic_library: if let Some(data) = &self.dynamic_library {
                Some(data.to_memory()?)
            } else {
                None
            },
            dynamic_filename: self.dynamic_filename.clone(),
            framework: self.framework,
            system: self.system,
        })
    }
}

/// Represents a shared library.
#[derive(Clone, Debug, PartialEq)]
pub struct SharedLibrary {
    /// Name of the library.
    ///
    /// This is the import name, not the full filename.
    pub name: String,

    /// Holds the raw content of the shared library.
    pub data: FileData,

    /// The filename the library should be materialized as.
    pub filename: Option<PathBuf>,
}

impl TryFrom<&LibraryDependency> for SharedLibrary {
    type Error = &'static str;

    fn try_from(value: &LibraryDependency) -> Result<Self, Self::Error> {
        if let Some(data) = &value.dynamic_library {
            Ok(Self {
                name: value.name.clone(),
                data: data.clone(),
                filename: value.dynamic_filename.clone(),
            })
        } else {
            Err("library dependency does not have a shared library")
        }
    }
}

impl SharedLibrary {
    pub fn description(&self) -> String {
        format!("shared library {}", self.name)
    }
}

/// Represents a Python extension module.
#[derive(Clone, Debug, PartialEq)]
pub struct PythonExtensionModule {
    /// The module name this extension module is providing.
    pub name: String,
    /// Name of the C function initializing this extension module.
    pub init_fn: Option<String>,
    /// Filename suffix to use when writing extension module data.
    pub extension_file_suffix: String,
    /// File data for linked extension module.
    pub shared_library: Option<FileData>,
    // TODO capture static library?
    /// File data for object files linked together to produce this extension module.
    pub object_file_data: Vec<FileData>,
    /// Whether this extension module is a package.
    pub is_package: bool,
    /// Libraries that this extension depends on.
    pub link_libraries: Vec<LibraryDependency>,
    /// Whether this extension module is part of the Python standard library.
    ///
    /// This is true if the extension is distributed with Python itself.
    pub is_stdlib: bool,
    /// Whether the extension module is built-in by default.
    ///
    /// Some extension modules in Python distributions are always compiled into
    /// libpython. This field will be true for those extension modules.
    pub builtin_default: bool,
    /// Whether the extension must be loaded to initialize Python.
    pub required: bool,
    /// Name of the variant of this extension module.
    ///
    /// This may be set if there are multiple versions of an extension module
    /// available to choose from.
    pub variant: Option<String>,
    /// Licenses that apply to this extension.
    pub license: Option<LicensedComponent>,
}

impl PythonExtensionModule {
    pub fn description(&self) -> String {
        format!("Python extension module {}", self.name)
    }

    pub fn to_memory(&self) -> Result<Self> {
        Ok(Self {
            name: self.name.clone(),
            init_fn: self.init_fn.clone(),
            extension_file_suffix: self.extension_file_suffix.clone(),
            shared_library: if let Some(data) = &self.shared_library {
                Some(data.to_memory()?)
            } else {
                None
            },
            object_file_data: self.object_file_data.clone(),
            is_package: self.is_package,
            link_libraries: self
                .link_libraries
                .iter()
                .map(|l| l.to_memory())
                .collect::<Result<Vec<_>, _>>()?,
            is_stdlib: self.is_stdlib,
            builtin_default: self.builtin_default,
            required: self.required,
            variant: self.variant.clone(),
            license: self.license.clone(),
        })
    }

    /// The file name (without parent components) this extension module should be
    /// realized with.
    pub fn file_name(&self) -> String {
        if let Some(idx) = self.name.rfind('.') {
            let name = &self.name[idx + 1..self.name.len()];
            format!("{}{}", name, self.extension_file_suffix)
        } else {
            format!("{}{}", self.name, self.extension_file_suffix)
        }
    }

    /// Resolve the filesystem path for this extension module.
    pub fn resolve_path(&self, prefix: &str) -> PathBuf {
        let mut path = PathBuf::from(prefix);
        path.extend(self.package_parts());
        path.push(self.file_name());

        path
    }

    /// Returns the part strings constituting the package name.
    pub fn package_parts(&self) -> Vec<String> {
        if let Some(idx) = self.name.rfind('.') {
            let prefix = &self.name[0..idx];
            prefix.split('.').map(|x| x.to_string()).collect()
        } else {
            Vec::new()
        }
    }

    /// Whether the extension module requires additional libraries.
    pub fn requires_libraries(&self) -> bool {
        !self.link_libraries.is_empty()
    }

    /// Whether the extension module is minimally required for a Python interpreter.
    ///
    /// This will be true only for extension modules in the standard library that
    /// are builtins part of libpython or are required as part of Python interpreter
    /// initialization.
    pub fn is_minimally_required(&self) -> bool {
        self.is_stdlib && (self.builtin_default || self.required)
    }

    /// Whether this extension module is already in libpython.
    ///
    /// This is true if this is a stdlib extension module and is a core module or no
    /// shared library extension module is available.
    pub fn in_libpython(&self) -> bool {
        self.is_stdlib && (self.builtin_default || self.shared_library.is_none())
    }

    /// Obtain the top-level package name this module belongs to.
    pub fn top_level_package(&self) -> &str {
        if let Some(idx) = self.name.find('.') {
            &self.name[0..idx]
        } else {
            &self.name
        }
    }
}

/// Represents a collection of variants for a given Python extension module.
#[derive(Clone, Debug, Default)]
pub struct PythonExtensionModuleVariants {
    extensions: Vec<PythonExtensionModule>,
}

impl FromIterator<PythonExtensionModule> for PythonExtensionModuleVariants {
    fn from_iter<I: IntoIterator<Item = PythonExtensionModule>>(iter: I) -> Self {
        Self {
            extensions: Vec::from_iter(iter),
        }
    }
}

impl PythonExtensionModuleVariants {
    pub fn push(&mut self, em: PythonExtensionModule) {
        self.extensions.push(em);
    }

    pub fn is_empty(&self) -> bool {
        self.extensions.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &PythonExtensionModule> {
        self.extensions.iter()
    }

    /// Obtains the default / first variant of an extension module.
    pub fn default_variant(&self) -> &PythonExtensionModule {
        &self.extensions[0]
    }

    /// Choose a variant given preferences.
    pub fn choose_variant<S: BuildHasher>(
        &self,
        variants: &HashMap<String, String, S>,
    ) -> &PythonExtensionModule {
        // The default / first item is the chosen one by default.
        let mut chosen = self.default_variant();

        // But it can be overridden if we passed in a hash defining variant
        // preferences, the hash contains a key with the extension name, and the
        // requested variant value exists.
        if let Some(preferred) = variants.get(&chosen.name) {
            for em in self.iter() {
                if em.variant == Some(preferred.to_string()) {
                    chosen = em;
                    break;
                }
            }
        }

        chosen
    }
}

/// Represents a Python .egg file.
#[derive(Clone, Debug, PartialEq)]
pub struct PythonEggFile {
    /// Content of the .egg file.
    pub data: FileData,
}

impl PythonEggFile {
    pub fn to_memory(&self) -> Result<Self> {
        Ok(Self {
            data: self.data.to_memory()?,
        })
    }
}

/// Represents a Python path extension.
///
/// i.e. a .pth file.
#[derive(Clone, Debug, PartialEq)]
pub struct PythonPathExtension {
    /// Content of the .pth file.
    pub data: FileData,
}

impl PythonPathExtension {
    pub fn to_memory(&self) -> Result<Self> {
        Ok(Self {
            data: self.data.to_memory()?,
        })
    }
}

/// Represents a resource that can be read by Python somehow.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq)]
pub enum PythonResource<'a> {
    /// A module defined by source code.
    ModuleSource(Cow<'a, PythonModuleSource>),
    /// A module defined by a request to generate bytecode from source.
    ModuleBytecodeRequest(Cow<'a, PythonModuleBytecodeFromSource>),
    /// A module defined by existing bytecode.
    ModuleBytecode(Cow<'a, PythonModuleBytecode>),
    /// A non-module resource file.
    PackageResource(Cow<'a, PythonPackageResource>),
    /// A file in a Python package distribution metadata collection.
    PackageDistributionResource(Cow<'a, PythonPackageDistributionResource>),
    /// An extension module.
    ExtensionModule(Cow<'a, PythonExtensionModule>),
    /// A self-contained Python egg.
    EggFile(Cow<'a, PythonEggFile>),
    /// A path extension.
    PathExtension(Cow<'a, PythonPathExtension>),
    /// An arbitrary file and its data.
    File(Cow<'a, File>),
}

impl<'a> PythonResource<'a> {
    /// Resolves the fully qualified resource name.
    pub fn full_name(&self) -> String {
        match self {
            PythonResource::ModuleSource(m) => m.name.clone(),
            PythonResource::ModuleBytecode(m) => m.name.clone(),
            PythonResource::ModuleBytecodeRequest(m) => m.name.clone(),
            PythonResource::PackageResource(resource) => {
                format!("{}.{}", resource.leaf_package, resource.relative_name)
            }
            PythonResource::PackageDistributionResource(resource) => {
                format!("{}:{}", resource.package, resource.name)
            }
            PythonResource::ExtensionModule(em) => em.name.clone(),
            PythonResource::EggFile(_) => "".to_string(),
            PythonResource::PathExtension(_) => "".to_string(),
            PythonResource::File(f) => format!("{}", f.path().display()),
        }
    }

    pub fn is_in_packages(&self, packages: &[String]) -> bool {
        let name = match self {
            PythonResource::ModuleSource(m) => &m.name,
            PythonResource::ModuleBytecode(m) => &m.name,
            PythonResource::ModuleBytecodeRequest(m) => &m.name,
            PythonResource::PackageResource(resource) => &resource.leaf_package,
            PythonResource::PackageDistributionResource(resource) => &resource.package,
            PythonResource::ExtensionModule(em) => &em.name,
            PythonResource::EggFile(_) => return false,
            PythonResource::PathExtension(_) => return false,
            PythonResource::File(_) => return false,
        };

        for package in packages {
            // Even though the entity may not be marked as a package, we allow exact
            // name matches through the filter because this makes sense for filtering.
            // The package annotation is really only useful to influence file layout,
            // when __init__.py files need to be materialized.
            if name == package || packages_from_module_name(name).contains(package) {
                return true;
            }
        }

        false
    }

    /// Create a new instance that is guaranteed to be backed by memory.
    pub fn to_memory(&self) -> Result<Self> {
        Ok(match self {
            PythonResource::ModuleSource(m) => m.to_memory()?.into(),
            PythonResource::ModuleBytecode(m) => m.to_memory()?.into(),
            PythonResource::ModuleBytecodeRequest(m) => m.to_memory()?.into(),
            PythonResource::PackageResource(r) => r.to_memory()?.into(),
            PythonResource::PackageDistributionResource(r) => r.to_memory()?.into(),
            PythonResource::ExtensionModule(m) => m.to_memory()?.into(),
            PythonResource::EggFile(e) => e.to_memory()?.into(),
            PythonResource::PathExtension(e) => e.to_memory()?.into(),
            PythonResource::File(f) => f.to_memory()?.into(),
        })
    }
}

impl<'a> From<PythonModuleSource> for PythonResource<'a> {
    fn from(m: PythonModuleSource) -> Self {
        PythonResource::ModuleSource(Cow::Owned(m))
    }
}

impl<'a> From<&'a PythonModuleSource> for PythonResource<'a> {
    fn from(m: &'a PythonModuleSource) -> Self {
        PythonResource::ModuleSource(Cow::Borrowed(m))
    }
}

impl<'a> From<PythonModuleBytecodeFromSource> for PythonResource<'a> {
    fn from(m: PythonModuleBytecodeFromSource) -> Self {
        PythonResource::ModuleBytecodeRequest(Cow::Owned(m))
    }
}

impl<'a> From<&'a PythonModuleBytecodeFromSource> for PythonResource<'a> {
    fn from(m: &'a PythonModuleBytecodeFromSource) -> Self {
        PythonResource::ModuleBytecodeRequest(Cow::Borrowed(m))
    }
}

impl<'a> From<PythonModuleBytecode> for PythonResource<'a> {
    fn from(m: PythonModuleBytecode) -> Self {
        PythonResource::ModuleBytecode(Cow::Owned(m))
    }
}

impl<'a> From<&'a PythonModuleBytecode> for PythonResource<'a> {
    fn from(m: &'a PythonModuleBytecode) -> Self {
        PythonResource::ModuleBytecode(Cow::Borrowed(m))
    }
}

impl<'a> From<PythonPackageResource> for PythonResource<'a> {
    fn from(r: PythonPackageResource) -> Self {
        PythonResource::PackageResource(Cow::Owned(r))
    }
}

impl<'a> From<&'a PythonPackageResource> for PythonResource<'a> {
    fn from(r: &'a PythonPackageResource) -> Self {
        PythonResource::PackageResource(Cow::Borrowed(r))
    }
}

impl<'a> From<PythonPackageDistributionResource> for PythonResource<'a> {
    fn from(r: PythonPackageDistributionResource) -> Self {
        PythonResource::PackageDistributionResource(Cow::Owned(r))
    }
}

impl<'a> From<&'a PythonPackageDistributionResource> for PythonResource<'a> {
    fn from(r: &'a PythonPackageDistributionResource) -> Self {
        PythonResource::PackageDistributionResource(Cow::Borrowed(r))
    }
}

impl<'a> From<PythonExtensionModule> for PythonResource<'a> {
    fn from(r: PythonExtensionModule) -> Self {
        PythonResource::ExtensionModule(Cow::Owned(r))
    }
}

impl<'a> From<&'a PythonExtensionModule> for PythonResource<'a> {
    fn from(r: &'a PythonExtensionModule) -> Self {
        PythonResource::ExtensionModule(Cow::Borrowed(r))
    }
}

impl<'a> From<PythonEggFile> for PythonResource<'a> {
    fn from(e: PythonEggFile) -> Self {
        PythonResource::EggFile(Cow::Owned(e))
    }
}

impl<'a> From<&'a PythonEggFile> for PythonResource<'a> {
    fn from(e: &'a PythonEggFile) -> Self {
        PythonResource::EggFile(Cow::Borrowed(e))
    }
}

impl<'a> From<PythonPathExtension> for PythonResource<'a> {
    fn from(e: PythonPathExtension) -> Self {
        PythonResource::PathExtension(Cow::Owned(e))
    }
}

impl<'a> From<&'a PythonPathExtension> for PythonResource<'a> {
    fn from(e: &'a PythonPathExtension) -> Self {
        PythonResource::PathExtension(Cow::Borrowed(e))
    }
}

impl<'a> From<File> for PythonResource<'a> {
    fn from(f: File) -> Self {
        PythonResource::File(Cow::Owned(f))
    }
}

impl<'a> From<&'a File> for PythonResource<'a> {
    fn from(f: &'a File) -> Self {
        PythonResource::File(Cow::Borrowed(f))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULT_CACHE_TAG: &str = "cpython-39";

    #[test]
    fn test_is_in_packages() {
        let source = PythonResource::ModuleSource(Cow::Owned(PythonModuleSource {
            name: "foo".to_string(),
            source: FileData::Memory(vec![]),
            is_package: false,
            cache_tag: DEFAULT_CACHE_TAG.to_string(),
            is_stdlib: false,
            is_test: false,
        }));
        assert!(source.is_in_packages(&["foo".to_string()]));
        assert!(!source.is_in_packages(&[]));
        assert!(!source.is_in_packages(&["bar".to_string()]));

        let bytecode = PythonResource::ModuleBytecode(Cow::Owned(PythonModuleBytecode {
            name: "foo".to_string(),
            bytecode: FileData::Memory(vec![]),
            optimize_level: BytecodeOptimizationLevel::Zero,
            is_package: false,
            cache_tag: DEFAULT_CACHE_TAG.to_string(),
            is_stdlib: false,
            is_test: false,
        }));
        assert!(bytecode.is_in_packages(&["foo".to_string()]));
        assert!(!bytecode.is_in_packages(&[]));
        assert!(!bytecode.is_in_packages(&["bar".to_string()]));
    }

    #[test]
    fn package_distribution_resources_path_normalization() {
        // Package names are normalized to lowercase and have hyphens replaced
        // by underscores.
        let mut r = PythonPackageDistributionResource {
            location: PythonPackageDistributionResourceFlavor::DistInfo,
            package: "FoO-Bar".into(),
            version: "1.0".into(),
            name: "resource.txt".into(),
            data: vec![42].into(),
        };

        assert_eq!(
            r.resolve_path("prefix"),
            PathBuf::from("prefix")
                .join("foo_bar-1.0.dist-info")
                .join("resource.txt")
        );

        r.location = PythonPackageDistributionResourceFlavor::EggInfo;

        assert_eq!(
            r.resolve_path("prefix"),
            PathBuf::from("prefix")
                .join("foo_bar-1.0.egg-info")
                .join("resource.txt")
        );
    }
}
