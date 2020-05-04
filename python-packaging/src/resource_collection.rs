// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Functionality for collecting Python resources. */

use {
    crate::bytecode::{compute_bytecode_header, BytecodeCompiler, BytecodeHeaderMode, CompileMode},
    crate::module_util::{packages_from_module_name, resolve_path_for_module},
    crate::python_source::has_dunder_file,
    crate::resource::{
        BytecodeOptimizationLevel, DataLocation, PythonExtensionModule, PythonModuleBytecode,
        PythonModuleBytecodeFromSource, PythonModuleSource, PythonPackageDistributionResource,
        PythonPackageResource,
    },
    anyhow::{anyhow, Error, Result},
    python_packed_resources::data::{Resource, ResourceFlavor},
    std::borrow::Cow,
    std::collections::{BTreeMap, BTreeSet, HashMap},
    std::convert::TryFrom,
    std::iter::FromIterator,
    std::path::{Path, PathBuf},
};

/// Describes a policy for the location of Python resources.
#[derive(Clone, Debug, PartialEq)]
pub enum PythonResourcesPolicy {
    /// Only allow Python resources to be loaded from memory.
    ///
    /// If a resource cannot be loaded from memory, attempting to add it should result in
    /// error.
    InMemoryOnly,

    /// Only allow Python resources to be loaded from a filesystem path relative to the binary.
    ///
    /// The `String` represents the path prefix to install resources into.
    FilesystemRelativeOnly(String),

    /// Prefer loading resources from memory and fall back to filesystem relative loading.
    ///
    /// This is a hybrid between `InMemoryOnly` and `FilesystemRelativeOnly`. If
    /// in-memory loading works, it is used. Otherwise loading from a filesystem path
    /// relative to the produced binary is used.
    PreferInMemoryFallbackFilesystemRelative(String),
}

impl TryFrom<&str> for PythonResourcesPolicy {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        if value == "in-memory-only" {
            Ok(PythonResourcesPolicy::InMemoryOnly)
        } else if value.starts_with("filesystem-relative-only:") {
            let prefix = &value["filesystem-relative-only:".len()..];

            Ok(PythonResourcesPolicy::FilesystemRelativeOnly(
                prefix.to_string(),
            ))
        } else if value.starts_with("prefer-in-memory-fallback-filesystem-relative:") {
            let prefix = &value["prefer-in-memory-fallback-filesystem-relative:".len()..];

            Ok(PythonResourcesPolicy::PreferInMemoryFallbackFilesystemRelative(prefix.to_string()))
        } else {
            Err(anyhow!(
                "invalid value for Python Resources Policy: {}",
                value
            ))
        }
    }
}

impl Into<String> for &PythonResourcesPolicy {
    fn into(self) -> String {
        match self {
            PythonResourcesPolicy::FilesystemRelativeOnly(ref prefix) => {
                format!("filesystem-relative-only:{}", prefix)
            }
            PythonResourcesPolicy::InMemoryOnly => "in-memory-only".to_string(),
            PythonResourcesPolicy::PreferInMemoryFallbackFilesystemRelative(ref prefix) => {
                format!("prefer-in-memory-fallback-filesystem-relative:{}", prefix)
            }
        }
    }
}

/// Describes how Python module bytecode will be obtained.
#[derive(Clone, Debug, PartialEq)]
pub enum PythonModuleBytecodeProvider {
    /// Bytecode is already available.
    Provided(DataLocation),
    /// Bytecode will be computed from source.
    FromSource(DataLocation),
}

/// Represents a Python resource entry before it is packaged.
///
/// Instances hold the same fields as `Resource` except fields holding
/// content are backed by a `DataLocation` instead of `Vec<u8>`, since
/// we want data resolution to be lazy. In addition, bytecode can either be
/// provided verbatim or via source.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PrePackagedResource {
    pub flavor: ResourceFlavor,
    pub name: String,
    pub is_package: bool,
    pub is_namespace_package: bool,
    pub in_memory_source: Option<DataLocation>,
    pub in_memory_bytecode: Option<PythonModuleBytecodeProvider>,
    pub in_memory_bytecode_opt1: Option<PythonModuleBytecodeProvider>,
    pub in_memory_bytecode_opt2: Option<PythonModuleBytecodeProvider>,
    pub in_memory_extension_module_shared_library: Option<DataLocation>,
    pub in_memory_resources: Option<BTreeMap<String, DataLocation>>,
    pub in_memory_distribution_resources: Option<BTreeMap<String, DataLocation>>,
    pub in_memory_shared_library: Option<DataLocation>,
    pub shared_library_dependency_names: Option<Vec<String>>,
    // (prefix, source code)
    pub relative_path_module_source: Option<(String, DataLocation)>,
    // (prefix, bytecode tag, source code)
    pub relative_path_bytecode: Option<(String, String, PythonModuleBytecodeProvider)>,
    pub relative_path_bytecode_opt1: Option<(String, String, PythonModuleBytecodeProvider)>,
    pub relative_path_bytecode_opt2: Option<(String, String, PythonModuleBytecodeProvider)>,
    // (prefix, path, data)
    pub relative_path_extension_module_shared_library: Option<(String, PathBuf, DataLocation)>,
    pub relative_path_package_resources: Option<BTreeMap<String, (String, PathBuf, DataLocation)>>,
    pub relative_path_distribution_resources:
        Option<BTreeMap<String, (String, PathBuf, DataLocation)>>,
    pub relative_path_shared_library: Option<(String, DataLocation)>,
}

impl<'a> TryFrom<&PrePackagedResource> for Resource<'a, u8> {
    type Error = Error;

    fn try_from(value: &PrePackagedResource) -> Result<Self, Self::Error> {
        Ok(Self {
            flavor: value.flavor,
            name: Cow::Owned(value.name.clone()),
            is_package: value.is_package,
            is_namespace_package: value.is_namespace_package,
            in_memory_source: if let Some(location) = &value.in_memory_source {
                Some(Cow::Owned(location.resolve()?))
            } else {
                None
            },
            // If bytecode is provided, populate it. If derived from source, leave blank
            // and it will be filled in later.
            in_memory_bytecode: if let Some(PythonModuleBytecodeProvider::Provided(location)) =
                &value.in_memory_bytecode
            {
                Some(Cow::Owned(location.resolve()?))
            } else {
                None
            },
            in_memory_bytecode_opt1: if let Some(PythonModuleBytecodeProvider::Provided(location)) =
                &value.in_memory_bytecode_opt1
            {
                Some(Cow::Owned(location.resolve()?))
            } else {
                None
            },
            in_memory_bytecode_opt2: if let Some(PythonModuleBytecodeProvider::Provided(location)) =
                &value.in_memory_bytecode_opt2
            {
                Some(Cow::Owned(location.resolve()?))
            } else {
                None
            },
            in_memory_extension_module_shared_library: if let Some(location) =
                &value.in_memory_extension_module_shared_library
            {
                Some(Cow::Owned(location.resolve()?))
            } else {
                None
            },
            in_memory_package_resources: if let Some(resources) = &value.in_memory_resources {
                let mut res = HashMap::new();
                for (key, location) in resources {
                    res.insert(Cow::Owned(key.clone()), Cow::Owned(location.resolve()?));
                }
                Some(res)
            } else {
                None
            },
            in_memory_distribution_resources: if let Some(resources) =
                &value.in_memory_distribution_resources
            {
                let mut res = HashMap::new();
                for (key, location) in resources {
                    res.insert(Cow::Owned(key.clone()), Cow::Owned(location.resolve()?));
                }
                Some(res)
            } else {
                None
            },
            in_memory_shared_library: if let Some(location) = &value.in_memory_shared_library {
                Some(Cow::Owned(location.resolve()?))
            } else {
                None
            },
            shared_library_dependency_names: if let Some(names) =
                &value.shared_library_dependency_names
            {
                Some(names.iter().map(|x| Cow::Owned(x.clone())).collect())
            } else {
                None
            },
            relative_path_module_source: if let Some((prefix, _)) =
                &value.relative_path_module_source
            {
                Some(Cow::Owned(resolve_path_for_module(
                    prefix,
                    &value.name,
                    value.is_package,
                    None,
                )))
            } else {
                None
            },
            // Data is stored as source that must be compiled. These fields will be populated as
            // part of packaging, as necessary.
            relative_path_module_bytecode: None,
            relative_path_module_bytecode_opt1: None,
            relative_path_module_bytecode_opt2: None,
            relative_path_extension_module_shared_library: if let Some((_, path, _)) =
                &value.relative_path_extension_module_shared_library
            {
                Some(Cow::Owned(path.clone()))
            } else {
                None
            },
            relative_path_package_resources: if let Some(resources) =
                &value.relative_path_package_resources
            {
                let mut res = HashMap::new();
                for (key, (_, path, _)) in resources {
                    res.insert(Cow::Owned(key.clone()), Cow::Owned(path.clone()));
                }
                Some(res)
            } else {
                None
            },
            relative_path_distribution_resources: if let Some(resources) =
                &value.relative_path_distribution_resources
            {
                let mut res = HashMap::new();
                for (key, (_, path, _)) in resources {
                    res.insert(Cow::Owned(key.clone()), Cow::Owned(path.clone()));
                }
                Some(res)
            } else {
                None
            },
        })
    }
}

impl PrePackagedResource {
    /// Derive additional file installs to perform for filesystem-based resources.
    ///
    /// Returns 3-tuples denoting the relative resource path, data to materialize there,
    /// whether the file should be executable.
    pub fn derive_file_installs(&self) -> Result<Vec<(PathBuf, &DataLocation, bool)>> {
        let mut res = Vec::new();

        if let Some((prefix, location)) = &self.relative_path_module_source {
            res.push((
                resolve_path_for_module(prefix, &self.name, self.is_package, None),
                location,
                false,
            ));
        }

        if let Some((_, path, location)) = &self.relative_path_extension_module_shared_library {
            res.push((path.clone(), location, true));
        }

        if let Some(resources) = &self.relative_path_package_resources {
            for (_, path, location) in resources.values() {
                res.push((path.clone(), location, false));
            }
        }

        if let Some(resources) = &self.relative_path_distribution_resources {
            for (_, path, location) in resources.values() {
                res.push((path.clone(), location, false));
            }
        }

        if let Some((prefix, location)) = &self.relative_path_shared_library {
            res.push((PathBuf::from(prefix).join(&self.name), location, true));
        }

        Ok(res)
    }
}

/// Fill in missing data on parent packages.
///
/// When resources are added, their parent packages could be missing
/// data. If we simply materialized the child resources without the
/// parents, Python's importer would get confused due to the missing
/// resources.
///
/// This function fills in the blanks in our resources state.
///
/// The way this works is that if a child resource has data in
/// a particular field, we populate that field in all its parent
/// packages. If a corresponding fields is already populated, we
/// copy its data as well.
pub fn populate_parent_packages(
    resources: &mut BTreeMap<String, PrePackagedResource>,
) -> Result<()> {
    let original_resources = resources
        .iter()
        .filter_map(|(k, v)| {
            let emit = match v.flavor {
                ResourceFlavor::BuiltinExtensionModule => true,
                ResourceFlavor::Extension => true,
                ResourceFlavor::FrozenModule => true,
                ResourceFlavor::Module => true,
                ResourceFlavor::None => false,
                ResourceFlavor::SharedLibrary => false,
            };

            if emit {
                Some((k.to_owned(), v.to_owned()))
            } else {
                None
            }
        })
        .collect::<Vec<(String, PrePackagedResource)>>();

    for (name, original) in original_resources {
        for package in packages_from_module_name(&name) {
            let entry = resources
                .entry(package.clone())
                .or_insert_with(|| PrePackagedResource {
                    flavor: ResourceFlavor::Module,
                    name: package,
                    ..PrePackagedResource::default()
                });

            // Parents must be packages by definition.
            entry.is_package = true;

            // We want to materialize bytecode on parent packages no matter
            // what. If the original resource has a variant of bytecode in a
            // location, we materialize that variant on parents. We take
            // the source from the parent resource, if present. Otherwise
            // defaulting to empty.
            if original.in_memory_bytecode.is_some() && entry.in_memory_bytecode.is_none() {
                entry.in_memory_bytecode = Some(PythonModuleBytecodeProvider::FromSource(
                    if let Some(source) = &entry.in_memory_source {
                        source.clone()
                    } else {
                        DataLocation::Memory(vec![])
                    },
                ));
            }
            if original.in_memory_bytecode_opt1.is_some() && entry.in_memory_bytecode_opt1.is_none()
            {
                entry.in_memory_bytecode_opt1 = Some(PythonModuleBytecodeProvider::FromSource(
                    if let Some(source) = &entry.in_memory_source {
                        source.clone()
                    } else {
                        DataLocation::Memory(vec![])
                    },
                ));
            }
            if original.in_memory_bytecode_opt2.is_some() && entry.in_memory_bytecode_opt2.is_none()
            {
                entry.in_memory_bytecode_opt2 = Some(PythonModuleBytecodeProvider::FromSource(
                    if let Some(source) = &entry.in_memory_source {
                        source.clone()
                    } else {
                        DataLocation::Memory(vec![])
                    },
                ));
            }

            if let Some((prefix, cache_tag, _)) = &original.relative_path_bytecode {
                if entry.relative_path_bytecode.is_none() {
                    entry.relative_path_bytecode = Some((
                        prefix.clone(),
                        cache_tag.clone(),
                        PythonModuleBytecodeProvider::FromSource(
                            if let Some((_, location)) = &entry.relative_path_module_source {
                                location.clone()
                            } else {
                                DataLocation::Memory(vec![])
                            },
                        ),
                    ));
                }
            }

            if let Some((prefix, cache_tag, _)) = &original.relative_path_bytecode_opt1 {
                if entry.relative_path_bytecode_opt1.is_none() {
                    entry.relative_path_bytecode_opt1 = Some((
                        prefix.clone(),
                        cache_tag.clone(),
                        PythonModuleBytecodeProvider::FromSource(
                            if let Some((_, location)) = &entry.relative_path_module_source {
                                location.clone()
                            } else {
                                DataLocation::Memory(vec![])
                            },
                        ),
                    ));
                }
            }

            if let Some((prefix, cache_tag, _)) = &original.relative_path_bytecode_opt2 {
                if entry.relative_path_bytecode_opt2.is_none() {
                    entry.relative_path_bytecode_opt2 = Some((
                        prefix.clone(),
                        cache_tag.clone(),
                        PythonModuleBytecodeProvider::FromSource(
                            if let Some((_, location)) = &entry.relative_path_module_source {
                                location.clone()
                            } else {
                                DataLocation::Memory(vec![])
                            },
                        ),
                    ));
                }
            }

            // If the child had path-based source, we need to materialize source as well.
            if let Some((prefix, _)) = &original.relative_path_module_source {
                entry
                    .relative_path_module_source
                    .get_or_insert_with(|| (prefix.clone(), DataLocation::Memory(vec![])));
            }

            // Ditto for in-memory source.
            if original.in_memory_source.is_some() {
                entry
                    .in_memory_source
                    .get_or_insert(DataLocation::Memory(vec![]));
            }
        }
    }

    Ok(())
}

/// Describes the location of a Python resource.
pub enum ResourceLocation {
    /// Resource is loaded from memory.
    InMemory,
    /// Resource is loaded from a relative filesystem path.
    RelativePath,
}

/// Represents a finalized collection of Python resources.
///
/// Instances are produced from a `PythonResourceCollector` and a
/// Python interpreter (to compile bytecode).
#[derive(Clone, Debug, Default)]
pub struct PreparedPythonResources<'a> {
    pub resources: BTreeMap<String, Resource<'a, u8>>,
    pub extra_files: Vec<(PathBuf, DataLocation, bool)>,
}

impl<'a> PreparedPythonResources<'a> {
    /// Write resources to packed resources data, version 1.
    pub fn write_packed_resources_v1<W: std::io::Write>(&self, writer: &mut W) -> Result<()> {
        python_packed_resources::writer::write_packed_resources_v1(
            &self
                .resources
                .values()
                .cloned()
                .collect::<Vec<Resource<'a, u8>>>(),
            writer,
            None,
        )
    }
}

/// Type used to collect Python resources to they can be serialized.
///
/// We often want to turn Python resource primitives (module source,
/// bytecode, etc) into a collection of ``Resource`` so they can be
/// serialized to the *Python packed resources* format. This type
/// exists to facilitate doing this.
#[derive(Debug, Clone)]
pub struct PythonResourceCollector {
    policy: PythonResourcesPolicy,
    resources: BTreeMap<String, PrePackagedResource>,
    cache_tag: String,
}

impl PythonResourceCollector {
    /// Construct a new instance of the collector.
    ///
    /// The instance is associated with a resources policy to validate that
    /// added resources conform with rules.
    ///
    /// We also pass a Python bytecode cache tag, which is used to
    /// derive filenames.
    pub fn new(policy: &PythonResourcesPolicy, cache_tag: &str) -> Self {
        Self {
            policy: policy.clone(),
            resources: BTreeMap::new(),
            cache_tag: cache_tag.to_string(),
        }
    }

    /// Obtain the policy for this collector.
    pub fn get_policy(&self) -> &PythonResourcesPolicy {
        &self.policy
    }

    /// Validate that a resource add in the specified location is allowed.
    pub fn check_policy(&self, location: ResourceLocation) -> Result<()> {
        match self.policy {
            PythonResourcesPolicy::InMemoryOnly => match location {
                ResourceLocation::InMemory => Ok(()),
                ResourceLocation::RelativePath => Err(anyhow!(
                    "in-memory-only policy does not allow relative path resources"
                )),
            },
            PythonResourcesPolicy::FilesystemRelativeOnly(_) => match location {
                ResourceLocation::InMemory => Err(anyhow!(
                    "filesystem-relative-only policy does not allow in-memory resources"
                )),
                ResourceLocation::RelativePath => Ok(()),
            },
            PythonResourcesPolicy::PreferInMemoryFallbackFilesystemRelative(_) => Ok(()),
        }
    }

    /// Apply a filter function on resources in this collection and mutate in place.
    ///
    /// If the filter function returns true, the item will be preserved.
    pub fn filter_resources_mut<F>(&mut self, filter: F) -> Result<()>
    where
        F: Fn(&PrePackagedResource) -> bool,
    {
        self.resources = BTreeMap::from_iter(self.resources.iter().filter_map(|(k, v)| {
            if filter(v) {
                Some((k.clone(), v.clone()))
            } else {
                None
            }
        }));

        Ok(())
    }

    /// Obtain `PythonModuleSource` in this instance.
    pub fn get_in_memory_module_sources(&self) -> BTreeMap<String, PythonModuleSource> {
        BTreeMap::from_iter(self.resources.iter().filter_map(|(name, module)| {
            if let Some(location) = &module.in_memory_source {
                Some((
                    name.clone(),
                    PythonModuleSource {
                        name: name.clone(),
                        is_package: module.is_package,
                        source: location.clone(),
                        cache_tag: self.cache_tag.clone(),
                    },
                ))
            } else {
                None
            }
        }))
    }

    /// Obtain resource files in this instance.
    pub fn get_in_memory_package_resources(&self) -> BTreeMap<String, BTreeMap<String, Vec<u8>>> {
        BTreeMap::from_iter(self.resources.iter().filter_map(|(name, module)| {
            if let Some(resources) = &module.in_memory_resources {
                Some((
                    name.clone(),
                    BTreeMap::from_iter(resources.iter().map(|(key, value)| {
                        (
                            key.clone(),
                            // TODO should return a DataLocation or Result.
                            value.resolve().expect("resolved resource location"),
                        )
                    })),
                ))
            } else {
                None
            }
        }))
    }

    /// Add Python module source to be loaded from memory.
    pub fn add_in_memory_python_module_source(
        &mut self,
        module: &PythonModuleSource,
    ) -> Result<()> {
        self.check_policy(ResourceLocation::InMemory)?;

        let entry = self
            .resources
            .entry(module.name.clone())
            .or_insert_with(|| PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: module.name.clone(),
                ..PrePackagedResource::default()
            });
        entry.is_package = module.is_package;
        entry.in_memory_source = Some(module.source.clone());

        Ok(())
    }

    /// Add Python module source to be loaded from a file on the filesystem relative to the resources.
    pub fn add_relative_path_python_module_source(
        &mut self,
        module: &PythonModuleSource,
        prefix: &str,
    ) -> Result<()> {
        self.check_policy(ResourceLocation::RelativePath)?;
        let entry = self
            .resources
            .entry(module.name.clone())
            .or_insert_with(|| PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: module.name.clone(),
                ..PrePackagedResource::default()
            });

        entry.is_package = module.is_package;
        entry.relative_path_module_source = Some((prefix.to_string(), module.source.clone()));

        Ok(())
    }

    /// Add Python module bytecode to be loaded from memory.
    ///
    /// Actual bytecode is provided, not bytecode derived from source.
    pub fn add_in_memory_python_module_bytecode(
        &mut self,
        module: &PythonModuleBytecode,
    ) -> Result<()> {
        self.check_policy(ResourceLocation::InMemory)?;

        let entry = self
            .resources
            .entry(module.name.clone())
            .or_insert_with(|| PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: module.name.clone(),
                ..PrePackagedResource::default()
            });

        entry.is_package = module.is_package;

        match module.optimize_level {
            // TODO having to resolve the DataLocation here is a bit unfortunate.
            // We could invent a better type to allow the I/O to remain lazy.
            BytecodeOptimizationLevel::Zero => {
                entry.in_memory_bytecode = Some(PythonModuleBytecodeProvider::Provided(
                    DataLocation::Memory(module.resolve_bytecode()?),
                ));
            }
            BytecodeOptimizationLevel::One => {
                entry.in_memory_bytecode_opt1 = Some(PythonModuleBytecodeProvider::Provided(
                    DataLocation::Memory(module.resolve_bytecode()?),
                ));
            }
            BytecodeOptimizationLevel::Two => {
                entry.in_memory_bytecode_opt2 = Some(PythonModuleBytecodeProvider::Provided(
                    DataLocation::Memory(module.resolve_bytecode()?),
                ));
            }
        }

        Ok(())
    }

    /// Add Python module bytecode from source to the collection.
    pub fn add_in_memory_python_module_bytecode_from_source(
        &mut self,
        module: &PythonModuleBytecodeFromSource,
    ) -> Result<()> {
        self.check_policy(ResourceLocation::InMemory)?;
        let entry = self
            .resources
            .entry(module.name.clone())
            .or_insert_with(|| PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: module.name.clone(),
                ..PrePackagedResource::default()
            });

        entry.is_package = module.is_package;

        match module.optimize_level {
            BytecodeOptimizationLevel::Zero => {
                entry.in_memory_bytecode = Some(PythonModuleBytecodeProvider::FromSource(
                    module.source.clone(),
                ));
            }
            BytecodeOptimizationLevel::One => {
                entry.in_memory_bytecode_opt1 = Some(PythonModuleBytecodeProvider::FromSource(
                    module.source.clone(),
                ));
            }
            BytecodeOptimizationLevel::Two => {
                entry.in_memory_bytecode_opt2 = Some(PythonModuleBytecodeProvider::FromSource(
                    module.source.clone(),
                ));
            }
        }

        Ok(())
    }

    pub fn add_relative_path_python_module_bytecode(
        &mut self,
        module: &PythonModuleBytecode,
        prefix: &str,
    ) -> Result<()> {
        self.check_policy(ResourceLocation::RelativePath)?;

        let entry = self
            .resources
            .entry(module.name.clone())
            .or_insert_with(|| PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: module.name.clone(),
                ..PrePackagedResource::default()
            });

        entry.is_package = module.is_package;

        match module.optimize_level {
            BytecodeOptimizationLevel::Zero => {
                entry.relative_path_bytecode = Some((
                    prefix.to_string(),
                    module.cache_tag.clone(),
                    PythonModuleBytecodeProvider::Provided(DataLocation::Memory(
                        module.resolve_bytecode()?,
                    )),
                ));
            }
            BytecodeOptimizationLevel::One => {
                entry.relative_path_bytecode_opt1 = Some((
                    prefix.to_string(),
                    module.cache_tag.clone(),
                    PythonModuleBytecodeProvider::Provided(DataLocation::Memory(
                        module.resolve_bytecode()?,
                    )),
                ));
            }
            BytecodeOptimizationLevel::Two => {
                entry.relative_path_bytecode_opt2 = Some((
                    prefix.to_string(),
                    module.cache_tag.clone(),
                    PythonModuleBytecodeProvider::Provided(DataLocation::Memory(
                        module.resolve_bytecode()?,
                    )),
                ));
            }
        }

        Ok(())
    }

    /// Add a Python bytecode module from source to be loaded from the filesystem relative to some entity.
    pub fn add_relative_path_python_module_bytecode_from_source(
        &mut self,
        module: &PythonModuleBytecodeFromSource,
        prefix: &str,
    ) -> Result<()> {
        self.check_policy(ResourceLocation::RelativePath)?;
        let entry = self
            .resources
            .entry(module.name.clone())
            .or_insert_with(|| PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: module.name.clone(),
                ..PrePackagedResource::default()
            });

        entry.is_package = module.is_package;

        match module.optimize_level {
            BytecodeOptimizationLevel::Zero => {
                entry.relative_path_bytecode = Some((
                    prefix.to_string(),
                    module.cache_tag.clone(),
                    PythonModuleBytecodeProvider::FromSource(module.source.clone()),
                ))
            }
            BytecodeOptimizationLevel::One => {
                entry.relative_path_bytecode_opt1 = Some((
                    prefix.to_string(),
                    module.cache_tag.clone(),
                    PythonModuleBytecodeProvider::FromSource(module.source.clone()),
                ))
            }
            BytecodeOptimizationLevel::Two => {
                entry.relative_path_bytecode_opt2 = Some((
                    prefix.to_string(),
                    module.cache_tag.clone(),
                    PythonModuleBytecodeProvider::FromSource(module.source.clone()),
                ))
            }
        }

        Ok(())
    }

    /// Add resource data.
    ///
    /// Resource data belongs to a Python package and has a name and bytes data.
    pub fn add_in_memory_python_package_resource(
        &mut self,
        resource: &PythonPackageResource,
    ) -> Result<()> {
        self.check_policy(ResourceLocation::InMemory)?;
        let entry = self
            .resources
            .entry(resource.leaf_package.clone())
            .or_insert_with(|| PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: resource.leaf_package.clone(),
                ..PrePackagedResource::default()
            });

        // Adding a resource automatically makes the module a package.
        entry.is_package = true;

        if entry.in_memory_resources.is_none() {
            entry.in_memory_resources = Some(BTreeMap::new());
        }

        entry
            .in_memory_resources
            .as_mut()
            .unwrap()
            .insert(resource.relative_name.clone(), resource.data.clone());

        Ok(())
    }

    /// Add resource data to be loaded from the filesystem.
    pub fn add_relative_path_python_package_resource(
        &mut self,
        prefix: &str,
        resource: &PythonPackageResource,
    ) -> Result<()> {
        self.check_policy(ResourceLocation::RelativePath)?;
        let entry = self
            .resources
            .entry(resource.leaf_package.clone())
            .or_insert_with(|| PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: resource.leaf_package.clone(),
                ..PrePackagedResource::default()
            });

        // Adding a resource automatically makes the module a package.
        entry.is_package = true;

        if entry.relative_path_package_resources.is_none() {
            entry.relative_path_package_resources = Some(BTreeMap::new());
        }

        entry
            .relative_path_package_resources
            .as_mut()
            .unwrap()
            .insert(
                resource.relative_name.clone(),
                (
                    prefix.to_string(),
                    resource.resolve_path(prefix),
                    resource.data.clone(),
                ),
            );

        Ok(())
    }

    /// Add a package distribution resource to be loaded from memory.
    pub fn add_in_memory_package_distribution_resource(
        &mut self,
        resource: &PythonPackageDistributionResource,
    ) -> Result<()> {
        self.check_policy(ResourceLocation::InMemory)?;

        let entry = self
            .resources
            .entry(resource.package.clone())
            .or_insert_with(|| PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: resource.package.clone(),
                ..PrePackagedResource::default()
            });

        // A distribution resource makes the entity a package.
        entry.is_package = true;

        if entry.in_memory_distribution_resources.is_none() {
            entry.in_memory_distribution_resources = Some(BTreeMap::new());
        }

        entry
            .in_memory_distribution_resources
            .as_mut()
            .unwrap()
            .insert(resource.name.clone(), resource.data.clone());

        Ok(())
    }

    /// Add a `PythonPackageDistributionResource` to be loaded from a relative filesystem path.
    pub fn add_relative_path_package_distribution_resource(
        &mut self,
        prefix: &str,
        resource: &PythonPackageDistributionResource,
    ) -> Result<()> {
        self.check_policy(ResourceLocation::RelativePath)?;
        let entry = self
            .resources
            .entry(resource.package.clone())
            .or_insert_with(|| PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: resource.package.clone(),
                ..PrePackagedResource::default()
            });

        entry.is_package = true;

        if entry.relative_path_distribution_resources.is_none() {
            entry.relative_path_distribution_resources = Some(BTreeMap::new());
        }

        entry
            .relative_path_distribution_resources
            .as_mut()
            .unwrap()
            .insert(
                resource.name.clone(),
                (
                    prefix.to_string(),
                    resource.resolve_path(prefix),
                    resource.data.clone(),
                ),
            );

        Ok(())
    }

    /// Add a built-in extension module.
    ///
    /// Built-in extension modules are statically linked into the binary.
    pub fn add_builtin_python_extension_module(
        &mut self,
        module: &PythonExtensionModule,
    ) -> Result<()> {
        self.check_policy(ResourceLocation::InMemory)?;

        let entry = self
            .resources
            .entry(module.name.clone())
            .or_insert_with(|| PrePackagedResource {
                flavor: ResourceFlavor::BuiltinExtensionModule,
                name: module.name.clone(),
                ..PrePackagedResource::default()
            });

        entry.is_package = module.is_package;

        Ok(())
    }

    /// Add a Python extension module shared library that should be imported from memory.
    ///
    /// TODO pass in a PythonExtensionModule.
    pub fn add_in_memory_python_extension_module_shared_library(
        &mut self,
        module: &str,
        is_package: bool,
        data: &[u8],
        shared_library_dependency_names: &[&str],
    ) -> Result<()> {
        self.check_policy(ResourceLocation::InMemory)?;
        let entry =
            self.resources
                .entry(module.to_string())
                .or_insert_with(|| PrePackagedResource {
                    flavor: ResourceFlavor::Extension,
                    name: module.to_string(),
                    ..PrePackagedResource::default()
                });

        if is_package {
            entry.is_package = true;
        }
        entry.in_memory_extension_module_shared_library = Some(DataLocation::Memory(data.to_vec()));
        entry.shared_library_dependency_names = Some(
            shared_library_dependency_names
                .iter()
                .map(|x| x.to_string())
                .collect(),
        );

        // TODO add shared library dependency names.

        Ok(())
    }

    /// Add an extension module to be loaded from the filesystem as a dynamic library.
    pub fn add_relative_path_python_extension_module(
        &mut self,
        module: &PythonExtensionModule,
        prefix: &str,
    ) -> Result<()> {
        self.check_policy(ResourceLocation::RelativePath)?;

        if module.extension_data.is_none() {
            return Err(anyhow!("extension module {} lacks shared library data and cannot be loaded from the filesystem", module.name));
        }

        let entry = self
            .resources
            .entry(module.name.clone())
            .or_insert_with(|| PrePackagedResource {
                flavor: ResourceFlavor::Extension,
                name: module.name.clone(),
                ..PrePackagedResource::default()
            });
        entry.is_package = module.is_package;
        entry.relative_path_extension_module_shared_library = Some((
            prefix.to_string(),
            module.resolve_path(prefix),
            module.extension_data.as_ref().unwrap().clone(),
        ));

        // TODO add shared library dependencies.

        Ok(())
    }

    /// Add a shared library to be loaded from memory.
    pub fn add_in_memory_shared_library(&mut self, name: &str, data: &DataLocation) -> Result<()> {
        self.check_policy(ResourceLocation::InMemory)?;

        let entry = self
            .resources
            .entry(name.to_string())
            .or_insert_with(|| PrePackagedResource {
                flavor: ResourceFlavor::SharedLibrary,
                name: name.to_string(),
                ..PrePackagedResource::default()
            });

        entry.in_memory_shared_library = Some(data.clone());

        Ok(())
    }

    /// Add a shared library to be loaded from a relative path.
    pub fn add_relative_path_shared_library(
        &mut self,
        prefix: &str,
        name: &str,
        data: &DataLocation,
    ) -> Result<()> {
        self.check_policy(ResourceLocation::RelativePath)?;

        let resource =
            self.resources
                .entry(name.to_string())
                .or_insert_with(|| PrePackagedResource {
                    flavor: ResourceFlavor::SharedLibrary,
                    name: name.to_string().clone(),
                    ..PrePackagedResource::default()
                });

        resource.relative_path_shared_library = Some((prefix.to_string(), data.clone()));

        Ok(())
    }

    /// Searches for Python sources for references to __file__.
    ///
    /// __file__ usage can be problematic for in-memory modules. This method searches
    /// for its occurrences and returns module names having it present.
    pub fn find_dunder_file(&self) -> Result<BTreeSet<String>> {
        let mut res = BTreeSet::new();

        for (name, module) in &self.resources {
            if let Some(location) = &module.in_memory_source {
                if has_dunder_file(&location.resolve()?)? {
                    res.insert(name.clone());
                }
            }

            if let Some(PythonModuleBytecodeProvider::FromSource(location)) =
                &module.in_memory_bytecode
            {
                if has_dunder_file(&location.resolve()?)? {
                    res.insert(name.clone());
                }
            }

            if let Some(PythonModuleBytecodeProvider::FromSource(location)) =
                &module.in_memory_bytecode_opt1
            {
                if has_dunder_file(&location.resolve()?)? {
                    res.insert(name.clone());
                }
            }

            if let Some(PythonModuleBytecodeProvider::FromSource(location)) =
                &module.in_memory_bytecode_opt2
            {
                if has_dunder_file(&location.resolve()?)? {
                    res.insert(name.clone());
                }
            }
        }

        Ok(res)
    }

    /// Derive a list of extra file installs that need to be performed for referenced resources.
    pub fn derive_file_installs(&self) -> Result<Vec<(PathBuf, &DataLocation, bool)>> {
        let mut res = Vec::new();

        for resource in self.resources.values() {
            res.append(&mut resource.derive_file_installs()?);
        }

        Ok(res)
    }

    /// Converts this collection of resources into a `PreparedPythonResources`.
    pub fn to_prepared_python_resources(
        &self,
        python_exe: &Path,
    ) -> Result<PreparedPythonResources> {
        let mut input_resources = self.resources.clone();
        populate_parent_packages(&mut input_resources)?;

        let mut resources = BTreeMap::new();
        let mut extra_files = Vec::new();

        let mut compiler = BytecodeCompiler::new(python_exe)?;
        {
            for (name, resource) in &input_resources {
                if resource.flavor != ResourceFlavor::Module {
                    continue;
                }

                let mut entry = Resource::try_from(resource)?;

                if let Some(PythonModuleBytecodeProvider::FromSource(location)) =
                    &resource.in_memory_bytecode
                {
                    entry.in_memory_bytecode = Some(Cow::Owned(compiler.compile(
                        &location.resolve()?,
                        &name,
                        BytecodeOptimizationLevel::Zero,
                        CompileMode::Bytecode,
                    )?));
                }

                if let Some(PythonModuleBytecodeProvider::FromSource(location)) =
                    &resource.in_memory_bytecode_opt1
                {
                    entry.in_memory_bytecode_opt1 = Some(Cow::Owned(compiler.compile(
                        &location.resolve()?,
                        &name,
                        BytecodeOptimizationLevel::One,
                        CompileMode::Bytecode,
                    )?));
                }

                if let Some(PythonModuleBytecodeProvider::FromSource(location)) =
                    &resource.in_memory_bytecode_opt2
                {
                    entry.in_memory_bytecode_opt2 = Some(Cow::Owned(compiler.compile(
                        &location.resolve()?,
                        &name,
                        BytecodeOptimizationLevel::Two,
                        CompileMode::Bytecode,
                    )?));
                }

                if let Some((prefix, cache_tag, provider)) = &resource.relative_path_bytecode {
                    let path = resolve_path_for_module(
                        prefix,
                        &resource.name,
                        resource.is_package,
                        Some(&format!(
                            "{}{}",
                            cache_tag,
                            BytecodeOptimizationLevel::Zero.to_extra_tag()
                        )),
                    );

                    extra_files.push((
                        path.clone(),
                        DataLocation::Memory(match provider {
                            PythonModuleBytecodeProvider::FromSource(location) => compiler
                                .compile(
                                    &location.resolve()?,
                                    &name,
                                    BytecodeOptimizationLevel::Zero,
                                    CompileMode::PycUncheckedHash,
                                )?,
                            PythonModuleBytecodeProvider::Provided(location) => {
                                let mut data = compute_bytecode_header(
                                    compiler.magic_number,
                                    BytecodeHeaderMode::UncheckedHash(0),
                                )?;
                                data.extend(location.resolve()?);

                                data
                            }
                        }),
                        false,
                    ));

                    entry.relative_path_module_bytecode = Some(Cow::Owned(path));
                }

                if let Some((prefix, cache_tag, provider)) = &resource.relative_path_bytecode_opt1 {
                    let path = resolve_path_for_module(
                        prefix,
                        &resource.name,
                        resource.is_package,
                        Some(&format!(
                            "{}{}",
                            cache_tag,
                            BytecodeOptimizationLevel::One.to_extra_tag()
                        )),
                    );

                    extra_files.push((
                        path.clone(),
                        DataLocation::Memory(match provider {
                            PythonModuleBytecodeProvider::FromSource(location) => compiler
                                .compile(
                                    &location.resolve()?,
                                    &name,
                                    BytecodeOptimizationLevel::One,
                                    CompileMode::PycUncheckedHash,
                                )?,
                            PythonModuleBytecodeProvider::Provided(location) => {
                                let mut data = compute_bytecode_header(
                                    compiler.magic_number,
                                    BytecodeHeaderMode::UncheckedHash(0),
                                )?;
                                data.extend(location.resolve()?);

                                data
                            }
                        }),
                        false,
                    ));

                    entry.relative_path_module_bytecode_opt1 = Some(Cow::Owned(path));
                }

                if let Some((prefix, cache_tag, provider)) = &resource.relative_path_bytecode_opt2 {
                    let path = resolve_path_for_module(
                        prefix,
                        &resource.name,
                        resource.is_package,
                        Some(&format!(
                            "{}{}",
                            cache_tag,
                            BytecodeOptimizationLevel::Two.to_extra_tag()
                        )),
                    );

                    extra_files.push((
                        path.clone(),
                        DataLocation::Memory(match provider {
                            PythonModuleBytecodeProvider::FromSource(location) => compiler
                                .compile(
                                    &location.resolve()?,
                                    &name,
                                    BytecodeOptimizationLevel::Two,
                                    CompileMode::PycUncheckedHash,
                                )?,
                            PythonModuleBytecodeProvider::Provided(location) => {
                                let mut data = compute_bytecode_header(
                                    compiler.magic_number,
                                    BytecodeHeaderMode::UncheckedHash(0),
                                )?;
                                data.extend(location.resolve()?);

                                data
                            }
                        }),
                        false,
                    ));

                    entry.relative_path_module_bytecode_opt2 = Some(Cow::Owned(path));
                }

                resources.insert(name.clone(), entry);
            }
        }

        Ok(PreparedPythonResources {
            resources,
            extra_files,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULT_CACHE_TAG: &str = "cpython-37";

    #[test]
    fn test_resource_policy_from_str() -> Result<()> {
        assert_eq!(
            PythonResourcesPolicy::try_from("in-memory-only")?,
            PythonResourcesPolicy::InMemoryOnly
        );
        assert_eq!(
            PythonResourcesPolicy::try_from("filesystem-relative-only:lib")?,
            PythonResourcesPolicy::FilesystemRelativeOnly("lib".to_string())
        );
        assert_eq!(
            PythonResourcesPolicy::try_from("prefer-in-memory-fallback-filesystem-relative:lib")?,
            PythonResourcesPolicy::PreferInMemoryFallbackFilesystemRelative("lib".to_string())
        );
        assert_eq!(
            PythonResourcesPolicy::try_from("foo")
                .unwrap_err()
                .to_string(),
            "invalid value for Python Resources Policy: foo"
        );

        Ok(())
    }

    #[test]
    fn test_populate_parent_packages_in_memory_source() -> Result<()> {
        let mut h = BTreeMap::new();
        h.insert(
            "root.parent.child".to_string(),
            PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: "root.parent.child".to_string(),
                in_memory_source: Some(DataLocation::Memory(vec![42])),
                is_package: true,
                ..PrePackagedResource::default()
            },
        );

        populate_parent_packages(&mut h)?;

        assert_eq!(h.len(), 3);
        assert_eq!(
            h.get("root.parent"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: "root.parent".to_string(),
                is_package: true,
                in_memory_source: Some(DataLocation::Memory(vec![])),
                ..PrePackagedResource::default()
            })
        );
        assert_eq!(
            h.get("root"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: "root".to_string(),
                is_package: true,
                in_memory_source: Some(DataLocation::Memory(vec![])),
                ..PrePackagedResource::default()
            })
        );

        Ok(())
    }

    #[test]
    fn test_populate_parent_packages_relative_path_source() -> Result<()> {
        let mut h = BTreeMap::new();
        h.insert(
            "root.parent.child".to_string(),
            PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: "root.parent.child".to_string(),
                relative_path_module_source: Some((
                    "prefix".to_string(),
                    DataLocation::Memory(vec![42]),
                )),
                is_package: true,
                ..PrePackagedResource::default()
            },
        );

        populate_parent_packages(&mut h)?;

        assert_eq!(h.len(), 3);
        assert_eq!(
            h.get("root.parent"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: "root.parent".to_string(),
                is_package: true,
                relative_path_module_source: Some((
                    "prefix".to_string(),
                    DataLocation::Memory(vec![])
                )),
                ..PrePackagedResource::default()
            })
        );
        assert_eq!(
            h.get("root"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: "root".to_string(),
                is_package: true,
                relative_path_module_source: Some((
                    "prefix".to_string(),
                    DataLocation::Memory(vec![])
                )),
                ..PrePackagedResource::default()
            })
        );

        Ok(())
    }

    #[test]
    fn test_populate_parent_packages_in_memory_bytecode() -> Result<()> {
        let mut h = BTreeMap::new();
        h.insert(
            "root.parent.child".to_string(),
            PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: "root.parent.child".to_string(),
                in_memory_bytecode: Some(PythonModuleBytecodeProvider::FromSource(
                    DataLocation::Memory(vec![42]),
                )),
                is_package: true,
                ..PrePackagedResource::default()
            },
        );

        populate_parent_packages(&mut h)?;

        assert_eq!(h.len(), 3);
        assert_eq!(
            h.get("root.parent"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: "root.parent".to_string(),
                is_package: true,
                in_memory_bytecode: Some(PythonModuleBytecodeProvider::FromSource(
                    DataLocation::Memory(vec![])
                )),
                ..PrePackagedResource::default()
            })
        );
        assert_eq!(
            h.get("root"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: "root".to_string(),
                is_package: true,
                in_memory_bytecode: Some(PythonModuleBytecodeProvider::FromSource(
                    DataLocation::Memory(vec![])
                )),
                ..PrePackagedResource::default()
            })
        );

        Ok(())
    }

    #[test]
    fn test_populate_parent_packages_distribution_extension_module() -> Result<()> {
        let mut h = BTreeMap::new();
        h.insert(
            "foo.bar".to_string(),
            PrePackagedResource {
                flavor: ResourceFlavor::BuiltinExtensionModule,
                name: "foo.bar".to_string(),
                relative_path_extension_module_shared_library: Some((
                    "prefix".to_string(),
                    PathBuf::from("prefix/foo/bar.so"),
                    DataLocation::Memory(vec![42]),
                )),
                ..PrePackagedResource::default()
            },
        );

        populate_parent_packages(&mut h)?;

        assert_eq!(
            h.get("foo"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: "foo".to_string(),
                is_package: true,
                ..PrePackagedResource::default()
            })
        );

        Ok(())
    }

    #[test]
    fn test_populate_parent_packages_relative_extension_module() -> Result<()> {
        let mut h = BTreeMap::new();
        h.insert(
            "foo.bar".to_string(),
            PrePackagedResource {
                flavor: ResourceFlavor::Extension,
                name: "foo.bar".to_string(),
                relative_path_extension_module_shared_library: Some((
                    "prefix".to_string(),
                    PathBuf::from("prefix/foo/bar.so"),
                    DataLocation::Memory(vec![42]),
                )),
                ..PrePackagedResource::default()
            },
        );

        populate_parent_packages(&mut h)?;

        assert_eq!(h.len(), 2);

        assert_eq!(
            h.get("foo"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: "foo".to_string(),
                is_package: true,
                ..PrePackagedResource::default()
            })
        );

        Ok(())
    }

    #[test]
    fn test_add_in_memory_source_module() -> Result<()> {
        let mut r =
            PythonResourceCollector::new(&PythonResourcesPolicy::InMemoryOnly, DEFAULT_CACHE_TAG);
        r.add_in_memory_python_module_source(&PythonModuleSource {
            name: "foo".to_string(),
            source: DataLocation::Memory(vec![42]),
            is_package: false,
            cache_tag: DEFAULT_CACHE_TAG.to_string(),
        })?;

        assert!(r.resources.contains_key("foo"));
        assert_eq!(
            r.resources.get("foo"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: "foo".to_string(),
                is_package: false,
                in_memory_source: Some(DataLocation::Memory(vec![42])),
                ..PrePackagedResource::default()
            })
        );

        Ok(())
    }

    #[test]
    fn test_add_in_memory_source_module_parents() -> Result<()> {
        let mut r =
            PythonResourceCollector::new(&PythonResourcesPolicy::InMemoryOnly, DEFAULT_CACHE_TAG);
        r.add_in_memory_python_module_source(&PythonModuleSource {
            name: "root.parent.child".to_string(),
            source: DataLocation::Memory(vec![42]),
            is_package: true,
            cache_tag: DEFAULT_CACHE_TAG.to_string(),
        })?;

        assert_eq!(r.resources.len(), 1);
        assert_eq!(
            r.resources.get("root.parent.child"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: "root.parent.child".to_string(),
                is_package: true,
                in_memory_source: Some(DataLocation::Memory(vec![42])),
                ..PrePackagedResource::default()
            })
        );

        Ok(())
    }

    #[test]
    fn test_add_relative_path_source_module() -> Result<()> {
        let mut r = PythonResourceCollector::new(
            &PythonResourcesPolicy::FilesystemRelativeOnly("".to_string()),
            DEFAULT_CACHE_TAG,
        );
        r.add_relative_path_python_module_source(
            &PythonModuleSource {
                name: "foo".to_string(),
                source: DataLocation::Memory(vec![42]),
                is_package: false,
                cache_tag: DEFAULT_CACHE_TAG.to_string(),
            },
            "",
        )?;

        assert!(r.resources.contains_key("foo"));
        assert_eq!(
            r.resources.get("foo"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: "foo".to_string(),
                is_package: false,
                relative_path_module_source: Some(("".to_string(), DataLocation::Memory(vec![42]))),
                ..PrePackagedResource::default()
            })
        );
        let entries = r.derive_file_installs()?;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, PathBuf::from("foo.py"));
        assert_eq!(entries[0].1, &DataLocation::Memory(vec![42]));
        assert_eq!(entries[0].2, false);

        Ok(())
    }

    #[test]
    fn test_add_in_memory_bytecode_module() -> Result<()> {
        let mut r =
            PythonResourceCollector::new(&PythonResourcesPolicy::InMemoryOnly, DEFAULT_CACHE_TAG);
        r.add_in_memory_python_module_bytecode(&PythonModuleBytecode::new(
            "foo",
            BytecodeOptimizationLevel::Zero,
            false,
            DEFAULT_CACHE_TAG,
            &vec![42],
        ))?;

        assert!(r.resources.contains_key("foo"));
        assert_eq!(
            r.resources.get("foo"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: "foo".to_string(),
                in_memory_bytecode: Some(PythonModuleBytecodeProvider::Provided(
                    DataLocation::Memory(vec![42])
                )),
                is_package: false,
                ..PrePackagedResource::default()
            })
        );

        Ok(())
    }

    #[test]
    fn test_add_in_memory_bytecode_module_from_source() -> Result<()> {
        let mut r =
            PythonResourceCollector::new(&PythonResourcesPolicy::InMemoryOnly, DEFAULT_CACHE_TAG);
        r.add_in_memory_python_module_bytecode_from_source(&PythonModuleBytecodeFromSource {
            name: "foo".to_string(),
            source: DataLocation::Memory(vec![42]),
            optimize_level: BytecodeOptimizationLevel::Zero,
            is_package: false,
            cache_tag: DEFAULT_CACHE_TAG.to_string(),
        })?;

        assert!(r.resources.contains_key("foo"));
        assert_eq!(
            r.resources.get("foo"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: "foo".to_string(),
                in_memory_bytecode: Some(PythonModuleBytecodeProvider::FromSource(
                    DataLocation::Memory(vec![42])
                )),
                is_package: false,
                ..PrePackagedResource::default()
            })
        );

        Ok(())
    }

    #[test]
    fn test_add_in_memory_bytecode_module_parents() -> Result<()> {
        let mut r =
            PythonResourceCollector::new(&PythonResourcesPolicy::InMemoryOnly, DEFAULT_CACHE_TAG);
        r.add_in_memory_python_module_bytecode_from_source(&PythonModuleBytecodeFromSource {
            name: "root.parent.child".to_string(),
            source: DataLocation::Memory(vec![42]),
            optimize_level: BytecodeOptimizationLevel::One,
            is_package: true,
            cache_tag: DEFAULT_CACHE_TAG.to_string(),
        })?;

        assert_eq!(r.resources.len(), 1);
        assert_eq!(
            r.resources.get("root.parent.child"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: "root.parent.child".to_string(),
                in_memory_bytecode_opt1: Some(PythonModuleBytecodeProvider::FromSource(
                    DataLocation::Memory(vec![42])
                )),
                is_package: true,
                ..PrePackagedResource::default()
            })
        );

        Ok(())
    }

    #[test]
    fn test_add_in_memory_resource() -> Result<()> {
        let mut r =
            PythonResourceCollector::new(&PythonResourcesPolicy::InMemoryOnly, DEFAULT_CACHE_TAG);
        r.add_in_memory_python_package_resource(&PythonPackageResource {
            leaf_package: "foo".to_string(),
            relative_name: "resource.txt".to_string(),
            data: DataLocation::Memory(vec![42]),
        })?;

        assert_eq!(r.resources.len(), 1);
        assert_eq!(
            r.resources.get("foo"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: "foo".to_string(),
                is_package: true,
                in_memory_resources: Some(BTreeMap::from_iter(
                    [("resource.txt".to_string(), DataLocation::Memory(vec![42]))]
                        .iter()
                        .cloned()
                )),
                ..PrePackagedResource::default()
            })
        );

        Ok(())
    }

    #[test]
    fn test_add_relative_path_extension_module() -> Result<()> {
        let mut c = PythonResourceCollector::new(
            &PythonResourcesPolicy::FilesystemRelativeOnly("prefix".to_string()),
            DEFAULT_CACHE_TAG,
        );

        let em = PythonExtensionModule {
            name: "foo.bar".to_string(),
            init_fn: None,
            extension_file_suffix: ".so".to_string(),
            extension_data: Some(DataLocation::Memory(vec![42])),
            object_file_data: vec![],
            is_package: false,
            libraries: vec![],
            library_dirs: vec![],
        };

        c.add_relative_path_python_extension_module(&em, "prefix")?;
        assert_eq!(c.resources.len(), 1);
        assert_eq!(
            c.resources.get("foo.bar"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::Extension,
                name: "foo.bar".to_string(),
                is_package: false,
                relative_path_extension_module_shared_library: Some((
                    "prefix".to_string(),
                    PathBuf::from("prefix/foo/bar.so"),
                    DataLocation::Memory(vec![42])
                )),
                ..PrePackagedResource::default()
            })
        );

        let files = c.derive_file_installs()?;

        assert_eq!(files.len(), 1);
        assert_eq!(
            files[0],
            (
                PathBuf::from("prefix/foo/bar.so"),
                &DataLocation::Memory(vec![42]),
                true
            )
        );

        Ok(())
    }

    #[test]
    fn test_find_dunder_file() -> Result<()> {
        let mut r =
            PythonResourceCollector::new(&PythonResourcesPolicy::InMemoryOnly, DEFAULT_CACHE_TAG);
        assert_eq!(r.find_dunder_file()?.len(), 0);

        r.add_in_memory_python_module_source(&PythonModuleSource {
            name: "foo.bar".to_string(),
            source: DataLocation::Memory(vec![]),
            is_package: false,
            cache_tag: DEFAULT_CACHE_TAG.to_string(),
        })?;
        assert_eq!(r.find_dunder_file()?.len(), 0);

        r.add_in_memory_python_module_source(&PythonModuleSource {
            name: "baz".to_string(),
            source: DataLocation::Memory(Vec::from("import foo; if __file__ == 'ignored'")),
            is_package: false,
            cache_tag: DEFAULT_CACHE_TAG.to_string(),
        })?;
        assert_eq!(r.find_dunder_file()?.len(), 1);
        assert!(r.find_dunder_file()?.contains("baz"));

        r.add_in_memory_python_module_bytecode_from_source(&PythonModuleBytecodeFromSource {
            name: "bytecode".to_string(),
            source: DataLocation::Memory(Vec::from("import foo; if __file__")),
            optimize_level: BytecodeOptimizationLevel::Zero,
            is_package: false,
            cache_tag: DEFAULT_CACHE_TAG.to_string(),
        })?;
        assert_eq!(r.find_dunder_file()?.len(), 2);
        assert!(r.find_dunder_file()?.contains("bytecode"));

        Ok(())
    }
}
