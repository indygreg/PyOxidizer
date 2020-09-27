// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Functionality for defining how Python resources should be packaged.
*/

use {
    crate::{
        licensing::NON_GPL_LICENSES,
        resource::{PythonExtensionModule, PythonExtensionModuleVariants, PythonResource},
        resource_collection::{ConcreteResourceLocation, PythonResourceAddCollectionContext},
    },
    anyhow::{anyhow, Result},
    std::{collections::HashMap, convert::TryFrom, iter::FromIterator},
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

/// Denotes methods to filter extension modules.
#[derive(Clone, Debug, PartialEq)]
pub enum ExtensionModuleFilter {
    Minimal,
    All,
    NoLibraries,
    NoGPL,
}

impl TryFrom<&str> for ExtensionModuleFilter {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "minimal" => Ok(ExtensionModuleFilter::Minimal),
            "all" => Ok(ExtensionModuleFilter::All),
            "no-libraries" => Ok(ExtensionModuleFilter::NoLibraries),
            "no-gpl" => Ok(ExtensionModuleFilter::NoGPL),
            t => Err(format!("{} is not a valid extension module filter", t)),
        }
    }
}

impl AsRef<str> for ExtensionModuleFilter {
    fn as_ref(&self) -> &str {
        match self {
            ExtensionModuleFilter::All => "all",
            ExtensionModuleFilter::Minimal => "minimal",
            ExtensionModuleFilter::NoGPL => "no-gpl",
            ExtensionModuleFilter::NoLibraries => "no-libraries",
        }
    }
}

/// Defines how Python resources should be packaged.
#[derive(Clone, Debug, PartialEq)]
pub struct PythonPackagingPolicy {
    /// Which extension modules should be included.
    extension_module_filter: ExtensionModuleFilter,

    /// Preferred variants of extension modules.
    preferred_extension_module_variants: HashMap<String, String>,

    /// Where resources should be packaged by default.
    resources_policy: PythonResourcesPolicy,

    /// Whether to include source module from the Python distribution.
    include_distribution_sources: bool,

    /// Whether to include Python module source for non-distribution modules.
    include_non_distribution_sources: bool,

    /// Whether to include package resource files.
    include_distribution_resources: bool,

    /// Whether to include test files.
    include_test: bool,

    /// Mapping of target triple to list of extensions that don't work for that triple.
    ///
    /// Policy constructors can populate this with known broken extensions to
    /// prevent the policy from allowing an extension.
    broken_extensions: HashMap<String, Vec<String>>,

    /// Whether to write Python bytecode at optimization level 0.
    bytecode_optimize_level_zero: bool,

    /// Whether to write Python bytecode at optimization level 1.
    bytecode_optimize_level_one: bool,

    /// Whether to write Python bytecode at optimization level 2.
    bytecode_optimize_level_two: bool,
}

impl Default for PythonPackagingPolicy {
    fn default() -> Self {
        PythonPackagingPolicy {
            extension_module_filter: ExtensionModuleFilter::All,
            preferred_extension_module_variants: HashMap::new(),
            resources_policy: PythonResourcesPolicy::InMemoryOnly,
            include_distribution_sources: true,
            include_non_distribution_sources: true,
            include_distribution_resources: false,
            include_test: false,
            broken_extensions: HashMap::new(),
            bytecode_optimize_level_zero: true,
            bytecode_optimize_level_one: false,
            bytecode_optimize_level_two: false,
        }
    }
}

impl PythonPackagingPolicy {
    /// Obtain the active extension module filter for this instance.
    pub fn extension_module_filter(&self) -> &ExtensionModuleFilter {
        &self.extension_module_filter
    }

    /// Set the extension module filter to use.
    pub fn set_extension_module_filter(&mut self, filter: ExtensionModuleFilter) {
        self.extension_module_filter = filter;
    }

    /// Obtain the preferred extension module variants for this policy.
    ///
    /// The returned object is a mapping of extension name to its variant
    /// name.
    pub fn preferred_extension_module_variants(&self) -> &HashMap<String, String> {
        &self.preferred_extension_module_variants
    }

    /// Denote the preferred variant for an extension module.
    ///
    /// If set, the named variant will be chosen if it is present.
    pub fn set_preferred_extension_module_variant(&mut self, extension: &str, variant: &str) {
        self.preferred_extension_module_variants
            .insert(extension.to_string(), variant.to_string());
    }

    /// Obtain the active resources policy for this instance.
    pub fn resources_policy(&self) -> &PythonResourcesPolicy {
        &self.resources_policy
    }

    /// Set the resource loading policy.
    pub fn set_resources_policy(&mut self, policy: PythonResourcesPolicy) {
        self.resources_policy = policy;
    }

    /// Get setting for whether to include source modules from the distribution.
    pub fn include_distribution_sources(&self) -> bool {
        self.include_distribution_sources
    }

    /// Set whether we should include a Python distribution's module source code.
    pub fn set_include_distribution_sources(&mut self, include: bool) {
        self.include_distribution_sources = include;
    }

    /// Get setting for whether to include Python package resources from the distribution.
    pub fn include_distribution_resources(&self) -> bool {
        self.include_distribution_resources
    }

    /// Set whether to include package resources from the Python distribution.
    pub fn set_include_distribution_resources(&mut self, include: bool) {
        self.include_distribution_resources = include;
    }

    /// Whether to include Python sources for modules not in the standard library.
    pub fn include_non_distribution_sources(&self) -> bool {
        self.include_non_distribution_sources
    }

    /// Set whether to include Python sources for modules not in the standard library.
    pub fn set_include_non_distribution_sources(&mut self, include: bool) {
        self.include_non_distribution_sources = include;
    }

    /// Get setting for whether to include test files.
    pub fn include_test(&self) -> bool {
        self.include_test
    }

    /// Set whether we should include Python modules that define tests.
    pub fn set_include_test(&mut self, include: bool) {
        self.include_test = include;
    }

    /// Whether to write bytecode at optimization level 0.
    pub fn bytecode_optimize_level_zero(&self) -> bool {
        self.bytecode_optimize_level_zero
    }

    /// Set whether to write bytecode at optimization level 0.
    pub fn set_bytecode_optimize_level_zero(&mut self, value: bool) {
        self.bytecode_optimize_level_zero = value;
    }

    /// Whether to write bytecode at optimization level 1.
    pub fn bytecode_optimize_level_one(&self) -> bool {
        self.bytecode_optimize_level_one
    }

    /// Set whether to write bytecode at optimization level 1.
    pub fn set_bytecode_optimize_level_one(&mut self, value: bool) {
        self.bytecode_optimize_level_one = value;
    }

    /// Whether to write bytecode at optimization level 2.
    pub fn bytecode_optimize_level_two(&self) -> bool {
        self.bytecode_optimize_level_two
    }

    /// Set whether to write bytecode at optimization level 2.
    pub fn set_bytecode_optimize_level_two(&mut self, value: bool) {
        self.bytecode_optimize_level_two = value;
    }

    /// Mark an extension as broken on a target platform, preventing it from being used.
    pub fn register_broken_extension(&mut self, target_triple: &str, extension: &str) {
        if !self.broken_extensions.contains_key(target_triple) {
            self.broken_extensions
                .insert(target_triple.to_string(), vec![]);
        }

        self.broken_extensions
            .get_mut(target_triple)
            .unwrap()
            .push(extension.to_string());
    }

    /// Derive a `PythonResourceAddCollectionContext` for a resource using current settings.
    ///
    /// The returned object essentially says how the resource should be added
    /// to a `PythonResourceCollector` given this policy.
    pub fn derive_add_collection_context(
        &self,
        resource: &PythonResource,
    ) -> PythonResourceAddCollectionContext {
        let include = self.filter_python_resource(resource);

        let store_source = match resource {
            PythonResource::ModuleSource(ref module) => {
                if module.is_stdlib {
                    self.include_distribution_sources
                } else {
                    self.include_non_distribution_sources
                }
            }
            _ => false,
        };

        let (location, location_fallback) = match self.resources_policy {
            PythonResourcesPolicy::InMemoryOnly => (ConcreteResourceLocation::InMemory, None),
            PythonResourcesPolicy::FilesystemRelativeOnly(ref prefix) => {
                (ConcreteResourceLocation::RelativePath(prefix.clone()), None)
            }
            PythonResourcesPolicy::PreferInMemoryFallbackFilesystemRelative(ref prefix) => (
                ConcreteResourceLocation::InMemory,
                Some(ConcreteResourceLocation::RelativePath(prefix.clone())),
            ),
        };

        PythonResourceAddCollectionContext {
            include,
            location,
            location_fallback,
            store_source,
            optimize_level_zero: self.bytecode_optimize_level_zero,
            optimize_level_one: self.bytecode_optimize_level_one,
            optimize_level_two: self.bytecode_optimize_level_two,
        }
    }

    /// Determine if a Python resource is applicable to the current policy.
    ///
    /// Given a `PythonResource`, this answers the question of whether that
    /// resource meets the inclusion requirements for the current policy.
    ///
    /// Returns true if the resource should be included, false otherwise.
    fn filter_python_resource(&self, resource: &PythonResource) -> bool {
        match resource {
            PythonResource::ModuleSource(module) => {
                if !self.include_test && module.is_test {
                    false
                } else {
                    self.include_distribution_sources
                }
            }
            PythonResource::ModuleBytecodeRequest(module) => self.include_test || !module.is_test,
            PythonResource::ModuleBytecode(_) => false,
            PythonResource::PackageResource(resource) => {
                if self.include_distribution_resources {
                    self.include_test || !resource.is_test
                } else {
                    false
                }
            }
            PythonResource::PackageDistributionResource(_) => false,
            PythonResource::ExtensionModule(_) => false,
            PythonResource::PathExtension(_) => false,
            PythonResource::EggFile(_) => false,
        }
    }

    /// Resolve Python extension modules that are compliant with the policy.
    #[allow(clippy::if_same_then_else)]
    pub fn resolve_python_extension_modules<'a>(
        &self,
        extensions_variants: impl Iterator<Item = &'a PythonExtensionModuleVariants>,
        target_triple: &str,
    ) -> Result<Vec<PythonExtensionModule>> {
        let mut res = vec![];

        for variants in extensions_variants {
            let name = &variants.default_variant().name;

            // This extension is broken on this target. Ignore it.
            if self
                .broken_extensions
                .get(target_triple)
                .unwrap_or(&Vec::new())
                .contains(name)
            {
                continue;
            }

            // Always add minimally required extension modules, because things don't
            // work if we don't do this.
            let ext_variants =
                PythonExtensionModuleVariants::from_iter(variants.iter().filter_map(|em| {
                    if em.is_minimally_required() {
                        Some(em.clone())
                    } else {
                        None
                    }
                }));

            if !ext_variants.is_empty() {
                res.push(
                    ext_variants
                        .choose_variant(&self.preferred_extension_module_variants)
                        .clone(),
                );
            }

            match self.extension_module_filter {
                // Nothing to do here since we added minimal extensions above.
                ExtensionModuleFilter::Minimal => {}

                ExtensionModuleFilter::All => {
                    res.push(
                        variants
                            .choose_variant(&self.preferred_extension_module_variants)
                            .clone(),
                    );
                }

                ExtensionModuleFilter::NoLibraries => {
                    let ext_variants = PythonExtensionModuleVariants::from_iter(
                        variants.iter().filter_map(|em| {
                            if !em.requires_libraries() {
                                Some(em.clone())
                            } else {
                                None
                            }
                        }),
                    );

                    if !ext_variants.is_empty() {
                        res.push(
                            ext_variants
                                .choose_variant(&self.preferred_extension_module_variants)
                                .clone(),
                        );
                    }
                }

                ExtensionModuleFilter::NoGPL => {
                    let ext_variants = PythonExtensionModuleVariants::from_iter(
                        variants.iter().filter_map(|em| {
                            if em.link_libraries.is_empty() {
                                Some(em.clone())
                            // Public domain is always allowed.
                            } else if em.license_public_domain == Some(true) {
                                Some(em.clone())
                            // Use explicit license list if one is defined.
                            } else if let Some(ref licenses) = em.licenses {
                                // We filter through an allow list because it is safer. (No new GPL
                                // licenses can slip through.)
                                if licenses
                                    .iter()
                                    .all(|license| NON_GPL_LICENSES.contains(&license.as_str()))
                                {
                                    Some(em.clone())
                                } else {
                                    None
                                }
                            } else {
                                // In lack of evidence that it isn't GPL, assume GPL.
                                // TODO consider improving logic here, like allowing known system
                                // and framework libraries to be used.
                                None
                            }
                        }),
                    );

                    if !ext_variants.is_empty() {
                        res.push(
                            ext_variants
                                .choose_variant(&self.preferred_extension_module_variants)
                                .clone(),
                        );
                    }
                }
            }
        }

        Ok(res)
    }
}
