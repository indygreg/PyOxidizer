// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Functionality for collecting Python resources. */

use {
    crate::bytecode::{
        compute_bytecode_header, BytecodeHeaderMode, CompileMode, PythonBytecodeCompiler,
    },
    crate::module_util::{packages_from_module_name, resolve_path_for_module},
    crate::policy::PythonResourcesPolicy,
    crate::python_source::has_dunder_file,
    crate::resource::{
        BytecodeOptimizationLevel, DataLocation, PythonExtensionModule, PythonModuleBytecode,
        PythonModuleBytecodeFromSource, PythonModuleSource, PythonPackageDistributionResource,
        PythonPackageResource,
    },
    anyhow::{anyhow, Result},
    python_packed_resources::data::{Resource, ResourceFlavor},
    std::borrow::Cow,
    std::collections::{BTreeMap, BTreeSet, HashMap},
    std::iter::FromIterator,
    std::path::PathBuf,
};

/// Represents a single file install.
///
/// Tuple is the relative install path, the data to install, and whether the file
/// should be executable.
pub type FileInstall = (PathBuf, DataLocation, bool);

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
    // (path, data)
    pub relative_path_extension_module_shared_library: Option<(PathBuf, DataLocation)>,
    pub relative_path_package_resources: Option<BTreeMap<String, (PathBuf, DataLocation)>>,
    pub relative_path_distribution_resources: Option<BTreeMap<String, (PathBuf, DataLocation)>>,
    pub relative_path_shared_library: Option<(String, DataLocation)>,
}

impl PrePackagedResource {
    /// Convert the instance to a `Resource`.
    ///
    /// This will compile bytecode from source code using the specified compiler.
    /// It will also emit a list of file installs that must be performed for all
    /// referenced resources to function as intended.
    pub fn to_resource<'a>(
        &self,
        compiler: &mut dyn PythonBytecodeCompiler,
    ) -> Result<(Resource<'a, u8>, Vec<FileInstall>)> {
        let mut installs = Vec::new();

        let resource = Resource {
            flavor: self.flavor,
            name: Cow::Owned(self.name.clone()),
            is_package: self.is_package,
            is_namespace_package: self.is_namespace_package,
            in_memory_source: if let Some(location) = &self.in_memory_source {
                Some(Cow::Owned(location.resolve()?))
            } else {
                None
            },
            in_memory_bytecode: match &self.in_memory_bytecode {
                Some(PythonModuleBytecodeProvider::Provided(location)) => {
                    Some(Cow::Owned(location.resolve()?))
                }
                Some(PythonModuleBytecodeProvider::FromSource(location)) => {
                    Some(Cow::Owned(compiler.compile(
                        &location.resolve()?,
                        &self.name,
                        BytecodeOptimizationLevel::Zero,
                        CompileMode::Bytecode,
                    )?))
                }
                None => None,
            },
            in_memory_bytecode_opt1: match &self.in_memory_bytecode_opt1 {
                Some(PythonModuleBytecodeProvider::Provided(location)) => {
                    Some(Cow::Owned(location.resolve()?))
                }
                Some(PythonModuleBytecodeProvider::FromSource(location)) => {
                    Some(Cow::Owned(compiler.compile(
                        &location.resolve()?,
                        &self.name,
                        BytecodeOptimizationLevel::One,
                        CompileMode::Bytecode,
                    )?))
                }
                None => None,
            },
            in_memory_bytecode_opt2: match &self.in_memory_bytecode_opt2 {
                Some(PythonModuleBytecodeProvider::Provided(location)) => {
                    Some(Cow::Owned(location.resolve()?))
                }
                Some(PythonModuleBytecodeProvider::FromSource(location)) => {
                    Some(Cow::Owned(compiler.compile(
                        &location.resolve()?,
                        &self.name,
                        BytecodeOptimizationLevel::Two,
                        CompileMode::Bytecode,
                    )?))
                }
                None => None,
            },
            in_memory_extension_module_shared_library: if let Some(location) =
                &self.in_memory_extension_module_shared_library
            {
                Some(Cow::Owned(location.resolve()?))
            } else {
                None
            },
            in_memory_package_resources: if let Some(resources) = &self.in_memory_resources {
                let mut res = HashMap::new();
                for (key, location) in resources {
                    res.insert(Cow::Owned(key.clone()), Cow::Owned(location.resolve()?));
                }
                Some(res)
            } else {
                None
            },
            in_memory_distribution_resources: if let Some(resources) =
                &self.in_memory_distribution_resources
            {
                let mut res = HashMap::new();
                for (key, location) in resources {
                    res.insert(Cow::Owned(key.clone()), Cow::Owned(location.resolve()?));
                }
                Some(res)
            } else {
                None
            },
            in_memory_shared_library: if let Some(location) = &self.in_memory_shared_library {
                Some(Cow::Owned(location.resolve()?))
            } else {
                None
            },
            shared_library_dependency_names: if let Some(names) =
                &self.shared_library_dependency_names
            {
                Some(names.iter().map(|x| Cow::Owned(x.clone())).collect())
            } else {
                None
            },
            relative_path_module_source: if let Some((prefix, location)) =
                &self.relative_path_module_source
            {
                let path = resolve_path_for_module(prefix, &self.name, self.is_package, None);

                installs.push((path.clone(), location.clone(), false));

                Some(Cow::Owned(path))
            } else {
                None
            },
            relative_path_module_bytecode: if let Some((prefix, cache_tag, provider)) =
                &self.relative_path_bytecode
            {
                let path = resolve_path_for_module(
                    prefix,
                    &self.name,
                    self.is_package,
                    Some(&format!(
                        "{}{}",
                        cache_tag,
                        BytecodeOptimizationLevel::Zero.to_extra_tag()
                    )),
                );

                installs.push((
                    path.clone(),
                    DataLocation::Memory(match provider {
                        PythonModuleBytecodeProvider::FromSource(location) => compiler.compile(
                            &location.resolve()?,
                            &self.name,
                            BytecodeOptimizationLevel::Zero,
                            CompileMode::PycUncheckedHash,
                        )?,
                        PythonModuleBytecodeProvider::Provided(location) => {
                            let mut data = compute_bytecode_header(
                                compiler.get_magic_number(),
                                BytecodeHeaderMode::UncheckedHash(0),
                            )?;
                            data.extend(location.resolve()?);

                            data
                        }
                    }),
                    false,
                ));

                Some(Cow::Owned(path))
            } else {
                None
            },
            relative_path_module_bytecode_opt1: if let Some((prefix, cache_tag, provider)) =
                &self.relative_path_bytecode_opt1
            {
                let path = resolve_path_for_module(
                    prefix,
                    &self.name,
                    self.is_package,
                    Some(&format!(
                        "{}{}",
                        cache_tag,
                        BytecodeOptimizationLevel::One.to_extra_tag()
                    )),
                );

                installs.push((
                    path.clone(),
                    DataLocation::Memory(match provider {
                        PythonModuleBytecodeProvider::FromSource(location) => compiler.compile(
                            &location.resolve()?,
                            &self.name,
                            BytecodeOptimizationLevel::One,
                            CompileMode::PycUncheckedHash,
                        )?,
                        PythonModuleBytecodeProvider::Provided(location) => {
                            let mut data = compute_bytecode_header(
                                compiler.get_magic_number(),
                                BytecodeHeaderMode::UncheckedHash(0),
                            )?;
                            data.extend(location.resolve()?);

                            data
                        }
                    }),
                    false,
                ));

                Some(Cow::Owned(path))
            } else {
                None
            },
            relative_path_module_bytecode_opt2: if let Some((prefix, cache_tag, provider)) =
                &self.relative_path_bytecode_opt2
            {
                let path = resolve_path_for_module(
                    prefix,
                    &self.name,
                    self.is_package,
                    Some(&format!(
                        "{}{}",
                        cache_tag,
                        BytecodeOptimizationLevel::Two.to_extra_tag()
                    )),
                );

                installs.push((
                    path.clone(),
                    DataLocation::Memory(match provider {
                        PythonModuleBytecodeProvider::FromSource(location) => compiler.compile(
                            &location.resolve()?,
                            &self.name,
                            BytecodeOptimizationLevel::Two,
                            CompileMode::PycUncheckedHash,
                        )?,
                        PythonModuleBytecodeProvider::Provided(location) => {
                            let mut data = compute_bytecode_header(
                                compiler.get_magic_number(),
                                BytecodeHeaderMode::UncheckedHash(0),
                            )?;
                            data.extend(location.resolve()?);

                            data
                        }
                    }),
                    false,
                ));

                Some(Cow::Owned(path))
            } else {
                None
            },
            relative_path_extension_module_shared_library: if let Some((path, location)) =
                &self.relative_path_extension_module_shared_library
            {
                installs.push((path.clone(), location.clone(), true));

                Some(Cow::Owned(path.clone()))
            } else {
                None
            },
            relative_path_package_resources: if let Some(resources) =
                &self.relative_path_package_resources
            {
                let mut res = HashMap::new();
                for (key, (path, location)) in resources {
                    installs.push((path.clone(), location.clone(), false));

                    res.insert(Cow::Owned(key.clone()), Cow::Owned(path.clone()));
                }
                Some(res)
            } else {
                None
            },
            relative_path_distribution_resources: if let Some(resources) =
                &self.relative_path_distribution_resources
            {
                let mut res = HashMap::new();
                for (key, (path, location)) in resources {
                    installs.push((path.clone(), location.clone(), false));

                    res.insert(Cow::Owned(key.clone()), Cow::Owned(path.clone()));
                }
                Some(res)
            } else {
                None
            },
        };

        if let Some((prefix, location)) = &self.relative_path_shared_library {
            installs.push((
                PathBuf::from(prefix).join(&self.name),
                location.clone(),
                true,
            ));
        }

        Ok((resource, installs))
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
///
/// The location is abstract because a concrete location (such as the
/// relative path) is not specified.
pub enum AbstractResourceLocation {
    /// Resource is loaded from memory.
    InMemory,
    /// Resource is loaded from a relative filesystem path.
    RelativePath,
}

/// Describes the concrete location of a Python resource.
pub enum ConcreteResourceLocation {
    /// Resource is loaded from memory.
    InMemory,
    /// Reosurce is loaded from a relative filesystem path.
    RelativePath(String),
}

impl From<&ConcreteResourceLocation> for AbstractResourceLocation {
    fn from(l: &ConcreteResourceLocation) -> Self {
        match l {
            ConcreteResourceLocation::InMemory => AbstractResourceLocation::InMemory,
            ConcreteResourceLocation::RelativePath(_) => AbstractResourceLocation::RelativePath,
        }
    }
}

/// Represents a finalized collection of Python resources.
///
/// Instances are produced from a `PythonResourceCollector` and a
/// Python interpreter (to compile bytecode).
#[derive(Clone, Debug, Default)]
pub struct PreparedPythonResources<'a> {
    pub resources: BTreeMap<String, Resource<'a, u8>>,
    pub extra_files: Vec<FileInstall>,
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

/// Type used to collect Python resources so they can be serialized.
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
    pub fn check_policy(&self, location: AbstractResourceLocation) -> Result<()> {
        match self.policy {
            PythonResourcesPolicy::InMemoryOnly => match location {
                AbstractResourceLocation::InMemory => Ok(()),
                AbstractResourceLocation::RelativePath => Err(anyhow!(
                    "in-memory-only policy does not allow relative path resources"
                )),
            },
            PythonResourcesPolicy::FilesystemRelativeOnly(_) => match location {
                AbstractResourceLocation::InMemory => Err(anyhow!(
                    "filesystem-relative-only policy does not allow in-memory resources"
                )),
                AbstractResourceLocation::RelativePath => Ok(()),
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

    /// Obtain an iterator over the resources in this collector.
    pub fn iter_resources(&self) -> impl Iterator<Item = (&String, &PrePackagedResource)> {
        Box::new(self.resources.iter())
    }

    /// Add Python module source with a specific location.
    pub fn add_python_module_source(
        &mut self,
        module: &PythonModuleSource,
        location: &ConcreteResourceLocation,
    ) -> Result<()> {
        self.check_policy(location.into())?;

        let entry = self
            .resources
            .entry(module.name.clone())
            .or_insert_with(|| PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: module.name.clone(),
                ..PrePackagedResource::default()
            });
        entry.is_package = module.is_package;

        match location {
            ConcreteResourceLocation::InMemory => {
                entry.in_memory_source = Some(module.source.clone());
            }
            ConcreteResourceLocation::RelativePath(prefix) => {
                entry.relative_path_module_source =
                    Some((prefix.to_string(), module.source.clone()));
            }
        }

        Ok(())
    }

    /// Add Python module bytecode to the specified location.
    pub fn add_python_module_bytecode(
        &mut self,
        module: &PythonModuleBytecode,
        location: &ConcreteResourceLocation,
    ) -> Result<()> {
        self.check_policy(location.into())?;

        let entry = self
            .resources
            .entry(module.name.clone())
            .or_insert_with(|| PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: module.name.clone(),
                ..PrePackagedResource::default()
            });

        entry.is_package = module.is_package;

        // TODO having to resolve the DataLocation here is a bit unfortunate.
        // We could invent a better type to allow the I/O to remain lazy.
        let bytecode = PythonModuleBytecodeProvider::Provided(DataLocation::Memory(
            module.resolve_bytecode()?,
        ));

        match location {
            ConcreteResourceLocation::InMemory => match module.optimize_level {
                BytecodeOptimizationLevel::Zero => {
                    entry.in_memory_bytecode = Some(bytecode);
                }
                BytecodeOptimizationLevel::One => {
                    entry.in_memory_bytecode_opt1 = Some(bytecode);
                }
                BytecodeOptimizationLevel::Two => {
                    entry.in_memory_bytecode_opt2 = Some(bytecode);
                }
            },
            ConcreteResourceLocation::RelativePath(prefix) => match module.optimize_level {
                BytecodeOptimizationLevel::Zero => {
                    entry.relative_path_bytecode =
                        Some((prefix.to_string(), module.cache_tag.clone(), bytecode));
                }
                BytecodeOptimizationLevel::One => {
                    entry.relative_path_bytecode_opt1 =
                        Some((prefix.to_string(), module.cache_tag.clone(), bytecode));
                }
                BytecodeOptimizationLevel::Two => {
                    entry.relative_path_bytecode_opt2 =
                        Some((prefix.to_string(), module.cache_tag.clone(), bytecode));
                }
            },
        }

        Ok(())
    }

    /// Add Python module bytecode derived from source code to the collection.
    pub fn add_python_module_bytecode_from_source(
        &mut self,
        module: &PythonModuleBytecodeFromSource,
        location: &ConcreteResourceLocation,
    ) -> Result<()> {
        self.check_policy(location.into())?;

        let entry = self
            .resources
            .entry(module.name.clone())
            .or_insert_with(|| PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: module.name.clone(),
                ..PrePackagedResource::default()
            });

        entry.is_package = module.is_package;

        let bytecode = PythonModuleBytecodeProvider::FromSource(module.source.clone());

        match location {
            ConcreteResourceLocation::InMemory => match module.optimize_level {
                BytecodeOptimizationLevel::Zero => {
                    entry.in_memory_bytecode = Some(bytecode);
                }
                BytecodeOptimizationLevel::One => {
                    entry.in_memory_bytecode_opt1 = Some(bytecode);
                }
                BytecodeOptimizationLevel::Two => {
                    entry.in_memory_bytecode_opt2 = Some(bytecode);
                }
            },
            ConcreteResourceLocation::RelativePath(prefix) => match module.optimize_level {
                BytecodeOptimizationLevel::Zero => {
                    entry.relative_path_bytecode =
                        Some((prefix.to_string(), module.cache_tag.clone(), bytecode))
                }
                BytecodeOptimizationLevel::One => {
                    entry.relative_path_bytecode_opt1 =
                        Some((prefix.to_string(), module.cache_tag.clone(), bytecode))
                }
                BytecodeOptimizationLevel::Two => {
                    entry.relative_path_bytecode_opt2 =
                        Some((prefix.to_string(), module.cache_tag.clone(), bytecode))
                }
            },
        }

        Ok(())
    }

    /// Add resource data to a given location.
    ///
    /// Resource data belongs to a Python package and has a name and bytes data.
    pub fn add_python_package_resource(
        &mut self,
        resource: &PythonPackageResource,
        location: &ConcreteResourceLocation,
    ) -> Result<()> {
        self.check_policy(location.into())?;

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

        match location {
            ConcreteResourceLocation::InMemory => {
                if entry.in_memory_resources.is_none() {
                    entry.in_memory_resources = Some(BTreeMap::new());
                }
                entry
                    .in_memory_resources
                    .as_mut()
                    .unwrap()
                    .insert(resource.relative_name.clone(), resource.data.clone());
            }
            ConcreteResourceLocation::RelativePath(prefix) => {
                if entry.relative_path_package_resources.is_none() {
                    entry.relative_path_package_resources = Some(BTreeMap::new());
                }

                entry
                    .relative_path_package_resources
                    .as_mut()
                    .unwrap()
                    .insert(
                        resource.relative_name.clone(),
                        (resource.resolve_path(prefix), resource.data.clone()),
                    );
            }
        }

        Ok(())
    }

    /// Add a package distribution resource to a given location.
    pub fn add_package_distribution_resource(
        &mut self,
        resource: &PythonPackageDistributionResource,
        location: &ConcreteResourceLocation,
    ) -> Result<()> {
        self.check_policy(location.into())?;

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

        match location {
            ConcreteResourceLocation::InMemory => {
                if entry.in_memory_distribution_resources.is_none() {
                    entry.in_memory_distribution_resources = Some(BTreeMap::new());
                }

                entry
                    .in_memory_distribution_resources
                    .as_mut()
                    .unwrap()
                    .insert(resource.name.clone(), resource.data.clone());
            }
            ConcreteResourceLocation::RelativePath(prefix) => {
                if entry.relative_path_distribution_resources.is_none() {
                    entry.relative_path_distribution_resources = Some(BTreeMap::new());
                }

                entry
                    .relative_path_distribution_resources
                    .as_mut()
                    .unwrap()
                    .insert(
                        resource.name.clone(),
                        (resource.resolve_path(prefix), resource.data.clone()),
                    );
            }
        }

        Ok(())
    }

    /// Add a built-in extension module.
    ///
    /// Built-in extension modules are statically linked into the binary and
    /// cannot have their location defined.
    pub fn add_builtin_python_extension_module(
        &mut self,
        module: &PythonExtensionModule,
    ) -> Result<()> {
        self.check_policy(AbstractResourceLocation::InMemory)?;

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
    pub fn add_python_extension_module(
        &mut self,
        module: &PythonExtensionModule,
        location: &ConcreteResourceLocation,
    ) -> Result<()> {
        self.check_policy(location.into())?;

        let data = match &module.shared_library {
            Some(location) => location.resolve()?,
            None => return Err(anyhow!("no shared library data present")),
        };

        let mut depends = Vec::new();

        for link in &module.link_libraries {
            if let Some(shared_library) = &link.dynamic_library {
                let library_location = match location {
                    ConcreteResourceLocation::InMemory => ConcreteResourceLocation::InMemory,
                    ConcreteResourceLocation::RelativePath(prefix) => {
                        // We place the shared library next to the extension module.
                        let path = module
                            .resolve_path(prefix)
                            .parent()
                            .ok_or_else(|| anyhow!("unable to resolve parent directory"))?
                            .to_path_buf();

                        ConcreteResourceLocation::RelativePath(format!("{}", path.display()))
                    }
                };

                self.add_shared_library(&link.name, shared_library, &library_location)?;
                depends.push(link.name.to_string());
            }
        }

        let entry = self
            .resources
            .entry(module.name.to_string())
            .or_insert_with(|| PrePackagedResource {
                flavor: ResourceFlavor::Extension,
                name: module.name.to_string(),
                ..PrePackagedResource::default()
            });

        if module.is_package {
            entry.is_package = true;
        }

        match location {
            ConcreteResourceLocation::InMemory => {
                entry.in_memory_extension_module_shared_library = Some(DataLocation::Memory(data));
            }
            ConcreteResourceLocation::RelativePath(prefix) => {
                entry.relative_path_extension_module_shared_library =
                    Some((module.resolve_path(prefix), DataLocation::Memory(data)));
            }
        }

        entry.shared_library_dependency_names = Some(depends);

        Ok(())
    }

    /// Add a shared library to be loaded from a location.
    pub fn add_shared_library(
        &mut self,
        name: &str,
        data: &DataLocation,
        location: &ConcreteResourceLocation,
    ) -> Result<()> {
        self.check_policy(location.into())?;

        let entry = self
            .resources
            .entry(name.to_string())
            .or_insert_with(|| PrePackagedResource {
                flavor: ResourceFlavor::SharedLibrary,
                name: name.to_string(),
                ..PrePackagedResource::default()
            });

        match location {
            ConcreteResourceLocation::InMemory => {
                entry.in_memory_shared_library = Some(data.clone());
            }
            ConcreteResourceLocation::RelativePath(prefix) => {
                entry.relative_path_shared_library = Some((prefix.to_string(), data.clone()));
            }
        }

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

    /// Converts this collection of resources into a `PreparedPythonResources`.
    pub fn to_prepared_python_resources(
        &self,
        compiler: &mut dyn PythonBytecodeCompiler,
    ) -> Result<PreparedPythonResources> {
        let mut input_resources = self.resources.clone();
        populate_parent_packages(&mut input_resources)?;

        let mut resources = BTreeMap::new();
        let mut extra_files = Vec::new();

        for (name, resource) in &input_resources {
            let (entry, installs) = resource.to_resource(compiler)?;

            for install in installs {
                extra_files.push(install);
            }

            resources.insert(name.clone(), entry);
        }

        Ok(PreparedPythonResources {
            resources,
            extra_files,
        })
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::resource::{LibraryDependency, PythonPackageDistributionResourceFlavor},
        std::convert::TryFrom,
    };

    const DEFAULT_CACHE_TAG: &str = "cpython-37";

    pub struct FakeBytecodeCompiler {
        magic_number: u32,
    }

    impl PythonBytecodeCompiler for FakeBytecodeCompiler {
        fn get_magic_number(&self) -> u32 {
            self.magic_number
        }

        fn compile(
            &mut self,
            source: &[u8],
            _filename: &str,
            optimize: BytecodeOptimizationLevel,
            _output_mode: CompileMode,
        ) -> Result<Vec<u8>> {
            let mut res = Vec::new();

            res.extend(match optimize {
                BytecodeOptimizationLevel::Zero => b"bc0",
                BytecodeOptimizationLevel::One => b"bc1",
                BytecodeOptimizationLevel::Two => b"bc2",
            });

            res.extend(source);

            Ok(res)
        }
    }

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
    fn test_resource_conversion_basic() -> Result<()> {
        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let pre = PrePackagedResource {
            flavor: ResourceFlavor::Module,
            name: "module".to_string(),
            is_package: true,
            is_namespace_package: true,
            ..PrePackagedResource::default()
        };

        let (resource, installs) = pre.to_resource(&mut compiler)?;

        assert_eq!(
            resource,
            Resource {
                flavor: ResourceFlavor::Module,
                name: Cow::Owned("module".to_string()),
                is_package: true,
                is_namespace_package: true,
                ..Resource::default()
            }
        );

        assert!(installs.is_empty());

        Ok(())
    }

    #[test]
    fn test_resource_conversion_in_memory_source() -> Result<()> {
        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let pre = PrePackagedResource {
            flavor: ResourceFlavor::Module,
            name: "module".to_string(),
            in_memory_source: Some(DataLocation::Memory(b"source".to_vec())),
            ..PrePackagedResource::default()
        };

        let (resource, installs) = pre.to_resource(&mut compiler)?;

        assert_eq!(
            resource,
            Resource {
                flavor: ResourceFlavor::Module,
                name: Cow::Owned("module".to_string()),
                in_memory_source: Some(Cow::Owned(b"source".to_vec())),
                ..Resource::default()
            }
        );

        assert!(installs.is_empty());

        Ok(())
    }

    #[test]
    fn test_resource_conversion_in_memory_bytecode_provided() -> Result<()> {
        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let pre = PrePackagedResource {
            flavor: ResourceFlavor::Module,
            name: "module".to_string(),
            in_memory_bytecode: Some(PythonModuleBytecodeProvider::Provided(
                DataLocation::Memory(b"bytecode".to_vec()),
            )),
            ..PrePackagedResource::default()
        };

        let (resource, installs) = pre.to_resource(&mut compiler)?;

        assert_eq!(
            resource,
            Resource {
                flavor: ResourceFlavor::Module,
                name: Cow::Owned("module".to_string()),
                in_memory_bytecode: Some(Cow::Owned(b"bytecode".to_vec())),
                ..Resource::default()
            }
        );
        assert!(installs.is_empty());

        Ok(())
    }

    #[test]
    fn test_resource_conversion_in_memory_bytecode_from_source() -> Result<()> {
        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let pre = PrePackagedResource {
            flavor: ResourceFlavor::Module,
            name: "module".to_string(),
            in_memory_bytecode: Some(PythonModuleBytecodeProvider::FromSource(
                DataLocation::Memory(b"source".to_vec()),
            )),
            ..PrePackagedResource::default()
        };

        let (resource, installs) = pre.to_resource(&mut compiler)?;

        assert_eq!(
            resource,
            Resource {
                flavor: ResourceFlavor::Module,
                name: Cow::Owned("module".to_string()),
                in_memory_bytecode: Some(Cow::Owned(b"bc0source".to_vec())),
                ..Resource::default()
            }
        );
        assert!(installs.is_empty());

        Ok(())
    }

    #[test]
    fn test_resource_conversion_in_memory_bytecode_opt1_provided() -> Result<()> {
        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let pre = PrePackagedResource {
            flavor: ResourceFlavor::Module,
            name: "module".to_string(),
            in_memory_bytecode_opt1: Some(PythonModuleBytecodeProvider::Provided(
                DataLocation::Memory(b"bytecode".to_vec()),
            )),
            ..PrePackagedResource::default()
        };

        let (resource, installs) = pre.to_resource(&mut compiler)?;

        assert_eq!(
            resource,
            Resource {
                flavor: ResourceFlavor::Module,
                name: Cow::Owned("module".to_string()),
                in_memory_bytecode_opt1: Some(Cow::Owned(b"bytecode".to_vec())),
                ..Resource::default()
            }
        );
        assert!(installs.is_empty());

        Ok(())
    }

    #[test]
    fn test_resource_conversion_in_memory_bytecode_opt1_from_source() -> Result<()> {
        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let pre = PrePackagedResource {
            flavor: ResourceFlavor::Module,
            name: "module".to_string(),
            in_memory_bytecode_opt1: Some(PythonModuleBytecodeProvider::FromSource(
                DataLocation::Memory(b"source".to_vec()),
            )),
            ..PrePackagedResource::default()
        };

        let (resource, installs) = pre.to_resource(&mut compiler)?;

        assert_eq!(
            resource,
            Resource {
                flavor: ResourceFlavor::Module,
                name: Cow::Owned("module".to_string()),
                in_memory_bytecode_opt1: Some(Cow::Owned(b"bc1source".to_vec())),
                ..Resource::default()
            }
        );
        assert!(installs.is_empty());

        Ok(())
    }

    #[test]
    fn test_resource_conversion_in_memory_bytecode_opt2_provided() -> Result<()> {
        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let pre = PrePackagedResource {
            flavor: ResourceFlavor::Module,
            name: "module".to_string(),
            in_memory_bytecode_opt2: Some(PythonModuleBytecodeProvider::Provided(
                DataLocation::Memory(b"bytecode".to_vec()),
            )),
            ..PrePackagedResource::default()
        };

        let (resource, installs) = pre.to_resource(&mut compiler)?;

        assert_eq!(
            resource,
            Resource {
                flavor: ResourceFlavor::Module,
                name: Cow::Owned("module".to_string()),
                in_memory_bytecode_opt2: Some(Cow::Owned(b"bytecode".to_vec())),
                ..Resource::default()
            }
        );
        assert!(installs.is_empty());

        Ok(())
    }

    #[test]
    fn test_resource_conversion_in_memory_bytecode_opt2_from_source() -> Result<()> {
        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let pre = PrePackagedResource {
            flavor: ResourceFlavor::Module,
            name: "module".to_string(),
            in_memory_bytecode_opt2: Some(PythonModuleBytecodeProvider::FromSource(
                DataLocation::Memory(b"source".to_vec()),
            )),
            ..PrePackagedResource::default()
        };

        let (resource, installs) = pre.to_resource(&mut compiler)?;

        assert_eq!(
            resource,
            Resource {
                flavor: ResourceFlavor::Module,
                name: Cow::Owned("module".to_string()),
                in_memory_bytecode_opt2: Some(Cow::Owned(b"bc2source".to_vec())),
                ..Resource::default()
            }
        );
        assert!(installs.is_empty());

        Ok(())
    }

    #[test]
    fn test_resource_conversion_in_memory_extension_module_shared_library() -> Result<()> {
        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let pre = PrePackagedResource {
            flavor: ResourceFlavor::Module,
            name: "module".to_string(),
            in_memory_extension_module_shared_library: Some(DataLocation::Memory(
                b"library".to_vec(),
            )),
            ..PrePackagedResource::default()
        };

        let (resource, installs) = pre.to_resource(&mut compiler)?;

        assert_eq!(
            resource,
            Resource {
                flavor: ResourceFlavor::Module,
                name: Cow::Owned("module".to_string()),
                in_memory_extension_module_shared_library: Some(Cow::Owned(b"library".to_vec())),
                ..Resource::default()
            }
        );

        assert!(installs.is_empty());

        Ok(())
    }

    #[test]
    fn test_resource_conversion_in_memory_package_resources() -> Result<()> {
        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let mut resources = BTreeMap::new();
        resources.insert("foo".to_string(), DataLocation::Memory(b"value".to_vec()));

        let pre = PrePackagedResource {
            flavor: ResourceFlavor::Module,
            name: "module".to_string(),
            in_memory_resources: Some(resources),
            ..PrePackagedResource::default()
        };

        let (resource, installs) = pre.to_resource(&mut compiler)?;

        let mut resources = HashMap::new();
        resources.insert(Cow::Owned("foo".to_string()), Cow::Owned(b"value".to_vec()));

        assert_eq!(
            resource,
            Resource {
                flavor: ResourceFlavor::Module,
                name: Cow::Owned("module".to_string()),
                in_memory_package_resources: Some(resources),
                ..Resource::default()
            }
        );

        assert!(installs.is_empty());

        Ok(())
    }

    #[test]
    fn test_resource_conversion_in_memory_distribution_resources() -> Result<()> {
        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let mut resources = BTreeMap::new();
        resources.insert("foo".to_string(), DataLocation::Memory(b"value".to_vec()));

        let pre = PrePackagedResource {
            flavor: ResourceFlavor::Module,
            name: "module".to_string(),
            in_memory_distribution_resources: Some(resources),
            ..PrePackagedResource::default()
        };

        let (resource, installs) = pre.to_resource(&mut compiler)?;

        let mut resources = HashMap::new();
        resources.insert(Cow::Owned("foo".to_string()), Cow::Owned(b"value".to_vec()));

        assert_eq!(
            resource,
            Resource {
                flavor: ResourceFlavor::Module,
                name: Cow::Owned("module".to_string()),
                in_memory_distribution_resources: Some(resources),
                ..Resource::default()
            }
        );

        assert!(installs.is_empty());

        Ok(())
    }

    #[test]
    fn test_resource_conversion_in_memory_shared_library() -> Result<()> {
        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let pre = PrePackagedResource {
            flavor: ResourceFlavor::Module,
            name: "module".to_string(),
            in_memory_shared_library: Some(DataLocation::Memory(b"library".to_vec())),
            shared_library_dependency_names: Some(vec!["foo".to_string(), "bar".to_string()]),
            ..PrePackagedResource::default()
        };

        let (resource, installs) = pre.to_resource(&mut compiler)?;

        assert_eq!(
            resource,
            Resource {
                flavor: ResourceFlavor::Module,
                name: Cow::Owned("module".to_string()),
                in_memory_shared_library: Some(Cow::Owned(b"library".to_vec())),
                shared_library_dependency_names: Some(vec![
                    Cow::Owned("foo".to_string()),
                    Cow::Owned("bar".to_string())
                ]),
                ..Resource::default()
            }
        );

        assert!(installs.is_empty());

        Ok(())
    }

    #[test]
    fn test_resource_conversion_relative_path_module_source() -> Result<()> {
        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let pre = PrePackagedResource {
            flavor: ResourceFlavor::Module,
            name: "module".to_string(),
            relative_path_module_source: Some((
                "prefix".to_string(),
                DataLocation::Memory(b"source".to_vec()),
            )),
            ..PrePackagedResource::default()
        };

        let (resource, installs) = pre.to_resource(&mut compiler)?;

        assert_eq!(
            resource,
            Resource {
                flavor: ResourceFlavor::Module,
                name: Cow::Owned("module".to_string()),
                relative_path_module_source: Some(Cow::Owned(PathBuf::from("prefix/module.py"))),
                ..Resource::default()
            }
        );

        assert_eq!(
            installs,
            vec![(
                PathBuf::from("prefix/module.py"),
                DataLocation::Memory(b"source".to_vec()),
                false
            )]
        );

        Ok(())
    }

    #[test]
    fn test_resource_conversion_relative_path_module_bytecode_provided() -> Result<()> {
        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let pre = PrePackagedResource {
            flavor: ResourceFlavor::Module,
            name: "foo.bar".to_string(),
            relative_path_bytecode: Some((
                "prefix".to_string(),
                "tag".to_string(),
                PythonModuleBytecodeProvider::Provided(DataLocation::Memory(b"bytecode".to_vec())),
            )),
            ..PrePackagedResource::default()
        };

        let (resource, installs) = pre.to_resource(&mut compiler)?;

        assert_eq!(
            resource,
            Resource {
                flavor: ResourceFlavor::Module,
                name: Cow::Owned("foo.bar".to_string()),
                relative_path_module_bytecode: Some(Cow::Owned(PathBuf::from(
                    "prefix/foo/__pycache__/bar.tag.pyc"
                ))),
                ..Resource::default()
            }
        );

        assert_eq!(
            installs,
            vec![(
                PathBuf::from("prefix/foo/__pycache__/bar.tag.pyc"),
                DataLocation::Memory(
                    b"\x2a\x00\x00\x00\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00bytecode"
                        .to_vec()
                ),
                false
            )]
        );

        Ok(())
    }

    #[test]
    fn test_resource_conversion_relative_path_module_bytecode_from_source() -> Result<()> {
        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let pre = PrePackagedResource {
            flavor: ResourceFlavor::Module,
            name: "foo.bar".to_string(),
            relative_path_bytecode: Some((
                "prefix".to_string(),
                "tag".to_string(),
                PythonModuleBytecodeProvider::FromSource(DataLocation::Memory(b"source".to_vec())),
            )),
            ..PrePackagedResource::default()
        };

        let (resource, installs) = pre.to_resource(&mut compiler)?;

        assert_eq!(
            resource,
            Resource {
                flavor: ResourceFlavor::Module,
                name: Cow::Owned("foo.bar".to_string()),
                relative_path_module_bytecode: Some(Cow::Owned(PathBuf::from(
                    "prefix/foo/__pycache__/bar.tag.pyc"
                ))),
                ..Resource::default()
            }
        );

        assert_eq!(
            installs,
            vec![(
                PathBuf::from("prefix/foo/__pycache__/bar.tag.pyc"),
                DataLocation::Memory(b"bc0source".to_vec()),
                false
            )]
        );

        Ok(())
    }

    #[test]
    fn test_resource_conversion_relative_path_module_bytecode_opt1_provided() -> Result<()> {
        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let pre = PrePackagedResource {
            flavor: ResourceFlavor::Module,
            name: "foo.bar".to_string(),
            relative_path_bytecode_opt1: Some((
                "prefix".to_string(),
                "tag".to_string(),
                PythonModuleBytecodeProvider::Provided(DataLocation::Memory(b"bytecode".to_vec())),
            )),
            ..PrePackagedResource::default()
        };

        let (resource, installs) = pre.to_resource(&mut compiler)?;

        assert_eq!(
            resource,
            Resource {
                flavor: ResourceFlavor::Module,
                name: Cow::Owned("foo.bar".to_string()),
                relative_path_module_bytecode_opt1: Some(Cow::Owned(PathBuf::from(
                    "prefix/foo/__pycache__/bar.tag.opt-1.pyc"
                ))),
                ..Resource::default()
            }
        );

        assert_eq!(
            installs,
            vec![(
                PathBuf::from("prefix/foo/__pycache__/bar.tag.opt-1.pyc"),
                DataLocation::Memory(
                    b"\x2a\x00\x00\x00\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00bytecode"
                        .to_vec()
                ),
                false
            )]
        );

        Ok(())
    }

    #[test]
    fn test_resource_conversion_relative_path_module_bytecode_opt1_from_source() -> Result<()> {
        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let pre = PrePackagedResource {
            flavor: ResourceFlavor::Module,
            name: "foo.bar".to_string(),
            relative_path_bytecode_opt1: Some((
                "prefix".to_string(),
                "tag".to_string(),
                PythonModuleBytecodeProvider::FromSource(DataLocation::Memory(b"source".to_vec())),
            )),
            ..PrePackagedResource::default()
        };

        let (resource, installs) = pre.to_resource(&mut compiler)?;

        assert_eq!(
            resource,
            Resource {
                flavor: ResourceFlavor::Module,
                name: Cow::Owned("foo.bar".to_string()),
                relative_path_module_bytecode_opt1: Some(Cow::Owned(PathBuf::from(
                    "prefix/foo/__pycache__/bar.tag.opt-1.pyc"
                ))),
                ..Resource::default()
            }
        );

        assert_eq!(
            installs,
            vec![(
                PathBuf::from("prefix/foo/__pycache__/bar.tag.opt-1.pyc"),
                DataLocation::Memory(b"bc1source".to_vec()),
                false
            )]
        );

        Ok(())
    }

    #[test]
    fn test_resource_conversion_relative_path_module_bytecode_opt2_provided() -> Result<()> {
        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let pre = PrePackagedResource {
            flavor: ResourceFlavor::Module,
            name: "foo.bar".to_string(),
            relative_path_bytecode_opt2: Some((
                "prefix".to_string(),
                "tag".to_string(),
                PythonModuleBytecodeProvider::Provided(DataLocation::Memory(b"bytecode".to_vec())),
            )),
            ..PrePackagedResource::default()
        };

        let (resource, installs) = pre.to_resource(&mut compiler)?;

        assert_eq!(
            resource,
            Resource {
                flavor: ResourceFlavor::Module,
                name: Cow::Owned("foo.bar".to_string()),
                relative_path_module_bytecode_opt2: Some(Cow::Owned(PathBuf::from(
                    "prefix/foo/__pycache__/bar.tag.opt-2.pyc"
                ))),
                ..Resource::default()
            }
        );

        assert_eq!(
            installs,
            vec![(
                PathBuf::from("prefix/foo/__pycache__/bar.tag.opt-2.pyc"),
                DataLocation::Memory(
                    b"\x2a\x00\x00\x00\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00bytecode"
                        .to_vec()
                ),
                false
            )]
        );

        Ok(())
    }

    #[test]
    fn test_resource_conversion_relative_path_module_bytecode_opt2_from_source() -> Result<()> {
        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let pre = PrePackagedResource {
            flavor: ResourceFlavor::Module,
            name: "foo.bar".to_string(),
            relative_path_bytecode_opt2: Some((
                "prefix".to_string(),
                "tag".to_string(),
                PythonModuleBytecodeProvider::FromSource(DataLocation::Memory(b"source".to_vec())),
            )),
            ..PrePackagedResource::default()
        };

        let (resource, installs) = pre.to_resource(&mut compiler)?;

        assert_eq!(
            resource,
            Resource {
                flavor: ResourceFlavor::Module,
                name: Cow::Owned("foo.bar".to_string()),
                relative_path_module_bytecode_opt2: Some(Cow::Owned(PathBuf::from(
                    "prefix/foo/__pycache__/bar.tag.opt-2.pyc"
                ))),
                ..Resource::default()
            }
        );

        assert_eq!(
            installs,
            vec![(
                PathBuf::from("prefix/foo/__pycache__/bar.tag.opt-2.pyc"),
                DataLocation::Memory(b"bc2source".to_vec()),
                false
            )]
        );

        Ok(())
    }

    #[test]
    fn test_resource_conversion_relative_path_extension_module_shared_library() -> Result<()> {
        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let pre = PrePackagedResource {
            flavor: ResourceFlavor::Module,
            name: "module".to_string(),
            relative_path_extension_module_shared_library: Some((
                PathBuf::from("prefix/ext.so"),
                DataLocation::Memory(b"data".to_vec()),
            )),
            ..PrePackagedResource::default()
        };

        let (resource, installs) = pre.to_resource(&mut compiler)?;

        assert_eq!(
            resource,
            Resource {
                flavor: ResourceFlavor::Module,
                name: Cow::Owned("module".to_string()),
                relative_path_extension_module_shared_library: Some(Cow::Owned(PathBuf::from(
                    "prefix/ext.so"
                ))),
                ..Resource::default()
            }
        );

        assert_eq!(
            installs,
            vec![(
                PathBuf::from("prefix/ext.so"),
                DataLocation::Memory(b"data".to_vec()),
                true
            )]
        );

        Ok(())
    }

    #[test]
    fn test_resource_conversion_relative_path_package_resources() -> Result<()> {
        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let mut resources = BTreeMap::new();
        resources.insert(
            "foo.txt".to_string(),
            (
                PathBuf::from("module/foo.txt"),
                DataLocation::Memory(b"data".to_vec()),
            ),
        );
        resources.insert(
            "bar.txt".to_string(),
            (
                PathBuf::from("module/bar.txt"),
                DataLocation::Memory(b"bar".to_vec()),
            ),
        );

        let pre = PrePackagedResource {
            flavor: ResourceFlavor::Module,
            name: "module".to_string(),
            is_package: true,
            relative_path_package_resources: Some(resources),
            ..PrePackagedResource::default()
        };

        let (resource, installs) = pre.to_resource(&mut compiler)?;

        let mut resources = HashMap::new();
        resources.insert(
            Cow::Owned("foo.txt".to_string()),
            Cow::Owned(PathBuf::from("module/foo.txt")),
        );
        resources.insert(
            Cow::Owned("bar.txt".to_string()),
            Cow::Owned(PathBuf::from("module/bar.txt")),
        );

        assert_eq!(
            resource,
            Resource {
                flavor: ResourceFlavor::Module,
                name: Cow::Owned("module".to_string()),
                is_package: true,
                relative_path_package_resources: Some(resources),
                ..Resource::default()
            }
        );

        assert_eq!(
            installs,
            vec![
                (
                    PathBuf::from("module/bar.txt"),
                    DataLocation::Memory(b"bar".to_vec()),
                    false
                ),
                (
                    PathBuf::from("module/foo.txt"),
                    DataLocation::Memory(b"data".to_vec()),
                    false
                ),
            ]
        );

        Ok(())
    }

    #[test]
    fn test_resource_conversion_relative_path_distribution_resources() -> Result<()> {
        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let mut resources = BTreeMap::new();
        resources.insert(
            "foo.txt".to_string(),
            (
                PathBuf::from("foo.txt"),
                DataLocation::Memory(b"data".to_vec()),
            ),
        );

        let pre = PrePackagedResource {
            flavor: ResourceFlavor::Module,
            name: "module".to_string(),
            relative_path_distribution_resources: Some(resources),
            ..PrePackagedResource::default()
        };

        let (resource, installs) = pre.to_resource(&mut compiler)?;

        let mut resources = HashMap::new();
        resources.insert(
            Cow::Owned("foo.txt".to_string()),
            Cow::Owned(PathBuf::from("foo.txt")),
        );

        assert_eq!(
            resource,
            Resource {
                flavor: ResourceFlavor::Module,
                name: Cow::Owned("module".to_string()),
                relative_path_distribution_resources: Some(resources),
                ..Resource::default()
            }
        );

        assert_eq!(
            installs,
            vec![(
                PathBuf::from("foo.txt"),
                DataLocation::Memory(b"data".to_vec()),
                false
            )]
        );

        Ok(())
    }

    #[test]
    fn test_resource_conversion_relative_path_shared_library() -> Result<()> {
        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let pre = PrePackagedResource {
            flavor: ResourceFlavor::SharedLibrary,
            name: "libfoo".to_string(),
            relative_path_shared_library: Some((
                "prefix".to_string(),
                DataLocation::Memory(b"data".to_vec()),
            )),
            ..PrePackagedResource::default()
        };

        let (resource, installs) = pre.to_resource(&mut compiler)?;

        assert_eq!(
            resource,
            Resource {
                flavor: ResourceFlavor::SharedLibrary,
                name: Cow::Owned("libfoo".to_string()),
                ..Resource::default()
            }
        );

        assert_eq!(
            installs,
            vec![(
                PathBuf::from("prefix/libfoo"),
                DataLocation::Memory(b"data".to_vec()),
                true
            )]
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
        r.add_python_module_source(
            &PythonModuleSource {
                name: "foo".to_string(),
                source: DataLocation::Memory(vec![42]),
                is_package: false,
                cache_tag: DEFAULT_CACHE_TAG.to_string(),
                is_stdlib: false,
                is_test: false,
            },
            &ConcreteResourceLocation::InMemory,
        )?;

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

        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let resources = r.to_prepared_python_resources(&mut compiler)?;

        assert_eq!(resources.resources.len(), 1);
        assert_eq!(
            resources.resources.get("foo"),
            Some(&Resource {
                flavor: ResourceFlavor::Module,
                name: Cow::Owned("foo".to_string()),
                in_memory_source: Some(Cow::Owned(vec![42])),
                ..Resource::default()
            })
        );
        assert!(resources.extra_files.is_empty());

        Ok(())
    }

    #[test]
    fn test_add_in_memory_source_module_parents() -> Result<()> {
        let mut r =
            PythonResourceCollector::new(&PythonResourcesPolicy::InMemoryOnly, DEFAULT_CACHE_TAG);
        r.add_python_module_source(
            &PythonModuleSource {
                name: "root.parent.child".to_string(),
                source: DataLocation::Memory(vec![42]),
                is_package: true,
                cache_tag: DEFAULT_CACHE_TAG.to_string(),
                is_stdlib: false,
                is_test: false,
            },
            &ConcreteResourceLocation::InMemory,
        )?;

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

        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let resources = r.to_prepared_python_resources(&mut compiler)?;

        assert_eq!(resources.resources.len(), 3);
        assert_eq!(
            resources.resources.get("root"),
            Some(&Resource {
                flavor: ResourceFlavor::Module,
                name: Cow::Owned("root".to_string()),
                is_package: true,
                in_memory_source: Some(Cow::Owned(vec![])),
                ..Resource::default()
            })
        );
        assert_eq!(
            resources.resources.get("root.parent"),
            Some(&Resource {
                flavor: ResourceFlavor::Module,
                name: Cow::Owned("root.parent".to_string()),
                is_package: true,
                in_memory_source: Some(Cow::Owned(vec![])),
                ..Resource::default()
            })
        );
        assert_eq!(
            resources.resources.get("root.parent.child"),
            Some(&Resource {
                flavor: ResourceFlavor::Module,
                name: Cow::Owned("root.parent.child".to_string()),
                is_package: true,
                in_memory_source: Some(Cow::Owned(vec![42])),
                ..Resource::default()
            })
        );

        assert!(resources.extra_files.is_empty());

        Ok(())
    }

    #[test]
    fn test_add_relative_path_source_module() -> Result<()> {
        let mut r = PythonResourceCollector::new(
            &PythonResourcesPolicy::FilesystemRelativeOnly("".to_string()),
            DEFAULT_CACHE_TAG,
        );
        r.add_python_module_source(
            &PythonModuleSource {
                name: "foo.bar".to_string(),
                source: DataLocation::Memory(vec![42]),
                is_package: false,
                cache_tag: DEFAULT_CACHE_TAG.to_string(),
                is_stdlib: false,
                is_test: false,
            },
            &ConcreteResourceLocation::RelativePath("prefix".to_string()),
        )?;

        assert_eq!(r.resources.len(), 1);
        assert_eq!(
            r.resources.get("foo.bar"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: "foo.bar".to_string(),
                is_package: false,
                relative_path_module_source: Some((
                    "prefix".to_string(),
                    DataLocation::Memory(vec![42])
                )),
                ..PrePackagedResource::default()
            })
        );

        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let resources = r.to_prepared_python_resources(&mut compiler)?;

        assert_eq!(resources.resources.len(), 2);
        assert_eq!(
            resources.resources.get("foo"),
            Some(&Resource {
                flavor: ResourceFlavor::Module,
                name: Cow::Owned("foo".to_string()),
                is_package: true,
                relative_path_module_source: Some(Cow::Owned(PathBuf::from(
                    "prefix/foo/__init__.py"
                ))),
                ..Resource::default()
            })
        );
        assert_eq!(
            resources.resources.get("foo.bar"),
            Some(&Resource {
                flavor: ResourceFlavor::Module,
                name: Cow::Owned("foo.bar".to_string()),
                relative_path_module_source: Some(Cow::Owned(PathBuf::from("prefix/foo/bar.py"))),
                ..Resource::default()
            })
        );
        assert_eq!(
            resources.extra_files,
            vec![
                (
                    PathBuf::from("prefix/foo/__init__.py"),
                    DataLocation::Memory(vec![]),
                    false
                ),
                (
                    PathBuf::from("prefix/foo/bar.py"),
                    DataLocation::Memory(vec![42]),
                    false
                )
            ]
        );

        Ok(())
    }

    #[test]
    fn test_add_in_memory_bytecode_module() -> Result<()> {
        let mut r =
            PythonResourceCollector::new(&PythonResourcesPolicy::InMemoryOnly, DEFAULT_CACHE_TAG);
        r.add_python_module_bytecode(
            &PythonModuleBytecode::new(
                "foo",
                BytecodeOptimizationLevel::Zero,
                false,
                DEFAULT_CACHE_TAG,
                &[42],
            ),
            &ConcreteResourceLocation::InMemory,
        )?;

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

        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let resources = r.to_prepared_python_resources(&mut compiler)?;

        assert_eq!(resources.resources.len(), 1);
        assert_eq!(
            resources.resources.get("foo"),
            Some(&Resource {
                flavor: ResourceFlavor::Module,
                name: Cow::Owned("foo".to_string()),
                in_memory_bytecode: Some(Cow::Owned(vec![42])),
                ..Resource::default()
            })
        );
        assert!(resources.extra_files.is_empty());

        Ok(())
    }

    #[test]
    fn test_add_in_memory_bytecode_module_from_source() -> Result<()> {
        let mut r =
            PythonResourceCollector::new(&PythonResourcesPolicy::InMemoryOnly, DEFAULT_CACHE_TAG);
        r.add_python_module_bytecode_from_source(
            &PythonModuleBytecodeFromSource {
                name: "foo".to_string(),
                source: DataLocation::Memory(vec![42]),
                optimize_level: BytecodeOptimizationLevel::Zero,
                is_package: false,
                cache_tag: DEFAULT_CACHE_TAG.to_string(),
                is_stdlib: false,
                is_test: false,
            },
            &ConcreteResourceLocation::InMemory,
        )?;

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

        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let resources = r.to_prepared_python_resources(&mut compiler)?;

        assert_eq!(resources.resources.len(), 1);
        assert_eq!(
            resources.resources.get("foo"),
            Some(&Resource {
                flavor: ResourceFlavor::Module,
                name: Cow::Owned("foo".to_string()),
                in_memory_bytecode: Some(Cow::Owned(b"bc0\x2a".to_vec())),
                ..Resource::default()
            })
        );
        assert!(resources.extra_files.is_empty());

        Ok(())
    }

    #[test]
    fn test_add_in_memory_bytecode_module_parents() -> Result<()> {
        let mut r =
            PythonResourceCollector::new(&PythonResourcesPolicy::InMemoryOnly, DEFAULT_CACHE_TAG);
        r.add_python_module_bytecode_from_source(
            &PythonModuleBytecodeFromSource {
                name: "root.parent.child".to_string(),
                source: DataLocation::Memory(vec![42]),
                optimize_level: BytecodeOptimizationLevel::One,
                is_package: true,
                cache_tag: DEFAULT_CACHE_TAG.to_string(),
                is_stdlib: false,
                is_test: false,
            },
            &ConcreteResourceLocation::InMemory,
        )?;

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

        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let resources = r.to_prepared_python_resources(&mut compiler)?;

        assert_eq!(resources.resources.len(), 3);
        assert_eq!(
            resources.resources.get("root"),
            Some(&Resource {
                flavor: ResourceFlavor::Module,
                name: Cow::Owned("root".to_string()),
                is_package: true,
                in_memory_bytecode_opt1: Some(Cow::Owned(b"bc1".to_vec())),
                ..Resource::default()
            })
        );
        assert_eq!(
            resources.resources.get("root.parent"),
            Some(&Resource {
                flavor: ResourceFlavor::Module,
                name: Cow::Owned("root.parent".to_string()),
                is_package: true,
                in_memory_bytecode_opt1: Some(Cow::Owned(b"bc1".to_vec())),
                ..Resource::default()
            })
        );
        assert_eq!(
            resources.resources.get("root.parent.child"),
            Some(&Resource {
                flavor: ResourceFlavor::Module,
                name: Cow::Owned("root.parent.child".to_string()),
                is_package: true,
                in_memory_bytecode_opt1: Some(Cow::Owned(b"bc1\x2a".to_vec())),
                ..Resource::default()
            })
        );
        assert!(resources.extra_files.is_empty());

        Ok(())
    }

    #[test]
    fn test_add_in_memory_package_resource() -> Result<()> {
        let mut r =
            PythonResourceCollector::new(&PythonResourcesPolicy::InMemoryOnly, DEFAULT_CACHE_TAG);
        r.add_python_package_resource(
            &PythonPackageResource {
                leaf_package: "foo".to_string(),
                relative_name: "resource.txt".to_string(),
                data: DataLocation::Memory(vec![42]),
                is_stdlib: false,
                is_test: false,
            },
            &ConcreteResourceLocation::InMemory,
        )?;

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

        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let resources = r.to_prepared_python_resources(&mut compiler)?;

        assert_eq!(resources.resources.len(), 1);
        assert_eq!(
            resources.resources.get("foo"),
            Some(&Resource {
                flavor: ResourceFlavor::Module,
                name: Cow::Owned("foo".to_string()),
                is_package: true,
                in_memory_package_resources: Some(HashMap::from_iter(
                    [(Cow::Owned("resource.txt".to_string()), Cow::Owned(vec![42]))]
                        .iter()
                        .cloned()
                )),
                ..Resource::default()
            })
        );
        assert!(resources.extra_files.is_empty());

        Ok(())
    }

    #[test]
    fn test_add_relative_path_package_resource() -> Result<()> {
        let mut r = PythonResourceCollector::new(
            &PythonResourcesPolicy::FilesystemRelativeOnly("".to_string()),
            DEFAULT_CACHE_TAG,
        );
        r.add_python_package_resource(
            &PythonPackageResource {
                leaf_package: "foo".to_string(),
                relative_name: "resource.txt".to_string(),
                data: DataLocation::Memory(vec![42]),
                is_stdlib: false,
                is_test: false,
            },
            &ConcreteResourceLocation::RelativePath("prefix".to_string()),
        )?;

        assert_eq!(r.resources.len(), 1);
        assert_eq!(
            r.resources.get("foo"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: "foo".to_string(),
                is_package: true,
                relative_path_package_resources: Some(BTreeMap::from_iter(
                    [(
                        "resource.txt".to_string(),
                        (
                            PathBuf::from("prefix/foo/resource.txt"),
                            DataLocation::Memory(vec![42])
                        )
                    )]
                    .iter()
                    .cloned()
                )),
                ..PrePackagedResource::default()
            })
        );

        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let resources = r.to_prepared_python_resources(&mut compiler)?;

        assert_eq!(resources.resources.len(), 1);
        assert_eq!(
            resources.resources.get("foo"),
            Some(&Resource {
                flavor: ResourceFlavor::Module,
                name: Cow::Owned("foo".to_string()),
                is_package: true,
                relative_path_package_resources: Some(HashMap::from_iter(
                    [(
                        Cow::Owned("resource.txt".to_string()),
                        Cow::Owned(PathBuf::from("prefix/foo/resource.txt")),
                    )]
                    .iter()
                    .cloned()
                )),
                ..Resource::default()
            })
        );
        assert_eq!(
            resources.extra_files,
            vec![(
                PathBuf::from("prefix/foo/resource.txt"),
                DataLocation::Memory(vec![42]),
                false
            ),]
        );

        Ok(())
    }

    #[test]
    fn test_add_in_memory_package_distribution_resource() -> Result<()> {
        let mut r =
            PythonResourceCollector::new(&PythonResourcesPolicy::InMemoryOnly, DEFAULT_CACHE_TAG);
        r.add_package_distribution_resource(
            &PythonPackageDistributionResource {
                location: PythonPackageDistributionResourceFlavor::DistInfo,
                package: "mypackage".to_string(),
                version: "1.0".to_string(),
                name: "resource.txt".to_string(),
                data: DataLocation::Memory(vec![42]),
            },
            &ConcreteResourceLocation::InMemory,
        )?;

        assert_eq!(r.resources.len(), 1);
        assert_eq!(
            r.resources.get("mypackage"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: "mypackage".to_string(),
                is_package: true,
                in_memory_distribution_resources: Some(BTreeMap::from_iter(
                    [("resource.txt".to_string(), DataLocation::Memory(vec![42]))]
                        .iter()
                        .cloned()
                )),
                ..PrePackagedResource::default()
            })
        );

        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let resources = r.to_prepared_python_resources(&mut compiler)?;

        assert_eq!(resources.resources.len(), 1);
        assert_eq!(
            resources.resources.get("mypackage"),
            Some(&Resource {
                flavor: ResourceFlavor::Module,
                name: Cow::Owned("mypackage".to_string()),
                is_package: true,
                in_memory_distribution_resources: Some(HashMap::from_iter(
                    [(Cow::Owned("resource.txt".to_string()), Cow::Owned(vec![42]))]
                        .iter()
                        .cloned()
                )),
                ..Resource::default()
            })
        );
        assert!(resources.extra_files.is_empty());

        Ok(())
    }

    #[test]
    fn test_add_relative_path_package_distribution_resource() -> Result<()> {
        let mut r = PythonResourceCollector::new(
            &PythonResourcesPolicy::FilesystemRelativeOnly("".to_string()),
            DEFAULT_CACHE_TAG,
        );
        r.add_package_distribution_resource(
            &PythonPackageDistributionResource {
                location: PythonPackageDistributionResourceFlavor::DistInfo,
                package: "mypackage".to_string(),
                version: "1.0".to_string(),
                name: "resource.txt".to_string(),
                data: DataLocation::Memory(vec![42]),
            },
            &ConcreteResourceLocation::RelativePath("prefix".to_string()),
        )?;

        assert_eq!(r.resources.len(), 1);
        assert_eq!(
            r.resources.get("mypackage"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::Module,
                name: "mypackage".to_string(),
                is_package: true,
                relative_path_distribution_resources: Some(BTreeMap::from_iter(
                    [(
                        "resource.txt".to_string(),
                        (
                            PathBuf::from("prefix/mypackage-1.0.dist-info/resource.txt"),
                            DataLocation::Memory(vec![42])
                        )
                    )]
                    .iter()
                    .cloned()
                )),
                ..PrePackagedResource::default()
            })
        );

        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let resources = r.to_prepared_python_resources(&mut compiler)?;

        assert_eq!(resources.resources.len(), 1);
        assert_eq!(
            resources.resources.get("mypackage"),
            Some(&Resource {
                flavor: ResourceFlavor::Module,
                name: Cow::Owned("mypackage".to_string()),
                is_package: true,
                relative_path_distribution_resources: Some(HashMap::from_iter(
                    [(
                        Cow::Owned("resource.txt".to_string()),
                        Cow::Owned(PathBuf::from("prefix/mypackage-1.0.dist-info/resource.txt")),
                    )]
                    .iter()
                    .cloned()
                )),
                ..Resource::default()
            })
        );
        assert_eq!(
            resources.extra_files,
            vec![(
                PathBuf::from("prefix/mypackage-1.0.dist-info/resource.txt"),
                DataLocation::Memory(vec![42]),
                false
            ),]
        );

        Ok(())
    }

    #[test]
    fn test_add_builtin_python_extension_module() -> Result<()> {
        let mut c =
            PythonResourceCollector::new(&PythonResourcesPolicy::InMemoryOnly, DEFAULT_CACHE_TAG);

        let em = PythonExtensionModule {
            name: "_io".to_string(),
            init_fn: Some("PyInit__io".to_string()),
            extension_file_suffix: "".to_string(),
            shared_library: None,
            object_file_data: vec![],
            is_package: false,
            link_libraries: vec![],
            is_stdlib: true,
            builtin_default: true,
            required: true,
            variant: None,
            licenses: None,
            license_texts: None,
            license_public_domain: None,
        };

        c.add_builtin_python_extension_module(&em)?;
        assert_eq!(c.resources.len(), 1);
        assert_eq!(
            c.resources.get("_io"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::BuiltinExtensionModule,
                name: "_io".to_string(),
                ..PrePackagedResource::default()
            })
        );

        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let resources = c.to_prepared_python_resources(&mut compiler)?;

        assert_eq!(resources.resources.len(), 1);
        assert_eq!(
            resources.resources.get("_io"),
            Some(&Resource {
                flavor: ResourceFlavor::BuiltinExtensionModule,
                name: Cow::Owned("_io".to_string()),
                ..Resource::default()
            })
        );
        assert!(resources.extra_files.is_empty());

        Ok(())
    }

    #[test]
    fn test_add_in_memory_python_extension_module_shared_library() -> Result<()> {
        let mut c =
            PythonResourceCollector::new(&PythonResourcesPolicy::InMemoryOnly, DEFAULT_CACHE_TAG);

        let em = PythonExtensionModule {
            name: "myext".to_string(),
            init_fn: Some("PyInit__myext".to_string()),
            extension_file_suffix: ".so".to_string(),
            shared_library: Some(DataLocation::Memory(vec![42])),
            object_file_data: vec![],
            is_package: false,
            link_libraries: vec![LibraryDependency {
                name: "foo".to_string(),
                static_library: None,
                dynamic_library: Some(DataLocation::Memory(vec![40])),
                framework: false,
                system: false,
            }],
            is_stdlib: false,
            builtin_default: false,
            required: false,
            variant: None,
            licenses: None,
            license_texts: None,
            license_public_domain: None,
        };

        c.add_python_extension_module(&em, &ConcreteResourceLocation::InMemory)?;
        assert_eq!(c.resources.len(), 2);
        assert_eq!(
            c.resources.get("myext"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::Extension,
                name: "myext".to_string(),
                in_memory_extension_module_shared_library: Some(DataLocation::Memory(vec![42])),
                shared_library_dependency_names: Some(vec!["foo".to_string()]),
                ..PrePackagedResource::default()
            })
        );
        assert_eq!(
            c.resources.get("foo"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::SharedLibrary,
                name: "foo".to_string(),
                in_memory_shared_library: Some(DataLocation::Memory(vec![40])),
                ..PrePackagedResource::default()
            })
        );

        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let resources = c.to_prepared_python_resources(&mut compiler)?;

        assert_eq!(resources.resources.len(), 2);
        assert_eq!(
            resources.resources.get("myext"),
            Some(&Resource {
                flavor: ResourceFlavor::Extension,
                name: Cow::Owned("myext".to_string()),
                in_memory_extension_module_shared_library: Some(Cow::Owned(vec![42])),
                shared_library_dependency_names: Some(vec![Cow::Owned("foo".to_string())]),
                ..Resource::default()
            })
        );
        assert_eq!(
            resources.resources.get("foo"),
            Some(&Resource {
                flavor: ResourceFlavor::SharedLibrary,
                name: Cow::Owned("foo".to_string()),
                in_memory_shared_library: Some(Cow::Owned(vec![40])),
                ..Resource::default()
            })
        );

        assert!(resources.extra_files.is_empty());

        Ok(())
    }

    #[test]
    fn test_add_relative_path_python_extension_module() -> Result<()> {
        let mut c = PythonResourceCollector::new(
            &PythonResourcesPolicy::FilesystemRelativeOnly("prefix".to_string()),
            DEFAULT_CACHE_TAG,
        );

        let em = PythonExtensionModule {
            name: "foo.bar".to_string(),
            init_fn: None,
            extension_file_suffix: ".so".to_string(),
            shared_library: Some(DataLocation::Memory(vec![42])),
            object_file_data: vec![],
            is_package: false,
            link_libraries: vec![LibraryDependency {
                name: "mylib".to_string(),
                static_library: None,
                dynamic_library: Some(DataLocation::Memory(vec![40])),
                framework: false,
                system: false,
            }],
            is_stdlib: false,
            builtin_default: false,
            required: false,
            variant: None,
            licenses: None,
            license_texts: None,
            license_public_domain: None,
        };

        c.add_python_extension_module(
            &em,
            &ConcreteResourceLocation::RelativePath("prefix".to_string()),
        )?;
        assert_eq!(c.resources.len(), 2);
        assert_eq!(
            c.resources.get("foo.bar"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::Extension,
                name: "foo.bar".to_string(),
                is_package: false,
                relative_path_extension_module_shared_library: Some((
                    PathBuf::from("prefix/foo/bar.so"),
                    DataLocation::Memory(vec![42])
                )),
                shared_library_dependency_names: Some(vec!["mylib".to_string()]),
                ..PrePackagedResource::default()
            })
        );
        assert_eq!(
            c.resources.get("mylib"),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::SharedLibrary,
                name: "mylib".to_string(),
                relative_path_shared_library: Some((
                    "prefix/foo".to_string(),
                    DataLocation::Memory(vec![40])
                )),
                ..PrePackagedResource::default()
            })
        );

        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let resources = c.to_prepared_python_resources(&mut compiler)?;

        assert_eq!(resources.resources.len(), 3);
        assert_eq!(
            resources.resources.get("foo"),
            Some(&Resource {
                flavor: ResourceFlavor::Module,
                name: Cow::Owned("foo".to_string()),
                is_package: true,
                ..Resource::default()
            })
        );
        assert_eq!(
            resources.resources.get("foo.bar"),
            Some(&Resource {
                flavor: ResourceFlavor::Extension,
                name: Cow::Owned("foo.bar".to_string()),
                is_package: false,
                relative_path_extension_module_shared_library: Some(Cow::Owned(PathBuf::from(
                    "prefix/foo/bar.so"
                ))),
                shared_library_dependency_names: Some(vec![Cow::Owned("mylib".to_string())]),
                ..Resource::default()
            })
        );
        assert_eq!(
            resources.resources.get("mylib"),
            Some(&Resource {
                flavor: ResourceFlavor::SharedLibrary,
                name: Cow::Owned("mylib".to_string()),
                ..Resource::default()
            })
        );

        assert_eq!(
            resources.extra_files,
            vec![
                (
                    PathBuf::from("prefix/foo/bar.so"),
                    DataLocation::Memory(vec![42]),
                    true
                ),
                (
                    PathBuf::from("prefix/foo/mylib"),
                    DataLocation::Memory(vec![40]),
                    true
                )
            ]
        );

        Ok(())
    }

    #[test]
    fn test_find_dunder_file() -> Result<()> {
        let mut r =
            PythonResourceCollector::new(&PythonResourcesPolicy::InMemoryOnly, DEFAULT_CACHE_TAG);
        assert_eq!(r.find_dunder_file()?.len(), 0);

        r.add_python_module_source(
            &PythonModuleSource {
                name: "foo.bar".to_string(),
                source: DataLocation::Memory(vec![]),
                is_package: false,
                cache_tag: DEFAULT_CACHE_TAG.to_string(),
                is_stdlib: false,
                is_test: false,
            },
            &ConcreteResourceLocation::InMemory,
        )?;
        assert_eq!(r.find_dunder_file()?.len(), 0);

        r.add_python_module_source(
            &PythonModuleSource {
                name: "baz".to_string(),
                source: DataLocation::Memory(Vec::from("import foo; if __file__ == 'ignored'")),
                is_package: false,
                cache_tag: DEFAULT_CACHE_TAG.to_string(),
                is_stdlib: false,
                is_test: false,
            },
            &ConcreteResourceLocation::InMemory,
        )?;
        assert_eq!(r.find_dunder_file()?.len(), 1);
        assert!(r.find_dunder_file()?.contains("baz"));

        r.add_python_module_bytecode_from_source(
            &PythonModuleBytecodeFromSource {
                name: "bytecode".to_string(),
                source: DataLocation::Memory(Vec::from("import foo; if __file__")),
                optimize_level: BytecodeOptimizationLevel::Zero,
                is_package: false,
                cache_tag: DEFAULT_CACHE_TAG.to_string(),
                is_stdlib: false,
                is_test: false,
            },
            &ConcreteResourceLocation::InMemory,
        )?;
        assert_eq!(r.find_dunder_file()?.len(), 2);
        assert!(r.find_dunder_file()?.contains("bytecode"));

        Ok(())
    }
}
