// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Functionality for defining how Python resources should be packaged.
*/

use {
    crate::{
        licensing::SAFE_SYSTEM_LIBRARIES,
        location::ConcreteResourceLocation,
        resource::{PythonExtensionModule, PythonExtensionModuleVariants, PythonResource},
        resource_collection::PythonResourceAddCollectionContext,
    },
    anyhow::Result,
    std::{collections::HashMap, convert::TryFrom},
    tugger_licensing::LicenseFlavor,
};

/// Denotes methods to filter extension modules.
#[derive(Clone, Debug, PartialEq)]
pub enum ExtensionModuleFilter {
    /// Only use the minimum set of extension modules needed to initialize an interpreter.
    Minimal,
    /// Use all extension modules.
    All,
    /// Only use extension modules without library dependencies.
    NoLibraries,
    NoCopyleft,
}

impl TryFrom<&str> for ExtensionModuleFilter {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "minimal" => Ok(ExtensionModuleFilter::Minimal),
            "all" => Ok(ExtensionModuleFilter::All),
            "no-libraries" => Ok(ExtensionModuleFilter::NoLibraries),
            "no-copyleft" => Ok(ExtensionModuleFilter::NoCopyleft),
            t => Err(format!("{} is not a valid extension module filter", t)),
        }
    }
}

impl AsRef<str> for ExtensionModuleFilter {
    fn as_ref(&self) -> &str {
        match self {
            ExtensionModuleFilter::All => "all",
            ExtensionModuleFilter::Minimal => "minimal",
            ExtensionModuleFilter::NoCopyleft => "no-copyleft",
            ExtensionModuleFilter::NoLibraries => "no-libraries",
        }
    }
}

/// Describes how resources should be handled.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ResourceHandlingMode {
    /// Files should be classified as typed resources.
    Classify,

    /// Files should be handled as files.
    Files,
}

impl TryFrom<&str> for ResourceHandlingMode {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "classify" => Ok(Self::Classify),
            "files" => Ok(Self::Files),
            _ => Err(format!(
                "{} is not a valid resource handling mode; use \"classify\" or \"files\"",
                value
            )),
        }
    }
}

impl AsRef<str> for ResourceHandlingMode {
    fn as_ref(&self) -> &str {
        match self {
            Self::Classify => "classify",
            Self::Files => "files",
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

    /// Where resources should be placed/loaded from by default.
    resources_location: ConcreteResourceLocation,

    /// Optional fallback location for resources should `resources_location` fail.
    resources_location_fallback: Option<ConcreteResourceLocation>,

    /// Whether to allow in-memory shared library loading.
    ///
    /// If true, we will attempt to load Python extension modules
    /// and their shared library dependencies from memory if supported.
    ///
    /// This feature is not supported on all platforms and this setting
    /// can get overrules by platform-specific capabilities.
    allow_in_memory_shared_library_loading: bool,

    /// Whether untyped files are allowed.
    ///
    /// If true, `File` instances can be added to the resource collector.
    ///
    /// If false, resources must be strongly typed (`PythonModuleSource`,
    /// `PythonPackageResource`, etc).
    allow_files: bool,

    /// Whether file scanning should emit `PythonResource::File` variants.
    ///
    /// If true, this resource variant is emitted when scanning for
    /// resources. If false, it isn't.
    ///
    /// This effectively says whether the file scanner should emit records
    /// corresponding to the actual file.
    file_scanner_emit_files: bool,

    /// Whether file scanning should classify files and emit `PythonResource::*`
    /// variants.
    ///
    /// If true, the file scanner will attempt to classify every file as
    /// a specific resource type and emit a `PythonResource::*` variant
    /// corresponding to the resource type.
    ///
    /// If false, this classification is not performed.
    file_scanner_classify_files: bool,

    /// Whether to classify non-`File` resources as `include = True` by default.
    include_classified_resources: bool,

    /// Whether to include source module from the Python distribution.
    include_distribution_sources: bool,

    /// Whether to include Python module source for non-distribution modules.
    include_non_distribution_sources: bool,

    /// Whether to include package resource files.
    include_distribution_resources: bool,

    /// Whether to include test files.
    include_test: bool,

    /// Whether to classify `File` resources as `include = True` by default.
    include_file_resources: bool,

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
            resources_location: ConcreteResourceLocation::InMemory,
            resources_location_fallback: None,
            allow_in_memory_shared_library_loading: false,
            allow_files: false,
            file_scanner_emit_files: false,
            file_scanner_classify_files: true,
            include_classified_resources: true,
            include_distribution_sources: true,
            include_non_distribution_sources: true,
            include_distribution_resources: false,
            include_test: false,
            include_file_resources: false,
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

    /// Obtain the primary location for added resources.
    pub fn resources_location(&self) -> &ConcreteResourceLocation {
        &self.resources_location
    }

    /// Set the primary location for added resources.
    pub fn set_resources_location(&mut self, location: ConcreteResourceLocation) {
        self.resources_location = location;
    }

    /// Obtain the fallback location for added resources.
    pub fn resources_location_fallback(&self) -> &Option<ConcreteResourceLocation> {
        &self.resources_location_fallback
    }

    /// Set the fallback location for added resources.
    pub fn set_resources_location_fallback(&mut self, location: Option<ConcreteResourceLocation>) {
        self.resources_location_fallback = location;
    }

    /// Whether to allow untyped `File` resources.
    pub fn allow_files(&self) -> bool {
        self.allow_files
    }

    /// Set whether to allow untyped `File` resources.
    pub fn set_allow_files(&mut self, value: bool) {
        self.allow_files = value;
    }

    /// Whether file scanning should emit `PythonResource::File` variants.
    pub fn file_scanner_emit_files(&self) -> bool {
        self.file_scanner_emit_files
    }

    /// Set whether file scanning should emit `PythonResource::File` variants.
    pub fn set_file_scanner_emit_files(&mut self, value: bool) {
        self.file_scanner_emit_files = value;
    }

    /// Whether file scanning should classify files into `PythonResource::*` variants.
    pub fn file_scanner_classify_files(&self) -> bool {
        self.file_scanner_classify_files
    }

    /// Set whether file scanning should classify files into `PythonResource::*` variants.
    pub fn set_file_scanner_classify_files(&mut self, value: bool) {
        self.file_scanner_classify_files = value;
    }

    /// Whether to allow in-memory shared library loading.
    pub fn allow_in_memory_shared_library_loading(&self) -> bool {
        self.allow_in_memory_shared_library_loading
    }

    /// Set the value for whether to allow in-memory shared library loading.
    pub fn set_allow_in_memory_shared_library_loading(&mut self, value: bool) {
        self.allow_in_memory_shared_library_loading = value;
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

    /// Get whether to classify `File` resources as include by default.
    pub fn include_file_resources(&self) -> bool {
        self.include_file_resources
    }

    /// Set whether to classify `File` resources as include by default.
    pub fn set_include_file_resources(&mut self, value: bool) {
        self.include_file_resources = value;
    }

    /// Get whether to classify non-`File` resources as include by default.
    pub fn include_classified_resources(&self) -> bool {
        self.include_classified_resources
    }

    /// Set whether to classify non-`File` resources as include by default.
    pub fn set_include_classified_resources(&mut self, value: bool) {
        self.include_classified_resources = value;
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

    /// Set the resource handling mode of the policy.
    ///
    /// This is a convenience function for mapping a `ResourceHandlingMode`
    /// to corresponding field values.
    pub fn set_resource_handling_mode(&mut self, mode: ResourceHandlingMode) {
        match mode {
            ResourceHandlingMode::Classify => {
                self.file_scanner_emit_files = false;
                self.file_scanner_classify_files = true;
                self.allow_files = false;
                self.include_file_resources = false;
                self.include_classified_resources = true;
            }
            ResourceHandlingMode::Files => {
                self.file_scanner_emit_files = true;
                self.file_scanner_classify_files = false;
                self.allow_files = true;
                self.include_file_resources = true;
                self.include_classified_resources = true;
            }
        }
    }

    /// Obtain broken extensions for a target triple.
    pub fn broken_extensions_for_triple(&self, target_triple: &str) -> Option<&Vec<String>> {
        self.broken_extensions.get(target_triple)
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

        let location = self.resources_location.clone();
        let location_fallback = self.resources_location_fallback.clone();

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
            PythonResource::File(_) => {
                if !self.include_file_resources {
                    return false;
                }
            }
            _ => {
                if !self.include_classified_resources {
                    return false;
                }
            }
        }

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
                if resource.is_stdlib {
                    if self.include_distribution_resources {
                        self.include_test || !resource.is_test
                    } else {
                        false
                    }
                } else {
                    true
                }
            }
            PythonResource::PackageDistributionResource(_) => true,
            PythonResource::ExtensionModule(_) => false,
            PythonResource::PathExtension(_) => false,
            PythonResource::EggFile(_) => false,
            PythonResource::File(_) => true,
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
            let ext_variants: PythonExtensionModuleVariants = variants
                .iter()
                .filter_map(|em| {
                    if em.is_minimally_required() {
                        Some(em.clone())
                    } else {
                        None
                    }
                })
                .collect();

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
                    let ext_variants: PythonExtensionModuleVariants = variants
                        .iter()
                        .filter_map(|em| {
                            if !em.requires_libraries() {
                                Some(em.clone())
                            } else {
                                None
                            }
                        })
                        .collect();

                    if !ext_variants.is_empty() {
                        res.push(
                            ext_variants
                                .choose_variant(&self.preferred_extension_module_variants)
                                .clone(),
                        );
                    }
                }

                ExtensionModuleFilter::NoCopyleft => {
                    let ext_variants: PythonExtensionModuleVariants = variants
                        .iter()
                        .filter_map(|em| {
                            // As a special case, if all we link against are system libraries
                            // that are known to be benign, allow that.
                            let all_safe_system_libraries = em.link_libraries.iter().all(|link| {
                                link.system && SAFE_SYSTEM_LIBRARIES.contains(&link.name.as_str())
                            });

                            if em.link_libraries.is_empty() || all_safe_system_libraries {
                                Some(em.clone())
                            } else if let Some(license) = &em.license {
                                match license.license() {
                                    LicenseFlavor::Spdx(expression) => {
                                        let copyleft = expression.evaluate(|req| {
                                            if let Some(id) = req.license.id() {
                                                id.is_copyleft()
                                            } else {
                                                true
                                            }
                                        });

                                        if !copyleft {
                                            Some(em.clone())
                                        } else {
                                            None
                                        }
                                    }
                                    LicenseFlavor::OtherExpression(_) => None,
                                    LicenseFlavor::PublicDomain => Some(em.clone()),
                                    LicenseFlavor::None => None,
                                    LicenseFlavor::Unknown(_) => None,
                                }
                            } else {
                                None
                            }
                        })
                        .collect();

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

#[cfg(test)]
mod tests {
    use {
        super::*,
        std::path::PathBuf,
        tugger_file_manifest::{File, FileEntry},
    };

    #[test]
    fn test_add_collection_context_file() -> Result<()> {
        let mut policy = PythonPackagingPolicy::default();
        policy.include_file_resources = false;

        let file = File {
            path: PathBuf::from("foo.py"),
            entry: FileEntry {
                executable: false,
                data: vec![42].into(),
            },
        };

        let add_context = policy.derive_add_collection_context(&file.clone().into());
        assert!(!add_context.include);

        policy.include_file_resources = true;
        let add_context = policy.derive_add_collection_context(&file.into());
        assert!(add_context.include);

        Ok(())
    }
}
