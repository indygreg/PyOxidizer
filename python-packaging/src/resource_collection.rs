// Copyright 2022 Gregory Szorc.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*! Functionality for collecting Python resources. */

use {
    crate::{
        bytecode::{
            compute_bytecode_header, BytecodeHeaderMode, CompileMode, PythonBytecodeCompiler,
        },
        libpython::LibPythonBuildContext,
        licensing::{LicensedComponent, LicensedComponents},
        location::{AbstractResourceLocation, ConcreteResourceLocation},
        module_util::{packages_from_module_name, resolve_path_for_module},
        python_source::has_dunder_file,
        resource::{
            BytecodeOptimizationLevel, PythonExtensionModule, PythonModuleBytecode,
            PythonModuleBytecodeFromSource, PythonModuleSource, PythonPackageDistributionResource,
            PythonPackageResource, PythonResource, SharedLibrary,
        },
    },
    anyhow::{anyhow, Context, Result},
    python_packed_resources::Resource,
    simple_file_manifest::{File, FileData, FileEntry, FileManifest},
    std::{
        borrow::Cow,
        collections::{BTreeMap, BTreeSet, HashMap},
        path::PathBuf,
    },
};

/// Represents a single file install.
///
/// Tuple is the relative install path, the data to install, and whether the file
/// should be executable.
pub type FileInstall = (PathBuf, FileData, bool);

/// Describes how Python module bytecode will be obtained.
#[derive(Clone, Debug, PartialEq)]
pub enum PythonModuleBytecodeProvider {
    /// Bytecode is already available.
    Provided(FileData),
    /// Bytecode will be computed from source.
    FromSource(FileData),
}

/// Represents a Python resource entry before it is packaged.
///
/// Instances hold the same fields as `Resource` except fields holding
/// content are backed by a `FileData` instead of `Vec<u8>`, since
/// we want data resolution to be lazy. In addition, bytecode can either be
/// provided verbatim or via source.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PrePackagedResource {
    pub name: String,
    pub is_package: bool,
    pub is_namespace_package: bool,
    pub in_memory_source: Option<FileData>,
    pub in_memory_bytecode: Option<PythonModuleBytecodeProvider>,
    pub in_memory_bytecode_opt1: Option<PythonModuleBytecodeProvider>,
    pub in_memory_bytecode_opt2: Option<PythonModuleBytecodeProvider>,
    pub in_memory_extension_module_shared_library: Option<FileData>,
    pub in_memory_resources: Option<BTreeMap<String, FileData>>,
    pub in_memory_distribution_resources: Option<BTreeMap<String, FileData>>,
    pub in_memory_shared_library: Option<FileData>,
    pub shared_library_dependency_names: Option<Vec<String>>,
    // (prefix, source code)
    pub relative_path_module_source: Option<(String, FileData)>,
    // (prefix, bytecode tag, source code)
    pub relative_path_bytecode: Option<(String, String, PythonModuleBytecodeProvider)>,
    pub relative_path_bytecode_opt1: Option<(String, String, PythonModuleBytecodeProvider)>,
    pub relative_path_bytecode_opt2: Option<(String, String, PythonModuleBytecodeProvider)>,
    // (path, data)
    pub relative_path_extension_module_shared_library: Option<(PathBuf, FileData)>,
    pub relative_path_package_resources: Option<BTreeMap<String, (PathBuf, FileData)>>,
    pub relative_path_distribution_resources: Option<BTreeMap<String, (PathBuf, FileData)>>,
    pub relative_path_shared_library: Option<(String, PathBuf, FileData)>,
    pub is_module: bool,
    pub is_builtin_extension_module: bool,
    pub is_frozen_module: bool,
    pub is_extension_module: bool,
    pub is_shared_library: bool,
    pub is_utf8_filename_data: bool,
    pub file_executable: bool,
    pub file_data_embedded: Option<FileData>,
    pub file_data_utf8_relative_path: Option<(PathBuf, FileData)>,
}

impl PrePackagedResource {
    /// Whether this resource represents a Python resource.
    pub fn is_python_resource(&self) -> bool {
        self.is_module
            || self.is_builtin_extension_module
            || self.is_frozen_module
            || self.is_extension_module
    }

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
            name: Cow::Owned(self.name.clone()),
            is_python_package: self.is_package,
            is_python_namespace_package: self.is_namespace_package,
            in_memory_source: if let Some(location) = &self.in_memory_source {
                Some(Cow::Owned(location.resolve_content()?))
            } else {
                None
            },
            in_memory_bytecode: match &self.in_memory_bytecode {
                Some(PythonModuleBytecodeProvider::Provided(location)) => {
                    Some(Cow::Owned(location.resolve_content()?))
                }
                Some(PythonModuleBytecodeProvider::FromSource(location)) => Some(Cow::Owned(
                    compiler
                        .compile(
                            &location.resolve_content()?,
                            &self.name,
                            BytecodeOptimizationLevel::Zero,
                            CompileMode::Bytecode,
                        )
                        .context("compiling in-memory bytecode")?,
                )),
                None => None,
            },
            in_memory_bytecode_opt1: match &self.in_memory_bytecode_opt1 {
                Some(PythonModuleBytecodeProvider::Provided(location)) => {
                    Some(Cow::Owned(location.resolve_content()?))
                }
                Some(PythonModuleBytecodeProvider::FromSource(location)) => Some(Cow::Owned(
                    compiler
                        .compile(
                            &location.resolve_content()?,
                            &self.name,
                            BytecodeOptimizationLevel::One,
                            CompileMode::Bytecode,
                        )
                        .context("compiling in-memory bytecode opt-1")?,
                )),
                None => None,
            },
            in_memory_bytecode_opt2: match &self.in_memory_bytecode_opt2 {
                Some(PythonModuleBytecodeProvider::Provided(location)) => {
                    Some(Cow::Owned(location.resolve_content()?))
                }
                Some(PythonModuleBytecodeProvider::FromSource(location)) => Some(Cow::Owned(
                    compiler
                        .compile(
                            &location.resolve_content()?,
                            &self.name,
                            BytecodeOptimizationLevel::Two,
                            CompileMode::Bytecode,
                        )
                        .context("compiling in-memory bytecode opt2")?,
                )),
                None => None,
            },
            in_memory_extension_module_shared_library: if let Some(location) =
                &self.in_memory_extension_module_shared_library
            {
                Some(Cow::Owned(location.resolve_content()?))
            } else {
                None
            },
            in_memory_package_resources: if let Some(resources) = &self.in_memory_resources {
                let mut res = HashMap::new();
                for (key, location) in resources {
                    res.insert(
                        Cow::Owned(key.clone()),
                        Cow::Owned(location.resolve_content()?),
                    );
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
                    res.insert(
                        Cow::Owned(key.clone()),
                        Cow::Owned(location.resolve_content()?),
                    );
                }
                Some(res)
            } else {
                None
            },
            in_memory_shared_library: if let Some(location) = &self.in_memory_shared_library {
                Some(Cow::Owned(location.resolve_content()?))
            } else {
                None
            },
            shared_library_dependency_names: self
                .shared_library_dependency_names
                .as_ref()
                .map(|x| x.iter().map(|x| Cow::Owned(x.clone())).collect()),
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
                    FileData::Memory(match provider {
                        PythonModuleBytecodeProvider::FromSource(location) => compiler
                            .compile(
                                &location.resolve_content()?,
                                &self.name,
                                BytecodeOptimizationLevel::Zero,
                                CompileMode::PycUncheckedHash,
                            )
                            .context("compiling relative path module bytecode")?,
                        PythonModuleBytecodeProvider::Provided(location) => {
                            let mut data = compute_bytecode_header(
                                compiler.get_magic_number(),
                                BytecodeHeaderMode::UncheckedHash(0),
                            )?;
                            data.extend(location.resolve_content()?);

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
                    FileData::Memory(match provider {
                        PythonModuleBytecodeProvider::FromSource(location) => compiler
                            .compile(
                                &location.resolve_content()?,
                                &self.name,
                                BytecodeOptimizationLevel::One,
                                CompileMode::PycUncheckedHash,
                            )
                            .context("compiling relative path module bytecode opt-1")?,
                        PythonModuleBytecodeProvider::Provided(location) => {
                            let mut data = compute_bytecode_header(
                                compiler.get_magic_number(),
                                BytecodeHeaderMode::UncheckedHash(0),
                            )?;
                            data.extend(location.resolve_content()?);

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
                    FileData::Memory(match provider {
                        PythonModuleBytecodeProvider::FromSource(location) => compiler.compile(
                            &location.resolve_content()?,
                            &self.name,
                            BytecodeOptimizationLevel::Two,
                            CompileMode::PycUncheckedHash,
                        )?,
                        PythonModuleBytecodeProvider::Provided(location) => {
                            let mut data = compute_bytecode_header(
                                compiler.get_magic_number(),
                                BytecodeHeaderMode::UncheckedHash(0),
                            )
                            .context("compiling relative path module bytecode opt-2")?;
                            data.extend(location.resolve_content()?);

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
            is_python_module: self.is_module,
            is_python_builtin_extension_module: self.is_builtin_extension_module,
            is_python_frozen_module: self.is_frozen_module,
            is_python_extension_module: self.is_extension_module,
            is_shared_library: self.is_shared_library,
            is_utf8_filename_data: self.is_utf8_filename_data,
            file_executable: self.file_executable,
            file_data_embedded: if let Some(location) = &self.file_data_embedded {
                Some(Cow::Owned(location.resolve_content()?))
            } else {
                None
            },
            file_data_utf8_relative_path: if let Some((path, location)) =
                &self.file_data_utf8_relative_path
            {
                installs.push((path.clone(), location.clone(), self.file_executable));

                Some(Cow::Owned(path.to_string_lossy().to_string()))
            } else {
                None
            },
        };

        if let Some((prefix, filename, location)) = &self.relative_path_shared_library {
            installs.push((PathBuf::from(prefix).join(filename), location.clone(), true));
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
            if v.is_python_resource() {
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
                    name: package,
                    ..PrePackagedResource::default()
                });

            // Parents must be modules + packages by definition.
            entry.is_module = true;
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
                        FileData::Memory(vec![])
                    },
                ));
            }
            if original.in_memory_bytecode_opt1.is_some() && entry.in_memory_bytecode_opt1.is_none()
            {
                entry.in_memory_bytecode_opt1 = Some(PythonModuleBytecodeProvider::FromSource(
                    if let Some(source) = &entry.in_memory_source {
                        source.clone()
                    } else {
                        FileData::Memory(vec![])
                    },
                ));
            }
            if original.in_memory_bytecode_opt2.is_some() && entry.in_memory_bytecode_opt2.is_none()
            {
                entry.in_memory_bytecode_opt2 = Some(PythonModuleBytecodeProvider::FromSource(
                    if let Some(source) = &entry.in_memory_source {
                        source.clone()
                    } else {
                        FileData::Memory(vec![])
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
                                FileData::Memory(vec![])
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
                                FileData::Memory(vec![])
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
                                FileData::Memory(vec![])
                            },
                        ),
                    ));
                }
            }

            // If the child had path-based source, we need to materialize source as well.
            if let Some((prefix, _)) = &original.relative_path_module_source {
                entry
                    .relative_path_module_source
                    .get_or_insert_with(|| (prefix.clone(), FileData::Memory(vec![])));
            }

            // Ditto for in-memory source.
            if original.in_memory_source.is_some() {
                entry
                    .in_memory_source
                    .get_or_insert(FileData::Memory(vec![]));
            }
        }
    }

    Ok(())
}

/// Defines how a Python resource should be added to a `PythonResourceCollector`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PythonResourceAddCollectionContext {
    /// Whether the resource should be included in `PythonResourceCollection`.
    pub include: bool,

    /// The location the resource should be loaded from.
    pub location: ConcreteResourceLocation,

    /// Optional fallback location from which to load the resource from.
    ///
    /// If adding the resource to `location` fails, and this is defined,
    /// we will fall back to adding the resource to this location.
    pub location_fallback: Option<ConcreteResourceLocation>,

    /// Whether to store Python source code for a `PythonModuleSource`.
    ///
    /// When handling a `PythonModuleSource`, sometimes you want to
    /// write just bytecode or source + bytecode. This flags allows
    /// controlling this behavior.
    pub store_source: bool,

    /// Whether to store Python bytecode for optimization level 0.
    pub optimize_level_zero: bool,

    /// Whether to store Python bytecode for optimization level 1.
    pub optimize_level_one: bool,

    /// Whether to store Python bytecode for optimization level 2.
    pub optimize_level_two: bool,
}

impl PythonResourceAddCollectionContext {
    /// Replace the content of `self` with content of `other`.
    pub fn replace(&mut self, other: &Self) {
        self.include = other.include;
        self.location = other.location.clone();
        self.location_fallback = other.location_fallback.clone();
        self.store_source = other.store_source;
        self.optimize_level_zero = other.optimize_level_zero;
        self.optimize_level_one = other.optimize_level_one;
        self.optimize_level_two = other.optimize_level_two;
    }
}

/// Describes the result of adding a resource to a collector.
#[derive(Clone, Debug)]
pub enum AddResourceAction {
    /// Resource with specified description wasn't added because add_include=false.
    NoInclude(String),
    /// Resource not added because of Python bytecode optimization level mismatch.
    BytecodeOptimizationLevelMismatch(String),
    /// Resource with specified description was added to the specified location.
    Added(String, ConcreteResourceLocation),
    /// Built-in Python extension module.
    AddedBuiltinExtensionModule(String),
}

impl ToString for AddResourceAction {
    fn to_string(&self) -> String {
        match self {
            Self::NoInclude(name) => {
                format!("ignored adding {} because fails inclusion filter", name)
            }
            Self::BytecodeOptimizationLevelMismatch(name) => {
                format!("ignored adding Python module bytecode for {} because of optimization level mismatch", name)
            }
            Self::Added(name, location) => {
                format!("added {} to {}", name, location.to_string())
            }
            Self::AddedBuiltinExtensionModule(name) => {
                format!("added builtin Python extension module {}", name)
            }
        }
    }
}

/// Represents a finalized collection of Python resources.
///
/// Instances are produced from a `PythonResourceCollector` and a
/// `PythonBytecodeCompiler` to produce bytecode.
#[derive(Clone, Debug, Default)]
pub struct CompiledResourcesCollection<'a> {
    /// All indexes resources.
    pub resources: BTreeMap<String, Resource<'a, u8>>,

    /// Extra file installs that must be performed so referenced files are available.
    pub extra_files: Vec<FileInstall>,
}

impl<'a> CompiledResourcesCollection<'a> {
    /// Write resources to packed resources data, version 1.
    pub fn write_packed_resources<W: std::io::Write>(&self, writer: &mut W) -> Result<()> {
        python_packed_resources::write_packed_resources_v3(
            &self
                .resources
                .values()
                .cloned()
                .collect::<Vec<Resource<'a, u8>>>(),
            writer,
            None,
        )
    }

    /// Convert the file installs to a [FileManifest].
    pub fn extra_files_manifest(&self) -> Result<FileManifest> {
        let mut m = FileManifest::default();

        for (path, location, executable) in &self.extra_files {
            m.add_file_entry(
                path,
                FileEntry::new_from_data(location.resolve_content()?, *executable),
            )?;
        }

        Ok(m)
    }
}

/// Type used to collect Python resources so they can be serialized.
///
/// We often want to turn Python resource primitives (module source,
/// bytecode, etc) into a collection of `Resource` so they can be
/// serialized to the *Python packed resources* format. This type
/// exists to facilitate doing this.
///
/// This type is not only responsible for tracking resources but also for
/// enforcing policies on where those resources can be loaded from and
/// what types of resources are allowed. This includes tracking the
/// licensing metadata for indexed resources.
#[derive(Debug, Clone)]
pub struct PythonResourceCollector {
    /// Where resources can be placed.
    allowed_locations: Vec<AbstractResourceLocation>,

    /// Allowed locations for extension modules.
    ///
    /// This is applied in addition to `allowed_locations` and can be
    /// more strict.
    allowed_extension_module_locations: Vec<AbstractResourceLocation>,

    /// Whether builtin extension modules outside the standard library are allowed.
    ///
    /// This is effectively "are we building a custom libpython." If true,
    /// we can take object files / static libraries from adding extension
    /// modules are add the extension module as a built-in. If false, only
    /// builtin extension modules already in libpython can be added as a
    /// built-in.
    allow_new_builtin_extension_modules: bool,

    /// Whether untyped files (`File`) can be added.
    allow_files: bool,

    /// Named resources that have been collected.
    resources: BTreeMap<String, PrePackagedResource>,

    /// Collection of software components which are licensed.
    licensed_components: LicensedComponents,
}

impl PythonResourceCollector {
    /// Construct a new instance of the collector.
    ///
    /// The instance is associated with a resources policy to validate that
    /// added resources conform with rules.
    ///
    /// We also pass a Python bytecode cache tag, which is used to
    /// derive filenames.
    pub fn new(
        allowed_locations: Vec<AbstractResourceLocation>,
        allowed_extension_module_locations: Vec<AbstractResourceLocation>,
        allow_new_builtin_extension_modules: bool,
        allow_files: bool,
    ) -> Self {
        Self {
            allowed_locations,
            allowed_extension_module_locations,
            allow_new_builtin_extension_modules,
            allow_files,
            resources: BTreeMap::new(),
            licensed_components: LicensedComponents::default(),
        }
    }

    /// Obtain locations that resources can be loaded from.
    pub fn allowed_locations(&self) -> &Vec<AbstractResourceLocation> {
        &self.allowed_locations
    }

    /// Obtain a set of all top-level Python module names registered with the collector.
    ///
    /// The returned values correspond to packages or single file modules without
    /// children modules.
    pub fn all_top_level_module_names(&self) -> BTreeSet<String> {
        self.resources
            .values()
            .filter_map(|r| {
                if r.is_python_resource() {
                    let name = if let Some(idx) = r.name.find('.') {
                        &r.name[0..idx]
                    } else {
                        &r.name
                    };

                    Some(name.to_string())
                } else {
                    None
                }
            })
            .collect::<BTreeSet<_>>()
    }

    /// Validate that a resource add in the specified location is allowed.
    pub fn check_policy(&self, location: AbstractResourceLocation) -> Result<()> {
        if self.allowed_locations.contains(&location) {
            Ok(())
        } else {
            Err(anyhow!(
                "resource collector does not allow resources in {}",
                (&location).to_string()
            ))
        }
    }

    /// Apply a filter function on resources in this collection and mutate in place.
    ///
    /// If the filter function returns true, the item will be preserved.
    pub fn filter_resources_mut<F>(&mut self, filter: F) -> Result<()>
    where
        F: Fn(&PrePackagedResource) -> bool,
    {
        self.resources = self
            .resources
            .iter()
            .filter_map(|(k, v)| {
                if filter(v) {
                    Some((k.clone(), v.clone()))
                } else {
                    None
                }
            })
            .collect();

        Ok(())
    }

    /// Obtain an iterator over the resources in this collector.
    pub fn iter_resources(&self) -> impl Iterator<Item = (&String, &PrePackagedResource)> {
        Box::new(self.resources.iter())
    }

    /// Register a licensed software component to this collection.
    pub fn add_licensed_component(&mut self, component: LicensedComponent) -> Result<()> {
        self.licensed_components.add_component(component);

        Ok(())
    }

    /// Obtain a finalized collection of licensed components.
    ///
    /// The collection has entries for components that lack licenses and has additional
    /// normalization performed.
    pub fn normalized_licensed_components(&self) -> LicensedComponents {
        self.licensed_components.normalize_python_modules()
    }

    /// Add Python module source with a specific location.
    pub fn add_python_module_source(
        &mut self,
        module: &PythonModuleSource,
        location: &ConcreteResourceLocation,
    ) -> Result<Vec<AddResourceAction>> {
        self.check_policy(location.into())?;

        let entry = self
            .resources
            .entry(module.name.clone())
            .or_insert_with(|| PrePackagedResource {
                name: module.name.clone(),
                ..PrePackagedResource::default()
            });

        entry.is_module = true;
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

        Ok(vec![AddResourceAction::Added(
            module.description(),
            location.clone(),
        )])
    }

    /// Add Python module source using an add context to influence operation.
    ///
    /// All of the context's properties are respected. This includes doing
    /// nothing if `include` is false, not adding source if `store_source` is
    /// false, and automatically deriving a bytecode request if the
    /// `optimize_level_*` fields are set.
    ///
    /// This method is a glorified proxy to other `add_*` methods: it
    /// simply contains the logic for expanding the context's wishes into
    /// function calls.
    pub fn add_python_module_source_with_context(
        &mut self,
        module: &PythonModuleSource,
        add_context: &PythonResourceAddCollectionContext,
    ) -> Result<Vec<AddResourceAction>> {
        if !add_context.include {
            return Ok(vec![AddResourceAction::NoInclude(module.description())]);
        }

        let mut actions = vec![];

        if add_context.store_source {
            actions.extend(self.add_python_resource_with_locations(
                &module.into(),
                &add_context.location,
                &add_context.location_fallback,
            )?);
        }

        // Derive bytecode as requested.
        if add_context.optimize_level_zero {
            actions.extend(
                self.add_python_resource_with_locations(
                    &module
                        .as_bytecode_module(BytecodeOptimizationLevel::Zero)
                        .into(),
                    &add_context.location,
                    &add_context.location_fallback,
                )?,
            );
        }

        if add_context.optimize_level_one {
            actions.extend(
                self.add_python_resource_with_locations(
                    &module
                        .as_bytecode_module(BytecodeOptimizationLevel::One)
                        .into(),
                    &add_context.location,
                    &add_context.location_fallback,
                )?,
            );
        }

        if add_context.optimize_level_two {
            actions.extend(
                self.add_python_resource_with_locations(
                    &module
                        .as_bytecode_module(BytecodeOptimizationLevel::Two)
                        .into(),
                    &add_context.location,
                    &add_context.location_fallback,
                )?,
            );
        }

        Ok(actions)
    }

    /// Add Python module bytecode to the specified location.
    pub fn add_python_module_bytecode(
        &mut self,
        module: &PythonModuleBytecode,
        location: &ConcreteResourceLocation,
    ) -> Result<Vec<AddResourceAction>> {
        self.check_policy(location.into())?;

        let entry = self
            .resources
            .entry(module.name.clone())
            .or_insert_with(|| PrePackagedResource {
                name: module.name.clone(),
                ..PrePackagedResource::default()
            });

        entry.is_module = true;
        entry.is_package = module.is_package;

        // TODO having to resolve the FileData here is a bit unfortunate.
        // We could invent a better type to allow the I/O to remain lazy.
        let bytecode =
            PythonModuleBytecodeProvider::Provided(FileData::Memory(module.resolve_bytecode()?));

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

        Ok(vec![AddResourceAction::Added(
            module.description(),
            location.clone(),
        )])
    }

    /// Add Python module bytecode using an add context.
    ///
    /// This takes the context's fields into consideration when adding
    /// the resource. If `include` is false, this is a no-op. The context
    /// must also have an `optimize_level_*` field set corresponding with
    /// the optimization level of the passed bytecode, or this is a no-op.
    pub fn add_python_module_bytecode_with_context(
        &mut self,
        module: &PythonModuleBytecode,
        add_context: &PythonResourceAddCollectionContext,
    ) -> Result<Vec<AddResourceAction>> {
        if !add_context.include {
            return Ok(vec![AddResourceAction::NoInclude(module.description())]);
        }

        match module.optimize_level {
            BytecodeOptimizationLevel::Zero => {
                if add_context.optimize_level_zero {
                    self.add_python_resource_with_locations(
                        &module.into(),
                        &add_context.location,
                        &add_context.location_fallback,
                    )
                } else {
                    Ok(vec![AddResourceAction::BytecodeOptimizationLevelMismatch(
                        module.name.clone(),
                    )])
                }
            }
            BytecodeOptimizationLevel::One => {
                if add_context.optimize_level_one {
                    self.add_python_resource_with_locations(
                        &module.into(),
                        &add_context.location,
                        &add_context.location_fallback,
                    )
                } else {
                    Ok(vec![AddResourceAction::BytecodeOptimizationLevelMismatch(
                        module.name.clone(),
                    )])
                }
            }
            BytecodeOptimizationLevel::Two => {
                if add_context.optimize_level_two {
                    self.add_python_resource_with_locations(
                        &module.into(),
                        &add_context.location,
                        &add_context.location_fallback,
                    )
                } else {
                    Ok(vec![AddResourceAction::BytecodeOptimizationLevelMismatch(
                        module.name.clone(),
                    )])
                }
            }
        }
    }

    /// Add Python module bytecode derived from source code to the collection.
    pub fn add_python_module_bytecode_from_source(
        &mut self,
        module: &PythonModuleBytecodeFromSource,
        location: &ConcreteResourceLocation,
    ) -> Result<Vec<AddResourceAction>> {
        self.check_policy(location.into())?;

        let entry = self
            .resources
            .entry(module.name.clone())
            .or_insert_with(|| PrePackagedResource {
                name: module.name.clone(),
                ..PrePackagedResource::default()
            });

        entry.is_module = true;
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

        Ok(vec![AddResourceAction::Added(
            module.description(),
            location.clone(),
        )])
    }

    /// Add Python module bytecode from source using an add context to influence operations.
    ///
    /// This method respects the settings of the context, including `include`
    /// and the `optimize_level_*` fields.
    ///
    /// `PythonModuleBytecodeFromSource` defines an explicit bytecode optimization level,
    /// so this method can result in at most 1 bytecode request being added to the
    /// collection.
    pub fn add_python_module_bytecode_from_source_with_context(
        &mut self,
        module: &PythonModuleBytecodeFromSource,
        add_context: &PythonResourceAddCollectionContext,
    ) -> Result<Vec<AddResourceAction>> {
        if !add_context.include {
            return Ok(vec![AddResourceAction::NoInclude(module.description())]);
        }

        match module.optimize_level {
            BytecodeOptimizationLevel::Zero => {
                if add_context.optimize_level_zero {
                    self.add_python_resource_with_locations(
                        &module.into(),
                        &add_context.location,
                        &add_context.location_fallback,
                    )
                } else {
                    Ok(vec![AddResourceAction::BytecodeOptimizationLevelMismatch(
                        module.name.clone(),
                    )])
                }
            }
            BytecodeOptimizationLevel::One => {
                if add_context.optimize_level_one {
                    self.add_python_resource_with_locations(
                        &module.into(),
                        &add_context.location,
                        &add_context.location_fallback,
                    )
                } else {
                    Ok(vec![AddResourceAction::BytecodeOptimizationLevelMismatch(
                        module.name.clone(),
                    )])
                }
            }
            BytecodeOptimizationLevel::Two => {
                if add_context.optimize_level_two {
                    self.add_python_resource_with_locations(
                        &module.into(),
                        &add_context.location,
                        &add_context.location_fallback,
                    )
                } else {
                    Ok(vec![AddResourceAction::BytecodeOptimizationLevelMismatch(
                        module.name.clone(),
                    )])
                }
            }
        }
    }

    /// Add resource data to a given location.
    ///
    /// Resource data belongs to a Python package and has a name and bytes data.
    pub fn add_python_package_resource(
        &mut self,
        resource: &PythonPackageResource,
        location: &ConcreteResourceLocation,
    ) -> Result<Vec<AddResourceAction>> {
        self.check_policy(location.into())?;

        let entry = self
            .resources
            .entry(resource.leaf_package.clone())
            .or_insert_with(|| PrePackagedResource {
                name: resource.leaf_package.clone(),
                ..PrePackagedResource::default()
            });

        // Adding a resource automatically makes the module a package.
        entry.is_module = true;
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

        Ok(vec![AddResourceAction::Added(
            resource.description(),
            location.clone(),
        )])
    }

    /// Add a Python package resource using an add context.
    ///
    /// The fields from the context will be respected. This includes not doing
    /// anything if `include` is false.
    pub fn add_python_package_resource_with_context(
        &mut self,
        resource: &PythonPackageResource,
        add_context: &PythonResourceAddCollectionContext,
    ) -> Result<Vec<AddResourceAction>> {
        if !add_context.include {
            return Ok(vec![AddResourceAction::NoInclude(resource.description())]);
        }

        self.add_python_resource_with_locations(
            &resource.into(),
            &add_context.location,
            &add_context.location_fallback,
        )
    }

    /// Add a Python package distribution resource to a given location.
    pub fn add_python_package_distribution_resource(
        &mut self,
        resource: &PythonPackageDistributionResource,
        location: &ConcreteResourceLocation,
    ) -> Result<Vec<AddResourceAction>> {
        self.check_policy(location.into())?;

        let entry = self
            .resources
            .entry(resource.package.clone())
            .or_insert_with(|| PrePackagedResource {
                name: resource.package.clone(),
                ..PrePackagedResource::default()
            });

        // A distribution resource makes the entity a package.
        entry.is_module = true;
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

        Ok(vec![AddResourceAction::Added(
            resource.description(),
            location.clone(),
        )])
    }

    /// Add a Python package distribution resource using an add context.
    ///
    /// The fields from the context will be respected. This includes not doing
    /// anything if `include` is false.
    pub fn add_python_package_distribution_resource_with_context(
        &mut self,
        resource: &PythonPackageDistributionResource,
        add_context: &PythonResourceAddCollectionContext,
    ) -> Result<Vec<AddResourceAction>> {
        if !add_context.include {
            return Ok(vec![AddResourceAction::NoInclude(resource.description())]);
        }

        self.add_python_resource_with_locations(
            &resource.into(),
            &add_context.location,
            &add_context.location_fallback,
        )
    }

    /// Add a Python extension module using an add context.
    #[allow(clippy::if_same_then_else)]
    pub fn add_python_extension_module_with_context(
        &mut self,
        extension_module: &PythonExtensionModule,
        add_context: &PythonResourceAddCollectionContext,
    ) -> Result<(Vec<AddResourceAction>, Option<LibPythonBuildContext>)> {
        // TODO consult this attribute (it isn't set for built-ins for some reason)
        //if !add_context.include {
        //    return Ok(None);
        // }

        // Whether we can load extension modules as standalone shared library files.
        let can_load_standalone = self
            .allowed_extension_module_locations
            .contains(&AbstractResourceLocation::RelativePath);

        // Whether we can load extension module dynamic libraries from memory. This
        // means we have a dynamic library extension module and that library is loaded
        // from memory: this is not a built-in extension!
        let can_load_dynamic_library_memory = self
            .allowed_extension_module_locations
            .contains(&AbstractResourceLocation::InMemory);

        // Whether we can link the extension as a built-in. This requires one of the
        // following:
        //
        // 1. The extension module is already a built-in.
        // 2. Object files or static library data and for the policy to allow new built-in
        //    extension modules.
        let can_link_builtin = if extension_module.in_libpython() {
            true
        } else {
            self.allow_new_builtin_extension_modules
                && !extension_module.object_file_data.is_empty()
        };

        // Whether we can produce a standalone shared library extension module.
        // TODO consider allowing this if object files are present.
        let can_link_standalone = extension_module.shared_library.is_some();

        let mut relative_path = if let Some(location) = &add_context.location_fallback {
            match location {
                ConcreteResourceLocation::RelativePath(ref prefix) => Some(prefix.clone()),
                ConcreteResourceLocation::InMemory => None,
            }
        } else {
            None
        };

        let prefer_in_memory = add_context.location == ConcreteResourceLocation::InMemory;
        let prefer_filesystem = match &add_context.location {
            ConcreteResourceLocation::RelativePath(_) => true,
            ConcreteResourceLocation::InMemory => false,
        };

        let fallback_in_memory =
            add_context.location_fallback == Some(ConcreteResourceLocation::InMemory);
        let fallback_filesystem = matches!(
            &add_context.location_fallback,
            Some(ConcreteResourceLocation::RelativePath(_))
        );

        // TODO support this.
        if prefer_filesystem && fallback_in_memory {
            return Err(anyhow!("a preferred location of the filesystem and a fallback from memory is not supported"));
        }

        let require_in_memory =
            prefer_in_memory && (add_context.location_fallback.is_none() || fallback_in_memory);
        let require_filesystem =
            prefer_filesystem && (add_context.location_fallback.is_none() || fallback_filesystem);

        match &add_context.location {
            ConcreteResourceLocation::RelativePath(prefix) => {
                relative_path = Some(prefix.clone());
            }
            ConcreteResourceLocation::InMemory => {}
        }

        // We produce a builtin extension module (by linking object files) if any
        // of the following conditions are met:
        //
        // We are a stdlib extension module built into libpython core
        let produce_builtin = if extension_module.is_stdlib && extension_module.builtin_default {
            true
        // Builtin linking is the only mechanism available to us.
        } else if can_link_builtin && (!can_link_standalone || !can_load_standalone) {
            true
        // We want in memory loading and we can link a builtin
        } else {
            prefer_in_memory && can_link_builtin && !require_filesystem
        };

        if require_in_memory && (!can_link_builtin && !can_load_dynamic_library_memory) {
            return Err(anyhow!(
                "extension module {} cannot be loaded from memory but memory loading required",
                extension_module.name
            ));
        }

        if require_filesystem && !can_link_standalone && !produce_builtin {
            return Err(anyhow!("extension module {} cannot be materialized as a shared library extension but filesystem loading required", extension_module.name));
        }

        if !produce_builtin && !can_load_standalone {
            return Err(anyhow!("extension module {} cannot be materialized as a shared library because distribution does not support loading extension module shared libraries", extension_module.name));
        }

        if produce_builtin {
            let mut build_context = LibPythonBuildContext::default();

            for depends in &extension_module.link_libraries {
                if depends.framework {
                    build_context.frameworks.insert(depends.name.clone());
                } else if depends.system {
                    build_context.system_libraries.insert(depends.name.clone());
                } else if depends.static_library.is_some() {
                    build_context.static_libraries.insert(depends.name.clone());
                } else if depends.dynamic_library.is_some() {
                    build_context.dynamic_libraries.insert(depends.name.clone());
                }
            }

            if let Some(component) = &extension_module.license {
                build_context
                    .licensed_components
                    .add_component(component.clone());
            }

            if let Some(init_fn) = &extension_module.init_fn {
                build_context
                    .init_functions
                    .insert(extension_module.name.clone(), init_fn.clone());
            }

            for location in &extension_module.object_file_data {
                build_context.object_files.push(location.clone());
            }

            let actions = self.add_builtin_python_extension_module(extension_module)?;

            Ok((actions, Some(build_context)))
        } else {
            // If we're not producing a builtin, we're producing a shared library
            // extension module. We currently only support extension modules that
            // already have a shared library present. So we simply call into
            // the resources collector.
            let location = if prefer_in_memory && can_load_dynamic_library_memory {
                ConcreteResourceLocation::InMemory
            } else {
                match relative_path {
                    Some(prefix) => ConcreteResourceLocation::RelativePath(prefix),
                    None => ConcreteResourceLocation::InMemory,
                }
            };

            let actions = self.add_python_extension_module(extension_module, &location)?;

            Ok((actions, None))
        }
    }

    /// Add a built-in extension module.
    ///
    /// Built-in extension modules are statically linked into the binary and
    /// cannot have their location defined.
    pub fn add_builtin_python_extension_module(
        &mut self,
        module: &PythonExtensionModule,
    ) -> Result<Vec<AddResourceAction>> {
        let entry = self
            .resources
            .entry(module.name.clone())
            .or_insert_with(|| PrePackagedResource {
                name: module.name.clone(),
                ..PrePackagedResource::default()
            });

        entry.is_builtin_extension_module = true;
        entry.is_package = module.is_package;

        Ok(vec![AddResourceAction::AddedBuiltinExtensionModule(
            module.name.clone(),
        )])
    }

    /// Add a Python extension module shared library that should be imported from memory.
    pub fn add_python_extension_module(
        &mut self,
        module: &PythonExtensionModule,
        location: &ConcreteResourceLocation,
    ) -> Result<Vec<AddResourceAction>> {
        self.check_policy(location.into())?;

        let data = match &module.shared_library {
            Some(location) => location.resolve_content()?,
            None => return Err(anyhow!("no shared library data present")),
        };

        match location {
            ConcreteResourceLocation::RelativePath(_) => {
                if !self
                    .allowed_extension_module_locations
                    .contains(&AbstractResourceLocation::RelativePath)
                {
                    return Err(anyhow!("cannot add extension module {} as a file because extension modules as files are not allowed", module.name));
                }
            }
            ConcreteResourceLocation::InMemory => {
                if !self
                    .allowed_extension_module_locations
                    .contains(&AbstractResourceLocation::InMemory)
                {
                    return Err(anyhow!("cannot add extension module {} for in-memory import because in-memory loading is not supported/allowed", module.name));
                }
            }
        }

        let mut depends = vec![];
        let mut actions = vec![];

        for link in &module.link_libraries {
            if link.dynamic_library.is_some() {
                let library_location = match location {
                    ConcreteResourceLocation::InMemory => ConcreteResourceLocation::InMemory,
                    ConcreteResourceLocation::RelativePath(prefix) => {
                        // We place the shared library next to the extension module.
                        let path = module
                            .resolve_path(prefix)
                            .parent()
                            .ok_or_else(|| anyhow!("unable to resolve parent directory"))?
                            .to_path_buf();

                        ConcreteResourceLocation::RelativePath(
                            path.display().to_string().replace('\\', "/"),
                        )
                    }
                };

                let library = SharedLibrary::try_from(link).map_err(|e| anyhow!(e.to_string()))?;

                actions.extend(self.add_shared_library(&library, &library_location)?);
                depends.push(link.name.to_string());
            }
        }

        let entry = self
            .resources
            .entry(module.name.to_string())
            .or_insert_with(|| PrePackagedResource {
                name: module.name.to_string(),
                ..PrePackagedResource::default()
            });

        entry.is_extension_module = true;

        if module.is_package {
            entry.is_package = true;
        }

        match location {
            ConcreteResourceLocation::InMemory => {
                entry.in_memory_extension_module_shared_library = Some(FileData::Memory(data));
            }
            ConcreteResourceLocation::RelativePath(prefix) => {
                entry.relative_path_extension_module_shared_library =
                    Some((module.resolve_path(prefix), FileData::Memory(data)));
            }
        }

        entry.shared_library_dependency_names = Some(depends);
        actions.push(AddResourceAction::Added(
            module.description(),
            location.clone(),
        ));

        Ok(actions)
    }

    /// Add a shared library to be loaded from a location.
    pub fn add_shared_library(
        &mut self,
        library: &SharedLibrary,
        location: &ConcreteResourceLocation,
    ) -> Result<Vec<AddResourceAction>> {
        self.check_policy(location.into())?;

        let entry = self
            .resources
            .entry(library.name.to_string())
            .or_insert_with(|| PrePackagedResource {
                name: library.name.to_string(),
                ..PrePackagedResource::default()
            });

        entry.is_shared_library = true;

        match location {
            ConcreteResourceLocation::InMemory => {
                entry.in_memory_shared_library = Some(library.data.clone());
            }
            ConcreteResourceLocation::RelativePath(prefix) => match &library.filename {
                Some(filename) => {
                    entry.relative_path_shared_library =
                        Some((prefix.to_string(), filename.clone(), library.data.clone()));
                }
                None => return Err(anyhow!("cannot add shared library without known filename")),
            },
        }

        Ok(vec![AddResourceAction::Added(
            library.description(),
            location.clone(),
        )])
    }

    pub fn add_file_data(
        &mut self,
        file: &File,
        location: &ConcreteResourceLocation,
    ) -> Result<Vec<AddResourceAction>> {
        if !self.allow_files {
            return Err(anyhow!(
                "untyped files are now allowed on this resource collector"
            ));
        }

        self.check_policy(location.into())?;

        let entry =
            self.resources
                .entry(file.path_string())
                .or_insert_with(|| PrePackagedResource {
                    name: file.path_string(),
                    ..PrePackagedResource::default()
                });

        entry.is_utf8_filename_data = true;
        entry.file_executable = file.entry().is_executable();

        match location {
            ConcreteResourceLocation::InMemory => {
                entry.file_data_embedded = Some(file.entry().file_data().clone());
            }
            ConcreteResourceLocation::RelativePath(prefix) => {
                let path = PathBuf::from(prefix).join(file.path());

                entry.file_data_utf8_relative_path = Some((
                    PathBuf::from(path.display().to_string().replace('\\', "/")),
                    file.entry().file_data().clone(),
                ));
            }
        }

        Ok(vec![AddResourceAction::Added(
            format!("file {}", file.path_string()),
            location.clone(),
        )])
    }

    pub fn add_file_data_with_context(
        &mut self,
        file: &File,
        add_context: &PythonResourceAddCollectionContext,
    ) -> Result<Vec<AddResourceAction>> {
        if !add_context.include {
            return Ok(vec![AddResourceAction::NoInclude(format!(
                "file {}",
                file.path_string()
            ))]);
        }

        self.add_python_resource_with_locations(
            &file.into(),
            &add_context.location,
            &add_context.location_fallback,
        )
    }

    fn add_python_resource_with_locations(
        &mut self,
        resource: &PythonResource,
        location: &ConcreteResourceLocation,
        fallback_location: &Option<ConcreteResourceLocation>,
    ) -> Result<Vec<AddResourceAction>> {
        match resource {
            PythonResource::ModuleSource(module) => {
                match self
                    .add_python_module_source(module, location)
                    .with_context(|| format!("adding PythonModuleSource<{}>", module.name))
                {
                    Ok(actions) => Ok(actions),
                    Err(err) => {
                        if let Some(location) = fallback_location {
                            self.add_python_module_source(module, location)
                        } else {
                            Err(err)
                        }
                    }
                }
            }
            PythonResource::ModuleBytecodeRequest(module) => {
                match self
                    .add_python_module_bytecode_from_source(module, location)
                    .with_context(|| {
                        format!("adding PythonModuleBytecodeFromSource<{}>", module.name)
                    }) {
                    Ok(actions) => Ok(actions),
                    Err(err) => {
                        if let Some(location) = fallback_location {
                            self.add_python_module_bytecode_from_source(module, location)
                        } else {
                            Err(err)
                        }
                    }
                }
            }
            PythonResource::ModuleBytecode(module) => {
                match self
                    .add_python_module_bytecode(module, location)
                    .with_context(|| format!("adding PythonModuleBytecode<{}>", module.name))
                {
                    Ok(actions) => Ok(actions),
                    Err(err) => {
                        if let Some(location) = fallback_location {
                            self.add_python_module_bytecode(module, location)
                        } else {
                            Err(err)
                        }
                    }
                }
            }
            PythonResource::PackageResource(resource) => {
                match self
                    .add_python_package_resource(resource, location)
                    .with_context(|| {
                        format!(
                            "adding PythonPackageResource<{}, {}>",
                            resource.leaf_package, resource.relative_name
                        )
                    }) {
                    Ok(actions) => Ok(actions),
                    Err(err) => {
                        if let Some(location) = fallback_location {
                            self.add_python_package_resource(resource, location)
                        } else {
                            Err(err)
                        }
                    }
                }
            }
            PythonResource::PackageDistributionResource(resource) => {
                match self
                    .add_python_package_distribution_resource(resource, location)
                    .with_context(|| {
                        format!(
                            "adding PythonPackageDistributionResource<{}, {}>",
                            resource.package, resource.name
                        )
                    }) {
                    Ok(actions) => Ok(actions),
                    Err(err) => {
                        if let Some(location) = fallback_location {
                            self.add_python_package_distribution_resource(resource, location)
                        } else {
                            Err(err)
                        }
                    }
                }
            }
            PythonResource::File(file) => match self
                .add_file_data(file, location)
                .with_context(|| format!("adding File<{}>", file.path().display()))
            {
                Ok(actions) => Ok(actions),
                Err(err) => {
                    if let Some(location) = fallback_location {
                        self.add_file_data(file, location)
                    } else {
                        Err(err)
                    }
                }
            },
            _ => Err(anyhow!("PythonResource variant not yet supported")),
        }
    }

    /// Searches for Python sources for references to __file__.
    ///
    /// __file__ usage can be problematic for in-memory modules. This method searches
    /// for its occurrences and returns module names having it present.
    pub fn find_dunder_file(&self) -> Result<BTreeSet<String>> {
        let mut res = BTreeSet::new();

        for (name, module) in &self.resources {
            if let Some(location) = &module.in_memory_source {
                if has_dunder_file(&location.resolve_content()?)? {
                    res.insert(name.clone());
                }
            }

            if let Some(PythonModuleBytecodeProvider::FromSource(location)) =
                &module.in_memory_bytecode
            {
                if has_dunder_file(&location.resolve_content()?)? {
                    res.insert(name.clone());
                }
            }

            if let Some(PythonModuleBytecodeProvider::FromSource(location)) =
                &module.in_memory_bytecode_opt1
            {
                if has_dunder_file(&location.resolve_content()?)? {
                    res.insert(name.clone());
                }
            }

            if let Some(PythonModuleBytecodeProvider::FromSource(location)) =
                &module.in_memory_bytecode_opt2
            {
                if has_dunder_file(&location.resolve_content()?)? {
                    res.insert(name.clone());
                }
            }
        }

        Ok(res)
    }

    /// Compiles resources into a finalized collection.
    ///
    /// This will take all resources collected so far and convert them into
    /// a collection of `Resource` plus extra file install rules.
    ///
    /// Missing parent packages will be added automatically.
    pub fn compile_resources(
        &self,
        compiler: &mut dyn PythonBytecodeCompiler,
    ) -> Result<CompiledResourcesCollection> {
        let mut input_resources = self.resources.clone();
        populate_parent_packages(&mut input_resources).context("populating parent packages")?;

        let mut resources = BTreeMap::new();
        let mut extra_files = Vec::new();

        for (name, resource) in &input_resources {
            let (entry, installs) = resource
                .to_resource(compiler)
                .with_context(|| format!("converting {} to resource", name))?;

            for install in installs {
                extra_files.push(install);
            }

            resources.insert(name.clone(), entry);
        }

        Ok(CompiledResourcesCollection {
            resources,
            extra_files,
        })
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::{
            resource::{LibraryDependency, PythonPackageDistributionResourceFlavor},
            testutil::FakeBytecodeCompiler,
        },
        simple_file_manifest::FileEntry,
    };

    const DEFAULT_CACHE_TAG: &str = "cpython-39";

    #[test]
    fn test_resource_conversion_basic() -> Result<()> {
        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let pre = PrePackagedResource {
            is_module: true,
            name: "module".to_string(),
            is_package: true,
            is_namespace_package: true,
            ..PrePackagedResource::default()
        };

        let (resource, installs) = pre.to_resource(&mut compiler)?;

        assert_eq!(
            resource,
            Resource {
                is_python_module: true,
                name: Cow::Owned("module".to_string()),
                is_python_package: true,
                is_python_namespace_package: true,
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
            is_module: true,
            name: "module".to_string(),
            in_memory_source: Some(FileData::Memory(b"source".to_vec())),
            ..PrePackagedResource::default()
        };

        let (resource, installs) = pre.to_resource(&mut compiler)?;

        assert_eq!(
            resource,
            Resource {
                is_python_module: true,
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
            is_module: true,
            name: "module".to_string(),
            in_memory_bytecode: Some(PythonModuleBytecodeProvider::Provided(FileData::Memory(
                b"bytecode".to_vec(),
            ))),
            ..PrePackagedResource::default()
        };

        let (resource, installs) = pre.to_resource(&mut compiler)?;

        assert_eq!(
            resource,
            Resource {
                is_python_module: true,
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
            is_module: true,
            name: "module".to_string(),
            in_memory_bytecode: Some(PythonModuleBytecodeProvider::FromSource(FileData::Memory(
                b"source".to_vec(),
            ))),
            ..PrePackagedResource::default()
        };

        let (resource, installs) = pre.to_resource(&mut compiler)?;

        assert_eq!(
            resource,
            Resource {
                is_python_module: true,
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
            is_module: true,
            name: "module".to_string(),
            in_memory_bytecode_opt1: Some(PythonModuleBytecodeProvider::Provided(
                FileData::Memory(b"bytecode".to_vec()),
            )),
            ..PrePackagedResource::default()
        };

        let (resource, installs) = pre.to_resource(&mut compiler)?;

        assert_eq!(
            resource,
            Resource {
                is_python_module: true,
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
            is_module: true,
            name: "module".to_string(),
            in_memory_bytecode_opt1: Some(PythonModuleBytecodeProvider::FromSource(
                FileData::Memory(b"source".to_vec()),
            )),
            ..PrePackagedResource::default()
        };

        let (resource, installs) = pre.to_resource(&mut compiler)?;

        assert_eq!(
            resource,
            Resource {
                is_python_module: true,
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
            is_module: true,
            name: "module".to_string(),
            in_memory_bytecode_opt2: Some(PythonModuleBytecodeProvider::Provided(
                FileData::Memory(b"bytecode".to_vec()),
            )),
            ..PrePackagedResource::default()
        };

        let (resource, installs) = pre.to_resource(&mut compiler)?;

        assert_eq!(
            resource,
            Resource {
                is_python_module: true,
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
            is_module: true,
            name: "module".to_string(),
            in_memory_bytecode_opt2: Some(PythonModuleBytecodeProvider::FromSource(
                FileData::Memory(b"source".to_vec()),
            )),
            ..PrePackagedResource::default()
        };

        let (resource, installs) = pre.to_resource(&mut compiler)?;

        assert_eq!(
            resource,
            Resource {
                is_python_module: true,
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
            is_module: true,
            name: "module".to_string(),
            in_memory_extension_module_shared_library: Some(FileData::Memory(b"library".to_vec())),
            ..PrePackagedResource::default()
        };

        let (resource, installs) = pre.to_resource(&mut compiler)?;

        assert_eq!(
            resource,
            Resource {
                is_python_module: true,
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
        resources.insert("foo".to_string(), FileData::Memory(b"value".to_vec()));

        let pre = PrePackagedResource {
            is_module: true,
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
                is_python_module: true,
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
        resources.insert("foo".to_string(), FileData::Memory(b"value".to_vec()));

        let pre = PrePackagedResource {
            is_module: true,
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
                is_python_module: true,
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
            is_shared_library: true,
            name: "module".to_string(),
            in_memory_shared_library: Some(FileData::Memory(b"library".to_vec())),
            shared_library_dependency_names: Some(vec!["foo".to_string(), "bar".to_string()]),
            ..PrePackagedResource::default()
        };

        let (resource, installs) = pre.to_resource(&mut compiler)?;

        assert_eq!(
            resource,
            Resource {
                is_shared_library: true,
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
            is_module: true,
            name: "module".to_string(),
            relative_path_module_source: Some((
                "prefix".to_string(),
                FileData::Memory(b"source".to_vec()),
            )),
            ..PrePackagedResource::default()
        };

        let (resource, installs) = pre.to_resource(&mut compiler)?;

        assert_eq!(
            resource,
            Resource {
                is_python_module: true,
                name: Cow::Owned("module".to_string()),
                relative_path_module_source: Some(Cow::Owned(PathBuf::from("prefix/module.py"))),
                ..Resource::default()
            }
        );

        assert_eq!(
            installs,
            vec![(
                PathBuf::from("prefix/module.py"),
                FileData::Memory(b"source".to_vec()),
                false
            )]
        );

        Ok(())
    }

    #[test]
    fn test_resource_conversion_relative_path_module_bytecode_provided() -> Result<()> {
        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let pre = PrePackagedResource {
            is_module: true,
            name: "foo.bar".to_string(),
            relative_path_bytecode: Some((
                "prefix".to_string(),
                "tag".to_string(),
                PythonModuleBytecodeProvider::Provided(FileData::Memory(b"bytecode".to_vec())),
            )),
            ..PrePackagedResource::default()
        };

        let (resource, installs) = pre.to_resource(&mut compiler)?;

        assert_eq!(
            resource,
            Resource {
                is_python_module: true,
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
                FileData::Memory(
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
            is_module: true,
            name: "foo.bar".to_string(),
            relative_path_bytecode: Some((
                "prefix".to_string(),
                "tag".to_string(),
                PythonModuleBytecodeProvider::FromSource(FileData::Memory(b"source".to_vec())),
            )),
            ..PrePackagedResource::default()
        };

        let (resource, installs) = pre.to_resource(&mut compiler)?;

        assert_eq!(
            resource,
            Resource {
                is_python_module: true,
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
                FileData::Memory(b"bc0source".to_vec()),
                false
            )]
        );

        Ok(())
    }

    #[test]
    fn test_resource_conversion_relative_path_module_bytecode_opt1_provided() -> Result<()> {
        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let pre = PrePackagedResource {
            is_module: true,
            name: "foo.bar".to_string(),
            relative_path_bytecode_opt1: Some((
                "prefix".to_string(),
                "tag".to_string(),
                PythonModuleBytecodeProvider::Provided(FileData::Memory(b"bytecode".to_vec())),
            )),
            ..PrePackagedResource::default()
        };

        let (resource, installs) = pre.to_resource(&mut compiler)?;

        assert_eq!(
            resource,
            Resource {
                is_python_module: true,
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
                FileData::Memory(
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
            is_module: true,
            name: "foo.bar".to_string(),
            relative_path_bytecode_opt1: Some((
                "prefix".to_string(),
                "tag".to_string(),
                PythonModuleBytecodeProvider::FromSource(FileData::Memory(b"source".to_vec())),
            )),
            ..PrePackagedResource::default()
        };

        let (resource, installs) = pre.to_resource(&mut compiler)?;

        assert_eq!(
            resource,
            Resource {
                is_python_module: true,
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
                FileData::Memory(b"bc1source".to_vec()),
                false
            )]
        );

        Ok(())
    }

    #[test]
    fn test_resource_conversion_relative_path_module_bytecode_opt2_provided() -> Result<()> {
        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let pre = PrePackagedResource {
            is_module: true,
            name: "foo.bar".to_string(),
            relative_path_bytecode_opt2: Some((
                "prefix".to_string(),
                "tag".to_string(),
                PythonModuleBytecodeProvider::Provided(FileData::Memory(b"bytecode".to_vec())),
            )),
            ..PrePackagedResource::default()
        };

        let (resource, installs) = pre.to_resource(&mut compiler)?;

        assert_eq!(
            resource,
            Resource {
                is_python_module: true,
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
                FileData::Memory(
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
            is_module: true,
            name: "foo.bar".to_string(),
            relative_path_bytecode_opt2: Some((
                "prefix".to_string(),
                "tag".to_string(),
                PythonModuleBytecodeProvider::FromSource(FileData::Memory(b"source".to_vec())),
            )),
            ..PrePackagedResource::default()
        };

        let (resource, installs) = pre.to_resource(&mut compiler)?;

        assert_eq!(
            resource,
            Resource {
                is_python_module: true,
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
                FileData::Memory(b"bc2source".to_vec()),
                false
            )]
        );

        Ok(())
    }

    #[test]
    fn test_resource_conversion_relative_path_extension_module_shared_library() -> Result<()> {
        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let pre = PrePackagedResource {
            is_module: true,
            name: "module".to_string(),
            relative_path_extension_module_shared_library: Some((
                PathBuf::from("prefix/ext.so"),
                FileData::Memory(b"data".to_vec()),
            )),
            ..PrePackagedResource::default()
        };

        let (resource, installs) = pre.to_resource(&mut compiler)?;

        assert_eq!(
            resource,
            Resource {
                is_python_module: true,
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
                FileData::Memory(b"data".to_vec()),
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
                FileData::Memory(b"data".to_vec()),
            ),
        );
        resources.insert(
            "bar.txt".to_string(),
            (
                PathBuf::from("module/bar.txt"),
                FileData::Memory(b"bar".to_vec()),
            ),
        );

        let pre = PrePackagedResource {
            is_module: true,
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
                is_python_module: true,
                name: Cow::Owned("module".to_string()),
                is_python_package: true,
                relative_path_package_resources: Some(resources),
                ..Resource::default()
            }
        );

        assert_eq!(
            installs,
            vec![
                (
                    PathBuf::from("module/bar.txt"),
                    FileData::Memory(b"bar".to_vec()),
                    false
                ),
                (
                    PathBuf::from("module/foo.txt"),
                    FileData::Memory(b"data".to_vec()),
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
            (PathBuf::from("foo.txt"), FileData::Memory(b"data".to_vec())),
        );

        let pre = PrePackagedResource {
            is_module: true,
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
                is_python_module: true,
                name: Cow::Owned("module".to_string()),
                relative_path_distribution_resources: Some(resources),
                ..Resource::default()
            }
        );

        assert_eq!(
            installs,
            vec![(
                PathBuf::from("foo.txt"),
                FileData::Memory(b"data".to_vec()),
                false
            )]
        );

        Ok(())
    }

    #[test]
    fn test_resource_conversion_relative_path_shared_library() -> Result<()> {
        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let pre = PrePackagedResource {
            is_shared_library: true,
            name: "libfoo".to_string(),
            relative_path_shared_library: Some((
                "prefix".to_string(),
                PathBuf::from("libfoo.so"),
                FileData::Memory(b"data".to_vec()),
            )),
            ..PrePackagedResource::default()
        };

        let (resource, installs) = pre.to_resource(&mut compiler)?;

        assert_eq!(
            resource,
            Resource {
                is_shared_library: true,
                name: Cow::Owned("libfoo".to_string()),
                ..Resource::default()
            }
        );

        assert_eq!(
            installs,
            vec![(
                PathBuf::from("prefix/libfoo.so"),
                FileData::Memory(b"data".to_vec()),
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
                is_module: true,
                name: "root.parent.child".to_string(),
                in_memory_source: Some(FileData::Memory(vec![42])),
                is_package: true,
                ..PrePackagedResource::default()
            },
        );

        populate_parent_packages(&mut h)?;

        assert_eq!(h.len(), 3);
        assert_eq!(
            h.get("root.parent"),
            Some(&PrePackagedResource {
                is_module: true,
                name: "root.parent".to_string(),
                is_package: true,
                in_memory_source: Some(FileData::Memory(vec![])),
                ..PrePackagedResource::default()
            })
        );
        assert_eq!(
            h.get("root"),
            Some(&PrePackagedResource {
                is_module: true,
                name: "root".to_string(),
                is_package: true,
                in_memory_source: Some(FileData::Memory(vec![])),
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
                is_module: true,
                name: "root.parent.child".to_string(),
                relative_path_module_source: Some((
                    "prefix".to_string(),
                    FileData::Memory(vec![42]),
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
                is_module: true,
                name: "root.parent".to_string(),
                is_package: true,
                relative_path_module_source: Some(("prefix".to_string(), FileData::Memory(vec![]))),
                ..PrePackagedResource::default()
            })
        );
        assert_eq!(
            h.get("root"),
            Some(&PrePackagedResource {
                is_module: true,
                name: "root".to_string(),
                is_package: true,
                relative_path_module_source: Some(("prefix".to_string(), FileData::Memory(vec![]))),
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
                is_module: true,
                name: "root.parent.child".to_string(),
                in_memory_bytecode: Some(PythonModuleBytecodeProvider::FromSource(
                    FileData::Memory(vec![42]),
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
                is_module: true,
                name: "root.parent".to_string(),
                is_package: true,
                in_memory_bytecode: Some(PythonModuleBytecodeProvider::FromSource(
                    FileData::Memory(vec![])
                )),
                ..PrePackagedResource::default()
            })
        );
        assert_eq!(
            h.get("root"),
            Some(&PrePackagedResource {
                is_module: true,
                name: "root".to_string(),
                is_package: true,
                in_memory_bytecode: Some(PythonModuleBytecodeProvider::FromSource(
                    FileData::Memory(vec![])
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
                is_builtin_extension_module: true,
                name: "foo.bar".to_string(),
                relative_path_extension_module_shared_library: Some((
                    PathBuf::from("prefix/foo/bar.so"),
                    FileData::Memory(vec![42]),
                )),
                ..PrePackagedResource::default()
            },
        );

        populate_parent_packages(&mut h)?;

        assert_eq!(
            h.get("foo"),
            Some(&PrePackagedResource {
                is_module: true,
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
                is_extension_module: true,
                name: "foo.bar".to_string(),
                relative_path_extension_module_shared_library: Some((
                    PathBuf::from("prefix/foo/bar.so"),
                    FileData::Memory(vec![42]),
                )),
                ..PrePackagedResource::default()
            },
        );

        populate_parent_packages(&mut h)?;

        assert_eq!(h.len(), 2);

        assert_eq!(
            h.get("foo"),
            Some(&PrePackagedResource {
                is_module: true,
                name: "foo".to_string(),
                is_package: true,
                ..PrePackagedResource::default()
            })
        );

        Ok(())
    }

    #[test]
    fn test_add_in_memory_source_module() -> Result<()> {
        let mut r = PythonResourceCollector::new(
            vec![AbstractResourceLocation::InMemory],
            vec![],
            false,
            false,
        );
        r.add_python_module_source(
            &PythonModuleSource {
                name: "foo".to_string(),
                source: FileData::Memory(vec![42]),
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
                is_module: true,
                name: "foo".to_string(),
                is_package: false,
                in_memory_source: Some(FileData::Memory(vec![42])),
                ..PrePackagedResource::default()
            })
        );

        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let resources = r.compile_resources(&mut compiler)?;

        assert_eq!(resources.resources.len(), 1);
        assert_eq!(
            resources.resources.get("foo"),
            Some(&Resource {
                is_python_module: true,
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
        let mut r = PythonResourceCollector::new(
            vec![AbstractResourceLocation::InMemory],
            vec![],
            false,
            false,
        );
        r.add_python_module_source(
            &PythonModuleSource {
                name: "root.parent.child".to_string(),
                source: FileData::Memory(vec![42]),
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
                is_module: true,
                name: "root.parent.child".to_string(),
                is_package: true,
                in_memory_source: Some(FileData::Memory(vec![42])),
                ..PrePackagedResource::default()
            })
        );

        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let resources = r.compile_resources(&mut compiler)?;

        assert_eq!(resources.resources.len(), 3);
        assert_eq!(
            resources.resources.get("root"),
            Some(&Resource {
                is_python_module: true,
                name: Cow::Owned("root".to_string()),
                is_python_package: true,
                in_memory_source: Some(Cow::Owned(vec![])),
                ..Resource::default()
            })
        );
        assert_eq!(
            resources.resources.get("root.parent"),
            Some(&Resource {
                is_python_module: true,
                name: Cow::Owned("root.parent".to_string()),
                is_python_package: true,
                in_memory_source: Some(Cow::Owned(vec![])),
                ..Resource::default()
            })
        );
        assert_eq!(
            resources.resources.get("root.parent.child"),
            Some(&Resource {
                is_python_module: true,
                name: Cow::Owned("root.parent.child".to_string()),
                is_python_package: true,
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
            vec![AbstractResourceLocation::RelativePath],
            vec![],
            false,
            false,
        );
        r.add_python_module_source(
            &PythonModuleSource {
                name: "foo.bar".to_string(),
                source: FileData::Memory(vec![42]),
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
                is_module: true,
                name: "foo.bar".to_string(),
                is_package: false,
                relative_path_module_source: Some((
                    "prefix".to_string(),
                    FileData::Memory(vec![42])
                )),
                ..PrePackagedResource::default()
            })
        );

        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let resources = r.compile_resources(&mut compiler)?;

        assert_eq!(resources.resources.len(), 2);
        assert_eq!(
            resources.resources.get("foo"),
            Some(&Resource {
                is_python_module: true,
                name: Cow::Owned("foo".to_string()),
                is_python_package: true,
                relative_path_module_source: Some(Cow::Owned(PathBuf::from(
                    "prefix/foo/__init__.py"
                ))),
                ..Resource::default()
            })
        );
        assert_eq!(
            resources.resources.get("foo.bar"),
            Some(&Resource {
                is_python_module: true,
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
                    FileData::Memory(vec![]),
                    false
                ),
                (
                    PathBuf::from("prefix/foo/bar.py"),
                    FileData::Memory(vec![42]),
                    false
                )
            ]
        );

        Ok(())
    }

    #[test]
    fn test_add_module_source_with_context() -> Result<()> {
        let mut r = PythonResourceCollector::new(
            vec![AbstractResourceLocation::InMemory],
            vec![],
            false,
            false,
        );

        let module = PythonModuleSource {
            name: "foo".to_string(),
            source: FileData::Memory(vec![42]),
            is_package: false,
            cache_tag: DEFAULT_CACHE_TAG.to_string(),
            is_stdlib: false,
            is_test: false,
        };

        let mut add_context = PythonResourceAddCollectionContext {
            include: false,
            location: ConcreteResourceLocation::InMemory,
            location_fallback: None,
            store_source: false,
            optimize_level_zero: false,
            optimize_level_one: false,
            optimize_level_two: false,
        };

        // include=false is a noop.
        assert!(r.resources.is_empty());
        r.add_python_module_source_with_context(&module, &add_context)?;
        assert!(r.resources.is_empty());

        add_context.include = true;

        // store_source=false is a noop.
        r.add_python_module_source_with_context(&module, &add_context)?;
        assert!(r.resources.is_empty());

        add_context.store_source = true;

        // store_source=true adds just the source.
        r.add_python_module_source_with_context(&module, &add_context)?;
        assert_eq!(
            r.resources.get(&module.name),
            Some(&PrePackagedResource {
                is_module: true,
                name: module.name.clone(),
                is_package: module.is_package,
                in_memory_source: Some(module.source.clone()),
                ..PrePackagedResource::default()
            })
        );

        r.resources.clear();
        add_context.store_source = false;

        // optimize_level_zero stores the bytecode.

        add_context.optimize_level_zero = true;
        r.add_python_module_source_with_context(&module, &add_context)?;
        assert_eq!(
            r.resources.get(&module.name),
            Some(&PrePackagedResource {
                is_module: true,
                name: module.name.clone(),
                is_package: module.is_package,
                in_memory_bytecode: Some(PythonModuleBytecodeProvider::FromSource(
                    module.source.clone()
                )),
                ..PrePackagedResource::default()
            })
        );

        r.resources.clear();
        add_context.optimize_level_zero = false;

        // optimize_level_one stores the bytecode.

        add_context.optimize_level_one = true;
        r.add_python_module_source_with_context(&module, &add_context)?;
        assert_eq!(
            r.resources.get(&module.name),
            Some(&PrePackagedResource {
                is_module: true,
                name: module.name.clone(),
                is_package: module.is_package,
                in_memory_bytecode_opt1: Some(PythonModuleBytecodeProvider::FromSource(
                    module.source.clone()
                )),
                ..PrePackagedResource::default()
            })
        );

        r.resources.clear();
        add_context.optimize_level_one = false;

        // optimize_level_two stores the bytecode.

        add_context.optimize_level_two = true;
        r.add_python_module_source_with_context(&module, &add_context)?;
        assert_eq!(
            r.resources.get(&module.name),
            Some(&PrePackagedResource {
                is_module: true,
                name: module.name.clone(),
                is_package: module.is_package,
                in_memory_bytecode_opt2: Some(PythonModuleBytecodeProvider::FromSource(
                    module.source.clone()
                )),
                ..PrePackagedResource::default()
            })
        );

        r.resources.clear();
        add_context.optimize_level_two = false;

        Ok(())
    }

    #[test]
    fn test_add_in_memory_bytecode_module() -> Result<()> {
        let mut r = PythonResourceCollector::new(
            vec![AbstractResourceLocation::InMemory],
            vec![],
            false,
            false,
        );
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
                is_module: true,
                name: "foo".to_string(),
                in_memory_bytecode: Some(PythonModuleBytecodeProvider::Provided(FileData::Memory(
                    vec![42]
                ))),
                is_package: false,
                ..PrePackagedResource::default()
            })
        );

        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let resources = r.compile_resources(&mut compiler)?;

        assert_eq!(resources.resources.len(), 1);
        assert_eq!(
            resources.resources.get("foo"),
            Some(&Resource {
                is_python_module: true,
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
        let mut r = PythonResourceCollector::new(
            vec![AbstractResourceLocation::InMemory],
            vec![],
            false,
            false,
        );
        r.add_python_module_bytecode_from_source(
            &PythonModuleBytecodeFromSource {
                name: "foo".to_string(),
                source: FileData::Memory(vec![42]),
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
                is_module: true,
                name: "foo".to_string(),
                in_memory_bytecode: Some(PythonModuleBytecodeProvider::FromSource(
                    FileData::Memory(vec![42])
                )),
                is_package: false,
                ..PrePackagedResource::default()
            })
        );

        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let resources = r.compile_resources(&mut compiler)?;

        assert_eq!(resources.resources.len(), 1);
        assert_eq!(
            resources.resources.get("foo"),
            Some(&Resource {
                is_python_module: true,
                name: Cow::Owned("foo".to_string()),
                in_memory_bytecode: Some(Cow::Owned(b"bc0\x2a".to_vec())),
                ..Resource::default()
            })
        );
        assert!(resources.extra_files.is_empty());

        Ok(())
    }

    #[test]
    fn test_add_module_bytecode_with_context() -> Result<()> {
        let mut r = PythonResourceCollector::new(
            vec![AbstractResourceLocation::InMemory],
            vec![],
            false,
            false,
        );

        let mut module = PythonModuleBytecode::new(
            "foo",
            BytecodeOptimizationLevel::Zero,
            false,
            DEFAULT_CACHE_TAG,
            &[42],
        );

        let mut add_context = PythonResourceAddCollectionContext {
            include: false,
            location: ConcreteResourceLocation::InMemory,
            location_fallback: None,
            store_source: false,
            optimize_level_zero: false,
            optimize_level_one: false,
            optimize_level_two: false,
        };

        // include=false is a noop.
        assert!(r.resources.is_empty());
        r.add_python_module_bytecode_with_context(&module, &add_context)?;
        assert!(r.resources.is_empty());

        add_context.include = true;

        // optimize_level_zero=false is a noop.
        r.add_python_module_bytecode_with_context(&module, &add_context)?;
        assert!(r.resources.is_empty());

        // optimize_level_zero=true adds the resource.
        add_context.optimize_level_zero = true;
        r.add_python_module_bytecode_with_context(&module, &add_context)?;
        assert_eq!(
            r.resources.get(&module.name),
            Some(&PrePackagedResource {
                is_module: true,
                name: module.name.clone(),
                is_package: module.is_package,
                in_memory_bytecode: Some(PythonModuleBytecodeProvider::Provided(FileData::Memory(
                    module.resolve_bytecode()?
                ))),
                ..PrePackagedResource::default()
            })
        );

        r.resources.clear();
        add_context.optimize_level_zero = false;

        // Other optimize_level_* fields are noop.
        add_context.optimize_level_one = true;
        add_context.optimize_level_two = true;
        r.add_python_module_bytecode_with_context(&module, &add_context)?;
        assert!(r.resources.is_empty());

        // No-ops for other module optimization levels.
        add_context.optimize_level_zero = false;
        add_context.optimize_level_one = false;
        add_context.optimize_level_two = false;

        module.optimize_level = BytecodeOptimizationLevel::One;
        r.add_python_module_bytecode_with_context(&module, &add_context)?;
        assert!(r.resources.is_empty());
        module.optimize_level = BytecodeOptimizationLevel::Two;
        r.add_python_module_bytecode_with_context(&module, &add_context)?;
        assert!(r.resources.is_empty());

        // optimize_level_one=true adds the resource.
        module.optimize_level = BytecodeOptimizationLevel::One;
        add_context.optimize_level_zero = true;
        add_context.optimize_level_one = true;
        add_context.optimize_level_two = true;
        r.add_python_module_bytecode_with_context(&module, &add_context)?;

        assert_eq!(
            r.resources.get(&module.name),
            Some(&PrePackagedResource {
                is_module: true,
                name: module.name.clone(),
                is_package: module.is_package,
                in_memory_bytecode_opt1: Some(PythonModuleBytecodeProvider::Provided(
                    FileData::Memory(module.resolve_bytecode()?)
                )),
                ..PrePackagedResource::default()
            })
        );

        r.resources.clear();

        // optimize_level_two=true adds the resource.
        module.optimize_level = BytecodeOptimizationLevel::Two;
        r.add_python_module_bytecode_with_context(&module, &add_context)?;

        assert_eq!(
            r.resources.get(&module.name),
            Some(&PrePackagedResource {
                is_module: true,
                name: module.name.clone(),
                is_package: module.is_package,
                in_memory_bytecode_opt2: Some(PythonModuleBytecodeProvider::Provided(
                    FileData::Memory(module.resolve_bytecode()?)
                )),
                ..PrePackagedResource::default()
            })
        );

        r.resources.clear();

        Ok(())
    }

    #[test]
    fn test_add_in_memory_bytecode_module_parents() -> Result<()> {
        let mut r = PythonResourceCollector::new(
            vec![AbstractResourceLocation::InMemory],
            vec![],
            false,
            false,
        );
        r.add_python_module_bytecode_from_source(
            &PythonModuleBytecodeFromSource {
                name: "root.parent.child".to_string(),
                source: FileData::Memory(vec![42]),
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
                is_module: true,
                name: "root.parent.child".to_string(),
                in_memory_bytecode_opt1: Some(PythonModuleBytecodeProvider::FromSource(
                    FileData::Memory(vec![42])
                )),
                is_package: true,
                ..PrePackagedResource::default()
            })
        );

        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let resources = r.compile_resources(&mut compiler)?;

        assert_eq!(resources.resources.len(), 3);
        assert_eq!(
            resources.resources.get("root"),
            Some(&Resource {
                is_python_module: true,
                name: Cow::Owned("root".to_string()),
                is_python_package: true,
                in_memory_bytecode_opt1: Some(Cow::Owned(b"bc1".to_vec())),
                ..Resource::default()
            })
        );
        assert_eq!(
            resources.resources.get("root.parent"),
            Some(&Resource {
                is_python_module: true,
                name: Cow::Owned("root.parent".to_string()),
                is_python_package: true,
                in_memory_bytecode_opt1: Some(Cow::Owned(b"bc1".to_vec())),
                ..Resource::default()
            })
        );
        assert_eq!(
            resources.resources.get("root.parent.child"),
            Some(&Resource {
                is_python_module: true,
                name: Cow::Owned("root.parent.child".to_string()),
                is_python_package: true,
                in_memory_bytecode_opt1: Some(Cow::Owned(b"bc1\x2a".to_vec())),
                ..Resource::default()
            })
        );
        assert!(resources.extra_files.is_empty());

        Ok(())
    }

    #[test]
    fn test_add_module_bytecode_from_source_with_context() -> Result<()> {
        let mut r = PythonResourceCollector::new(
            vec![AbstractResourceLocation::InMemory],
            vec![],
            false,
            false,
        );

        let mut module = PythonModuleBytecodeFromSource {
            name: "foo".to_string(),
            source: FileData::Memory(vec![42]),
            optimize_level: BytecodeOptimizationLevel::Zero,
            is_package: false,
            cache_tag: DEFAULT_CACHE_TAG.to_string(),
            is_stdlib: false,
            is_test: false,
        };

        let mut add_context = PythonResourceAddCollectionContext {
            include: false,
            location: ConcreteResourceLocation::InMemory,
            location_fallback: None,
            store_source: false,
            optimize_level_zero: false,
            optimize_level_one: false,
            optimize_level_two: false,
        };

        // include=false is a noop.
        assert!(r.resources.is_empty());
        r.add_python_module_bytecode_from_source_with_context(&module, &add_context)?;
        assert!(r.resources.is_empty());

        add_context.include = true;

        // optimize_level_zero=false is a noop.
        r.add_python_module_bytecode_from_source_with_context(&module, &add_context)?;
        assert!(r.resources.is_empty());

        // optimize_level_zero=true adds the resource.
        add_context.optimize_level_zero = true;
        r.add_python_module_bytecode_from_source_with_context(&module, &add_context)?;
        assert_eq!(
            r.resources.get(&module.name),
            Some(&PrePackagedResource {
                is_module: true,
                name: module.name.clone(),
                is_package: module.is_package,
                in_memory_bytecode: Some(PythonModuleBytecodeProvider::FromSource(
                    module.source.clone()
                )),
                ..PrePackagedResource::default()
            })
        );

        r.resources.clear();
        add_context.optimize_level_zero = false;

        // Other optimize_level_* fields are noop.
        add_context.optimize_level_one = true;
        add_context.optimize_level_two = true;
        r.add_python_module_bytecode_from_source_with_context(&module, &add_context)?;
        assert!(r.resources.is_empty());

        // No-ops for other module optimization levels.
        add_context.optimize_level_zero = false;
        add_context.optimize_level_one = false;
        add_context.optimize_level_two = false;

        module.optimize_level = BytecodeOptimizationLevel::One;
        r.add_python_module_bytecode_from_source_with_context(&module, &add_context)?;
        assert!(r.resources.is_empty());
        module.optimize_level = BytecodeOptimizationLevel::Two;
        r.add_python_module_bytecode_from_source_with_context(&module, &add_context)?;
        assert!(r.resources.is_empty());

        // optimize_level_one=true adds the resource.
        module.optimize_level = BytecodeOptimizationLevel::One;
        add_context.optimize_level_zero = true;
        add_context.optimize_level_one = true;
        add_context.optimize_level_two = true;
        r.add_python_module_bytecode_from_source_with_context(&module, &add_context)?;

        assert_eq!(
            r.resources.get(&module.name),
            Some(&PrePackagedResource {
                is_module: true,
                name: module.name.clone(),
                is_package: module.is_package,
                in_memory_bytecode_opt1: Some(PythonModuleBytecodeProvider::FromSource(
                    module.source.clone()
                )),
                ..PrePackagedResource::default()
            })
        );

        r.resources.clear();

        // optimize_level_two=true adds the resource.
        module.optimize_level = BytecodeOptimizationLevel::Two;
        r.add_python_module_bytecode_from_source_with_context(&module, &add_context)?;

        assert_eq!(
            r.resources.get(&module.name),
            Some(&PrePackagedResource {
                is_module: true,
                name: module.name.clone(),
                is_package: module.is_package,
                in_memory_bytecode_opt2: Some(PythonModuleBytecodeProvider::FromSource(
                    module.source.clone()
                )),
                ..PrePackagedResource::default()
            })
        );

        r.resources.clear();

        Ok(())
    }

    #[test]
    fn test_add_in_memory_package_resource() -> Result<()> {
        let mut r = PythonResourceCollector::new(
            vec![AbstractResourceLocation::InMemory],
            vec![],
            false,
            false,
        );
        r.add_python_package_resource(
            &PythonPackageResource {
                leaf_package: "foo".to_string(),
                relative_name: "resource.txt".to_string(),
                data: FileData::Memory(vec![42]),
                is_stdlib: false,
                is_test: false,
            },
            &ConcreteResourceLocation::InMemory,
        )?;

        assert_eq!(r.resources.len(), 1);
        assert_eq!(
            r.resources.get("foo"),
            Some(&PrePackagedResource {
                is_module: true,
                name: "foo".to_string(),
                is_package: true,
                in_memory_resources: Some(
                    [("resource.txt".to_string(), FileData::Memory(vec![42]))]
                        .iter()
                        .cloned()
                        .collect()
                ),
                ..PrePackagedResource::default()
            })
        );

        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let resources = r.compile_resources(&mut compiler)?;

        assert_eq!(resources.resources.len(), 1);
        assert_eq!(
            resources.resources.get("foo"),
            Some(&Resource {
                is_python_module: true,
                name: Cow::Owned("foo".to_string()),
                is_python_package: true,
                in_memory_package_resources: Some(
                    [(Cow::Owned("resource.txt".to_string()), Cow::Owned(vec![42]))]
                        .iter()
                        .cloned()
                        .collect()
                ),
                ..Resource::default()
            })
        );
        assert!(resources.extra_files.is_empty());

        Ok(())
    }

    #[test]
    fn test_add_relative_path_package_resource() -> Result<()> {
        let mut r = PythonResourceCollector::new(
            vec![AbstractResourceLocation::RelativePath],
            vec![],
            false,
            false,
        );
        r.add_python_package_resource(
            &PythonPackageResource {
                leaf_package: "foo".to_string(),
                relative_name: "resource.txt".to_string(),
                data: FileData::Memory(vec![42]),
                is_stdlib: false,
                is_test: false,
            },
            &ConcreteResourceLocation::RelativePath("prefix".to_string()),
        )?;

        assert_eq!(r.resources.len(), 1);
        assert_eq!(
            r.resources.get("foo"),
            Some(&PrePackagedResource {
                is_module: true,
                name: "foo".to_string(),
                is_package: true,
                relative_path_package_resources: Some(
                    [(
                        "resource.txt".to_string(),
                        (
                            PathBuf::from("prefix/foo/resource.txt"),
                            FileData::Memory(vec![42])
                        )
                    )]
                    .iter()
                    .cloned()
                    .collect()
                ),
                ..PrePackagedResource::default()
            })
        );

        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let resources = r.compile_resources(&mut compiler)?;

        assert_eq!(resources.resources.len(), 1);
        assert_eq!(
            resources.resources.get("foo"),
            Some(&Resource {
                is_python_module: true,
                name: Cow::Owned("foo".to_string()),
                is_python_package: true,
                relative_path_package_resources: Some(
                    [(
                        Cow::Owned("resource.txt".to_string()),
                        Cow::Owned(PathBuf::from("prefix/foo/resource.txt")),
                    )]
                    .iter()
                    .cloned()
                    .collect()
                ),
                ..Resource::default()
            })
        );
        assert_eq!(
            resources.extra_files,
            vec![(
                PathBuf::from("prefix/foo/resource.txt"),
                FileData::Memory(vec![42]),
                false
            ),]
        );

        Ok(())
    }

    #[test]
    fn test_add_package_resource_with_context() -> Result<()> {
        let mut r = PythonResourceCollector::new(
            vec![AbstractResourceLocation::InMemory],
            vec![],
            false,
            false,
        );

        let resource = PythonPackageResource {
            leaf_package: "foo".to_string(),
            relative_name: "bar.txt".to_string(),
            data: FileData::Memory(vec![42]),
            is_stdlib: false,
            is_test: false,
        };

        let mut add_context = PythonResourceAddCollectionContext {
            include: false,
            location: ConcreteResourceLocation::InMemory,
            location_fallback: None,
            store_source: false,
            optimize_level_zero: false,
            optimize_level_one: false,
            optimize_level_two: false,
        };

        // include=false is a noop.
        assert!(r.resources.is_empty());
        r.add_python_package_resource_with_context(&resource, &add_context)?;
        assert!(r.resources.is_empty());

        // include=true adds the resource.
        add_context.include = true;
        r.add_python_package_resource_with_context(&resource, &add_context)?;
        assert_eq!(
            r.resources.get(&resource.leaf_package),
            Some(&PrePackagedResource {
                is_module: true,
                name: resource.leaf_package.clone(),
                is_package: true,
                in_memory_resources: Some(
                    [(resource.relative_name.clone(), resource.data.clone())]
                        .iter()
                        .cloned()
                        .collect()
                ),
                ..PrePackagedResource::default()
            })
        );

        r.resources.clear();

        // location_fallback works.
        r.allowed_locations = vec![AbstractResourceLocation::RelativePath];
        add_context.location_fallback =
            Some(ConcreteResourceLocation::RelativePath("prefix".to_string()));
        r.add_python_package_resource_with_context(&resource, &add_context)?;
        assert_eq!(
            r.resources.get(&resource.leaf_package),
            Some(&PrePackagedResource {
                is_module: true,
                name: resource.leaf_package.clone(),
                is_package: true,
                relative_path_package_resources: Some(
                    [(
                        resource.relative_name.clone(),
                        (
                            PathBuf::from("prefix")
                                .join(resource.leaf_package)
                                .join(resource.relative_name),
                            resource.data.clone()
                        )
                    )]
                    .iter()
                    .cloned()
                    .collect()
                ),
                ..PrePackagedResource::default()
            })
        );

        r.resources.clear();

        Ok(())
    }

    #[test]
    fn test_add_in_memory_package_distribution_resource() -> Result<()> {
        let mut r = PythonResourceCollector::new(
            vec![AbstractResourceLocation::InMemory],
            vec![],
            false,
            false,
        );
        r.add_python_package_distribution_resource(
            &PythonPackageDistributionResource {
                location: PythonPackageDistributionResourceFlavor::DistInfo,
                package: "mypackage".to_string(),
                version: "1.0".to_string(),
                name: "resource.txt".to_string(),
                data: FileData::Memory(vec![42]),
            },
            &ConcreteResourceLocation::InMemory,
        )?;

        assert_eq!(r.resources.len(), 1);
        assert_eq!(
            r.resources.get("mypackage"),
            Some(&PrePackagedResource {
                is_module: true,
                name: "mypackage".to_string(),
                is_package: true,
                in_memory_distribution_resources: Some(
                    [("resource.txt".to_string(), FileData::Memory(vec![42]))]
                        .iter()
                        .cloned()
                        .collect()
                ),
                ..PrePackagedResource::default()
            })
        );

        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let resources = r.compile_resources(&mut compiler)?;

        assert_eq!(resources.resources.len(), 1);
        assert_eq!(
            resources.resources.get("mypackage"),
            Some(&Resource {
                is_python_module: true,
                name: Cow::Owned("mypackage".to_string()),
                is_python_package: true,
                in_memory_distribution_resources: Some(
                    [(Cow::Owned("resource.txt".to_string()), Cow::Owned(vec![42]))]
                        .iter()
                        .cloned()
                        .collect()
                ),
                ..Resource::default()
            })
        );
        assert!(resources.extra_files.is_empty());

        Ok(())
    }

    #[test]
    fn test_add_relative_path_package_distribution_resource() -> Result<()> {
        let mut r = PythonResourceCollector::new(
            vec![AbstractResourceLocation::RelativePath],
            vec![],
            false,
            false,
        );
        r.add_python_package_distribution_resource(
            &PythonPackageDistributionResource {
                location: PythonPackageDistributionResourceFlavor::DistInfo,
                package: "mypackage".to_string(),
                version: "1.0".to_string(),
                name: "resource.txt".to_string(),
                data: FileData::Memory(vec![42]),
            },
            &ConcreteResourceLocation::RelativePath("prefix".to_string()),
        )?;

        assert_eq!(r.resources.len(), 1);
        assert_eq!(
            r.resources.get("mypackage"),
            Some(&PrePackagedResource {
                is_module: true,
                name: "mypackage".to_string(),
                is_package: true,
                relative_path_distribution_resources: Some(
                    [(
                        "resource.txt".to_string(),
                        (
                            PathBuf::from("prefix/mypackage-1.0.dist-info/resource.txt"),
                            FileData::Memory(vec![42])
                        )
                    )]
                    .iter()
                    .cloned()
                    .collect()
                ),
                ..PrePackagedResource::default()
            })
        );

        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let resources = r.compile_resources(&mut compiler)?;

        assert_eq!(resources.resources.len(), 1);
        assert_eq!(
            resources.resources.get("mypackage"),
            Some(&Resource {
                is_python_module: true,
                name: Cow::Owned("mypackage".to_string()),
                is_python_package: true,
                relative_path_distribution_resources: Some(
                    [(
                        Cow::Owned("resource.txt".to_string()),
                        Cow::Owned(PathBuf::from("prefix/mypackage-1.0.dist-info/resource.txt")),
                    )]
                    .iter()
                    .cloned()
                    .collect()
                ),
                ..Resource::default()
            })
        );
        assert_eq!(
            resources.extra_files,
            vec![(
                PathBuf::from("prefix/mypackage-1.0.dist-info/resource.txt"),
                FileData::Memory(vec![42]),
                false
            ),]
        );

        Ok(())
    }

    #[test]
    fn test_add_package_distribution_resource_with_context() -> Result<()> {
        let mut r = PythonResourceCollector::new(
            vec![AbstractResourceLocation::InMemory],
            vec![],
            false,
            false,
        );

        let resource = PythonPackageDistributionResource {
            location: PythonPackageDistributionResourceFlavor::DistInfo,
            package: "foo".to_string(),
            version: "1.0".to_string(),
            name: "resource.txt".to_string(),
            data: FileData::Memory(vec![42]),
        };

        let mut add_context = PythonResourceAddCollectionContext {
            include: false,
            location: ConcreteResourceLocation::InMemory,
            location_fallback: None,
            store_source: false,
            optimize_level_zero: false,
            optimize_level_one: false,
            optimize_level_two: false,
        };

        // include=false is a noop.
        assert!(r.resources.is_empty());
        r.add_python_package_distribution_resource_with_context(&resource, &add_context)?;
        assert!(r.resources.is_empty());

        // include=true adds the resource.
        add_context.include = true;
        r.add_python_package_distribution_resource_with_context(&resource, &add_context)?;
        assert_eq!(
            r.resources.get(&resource.package),
            Some(&PrePackagedResource {
                is_module: true,
                name: resource.package.clone(),
                is_package: true,
                in_memory_distribution_resources: Some(
                    [(resource.name.clone(), resource.data.clone())]
                        .iter()
                        .cloned()
                        .collect()
                ),
                ..PrePackagedResource::default()
            })
        );

        r.resources.clear();

        // location_fallback works.
        r.allowed_locations = vec![AbstractResourceLocation::RelativePath];
        add_context.location_fallback =
            Some(ConcreteResourceLocation::RelativePath("prefix".to_string()));
        r.add_python_package_distribution_resource_with_context(&resource, &add_context)?;
        assert_eq!(
            r.resources.get(&resource.package),
            Some(&PrePackagedResource {
                is_module: true,
                name: resource.package.clone(),
                is_package: true,
                relative_path_distribution_resources: Some(
                    [(
                        resource.name.clone(),
                        (resource.resolve_path("prefix"), resource.data.clone())
                    )]
                    .iter()
                    .cloned()
                    .collect()
                ),
                ..PrePackagedResource::default()
            })
        );

        r.resources.clear();

        Ok(())
    }

    #[test]
    fn test_add_builtin_python_extension_module() -> Result<()> {
        let mut c = PythonResourceCollector::new(
            vec![AbstractResourceLocation::InMemory],
            vec![AbstractResourceLocation::InMemory],
            false,
            false,
        );

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
            license: None,
        };

        c.add_builtin_python_extension_module(&em)?;
        assert_eq!(c.resources.len(), 1);
        assert_eq!(
            c.resources.get("_io"),
            Some(&PrePackagedResource {
                is_builtin_extension_module: true,
                name: "_io".to_string(),
                ..PrePackagedResource::default()
            })
        );

        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let resources = c.compile_resources(&mut compiler)?;

        assert_eq!(resources.resources.len(), 1);
        assert_eq!(
            resources.resources.get("_io"),
            Some(&Resource {
                is_python_builtin_extension_module: true,
                name: Cow::Owned("_io".to_string()),
                ..Resource::default()
            })
        );
        assert!(resources.extra_files.is_empty());

        Ok(())
    }

    #[test]
    fn test_add_in_memory_python_extension_module_shared_library() -> Result<()> {
        let em = PythonExtensionModule {
            name: "myext".to_string(),
            init_fn: Some("PyInit__myext".to_string()),
            extension_file_suffix: ".so".to_string(),
            shared_library: Some(FileData::Memory(vec![42])),
            object_file_data: vec![],
            is_package: false,
            link_libraries: vec![LibraryDependency {
                name: "foo".to_string(),
                static_library: None,
                static_filename: None,
                dynamic_library: Some(FileData::Memory(vec![40])),
                dynamic_filename: Some(PathBuf::from("libfoo.so")),
                framework: false,
                system: false,
            }],
            is_stdlib: false,
            builtin_default: false,
            required: false,
            variant: None,
            license: None,
        };

        let mut c = PythonResourceCollector::new(
            vec![AbstractResourceLocation::InMemory],
            vec![],
            false,
            false,
        );

        let res = c.add_python_extension_module(&em, &ConcreteResourceLocation::InMemory);
        assert!(res.is_err());
        assert_eq!(res.err().unwrap().to_string(), "cannot add extension module myext for in-memory import because in-memory loading is not supported/allowed");

        let mut c = PythonResourceCollector::new(
            vec![AbstractResourceLocation::InMemory],
            vec![AbstractResourceLocation::InMemory],
            false,
            false,
        );

        c.add_python_extension_module(&em, &ConcreteResourceLocation::InMemory)?;
        assert_eq!(c.resources.len(), 2);
        assert_eq!(
            c.resources.get("myext"),
            Some(&PrePackagedResource {
                is_extension_module: true,
                name: "myext".to_string(),
                in_memory_extension_module_shared_library: Some(FileData::Memory(vec![42])),
                shared_library_dependency_names: Some(vec!["foo".to_string()]),
                ..PrePackagedResource::default()
            })
        );
        assert_eq!(
            c.resources.get("foo"),
            Some(&PrePackagedResource {
                is_shared_library: true,
                name: "foo".to_string(),
                in_memory_shared_library: Some(FileData::Memory(vec![40])),
                ..PrePackagedResource::default()
            })
        );

        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let resources = c.compile_resources(&mut compiler)?;

        assert_eq!(resources.resources.len(), 2);
        assert_eq!(
            resources.resources.get("myext"),
            Some(&Resource {
                is_python_extension_module: true,
                name: Cow::Owned("myext".to_string()),
                in_memory_extension_module_shared_library: Some(Cow::Owned(vec![42])),
                shared_library_dependency_names: Some(vec![Cow::Owned("foo".to_string())]),
                ..Resource::default()
            })
        );
        assert_eq!(
            resources.resources.get("foo"),
            Some(&Resource {
                is_shared_library: true,
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
        let em = PythonExtensionModule {
            name: "foo.bar".to_string(),
            init_fn: None,
            extension_file_suffix: ".so".to_string(),
            shared_library: Some(FileData::Memory(vec![42])),
            object_file_data: vec![],
            is_package: false,
            link_libraries: vec![LibraryDependency {
                name: "mylib".to_string(),
                static_library: None,
                static_filename: None,
                dynamic_library: Some(FileData::Memory(vec![40])),
                dynamic_filename: Some(PathBuf::from("libmylib.so")),
                framework: false,
                system: false,
            }],
            is_stdlib: false,
            builtin_default: false,
            required: false,
            variant: None,
            license: None,
        };

        let mut c = PythonResourceCollector::new(
            vec![AbstractResourceLocation::RelativePath],
            vec![],
            false,
            false,
        );
        let res = c.add_python_extension_module(
            &em,
            &ConcreteResourceLocation::RelativePath("prefix".to_string()),
        );
        assert!(res.is_err());
        assert_eq!(res.err().unwrap().to_string(), "cannot add extension module foo.bar as a file because extension modules as files are not allowed");

        let mut c = PythonResourceCollector::new(
            vec![AbstractResourceLocation::RelativePath],
            vec![AbstractResourceLocation::RelativePath],
            false,
            false,
        );

        c.add_python_extension_module(
            &em,
            &ConcreteResourceLocation::RelativePath("prefix".to_string()),
        )?;
        assert_eq!(c.resources.len(), 2);
        assert_eq!(
            c.resources.get("foo.bar"),
            Some(&PrePackagedResource {
                is_extension_module: true,
                name: "foo.bar".to_string(),
                is_package: false,
                relative_path_extension_module_shared_library: Some((
                    PathBuf::from("prefix/foo/bar.so"),
                    FileData::Memory(vec![42])
                )),
                shared_library_dependency_names: Some(vec!["mylib".to_string()]),
                ..PrePackagedResource::default()
            })
        );
        assert_eq!(
            c.resources.get("mylib"),
            Some(&PrePackagedResource {
                is_shared_library: true,
                name: "mylib".to_string(),
                relative_path_shared_library: Some((
                    "prefix/foo".to_string(),
                    PathBuf::from("libmylib.so"),
                    FileData::Memory(vec![40])
                )),
                ..PrePackagedResource::default()
            })
        );

        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let resources = c.compile_resources(&mut compiler)?;

        assert_eq!(resources.resources.len(), 3);
        assert_eq!(
            resources.resources.get("foo"),
            Some(&Resource {
                is_python_module: true,
                name: Cow::Owned("foo".to_string()),
                is_python_package: true,
                ..Resource::default()
            })
        );
        assert_eq!(
            resources.resources.get("foo.bar"),
            Some(&Resource {
                is_python_extension_module: true,
                name: Cow::Owned("foo.bar".to_string()),
                is_python_package: false,
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
                is_shared_library: true,
                name: Cow::Owned("mylib".to_string()),
                ..Resource::default()
            })
        );

        assert_eq!(
            resources.extra_files,
            vec![
                (
                    PathBuf::from("prefix/foo/bar.so"),
                    FileData::Memory(vec![42]),
                    true
                ),
                (
                    PathBuf::from("prefix/foo/libmylib.so"),
                    FileData::Memory(vec![40]),
                    true
                )
            ]
        );

        Ok(())
    }

    #[test]
    fn test_add_shared_library_and_module() -> Result<()> {
        let mut r = PythonResourceCollector::new(
            vec![AbstractResourceLocation::InMemory],
            vec![],
            false,
            false,
        );

        r.add_python_module_source(
            &PythonModuleSource {
                name: "foo".to_string(),
                source: FileData::Memory(vec![1]),
                is_package: true,
                cache_tag: DEFAULT_CACHE_TAG.to_string(),
                is_stdlib: false,
                is_test: false,
            },
            &ConcreteResourceLocation::InMemory,
        )?;

        r.add_shared_library(
            &SharedLibrary {
                name: "foo".to_string(),
                data: FileData::Memory(vec![2]),
                filename: None,
            },
            &ConcreteResourceLocation::InMemory,
        )?;

        assert_eq!(r.resources.len(), 1);
        assert_eq!(
            r.resources.get("foo"),
            Some(&PrePackagedResource {
                is_module: true,
                is_shared_library: true,
                name: "foo".to_string(),
                is_package: true,
                in_memory_source: Some(FileData::Memory(vec![1])),
                in_memory_shared_library: Some(FileData::Memory(vec![2])),
                ..PrePackagedResource::default()
            })
        );

        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let resources = r.compile_resources(&mut compiler)?;

        assert_eq!(resources.resources.len(), 1);
        assert_eq!(
            resources.resources.get("foo"),
            Some(&Resource {
                is_python_module: true,
                is_shared_library: true,
                name: Cow::Owned("foo".to_string()),
                is_python_package: true,
                in_memory_source: Some(Cow::Owned(vec![1])),
                in_memory_shared_library: Some(Cow::Owned(vec![2])),
                ..Resource::default()
            })
        );

        Ok(())
    }

    #[test]
    fn test_add_in_memory_file_data() -> Result<()> {
        let mut r = PythonResourceCollector::new(
            vec![AbstractResourceLocation::InMemory],
            vec![],
            false,
            false,
        );
        assert!(r
            .add_file_data(
                &File::new("foo/bar.py", vec![42]),
                &ConcreteResourceLocation::InMemory,
            )
            .is_err());

        r.allow_files = true;
        r.add_file_data(
            &File::new("foo/bar.py", vec![42]),
            &ConcreteResourceLocation::InMemory,
        )?;

        assert!(r.resources.contains_key("foo/bar.py"));
        assert_eq!(
            r.resources.get("foo/bar.py"),
            Some(&PrePackagedResource {
                is_utf8_filename_data: true,
                name: "foo/bar.py".to_string(),
                file_data_embedded: Some(FileData::Memory(vec![42])),
                ..PrePackagedResource::default()
            })
        );

        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let resources = r.compile_resources(&mut compiler)?;

        assert_eq!(resources.resources.len(), 1);
        assert_eq!(
            resources.resources.get("foo/bar.py"),
            Some(&Resource {
                is_utf8_filename_data: true,
                name: Cow::Owned("foo/bar.py".to_string()),
                file_data_embedded: Some(Cow::Owned(vec![42])),
                ..Resource::default()
            })
        );
        assert!(resources.extra_files.is_empty());

        Ok(())
    }

    #[test]
    fn test_add_relative_path_file_data() -> Result<()> {
        let mut r = PythonResourceCollector::new(
            vec![AbstractResourceLocation::RelativePath],
            vec![],
            false,
            true,
        );
        r.add_file_data(
            &File::new("foo/bar.py", vec![42]),
            &ConcreteResourceLocation::RelativePath("prefix".to_string()),
        )?;

        assert!(r.resources.contains_key("foo/bar.py"));
        assert_eq!(
            r.resources.get("foo/bar.py"),
            Some(&PrePackagedResource {
                is_utf8_filename_data: true,
                name: "foo/bar.py".to_string(),
                file_data_utf8_relative_path: Some((
                    PathBuf::from("prefix/foo/bar.py"),
                    FileData::Memory(vec![42])
                )),
                ..PrePackagedResource::default()
            })
        );

        let mut compiler = FakeBytecodeCompiler { magic_number: 42 };

        let resources = r.compile_resources(&mut compiler)?;

        assert_eq!(resources.resources.len(), 1);
        assert_eq!(
            resources.resources.get("foo/bar.py"),
            Some(&Resource {
                is_utf8_filename_data: true,
                name: Cow::Owned("foo/bar.py".to_string()),
                file_data_utf8_relative_path: Some(Cow::Owned("prefix/foo/bar.py".to_string())),
                ..Resource::default()
            })
        );
        assert_eq!(
            resources.extra_files,
            vec![(
                PathBuf::from("prefix/foo/bar.py"),
                FileData::Memory(vec![42]),
                false
            )]
        );

        Ok(())
    }

    #[test]
    fn test_add_file_data_with_context() -> Result<()> {
        let mut r = PythonResourceCollector::new(
            vec![AbstractResourceLocation::InMemory],
            vec![],
            false,
            true,
        );

        let file = File::new("foo/bar.py", FileEntry::new_from_data(vec![42], true));

        let mut add_context = PythonResourceAddCollectionContext {
            include: false,
            location: ConcreteResourceLocation::InMemory,
            location_fallback: None,
            store_source: false,
            optimize_level_zero: false,
            optimize_level_one: false,
            optimize_level_two: false,
        };

        // include=false is a noop.
        assert!(r.resources.is_empty());
        r.add_file_data_with_context(&file, &add_context)?;
        assert!(r.resources.is_empty());

        // include=true adds the resource.
        add_context.include = true;
        r.add_file_data_with_context(&file, &add_context)?;
        assert_eq!(
            r.resources.get(&file.path_string()),
            Some(&PrePackagedResource {
                name: file.path_string(),
                is_utf8_filename_data: true,
                file_executable: true,
                file_data_embedded: Some(file.entry().file_data().clone()),
                ..PrePackagedResource::default()
            })
        );
        r.resources.clear();

        // location_fallback works.
        r.allowed_locations = vec![AbstractResourceLocation::RelativePath];
        add_context.location_fallback =
            Some(ConcreteResourceLocation::RelativePath("prefix".to_string()));
        r.add_file_data_with_context(&file, &add_context)?;
        assert_eq!(
            r.resources.get(&file.path_string()),
            Some(&PrePackagedResource {
                name: file.path_string(),
                is_utf8_filename_data: true,
                file_executable: true,
                file_data_utf8_relative_path: Some((
                    PathBuf::from("prefix").join(file.path_string()),
                    file.entry().file_data().clone()
                )),
                ..PrePackagedResource::default()
            })
        );

        Ok(())
    }

    #[test]
    fn test_find_dunder_file() -> Result<()> {
        let mut r = PythonResourceCollector::new(
            vec![AbstractResourceLocation::InMemory],
            vec![],
            false,
            false,
        );
        assert_eq!(r.find_dunder_file()?.len(), 0);

        r.add_python_module_source(
            &PythonModuleSource {
                name: "foo.bar".to_string(),
                source: FileData::Memory(vec![]),
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
                source: FileData::Memory(Vec::from("import foo; if __file__ == 'ignored'")),
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
                source: FileData::Memory(Vec::from("import foo; if __file__")),
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
