// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Defines primitives representing Python resources.
*/

use {
    super::bytecode::{python_source_encoding, BytecodeCompiler, CompileMode},
    super::fsscan::is_package_from_path,
    crate::app_packaging::resource::{FileContent, FileManifest},
    anyhow::{Context, Result},
    std::collections::BTreeSet,
    std::path::{Path, PathBuf},
};

/// Resolve the set of packages present in a fully qualified module name.
pub fn packages_from_module_name(module: &str) -> BTreeSet<String> {
    let mut package_names = BTreeSet::new();

    let mut search: &str = &module;

    while let Some(idx) = search.rfind('.') {
        package_names.insert(search[0..idx].to_string());
        search = &search[0..idx];
    }

    package_names
}

/// Resolve the set of packages present in a series of fully qualified module names.
pub fn packages_from_module_names<I>(names: I) -> BTreeSet<String>
where
    I: Iterator<Item = String>,
{
    let mut package_names = BTreeSet::new();

    for name in names {
        let mut search: &str = &name;

        while let Some(idx) = search.rfind('.') {
            package_names.insert(search[0..idx].to_string());
            search = &search[0..idx];
        }
    }

    package_names
}

/// Resolve the filesystem path for a module.
///
/// Takes a path prefix, fully-qualified module name, whether the module is a package,
/// and an optional bytecode tag to apply.
pub fn resolve_path_for_module(
    root: &str,
    name: &str,
    is_package: bool,
    bytecode_tag: Option<&str>,
) -> PathBuf {
    let mut module_path = PathBuf::from(root);

    let parts = name.split('.').collect::<Vec<&str>>();

    // All module parts up to the final one are packages/directories.
    for part in &parts[0..parts.len() - 1] {
        module_path.push(*part);
    }

    // A package always exists in its own directory.
    if is_package {
        module_path.push(parts[parts.len() - 1]);
    }

    // If this is a bytecode module, files go in a __pycache__ directories.
    if bytecode_tag.is_some() {
        module_path.push("__pycache__");
    }

    // Packages get normalized to /__init__.py.
    let basename = if is_package {
        "__init__"
    } else {
        parts[parts.len() - 1]
    };

    let suffix = if let Some(tag) = bytecode_tag {
        format!(".{}.pyc", tag)
    } else {
        ".py".to_string()
    };

    module_path.push(format!("{}{}", basename, suffix));

    module_path
}

/// Whether __file__ occurs in Python source code.
pub fn has_dunder_file(source: &[u8]) -> Result<bool> {
    // We can't just look for b"__file__ because the source file may be in
    // encodings like UTF-16. So we need to decode to Unicode first then look for
    // the code points.
    let encoding = python_source_encoding(source);

    let encoder = match encoding_rs::Encoding::for_label(&encoding) {
        Some(encoder) => encoder,
        None => encoding_rs::UTF_8,
    };

    let (source, ..) = encoder.decode(source);

    Ok(source.contains("__file__"))
}

/// Represents binary data that can be fetched from somewhere.
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

/// A Python module defined via source code.
#[derive(Clone, Debug, PartialEq)]
pub struct PythonModuleSource {
    /// The fully qualified Python module name.
    pub name: String,
    /// Python source code.
    pub source: DataLocation,
    /// Whether this module is also a package.
    pub is_package: bool,
}

impl PythonModuleSource {
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

    pub fn as_python_resource(&self) -> PythonResource {
        PythonResource::ModuleSource(self.clone())
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
        }
    }

    /// Resolve the filesystem path for this source module.
    pub fn resolve_path(&self, prefix: &str) -> PathBuf {
        resolve_path_for_module(prefix, &self.name, self.is_package, None)
    }

    /// Add this source module to a `FileManifest`.
    ///
    /// The reference added to `FileManifest` is a copy of this instance and won't
    /// reflect modification made to this instance.
    pub fn add_to_file_manifest(&self, manifest: &mut FileManifest, prefix: &str) -> Result<()> {
        let content = FileContent {
            data: self.source.resolve()?,
            executable: false,
        };

        manifest.add_file(&self.resolve_path(prefix), &content)?;

        for package in packages_from_module_name(&self.name) {
            let package_path = resolve_path_for_module(prefix, &package, true, None);

            if !manifest.has_path(&package_path) {
                manifest.add_file(
                    &package_path,
                    &FileContent {
                        data: vec![],
                        executable: false,
                    },
                )?;
            }
        }

        Ok(())
    }

    /// Whether the source code for this module has __file__
    pub fn has_dunder_file(&self) -> Result<bool> {
        has_dunder_file(&self.source.resolve()?)
    }
}

/// An optimization level for Python bytecode.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BytecodeOptimizationLevel {
    Zero,
    One,
    Two,
}

impl From<i32> for BytecodeOptimizationLevel {
    fn from(i: i32) -> Self {
        match i {
            0 => BytecodeOptimizationLevel::Zero,
            1 => BytecodeOptimizationLevel::One,
            2 => BytecodeOptimizationLevel::Two,
            _ => panic!("unsupported bytecode optimization level"),
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
}

impl PythonModuleBytecodeFromSource {
    pub fn as_python_resource(&self) -> PythonResource {
        PythonResource::ModuleBytecodeRequest(self.clone())
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
        resolve_path_for_module(
            prefix,
            &self.name,
            self.is_package,
            // TODO capture Python version properly
            Some(match self.optimize_level {
                BytecodeOptimizationLevel::Zero => "cpython-37",
                BytecodeOptimizationLevel::One => "cpython-37.opt-1",
                BytecodeOptimizationLevel::Two => "cpython-37.opt-2",
            }),
        )
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
}

impl PythonModuleBytecode {
    pub fn from_path(name: &str, optimize_level: BytecodeOptimizationLevel, path: &Path) -> Self {
        Self {
            name: name.to_string(),
            bytecode: DataLocation::Path(path.to_path_buf()),
            optimize_level,
            is_package: is_package_from_path(path),
        }
    }

    pub fn as_python_resource(&self) -> PythonResource {
        PythonResource::ModuleBytecode(self.clone())
    }

    /// Resolve the bytecode data for this module.
    pub fn resolve_bytecode(&self) -> Result<Vec<u8>> {
        match &self.bytecode {
            DataLocation::Memory(data) => Ok(data.clone()),
            DataLocation::Path(path) => {
                let data = std::fs::read(path)?;

                Ok(data[16..data.len()].to_vec())
            }
        }
    }
}

/// Python package resource data, agnostic of storage location.
#[derive(Clone, Debug, PartialEq)]
pub struct PythonPackageResource {
    /// The full relative path to this resource from a library root.
    pub full_name: String,
    /// The leaf-most Python package this resource belongs to.
    pub leaf_package: String,
    /// The relative path within `leaf_package` to this resource.
    pub relative_name: String,
    /// Location of resource data.
    pub data: DataLocation,
}

impl PythonPackageResource {
    pub fn as_python_resource(&self) -> PythonResource {
        PythonResource::Resource(self.clone())
    }

    pub fn symbolic_name(&self) -> String {
        format!("{}:{}", self.leaf_package, self.relative_name)
    }

    /// Resolve filesystem path to this bytecode.
    pub fn resolve_path(&self, prefix: &str) -> PathBuf {
        PathBuf::from(prefix).join(&self.full_name)
    }

    pub fn add_to_file_manifest(&self, manifest: &mut FileManifest, prefix: &str) -> Result<()> {
        let dest_path = self.resolve_path(prefix);

        manifest.add_file(
            &dest_path,
            &FileContent {
                data: self.data.resolve()?,
                executable: false,
            },
        )
    }
}

/// Represents a file defining Python package metadata.
///
/// Instances of this correspond to files in a `<package>-<version>.dist-info`
/// or `.egg-info` directory.
///
/// In terms of `importlib.metadata` terminology, instances correspond to
/// files in a `Distribution`.
#[derive(Clone, Debug, PartialEq)]
pub struct PythonPackageMetadataResource {
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

/// Represents an extension module that can be packaged.
///
/// This is like a light version of `ExtensionModule`.
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

    pub fn add_to_file_manifest(&self, manifest: &mut FileManifest, prefix: &str) -> Result<()> {
        if let Some(data) = &self.extension_data {
            manifest.add_file(
                &self.resolve_path(prefix),
                &FileContent {
                    data: data.resolve()?,
                    executable: true,
                },
            )
        } else {
            Ok(())
        }
    }
}

/// Represents a Python .egg file.
#[derive(Clone, Debug, PartialEq)]
pub struct PythonEggFile {
    /// Content of the .egg file.
    pub data: DataLocation,
}

/// Represents a Python path extension.
///
/// i.e. a .pth file.
#[derive(Clone, Debug, PartialEq)]
pub struct PythonPathExtension {
    /// Content of the .pth file.
    pub data: DataLocation,
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
    DistributionResource(PythonPackageMetadataResource),
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
}

#[cfg(test)]
mod tests {
    use {super::*, itertools::Itertools, std::iter::FromIterator};

    #[test]
    fn test_packages_from_module_name() {
        assert_eq!(
            packages_from_module_name("foo.bar"),
            BTreeSet::from_iter(vec!["foo".to_string()])
        );
        assert_eq!(
            packages_from_module_name("foo.bar.baz"),
            BTreeSet::from_iter(vec!["foo".to_string(), "foo.bar".to_string()])
        );
    }

    #[test]
    fn test_resolve_path_for_module() {
        assert_eq!(
            resolve_path_for_module(".", "foo", false, None),
            PathBuf::from("./foo.py")
        );
        assert_eq!(
            resolve_path_for_module(".", "foo", false, Some("cpython-37")),
            PathBuf::from("./__pycache__/foo.cpython-37.pyc")
        );
        assert_eq!(
            resolve_path_for_module(".", "foo", true, None),
            PathBuf::from("./foo/__init__.py")
        );
        assert_eq!(
            resolve_path_for_module(".", "foo", true, Some("cpython-37")),
            PathBuf::from("./foo/__pycache__/__init__.cpython-37.pyc")
        );
        assert_eq!(
            resolve_path_for_module(".", "foo.bar", false, None),
            PathBuf::from("./foo/bar.py")
        );
        assert_eq!(
            resolve_path_for_module(".", "foo.bar", false, Some("cpython-37")),
            PathBuf::from("./foo/__pycache__/bar.cpython-37.pyc")
        );
        assert_eq!(
            resolve_path_for_module(".", "foo.bar", true, None),
            PathBuf::from("./foo/bar/__init__.py")
        );
        assert_eq!(
            resolve_path_for_module(".", "foo.bar", true, Some("cpython-37")),
            PathBuf::from("./foo/bar/__pycache__/__init__.cpython-37.pyc")
        );
        assert_eq!(
            resolve_path_for_module(".", "foo.bar.baz", false, None),
            PathBuf::from("./foo/bar/baz.py")
        );
        assert_eq!(
            resolve_path_for_module(".", "foo.bar.baz", false, Some("cpython-37")),
            PathBuf::from("./foo/bar/__pycache__/baz.cpython-37.pyc")
        );
        assert_eq!(
            resolve_path_for_module(".", "foo.bar.baz", true, None),
            PathBuf::from("./foo/bar/baz/__init__.py")
        );
        assert_eq!(
            resolve_path_for_module(".", "foo.bar.baz", true, Some("cpython-37")),
            PathBuf::from("./foo/bar/baz/__pycache__/__init__.cpython-37.pyc")
        );
    }

    #[test]
    fn test_source_module_add_to_manifest_top_level() -> Result<()> {
        let mut m = FileManifest::default();

        PythonModuleSource {
            name: "foo".to_string(),
            source: DataLocation::Memory(vec![]),
            is_package: false,
        }
        .add_to_file_manifest(&mut m, ".")?;

        PythonModuleSource {
            name: "bar".to_string(),
            source: DataLocation::Memory(vec![]),
            is_package: false,
        }
        .add_to_file_manifest(&mut m, ".")?;

        let entries = m.entries().collect_vec();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].0, &PathBuf::from("./bar.py"));
        assert_eq!(entries[1].0, &PathBuf::from("./foo.py"));

        Ok(())
    }

    #[test]
    fn test_source_module_add_to_manifest_top_level_package() -> Result<()> {
        let mut m = FileManifest::default();

        PythonModuleSource {
            name: "foo".to_string(),
            source: DataLocation::Memory(vec![]),
            is_package: true,
        }
        .add_to_file_manifest(&mut m, ".")?;

        let entries = m.entries().collect_vec();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, &PathBuf::from("./foo/__init__.py"));

        Ok(())
    }

    #[test]
    fn test_source_module_add_to_manifest_missing_parent() -> Result<()> {
        let mut m = FileManifest::default();

        PythonModuleSource {
            name: "root.parent.child".to_string(),
            source: DataLocation::Memory(vec![]),
            is_package: false,
        }
        .add_to_file_manifest(&mut m, ".")?;

        let entries = m.entries().collect_vec();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].0, &PathBuf::from("./root/__init__.py"));
        assert_eq!(entries[1].0, &PathBuf::from("./root/parent/__init__.py"));
        assert_eq!(entries[2].0, &PathBuf::from("./root/parent/child.py"));

        Ok(())
    }

    #[test]
    fn test_source_module_add_to_manifest_missing_parent_package() -> Result<()> {
        let mut m = FileManifest::default();

        PythonModuleSource {
            name: "root.parent.child".to_string(),
            source: DataLocation::Memory(vec![]),
            is_package: true,
        }
        .add_to_file_manifest(&mut m, ".")?;

        let entries = m.entries().collect_vec();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].0, &PathBuf::from("./root/__init__.py"));
        assert_eq!(entries[1].0, &PathBuf::from("./root/parent/__init__.py"));
        assert_eq!(
            entries[2].0,
            &PathBuf::from("./root/parent/child/__init__.py")
        );

        Ok(())
    }

    #[test]
    fn test_is_in_packages() {
        let source = PythonResource::ModuleSource(PythonModuleSource {
            name: "foo".to_string(),
            source: DataLocation::Memory(vec![]),
            is_package: false,
        });
        assert!(source.is_in_packages(&["foo".to_string()]));
        assert!(!source.is_in_packages(&[]));
        assert!(!source.is_in_packages(&["bar".to_string()]));

        let bytecode = PythonResource::ModuleBytecode(PythonModuleBytecode {
            name: "foo".to_string(),
            bytecode: DataLocation::Memory(vec![]),
            optimize_level: BytecodeOptimizationLevel::Zero,
            is_package: false,
        });
        assert!(bytecode.is_in_packages(&["foo".to_string()]));
        assert!(!bytecode.is_in_packages(&[]));
        assert!(!bytecode.is_in_packages(&["bar".to_string()]));
    }
}
