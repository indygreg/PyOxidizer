// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Defines types representing Python resources. */

use {
    crate::bytecode::{BytecodeCompiler, CompileMode},
    crate::module_util::{
        is_package_from_path, packages_from_module_name, resolve_path_for_module,
    },
    crate::python_source::has_dunder_file,
    anyhow::{anyhow, Context, Result},
    std::convert::TryFrom,
    std::path::{Path, PathBuf},
};

/// Represents an abstract location for binary data.
///
/// Data can be backed by memory or by a path in the filesystem.
#[derive(Clone, Debug, PartialEq)]
pub enum DataLocation {
    Path(PathBuf),
    Memory(Vec<u8>),
}

impl DataLocation {
    /// Resolve the raw content of this instance.
    pub fn resolve(&self) -> Result<Vec<u8>> {
        match self {
            DataLocation::Path(p) => std::fs::read(p).context(format!("reading {}", p.display())),
            DataLocation::Memory(data) => Ok(data.clone()),
        }
    }

    /// Resolve the instance to a Memory variant.
    pub fn to_memory(&self) -> Result<DataLocation> {
        Ok(DataLocation::Memory(self.resolve()?))
    }
}

/// An optimization level for Python bytecode.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BytecodeOptimizationLevel {
    Zero,
    One,
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
    pub source: DataLocation,
    /// Whether this module is also a package.
    pub is_package: bool,
    /// Tag to apply to bytecode files.
    ///
    /// e.g. `cpython-37`.
    pub cache_tag: String,
}

impl PythonModuleSource {
    pub fn to_memory(&self) -> Result<Self> {
        Ok(Self {
            name: self.name.clone(),
            source: self.source.to_memory()?,
            is_package: self.is_package,
            cache_tag: self.cache_tag.clone(),
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
        }
    }

    /// Resolve the filesystem path for this source module.
    pub fn resolve_path(&self, prefix: &str) -> PathBuf {
        resolve_path_for_module(prefix, &self.name, self.is_package, None)
    }

    /// Whether the source code for this module has __file__
    pub fn has_dunder_file(&self) -> Result<bool> {
        has_dunder_file(&self.source.resolve()?)
    }
}

/// Python module bytecode defined via source code.
///
/// This is essentially a request to generate bytecode from Python module
/// source code.
#[derive(Clone, Debug, PartialEq)]
pub struct PythonModuleBytecodeFromSource {
    pub name: String,
    pub source: DataLocation,
    pub optimize_level: BytecodeOptimizationLevel,
    pub is_package: bool,
    /// Tag to apply to bytecode files.
    ///
    /// e.g. `cpython-37`.
    pub cache_tag: String,
}

impl PythonModuleBytecodeFromSource {
    pub fn to_memory(&self) -> Result<Self> {
        Ok(Self {
            name: self.name.clone(),
            source: self.source.to_memory()?,
            optimize_level: self.optimize_level,
            is_package: self.is_package,
            cache_tag: self.cache_tag.clone(),
        })
    }

    /// Compile source to bytecode using a compiler.
    pub fn compile(&self, compiler: &mut BytecodeCompiler, mode: CompileMode) -> Result<Vec<u8>> {
        compiler.compile(
            &self.source.resolve()?,
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
        has_dunder_file(&self.source.resolve()?)
    }
}

/// Compiled Python module bytecode.
#[derive(Clone, Debug, PartialEq)]
pub struct PythonModuleBytecode {
    pub name: String,
    bytecode: DataLocation,
    pub optimize_level: BytecodeOptimizationLevel,
    pub is_package: bool,
    /// Tag to apply to bytecode files.
    ///
    /// e.g. `cpython-37`.
    pub cache_tag: String,
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
            bytecode: DataLocation::Memory(data.to_vec()),
            optimize_level,
            is_package,
            cache_tag: cache_tag.to_string(),
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
            bytecode: DataLocation::Path(path.to_path_buf()),
            optimize_level,
            is_package: is_package_from_path(path),
            cache_tag: cache_tag.to_string(),
        }
    }

    pub fn to_memory(&self) -> Result<Self> {
        Ok(Self {
            name: self.name.clone(),
            bytecode: DataLocation::Memory(self.resolve_bytecode()?),
            optimize_level: self.optimize_level,
            is_package: self.is_package,
            cache_tag: self.cache_tag.clone(),
        })
    }

    /// Resolve the bytecode data for this module.
    pub fn resolve_bytecode(&self) -> Result<Vec<u8>> {
        match &self.bytecode {
            DataLocation::Memory(data) => Ok(data.clone()),
            DataLocation::Path(path) => {
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
        self.bytecode = DataLocation::Memory(data.to_vec());
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
    pub data: DataLocation,
}

impl PythonPackageResource {
    pub fn to_memory(&self) -> Result<Self> {
        Ok(Self {
            leaf_package: self.leaf_package.clone(),
            relative_name: self.relative_name.clone(),
            data: self.data.to_memory()?,
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
#[derive(Clone, Debug, PartialEq)]
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
    pub data: DataLocation,
}

impl PythonPackageDistributionResource {
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
        let p = match self.location {
            PythonPackageDistributionResourceFlavor::DistInfo => {
                format!("{}-{}.dist-info", self.package, self.version)
            }
            PythonPackageDistributionResourceFlavor::EggInfo => {
                format!("{}-{}.egg-info", self.package, self.version)
            }
        };

        PathBuf::from(prefix).join(p).join(&self.name)
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
    pub extension_data: Option<DataLocation>,
    /// File data for object files linked together to produce this extension module.
    pub object_file_data: Vec<Vec<u8>>,
    /// Whether this extension module is a package.
    pub is_package: bool,
    /// Names of libraries that we need to link when building extension module.
    pub libraries: Vec<String>,
    /// Paths to directories holding libraries needed for extension module.
    pub library_dirs: Vec<PathBuf>,
}

impl PythonExtensionModule {
    pub fn to_memory(&self) -> Result<Self> {
        Ok(Self {
            name: self.name.clone(),
            init_fn: self.init_fn.clone(),
            extension_file_suffix: self.extension_file_suffix.clone(),
            extension_data: if let Some(data) = &self.extension_data {
                Some(data.to_memory()?)
            } else {
                None
            },
            object_file_data: self.object_file_data.clone(),
            is_package: self.is_package,
            libraries: self.libraries.clone(),
            library_dirs: self.library_dirs.clone(),
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
}

/// Represents a Python .egg file.
#[derive(Clone, Debug, PartialEq)]
pub struct PythonEggFile {
    /// Content of the .egg file.
    pub data: DataLocation,
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
    pub data: DataLocation,
}

impl PythonPathExtension {
    pub fn to_memory(&self) -> Result<Self> {
        Ok(Self {
            data: self.data.to_memory()?,
        })
    }
}

/// Represents a resource that can be read by Python somehow.
#[derive(Clone, Debug, PartialEq)]
pub enum PythonResource {
    /// A module defined by source code.
    ModuleSource(PythonModuleSource),
    /// A module defined by a request to generate bytecode from source.
    ModuleBytecodeRequest(PythonModuleBytecodeFromSource),
    /// A module defined by existing bytecode.
    ModuleBytecode(PythonModuleBytecode),
    /// A non-module resource file.
    Resource(PythonPackageResource),
    /// A file in a Python package distribution metadata collection.
    DistributionResource(PythonPackageDistributionResource),
    /// An extension module that is represented by a dynamic library.
    ExtensionModuleDynamicLibrary(PythonExtensionModule),
    /// An extension module that was built from source and can be statically linked.
    ExtensionModuleStaticallyLinked(PythonExtensionModule),
    /// A self-contained Python egg.
    EggFile(PythonEggFile),
    /// A path extension.
    PathExtension(PythonPathExtension),
}

impl PythonResource {
    /// Resolves the fully qualified resource name.
    pub fn full_name(&self) -> String {
        match self {
            PythonResource::ModuleSource(m) => m.name.clone(),
            PythonResource::ModuleBytecode(m) => m.name.clone(),
            PythonResource::ModuleBytecodeRequest(m) => m.name.clone(),
            PythonResource::Resource(resource) => {
                format!("{}.{}", resource.leaf_package, resource.relative_name)
            }
            PythonResource::DistributionResource(resource) => {
                format!("{}:{}", resource.package, resource.name)
            }
            PythonResource::ExtensionModuleDynamicLibrary(em) => em.name.clone(),
            PythonResource::ExtensionModuleStaticallyLinked(em) => em.name.clone(),
            PythonResource::EggFile(_) => "".to_string(),
            PythonResource::PathExtension(_) => "".to_string(),
        }
    }

    pub fn is_in_packages(&self, packages: &[String]) -> bool {
        let name = match self {
            PythonResource::ModuleSource(m) => &m.name,
            PythonResource::ModuleBytecode(m) => &m.name,
            PythonResource::ModuleBytecodeRequest(m) => &m.name,
            PythonResource::Resource(resource) => &resource.leaf_package,
            PythonResource::DistributionResource(resource) => &resource.package,
            PythonResource::ExtensionModuleDynamicLibrary(em) => &em.name,
            PythonResource::ExtensionModuleStaticallyLinked(em) => &em.name,
            PythonResource::EggFile(_) => return false,
            PythonResource::PathExtension(_) => return false,
        };

        for package in packages {
            // Even though the entity may not be marked as a package, we allow exact
            // name matches through the filter because this makes sense for filtering.
            // The package annotation is really only useful to influence file layout,
            // when __init__.py files need to be materialized.
            if name == package || packages_from_module_name(&name).contains(package) {
                return true;
            }
        }

        false
    }

    /// Create a new instance that is guaranteed to be backed by memory.
    pub fn to_memory(&self) -> Result<Self> {
        Ok(match self {
            PythonResource::ModuleSource(m) => PythonResource::ModuleSource(m.to_memory()?),
            PythonResource::ModuleBytecode(m) => PythonResource::ModuleBytecode(m.to_memory()?),
            PythonResource::ModuleBytecodeRequest(m) => {
                PythonResource::ModuleBytecodeRequest(m.to_memory()?)
            }
            PythonResource::Resource(r) => PythonResource::Resource(r.to_memory()?),
            PythonResource::DistributionResource(r) => {
                PythonResource::DistributionResource(r.to_memory()?)
            }
            PythonResource::ExtensionModuleDynamicLibrary(m) => {
                PythonResource::ExtensionModuleDynamicLibrary(m.to_memory()?)
            }
            PythonResource::ExtensionModuleStaticallyLinked(m) => {
                PythonResource::ExtensionModuleStaticallyLinked(m.to_memory()?)
            }
            PythonResource::EggFile(e) => PythonResource::EggFile(e.to_memory()?),
            PythonResource::PathExtension(e) => PythonResource::PathExtension(e.to_memory()?),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULT_CACHE_TAG: &str = "cpython-37";

    #[test]
    fn test_is_in_packages() {
        let source = PythonResource::ModuleSource(PythonModuleSource {
            name: "foo".to_string(),
            source: DataLocation::Memory(vec![]),
            is_package: false,
            cache_tag: DEFAULT_CACHE_TAG.to_string(),
        });
        assert!(source.is_in_packages(&["foo".to_string()]));
        assert!(!source.is_in_packages(&[]));
        assert!(!source.is_in_packages(&["bar".to_string()]));

        let bytecode = PythonResource::ModuleBytecode(PythonModuleBytecode {
            name: "foo".to_string(),
            bytecode: DataLocation::Memory(vec![]),
            optimize_level: BytecodeOptimizationLevel::Zero,
            is_package: false,
            cache_tag: DEFAULT_CACHE_TAG.to_string(),
        });
        assert!(bytecode.is_in_packages(&["foo".to_string()]));
        assert!(!bytecode.is_in_packages(&[]));
        assert!(!bytecode.is_in_packages(&["bar".to_string()]));
    }
}
