// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Defines primitives representing Python resources.
*/

use {
    super::bytecode::{python_source_encoding, BytecodeCompiler, CompileMode},
    super::fsscan::{is_package_from_path, PythonFileResource},
    crate::app_packaging::resource::{FileContent, FileManifest},
    anyhow::{anyhow, Context, Error, Result},
    std::collections::BTreeSet,
    std::convert::TryFrom,
    std::path::PathBuf,
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
}

/// A Python source module agnostic of location.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceModule {
    /// The fully qualified Python module name.
    pub name: String,
    /// Python source code.
    pub source: DataLocation,
    /// Whether this module is also a package.
    pub is_package: bool,
}

impl SourceModule {
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
    pub fn as_bytecode_module(&self, optimize_level: BytecodeOptimizationLevel) -> BytecodeModule {
        BytecodeModule {
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

/// Python module bytecode, agnostic of location.
#[derive(Clone, Debug, PartialEq)]
pub struct BytecodeModule {
    pub name: String,
    pub source: DataLocation,
    pub optimize_level: BytecodeOptimizationLevel,
    pub is_package: bool,
}

impl BytecodeModule {
    pub fn as_python_resource(&self) -> PythonResource {
        PythonResource::ModuleBytecodeRequest {
            name: self.name.clone(),
            source: self.source.clone(),
            optimize_level: match self.optimize_level {
                BytecodeOptimizationLevel::Zero => 0,
                BytecodeOptimizationLevel::One => 1,
                BytecodeOptimizationLevel::Two => 2,
            },
            is_package: self.is_package,
        }
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

/// Python package resource data, agnostic of storage location.
#[derive(Clone, Debug, PartialEq)]
pub struct ResourceData {
    pub package: String,
    pub name: String,
    pub data: DataLocation,
}

impl ResourceData {
    pub fn full_name(&self) -> String {
        format!("{}:{}", self.package, self.name)
    }

    pub fn as_python_resource(&self) -> PythonResource {
        PythonResource::Resource {
            package: self.package.clone(),
            name: self.name.clone(),
            data: self.data.clone(),
        }
    }

    /// Resolve filesystem path to this bytecode.
    pub fn resolve_path(&self, prefix: &str) -> PathBuf {
        // TODO this logic needs shoring up and testing.
        let mut dest_path = PathBuf::from(prefix);
        dest_path.extend(self.package.split('.'));
        dest_path.push(&self.name);

        dest_path
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

/// Represents an extension module that can be packaged.
///
/// This is like a light version of `ExtensionModule`.
#[derive(Clone, Debug, PartialEq)]
pub struct ExtensionModuleData {
    /// The module name this extension module is providing.
    pub name: String,
    /// Name of the C function initializing this extension module.
    pub init_fn: Option<String>,
    /// Filename suffix to use when writing extension module data.
    pub extension_file_suffix: String,
    /// File data for linked extension module.
    pub extension_data: Option<Vec<u8>>,
    /// File data for object files linked together to produce this extension module.
    pub object_file_data: Vec<Vec<u8>>,
    /// Whether this extension module is a package.
    pub is_package: bool,
    /// Names of libraries that we need to link when building extension module.
    pub libraries: Vec<String>,
    /// Paths to directories holding libraries needed for extension module.
    pub library_dirs: Vec<PathBuf>,
}

impl ExtensionModuleData {
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
                    data: data.clone(),
                    executable: true,
                },
            )
        } else {
            Ok(())
        }
    }
}

/// Represents a resource to make available to the Python interpreter.
#[derive(Clone, Debug)]
pub enum PythonResource {
    /// A module defined by source code.
    ModuleSource(SourceModule),
    /// A module defined by a request to generate bytecode from source.
    ModuleBytecodeRequest {
        name: String,
        source: DataLocation,
        optimize_level: i32,
        is_package: bool,
    },
    /// A module defined by existing bytecode.
    ModuleBytecode {
        name: String,
        bytecode: DataLocation,
        optimize_level: BytecodeOptimizationLevel,
        is_package: bool,
    },
    /// A non-module resource file.
    Resource {
        package: String,
        name: String,
        data: DataLocation,
    },
    /// An extension module that is represented by a dynamic library.
    ExtensionModuleDynamicLibrary(ExtensionModuleData),

    /// An extension module that was built from source and can be statically linked.
    ExtensionModuleStaticallyLinked(ExtensionModuleData),
}

impl TryFrom<&PythonFileResource> for PythonResource {
    type Error = Error;

    fn try_from(resource: &PythonFileResource) -> Result<PythonResource> {
        match resource {
            PythonFileResource::Source {
                full_name, path, ..
            } => {
                let source =
                    std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;

                Ok(PythonResource::ModuleSource(SourceModule {
                    name: full_name.clone(),
                    source: DataLocation::Memory(source),
                    is_package: is_package_from_path(&path),
                }))
            }

            PythonFileResource::Bytecode {
                full_name, path, ..
            } => {
                let bytecode =
                    std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;

                // First 16 bytes are a validation header.
                let bytecode = bytecode[16..bytecode.len()].to_vec();

                Ok(PythonResource::ModuleBytecode {
                    name: full_name.clone(),
                    bytecode: DataLocation::Memory(bytecode),
                    optimize_level: BytecodeOptimizationLevel::Zero,
                    is_package: is_package_from_path(&path),
                })
            }

            PythonFileResource::BytecodeOpt1 {
                full_name, path, ..
            } => {
                let bytecode =
                    std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;

                // First 16 bytes are a validation header.
                let bytecode = bytecode[16..bytecode.len()].to_vec();

                Ok(PythonResource::ModuleBytecode {
                    name: full_name.clone(),
                    bytecode: DataLocation::Memory(bytecode),
                    optimize_level: BytecodeOptimizationLevel::One,
                    is_package: is_package_from_path(&path),
                })
            }

            PythonFileResource::BytecodeOpt2 {
                full_name, path, ..
            } => {
                let bytecode =
                    std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;

                // First 16 bytes are a validation header.
                let bytecode = bytecode[16..bytecode.len()].to_vec();

                Ok(PythonResource::ModuleBytecode {
                    name: full_name.clone(),
                    bytecode: DataLocation::Memory(bytecode),
                    optimize_level: BytecodeOptimizationLevel::Two,
                    is_package: is_package_from_path(&path),
                })
            }

            PythonFileResource::Resource(resource) => {
                let path = &(resource.path);
                let data =
                    std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;

                Ok(PythonResource::Resource {
                    package: resource.package.clone(),
                    name: resource.stem.clone(),
                    data: DataLocation::Memory(data),
                })
            }

            PythonFileResource::ExtensionModule {
                full_name,
                path,
                extension_file_suffix,
                ..
            } => {
                let module_components = full_name.split('.').collect::<Vec<&str>>();
                let final_name = module_components[module_components.len() - 1];
                let init_fn = Some(format!("PyInit_{}", final_name));

                Ok(PythonResource::ExtensionModuleDynamicLibrary(
                    ExtensionModuleData {
                        name: full_name.clone(),
                        init_fn,
                        extension_file_suffix: extension_file_suffix.clone(),
                        extension_data: Some(std::fs::read(path)?),
                        object_file_data: vec![],
                        is_package: is_package_from_path(path),
                        libraries: vec![],
                        library_dirs: vec![],
                    },
                ))
            }

            PythonFileResource::EggFile { .. } => {
                Err(anyhow!("converting egg files not yet supported"))
            }

            PythonFileResource::PthFile { .. } => {
                Err(anyhow!("converting pth files not yet supported"))
            }

            PythonFileResource::Other { .. } => {
                Err(anyhow!("converting other files not yet supported"))
            }
        }
    }
}

impl PythonResource {
    /// Resolves the fully qualified resource name.
    pub fn full_name(&self) -> String {
        match self {
            PythonResource::ModuleSource(m) => m.name.clone(),
            PythonResource::ModuleBytecode { name, .. } => name.clone(),
            PythonResource::ModuleBytecodeRequest { name, .. } => name.clone(),
            PythonResource::Resource { package, name, .. } => format!("{}.{}", package, name),
            PythonResource::ExtensionModuleDynamicLibrary(em) => em.name.clone(),
            PythonResource::ExtensionModuleStaticallyLinked(em) => em.name.clone(),
        }
    }

    pub fn is_in_packages(&self, packages: &[String]) -> bool {
        let name = match self {
            PythonResource::ModuleSource(m) => &m.name,
            PythonResource::ModuleBytecode { name, .. } => name,
            PythonResource::ModuleBytecodeRequest { name, .. } => name,
            PythonResource::Resource { package, .. } => package,
            PythonResource::ExtensionModuleDynamicLibrary(em) => &em.name,
            PythonResource::ExtensionModuleStaticallyLinked(em) => &em.name,
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

        SourceModule {
            name: "foo".to_string(),
            source: DataLocation::Memory(vec![]),
            is_package: false,
        }
        .add_to_file_manifest(&mut m, ".")?;

        SourceModule {
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

        SourceModule {
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

        SourceModule {
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

        SourceModule {
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
        let source = PythonResource::ModuleSource(SourceModule {
            name: "foo".to_string(),
            source: DataLocation::Memory(vec![]),
            is_package: false,
        });
        assert!(source.is_in_packages(&["foo".to_string()]));
        assert!(!source.is_in_packages(&[]));
        assert!(!source.is_in_packages(&["bar".to_string()]));

        let bytecode = PythonResource::ModuleBytecode {
            name: "foo".to_string(),
            bytecode: DataLocation::Memory(vec![]),
            optimize_level: BytecodeOptimizationLevel::Zero,
            is_package: false,
        };
        assert!(bytecode.is_in_packages(&["foo".to_string()]));
        assert!(!bytecode.is_in_packages(&[]));
        assert!(!bytecode.is_in_packages(&["bar".to_string()]));
    }
}
