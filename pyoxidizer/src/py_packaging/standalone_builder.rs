// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    super::{
        binary::{
            EmbeddedPythonContext, LibpythonLinkMode, PythonBinaryBuilder, PythonLinkingInfo,
        },
        config::{EmbeddedPythonConfig, RawAllocator},
        distribution::{BinaryLibpythonLinkMode, PythonDistribution},
        filtering::{filter_btreemap, resolve_resource_names_from_files},
        libpython::{link_libpython, LibPythonBuildContext},
        packaging_tool::{find_resources, pip_install, read_virtualenv, setup_py_install},
        standalone_distribution::StandaloneDistribution,
    },
    crate::app_packaging::resource::{FileContent, FileManifest},
    anyhow::{anyhow, Result},
    lazy_static::lazy_static,
    python_packaging::{
        bytecode::BytecodeCompiler,
        policy::{PythonPackagingPolicy, PythonResourcesPolicy},
        resource::{
            DataLocation, PythonExtensionModule, PythonModuleSource,
            PythonPackageDistributionResource, PythonPackageResource, PythonResource,
        },
        resource_collection::{
            ConcreteResourceLocation, PrePackagedResource, PythonResourceAddCollectionContext,
            PythonResourceCollector,
        },
    },
    slog::warn,
    std::{
        collections::{BTreeMap, HashMap},
        io::Write,
        path::{Path, PathBuf},
        sync::Arc,
    },
    tempdir::TempDir,
};

lazy_static! {
    /// Libraries that we should not link against on Linux.
    static ref LINUX_IGNORE_LIBRARIES: Vec<&'static str> = vec!["dl", "m",];

    /// Libraries that we should not link against on macOS.
    static ref MACOS_IGNORE_LIBRARIES: Vec<&'static str> = vec!["dl", "m",];
}

/// Obtain a list of ignored libraries for a given target triple.
fn ignored_libraries_for_target(target_triple: &str) -> Vec<&'static str> {
    if crate::environment::LINUX_TARGET_TRIPLES.contains(&target_triple) {
        LINUX_IGNORE_LIBRARIES.clone()
    } else if crate::environment::MACOS_TARGET_TRIPLES.contains(&target_triple) {
        MACOS_IGNORE_LIBRARIES.clone()
    } else {
        vec![]
    }
}

/// A self-contained Python executable before it is compiled.
#[derive(Clone, Debug)]
pub struct StandalonePythonExecutableBuilder {
    /// The target triple we are running on.
    host_triple: String,

    /// The target triple we are building for.
    target_triple: String,

    /// The name of the executable to build.
    exe_name: String,

    /// The Python distribution being used to build this executable.
    distribution: Arc<Box<StandaloneDistribution>>,

    /// How libpython should be linked.
    link_mode: LibpythonLinkMode,

    /// Whether the built binary is capable of loading dynamically linked
    /// extension modules from memory.
    supports_in_memory_dynamically_linked_extension_loading: bool,

    /// Policy to apply to added resources.
    packaging_policy: PythonPackagingPolicy,

    /// Python resources to be embedded in the binary.
    resources_collector: PythonResourceCollector,

    /// Holds state necessary to link libpython.
    core_build_context: LibPythonBuildContext,

    /// Holds linking context for individual extensions.
    ///
    /// We need to track per-extension state separately since we need
    /// to support filtering extensions as part of building.
    extension_build_contexts: BTreeMap<String, LibPythonBuildContext>,

    /// Configuration of the embedded Python interpreter.
    config: EmbeddedPythonConfig,

    /// Path to python executable that can be invoked at build time.
    python_exe: PathBuf,
}

impl StandalonePythonExecutableBuilder {
    #[allow(clippy::too_many_arguments)]
    pub fn from_distribution(
        distribution: Arc<Box<StandaloneDistribution>>,
        host_triple: String,
        target_triple: String,
        exe_name: String,
        link_mode: BinaryLibpythonLinkMode,
        packaging_policy: PythonPackagingPolicy,
        config: EmbeddedPythonConfig,
    ) -> Result<Box<Self>> {
        let python_exe = distribution.python_exe.clone();
        let cache_tag = distribution.cache_tag.clone();

        let (supports_static_libpython, supports_dynamic_libpython) =
            distribution.libpython_link_support();

        let link_mode = match link_mode {
            BinaryLibpythonLinkMode::Default => {
                if supports_static_libpython {
                    LibpythonLinkMode::Static
                } else if supports_dynamic_libpython {
                    LibpythonLinkMode::Dynamic
                } else {
                    return Err(anyhow!("no link modes supported; please report this bug"));
                }
            }
            BinaryLibpythonLinkMode::Static => {
                if !supports_static_libpython {
                    return Err(anyhow!(
                        "Python distribution does not support statically linking libpython"
                    ));
                }

                LibpythonLinkMode::Static
            }
            BinaryLibpythonLinkMode::Dynamic => {
                if !supports_dynamic_libpython {
                    return Err(anyhow!(
                        "Python distribution does not support dynamically linking libpython"
                    ));
                }

                LibpythonLinkMode::Dynamic
            }
        };

        let supports_in_memory_dynamically_linked_extension_loading =
            distribution.supports_in_memory_dynamically_linked_extension_loading();

        let mut builder = Box::new(Self {
            host_triple,
            target_triple,
            exe_name,
            distribution,
            link_mode,
            supports_in_memory_dynamically_linked_extension_loading,
            packaging_policy: packaging_policy.clone(),
            resources_collector: PythonResourceCollector::new(
                packaging_policy.resources_policy(),
                &cache_tag,
            ),
            core_build_context: LibPythonBuildContext::default(),
            extension_build_contexts: BTreeMap::new(),
            config,
            python_exe,
        });

        builder.add_distribution_resources(&packaging_policy)?;

        Ok(builder)
    }

    fn add_distribution_resources(&mut self, policy: &PythonPackagingPolicy) -> Result<()> {
        self.core_build_context.inittab_cflags = Some(self.distribution.inittab_cflags.clone());

        for (name, path) in &self.distribution.includes {
            self.core_build_context
                .includes
                .insert(PathBuf::from(name), DataLocation::Path(path.clone()));
        }

        // Add the distribution's object files from Python core to linking context.
        for fs_path in self.distribution.objs_core.values() {
            // libpython generation derives its own `_PyImport_Inittab`. So ignore
            // the object file containing it.
            if fs_path == &self.distribution.inittab_object {
                continue;
            }

            self.core_build_context
                .object_files
                .push(DataLocation::Path(fs_path.clone()));
        }

        for entry in &self.distribution.links_core {
            if entry.framework {
                self.core_build_context
                    .frameworks
                    .insert(entry.name.clone());
            } else if entry.system {
                self.core_build_context
                    .system_libraries
                    .insert(entry.name.clone());
            }
            // TODO handle static/dynamic libraries.
        }

        for location in self.distribution.libraries.values() {
            let path = match location {
                DataLocation::Path(p) => p,
                DataLocation::Memory(_) => {
                    return Err(anyhow!(
                        "cannot link libraries not backed by the filesystem"
                    ))
                }
            };

            self.core_build_context.library_search_paths.insert(
                path.parent()
                    .ok_or_else(|| anyhow!("unable to resolve parent directory"))?
                    .to_path_buf(),
            );
        }

        // Windows requires dynamic linking against msvcrt. Ensure that happens.
        if crate::environment::WINDOWS_TARGET_TRIPLES.contains(&self.target_triple.as_str()) {
            self.core_build_context
                .system_libraries
                .insert("msvcrt".to_string());
        }

        if let Some(lis) = self.distribution.license_infos.get("python") {
            self.core_build_context
                .license_infos
                .insert("python".to_string(), lis.clone());
        }

        for ext in self.packaging_policy.resolve_python_extension_modules(
            self.distribution.extension_modules.values(),
            &self.target_triple,
        )? {
            self.add_python_extension_module(&ext, None)?;
        }

        for source in self.distribution.source_modules()? {
            let add_context = policy.derive_collection_add_context(&(&source).into());
            self.add_python_module_source(&source, Some(add_context))?;
        }

        for resource in self.distribution.resource_datas()? {
            let add_context = policy.derive_collection_add_context(&(&resource).into());
            self.add_python_package_resource(&resource, Some(add_context))?;
        }

        Ok(())
    }

    /// Build a Python library suitable for linking.
    ///
    /// This will take the underlying distribution, resources, and
    /// configuration and produce a new executable binary.
    fn resolve_python_linking_info(
        &self,
        logger: &slog::Logger,
        opt_level: &str,
    ) -> Result<PythonLinkingInfo> {
        let libpythonxy_filename;
        let mut cargo_metadata: Vec<String> = Vec::new();
        let libpythonxy_data;
        let libpython_filename: Option<PathBuf>;
        let libpyembeddedconfig_data: Option<Vec<u8>>;
        let libpyembeddedconfig_filename: Option<PathBuf>;

        match self.link_mode {
            LibpythonLinkMode::Static => {
                let temp_dir = TempDir::new("pyoxidizer-build-exe")?;
                let temp_dir_path = temp_dir.path();

                warn!(
                    logger,
                    "generating custom link library containing Python..."
                );

                let mut link_contexts = vec![&self.core_build_context];
                for c in self.extension_build_contexts.values() {
                    link_contexts.push(c);
                }

                let library_info = link_libpython(
                    logger,
                    &LibPythonBuildContext::merge(&link_contexts),
                    &temp_dir_path,
                    &self.host_triple,
                    &self.target_triple,
                    opt_level,
                )?;

                libpythonxy_filename =
                    PathBuf::from(library_info.libpython_path.file_name().unwrap());
                cargo_metadata.extend(library_info.cargo_metadata);

                libpythonxy_data = std::fs::read(&library_info.libpython_path)?;
                libpython_filename = None;
                libpyembeddedconfig_filename = Some(PathBuf::from(
                    library_info.libpyembeddedconfig_path.file_name().unwrap(),
                ));
                libpyembeddedconfig_data =
                    Some(std::fs::read(&library_info.libpyembeddedconfig_path)?);
            }

            LibpythonLinkMode::Dynamic => {
                libpythonxy_filename = PathBuf::from("pythonXY.lib");
                libpythonxy_data = Vec::new();
                libpython_filename = self.distribution.libpython_shared_library.clone();
                libpyembeddedconfig_filename = None;
                libpyembeddedconfig_data = None;
            }
        }

        Ok(PythonLinkingInfo {
            libpythonxy_filename,
            libpythonxy_data,
            libpython_filename,
            libpyembeddedconfig_filename,
            libpyembeddedconfig_data,
            cargo_metadata,
        })
    }
}

impl PythonBinaryBuilder for StandalonePythonExecutableBuilder {
    fn clone_box(&self) -> Box<dyn PythonBinaryBuilder> {
        Box::new(self.clone())
    }

    fn name(&self) -> String {
        self.exe_name.clone()
    }

    fn libpython_link_mode(&self) -> LibpythonLinkMode {
        self.link_mode
    }

    fn cache_tag(&self) -> &str {
        self.distribution.cache_tag()
    }

    fn python_packaging_policy(&self) -> &PythonPackagingPolicy {
        &self.packaging_policy
    }

    fn python_exe_path(&self) -> &Path {
        &self.python_exe
    }

    fn iter_resources<'a>(
        &'a self,
    ) -> Box<dyn Iterator<Item = (&'a String, &'a PrePackagedResource)> + 'a> {
        Box::new(self.resources_collector.iter_resources())
    }

    fn pip_install(
        &self,
        logger: &slog::Logger,
        verbose: bool,
        install_args: &[String],
        extra_envs: &HashMap<String, String>,
    ) -> Result<Vec<PythonResource>> {
        pip_install(
            logger,
            &**self.distribution,
            self.link_mode,
            verbose,
            install_args,
            extra_envs,
        )
    }

    fn read_package_root(
        &self,
        _logger: &slog::Logger,
        path: &Path,
        packages: &[String],
    ) -> Result<Vec<PythonResource>> {
        Ok(find_resources(&**self.distribution, path, None)?
            .iter()
            .filter_map(|x| {
                if x.is_in_packages(packages) {
                    Some(x.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>())
    }

    fn read_virtualenv(&self, _logger: &slog::Logger, path: &Path) -> Result<Vec<PythonResource>> {
        read_virtualenv(&**self.distribution, path)
    }

    fn setup_py_install(
        &self,
        logger: &slog::Logger,
        package_path: &Path,
        verbose: bool,
        extra_envs: &HashMap<String, String>,
        extra_global_arguments: &[String],
    ) -> Result<Vec<PythonResource>> {
        setup_py_install(
            logger,
            &**self.distribution,
            self.link_mode,
            package_path,
            verbose,
            extra_envs,
            extra_global_arguments,
        )
    }

    fn add_python_module_source(
        &mut self,
        module: &PythonModuleSource,
        add_context: Option<PythonResourceAddCollectionContext>,
    ) -> Result<()> {
        let add_context = add_context.unwrap_or_else(|| {
            self.packaging_policy
                .derive_collection_add_context(&module.into())
        });

        self.resources_collector
            .add_python_module_source_with_context(module, &add_context)
    }

    fn add_python_package_resource(
        &mut self,
        resource: &PythonPackageResource,
        add_context: Option<PythonResourceAddCollectionContext>,
    ) -> Result<()> {
        let add_context = add_context.unwrap_or_else(|| {
            self.packaging_policy
                .derive_collection_add_context(&resource.into())
        });

        self.resources_collector
            .add_python_package_resource_with_context(resource, &add_context)
    }

    fn add_python_package_distribution_resource(
        &mut self,
        resource: &PythonPackageDistributionResource,
        add_context: Option<PythonResourceAddCollectionContext>,
    ) -> Result<()> {
        let add_context = add_context.unwrap_or_else(|| {
            self.packaging_policy
                .derive_collection_add_context(&resource.into())
        });

        self.resources_collector
            .add_python_package_distribution_resource_with_context(resource, &add_context)
    }

    #[allow(clippy::if_same_then_else)]
    fn add_python_extension_module(
        &mut self,
        extension_module: &PythonExtensionModule,
        add_context: Option<PythonResourceAddCollectionContext>,
    ) -> Result<()> {
        // TODO rewrite code in terms of add_context (the logic is a leftover from when
        // we passed an Option<ConcreteResourceLocation> into the function).
        let location = match &add_context {
            Some(add_context) => Some(add_context.location.clone()),
            None => None,
        };

        // Whether we can load extension modules as standalone shared library files.
        let can_load_standalone = self.distribution.is_extension_module_file_loadable();

        // Whether we can load extension module dynamic libraries from memory. This
        // means we have a dynamic library extension module and that library is loaded
        // from memory: this is not a built-in extension!
        let can_load_dynamic_library_memory =
            self.supports_in_memory_dynamically_linked_extension_loading;

        // Whether we can link the extension as a built-in. This requires the extension
        // to be builtin to the core distribution, have object files that we can link
        // together, or a static library to link.
        let can_link_builtin = if extension_module.builtin_default {
            true
        } else {
            !extension_module.object_file_data.is_empty()
        };

        // Whether we can produce a standalone shared library extension module.
        // TODO consider allowing this if object files are present.
        let can_link_standalone = extension_module.shared_library.is_some();

        // Whether the resources policy prefers in-memory loading.
        let policy_want_memory = match self.packaging_policy.clone().resources_policy() {
            PythonResourcesPolicy::InMemoryOnly => true,
            PythonResourcesPolicy::PreferInMemoryFallbackFilesystemRelative(_) => true,
            PythonResourcesPolicy::FilesystemRelativeOnly(_) => false,
        };

        let relative_path = match &add_context {
            Some(add_context) => match &add_context.location {
                ConcreteResourceLocation::InMemory => None,
                ConcreteResourceLocation::RelativePath(ref prefix) => Some(prefix.clone()),
            },
            None => match self.packaging_policy.clone().resources_policy() {
                PythonResourcesPolicy::FilesystemRelativeOnly(prefix) => Some(prefix.clone()),
                PythonResourcesPolicy::PreferInMemoryFallbackFilesystemRelative(prefix) => {
                    Some(prefix.clone())
                }
                PythonResourcesPolicy::InMemoryOnly => None,
            },
        };

        let want_in_memory = if let Some(add_context) = &add_context {
            add_context.location == ConcreteResourceLocation::InMemory
        } else {
            policy_want_memory
        };

        let require_in_memory = if let Some(add_context) = &add_context {
            add_context.location == ConcreteResourceLocation::InMemory
        } else {
            self.packaging_policy.clone().resources_policy() == &PythonResourcesPolicy::InMemoryOnly
        };

        let require_filesystem = if let Some(add_context) = &add_context {
            match &add_context.location {
                ConcreteResourceLocation::RelativePath(_) => true,
                ConcreteResourceLocation::InMemory => false,
            }
        } else {
            match *self.packaging_policy.clone().resources_policy() {
                PythonResourcesPolicy::FilesystemRelativeOnly(_) => true,
                PythonResourcesPolicy::InMemoryOnly => false,
                PythonResourcesPolicy::PreferInMemoryFallbackFilesystemRelative(_) => false,
            }
        };

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
            want_in_memory && can_link_builtin && !require_filesystem
        };

        // Reject explicit requests to load extension module from the filesystem
        // when the distribution doesn't support this.
        if let Some(ConcreteResourceLocation::RelativePath(_)) = location {
            if !can_load_standalone {
                return Err(anyhow!("explicit request to load extension module {} from the filesystem is not supported by this Python distribution", extension_module.name));
            }
        }

        // Reject explicit requests to load extension module from memory when
        // this isn't supported.
        if let Some(ConcreteResourceLocation::InMemory) = location {
            if !can_link_builtin && !can_load_dynamic_library_memory {
                return Err(anyhow!("rejecting request to load extension module {} from memory since it is not supported", extension_module.name));
            }
        }

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
                } else if depends.static_library.is_some()
                    && !ignored_libraries_for_target(&self.target_triple)
                        .contains(&depends.name.as_str())
                {
                    build_context.static_libraries.insert(depends.name.clone());
                } else if depends.dynamic_library.is_some()
                    && !ignored_libraries_for_target(&self.target_triple)
                        .contains(&depends.name.as_str())
                {
                    build_context.dynamic_libraries.insert(depends.name.clone());
                }
            }

            if let Some(lis) = self.distribution.license_infos.get(&extension_module.name) {
                build_context
                    .license_infos
                    .insert(extension_module.name.clone(), lis.clone());
            }

            if let Some(init_fn) = &extension_module.init_fn {
                build_context
                    .init_functions
                    .insert(extension_module.name.clone(), init_fn.clone());
            }

            for location in &extension_module.object_file_data {
                build_context.object_files.push(location.clone());
            }

            self.resources_collector
                .add_builtin_python_extension_module(extension_module)?;

            self.extension_build_contexts
                .insert(extension_module.name.clone(), build_context);
        } else {
            // If we're not producing a builtin, we're producing a shared library
            // extension module. We currently only support extension modules that
            // already have a shared library present. So we simply call into
            // the resources collector.
            let location = if policy_want_memory && can_load_dynamic_library_memory {
                ConcreteResourceLocation::InMemory
            } else {
                match relative_path {
                    Some(prefix) => ConcreteResourceLocation::RelativePath(prefix),
                    None => ConcreteResourceLocation::InMemory,
                }
            };

            self.resources_collector
                .add_python_extension_module(extension_module, &location)?;
        }

        Ok(())
    }

    fn filter_resources_from_files(
        &mut self,
        logger: &slog::Logger,
        files: &[&Path],
        glob_patterns: &[&str],
    ) -> Result<()> {
        let resource_names = resolve_resource_names_from_files(files, glob_patterns)?;

        warn!(logger, "filtering module entries");

        self.resources_collector.filter_resources_mut(|resource| {
            if !resource_names.contains(&resource.name) {
                warn!(logger, "removing {}", resource.name);
                false
            } else {
                true
            }
        })?;

        warn!(logger, "filtering embedded extension modules");
        filter_btreemap(logger, &mut self.extension_build_contexts, &resource_names);

        Ok(())
    }

    fn requires_jemalloc(&self) -> bool {
        self.config.raw_allocator == RawAllocator::Jemalloc
    }

    fn to_embedded_python_context(
        &self,
        logger: &slog::Logger,
        opt_level: &str,
    ) -> Result<EmbeddedPythonContext> {
        let mut file_seen = false;
        for module in self.resources_collector.find_dunder_file()? {
            file_seen = true;
            warn!(logger, "warning: {} contains __file__", module);
        }

        if file_seen {
            warn!(logger, "__file__ was encountered in some embedded modules");
            warn!(
                logger,
                "PyOxidizer does not set __file__ and this may create problems at run-time"
            );
            warn!(
                logger,
                "See https://github.com/indygreg/PyOxidizer/issues/69 for more"
            );
        }

        let compiled_resources = {
            let mut compiler = BytecodeCompiler::new(&self.python_exe)?;
            self.resources_collector.compile_resources(&mut compiler)?
        };

        let mut extra_files = FileManifest::default();

        for (path, location, executable) in &compiled_resources.extra_files {
            extra_files.add_file(
                path,
                &FileContent {
                    data: location.resolve()?,
                    executable: *executable,
                },
            )?;
        }

        let mut module_names = Vec::new();

        for name in compiled_resources.resources.keys() {
            module_names.write_all(name.as_bytes())?;
            module_names.write_all(b"\n")?;
        }

        let mut resources = Vec::new();
        compiled_resources.write_packed_resources_v1(&mut resources)?;

        let linking_info = self.resolve_python_linking_info(logger, opt_level)?;

        if self.link_mode == LibpythonLinkMode::Dynamic {
            if let Some(p) = &self.distribution.libpython_shared_library {
                let manifest_path = Path::new(p.file_name().unwrap());
                let content = FileContent {
                    data: std::fs::read(&p)?,
                    executable: false,
                };

                extra_files.add_file(&manifest_path, &content)?;
            }
        }

        Ok(EmbeddedPythonContext {
            config: self.config.clone(),
            linking_info,
            module_names,
            resources,
            extra_files,
            host_triple: self.host_triple.clone(),
            target_triple: self.target_triple.clone(),
        })
    }
}

#[cfg(test)]
pub mod tests {
    use {
        super::*,
        crate::py_packaging::distribution::{BinaryLibpythonLinkMode, DistributionFlavor},
        crate::python_distributions::PYTHON_DISTRIBUTIONS,
        crate::testutil::*,
        lazy_static::lazy_static,
        python_packaging::policy::ExtensionModuleFilter,
        python_packed_resources::data::ResourceFlavor,
        std::collections::BTreeSet,
        std::iter::FromIterator,
        std::ops::Deref,
    };

    lazy_static! {
        pub static ref WINDOWS_TARGET_TRIPLES: Vec<&'static str> =
            vec!["i686-pc-windows-msvc", "x86_64-pc-windows-msvc"];

        /// An extension module represented by a shared library file.
        pub static ref EXTENSION_MODULE_SHARED_LIBRARY_ONLY: PythonExtensionModule =
            PythonExtensionModule {
                name: "shared_only".to_string(),
                init_fn: Some("PyInit_shared_only".to_string()),
                extension_file_suffix: ".so".to_string(),
                shared_library: Some(DataLocation::Memory(vec![42])),
                object_file_data: vec![],
                is_package: false,
                link_libraries: vec![],
                is_stdlib: false,
                builtin_default: false,
                required: false,
                variant: None,
                licenses: None,
                license_texts: None,
                license_public_domain: None,
            };

        /// An extension module represented by only object files.
        pub static ref EXTENSION_MODULE_OBJECT_FILES_ONLY: PythonExtensionModule =
            PythonExtensionModule {
                name: "object_files_only".to_string(),
                init_fn: Some("PyInit_object_files_only".to_string()),
                extension_file_suffix: ".so".to_string(),
                shared_library: None,
                object_file_data: vec![DataLocation::Memory(vec![0]), DataLocation::Memory(vec![1])],
                is_package: false,
                link_libraries: vec![],
                is_stdlib: false,
                builtin_default: false,
                required: false,
                variant: None,
                licenses: None,
                license_texts: None,
                license_public_domain: None,
        };

        /// An extension module with both a shared library and object files.
        pub static ref EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES: PythonExtensionModule =
            PythonExtensionModule {
                name: "shared_and_object_files".to_string(),
                init_fn: Some("PyInit_shared_and_object_files".to_string()),
                extension_file_suffix: ".so".to_string(),
                shared_library: Some(DataLocation::Memory(b"shared".to_vec())),
                object_file_data: vec![DataLocation::Memory(vec![0]), DataLocation::Memory(vec![1])],
                is_package: false,
                link_libraries: vec![],
                is_stdlib: false,
                builtin_default: false,
                required: false,
                variant: None,
                licenses: None,
                license_texts: None,
                license_public_domain: None,
        };
    }

    /// Defines construction options for a `StandalonePythonExecutableBuilder`.
    ///
    /// This is mostly intended to be used by tests, to reduce boilerplate for
    /// constructing instances.
    pub struct StandalonePythonExecutableBuilderOptions {
        pub host_triple: String,
        pub target_triple: String,
        pub distribution_flavor: DistributionFlavor,
        pub app_name: String,
        pub libpython_link_mode: BinaryLibpythonLinkMode,
        pub extension_module_filter: ExtensionModuleFilter,
        pub resources_policy: PythonResourcesPolicy,
    }

    impl Default for StandalonePythonExecutableBuilderOptions {
        fn default() -> Self {
            // Grab default values from a default policy so they stay in sync.
            let default_policy = PythonPackagingPolicy::default();

            Self {
                host_triple: env!("HOST").to_string(),
                target_triple: env!("HOST").to_string(),
                distribution_flavor: DistributionFlavor::Standalone,
                app_name: "testapp".to_string(),
                libpython_link_mode: BinaryLibpythonLinkMode::Default,
                extension_module_filter: default_policy.extension_module_filter().clone(),
                resources_policy: default_policy.resources_policy().clone(),
            }
        }
    }

    impl StandalonePythonExecutableBuilderOptions {
        pub fn new_builder(&self) -> Result<Box<StandalonePythonExecutableBuilder>> {
            let record = PYTHON_DISTRIBUTIONS
                .find_distribution(&self.target_triple, &self.distribution_flavor)
                .ok_or_else(|| anyhow!("could not find Python distribution"))?;

            let distribution = get_distribution(&record.location)?;

            let mut policy = PythonPackagingPolicy::default();
            policy.set_extension_module_filter(self.extension_module_filter.clone());
            policy.set_resources_policy(self.resources_policy.clone());

            let config = EmbeddedPythonConfig::default();

            StandalonePythonExecutableBuilder::from_distribution(
                distribution,
                self.host_triple.clone(),
                self.target_triple.clone(),
                self.app_name.clone(),
                self.libpython_link_mode.clone(),
                policy,
                config,
            )
        }
    }

    pub fn get_embedded(logger: &slog::Logger) -> Result<EmbeddedPythonContext> {
        let options = StandalonePythonExecutableBuilderOptions::default();
        let exe = options.new_builder()?;

        exe.to_embedded_python_context(logger, "0")
    }

    fn assert_extension_builtin(
        builder: &StandalonePythonExecutableBuilder,
        extension: &PythonExtensionModule,
    ) -> Result<()> {
        assert_eq!(
            builder.iter_resources().find_map(|(name, r)| {
                if *name == extension.name {
                    Some(r)
                } else {
                    None
                }
            }),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::BuiltinExtensionModule,
                name: extension.name.clone(),
                ..PrePackagedResource::default()
            })
        );

        assert_eq!(
            builder.extension_build_contexts.get(&extension.name),
            Some(&LibPythonBuildContext {
                object_files: extension.object_file_data.clone(),
                init_functions: BTreeMap::from_iter(
                    [(
                        extension.name.to_string(),
                        extension.init_fn.as_ref().clone().unwrap().to_string()
                    )]
                    .iter()
                    .cloned()
                ),
                ..LibPythonBuildContext::default()
            })
        );

        Ok(())
    }

    fn assert_extension_shared_library(
        builder: &StandalonePythonExecutableBuilder,
        extension: &PythonExtensionModule,
        location: ConcreteResourceLocation,
    ) -> Result<()> {
        let mut entry = PrePackagedResource {
            flavor: ResourceFlavor::Extension,
            name: extension.name.clone(),
            shared_library_dependency_names: Some(vec![]),
            ..PrePackagedResource::default()
        };

        match location {
            ConcreteResourceLocation::InMemory => {
                assert!(extension.shared_library.is_some());
                entry.in_memory_extension_module_shared_library =
                    Some(extension.shared_library.as_ref().unwrap().clone());
            }
            ConcreteResourceLocation::RelativePath(prefix) => {
                assert!(extension.shared_library.is_some());
                entry.relative_path_extension_module_shared_library = Some((
                    PathBuf::from(prefix).join(format!(
                        "{}{}",
                        extension.name, extension.extension_file_suffix
                    )),
                    extension.shared_library.as_ref().unwrap().clone(),
                ));
            }
        }

        assert_eq!(
            builder.iter_resources().find_map(|(name, r)| {
                if *name == extension.name {
                    Some(r)
                } else {
                    None
                }
            }),
            Some(&entry)
        );

        // There is no build context for extensions materialized as shared libraries.
        // This could change if we ever link shared library extension modules from
        // object files.
        assert_eq!(builder.extension_build_contexts.get(&extension.name), None);

        Ok(())
    }

    #[test]
    fn test_write_embedded_files() -> Result<()> {
        let logger = get_logger()?;
        let embedded = get_embedded(&logger)?;
        let temp_dir = tempdir::TempDir::new("pyoxidizer-test")?;

        embedded.write_files(temp_dir.path())?;

        Ok(())
    }

    #[test]
    fn test_minimal_extensions_present() -> Result<()> {
        let options = StandalonePythonExecutableBuilderOptions::default();
        let builder = options.new_builder()?;

        let expected = builder
            .distribution
            .extension_modules
            .iter()
            .filter_map(|(_, extensions)| {
                if extensions.default_variant().is_minimally_required() {
                    Some(extensions.default_variant().name.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        // Sanity check.
        assert!(expected.contains(&"_io".to_string()));

        for name in &expected {
            // All extensions annotated as required in the distribution are marked
            // as built-ins.
            assert!(builder.extension_build_contexts.keys().any(|x| x == name));
            assert!(builder.iter_resources().any(|(x, _)| x == name));
        }

        Ok(())
    }

    #[test]
    fn test_linux_extensions_sanity() -> Result<()> {
        for libpython_link_mode in vec![
            BinaryLibpythonLinkMode::Static,
            BinaryLibpythonLinkMode::Dynamic,
        ] {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                extension_module_filter: ExtensionModuleFilter::All,
                libpython_link_mode,
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            let builder = options.new_builder()?;

            let builtin_names = builder.extension_build_contexts.keys().collect::<Vec<_>>();

            // All extensions compiled as built-ins by default.
            for (name, _) in builder.distribution.extension_modules.iter() {
                assert!(builtin_names.contains(&name));
            }
        }

        Ok(())
    }

    #[test]
    fn test_linux_distribution_extension_static() -> Result<()> {
        for libpython_link_mode in vec![
            BinaryLibpythonLinkMode::Static,
            BinaryLibpythonLinkMode::Dynamic,
        ] {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                extension_module_filter: ExtensionModuleFilter::Minimal,
                libpython_link_mode,
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            let mut builder = options.new_builder()?;

            // When adding an extension module in static link mode, it gets
            // added as a built-in and linked with libpython.

            let sqlite = builder
                .distribution
                .extension_modules
                .get("_sqlite3")
                .unwrap()
                .default_variant()
                .clone();

            builder.add_python_extension_module(&sqlite, None)?;

            assert_eq!(
                builder.extension_build_contexts.get("_sqlite3"),
                Some(&LibPythonBuildContext {
                    object_files: sqlite.object_file_data,
                    static_libraries: BTreeSet::from_iter(["sqlite3".to_string()].iter().cloned()),
                    init_functions: BTreeMap::from_iter(
                        [("_sqlite3".to_string(), "PyInit__sqlite3".to_string())]
                            .iter()
                            .cloned()
                    ),
                    license_infos: BTreeMap::from_iter(
                        [(
                            "_sqlite3".to_string(),
                            builder
                                .distribution
                                .license_infos
                                .get("_sqlite3")
                                .unwrap()
                                .clone()
                        )]
                        .iter()
                        .cloned()
                    ),
                    ..LibPythonBuildContext::default()
                })
            );

            assert_eq!(
                builder
                    .iter_resources()
                    .find_map(|(name, r)| if *name == "_sqlite3" { Some(r) } else { None }),
                Some(&PrePackagedResource {
                    flavor: ResourceFlavor::BuiltinExtensionModule,
                    name: "_sqlite3".to_string(),
                    ..PrePackagedResource::default()
                })
            );
        }

        Ok(())
    }

    #[test]
    fn test_linux_extension_in_memory_policy() -> Result<()> {
        for libpython_link_mode in vec![
            BinaryLibpythonLinkMode::Static,
            BinaryLibpythonLinkMode::Dynamic,
        ] {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                extension_module_filter: ExtensionModuleFilter::Minimal,
                libpython_link_mode,
                resources_policy: PythonResourcesPolicy::InMemoryOnly,
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            let mut builder = options.new_builder()?;

            let res =
                builder.add_python_extension_module(&EXTENSION_MODULE_SHARED_LIBRARY_ONLY, None);
            assert!(res.is_err());
            assert_eq!(
            res.err().unwrap().to_string(),
            "extension module shared_only cannot be loaded from memory but memory loading required"
        );

            builder.add_python_extension_module(&EXTENSION_MODULE_OBJECT_FILES_ONLY, None)?;
            assert_extension_builtin(&builder, &EXTENSION_MODULE_OBJECT_FILES_ONLY)?;

            builder.add_python_extension_module(
                &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES,
                None,
            )?;
            assert_extension_builtin(&builder, &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES)?;
        }

        Ok(())
    }

    #[test]
    fn test_linux_extension_in_memory_explicit() -> Result<()> {
        for libpython_link_mode in vec![
            BinaryLibpythonLinkMode::Static,
            BinaryLibpythonLinkMode::Dynamic,
        ] {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                extension_module_filter: ExtensionModuleFilter::Minimal,
                libpython_link_mode,
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            let mut builder = options.new_builder()?;

            let mut add_context = builder.packaging_policy.derive_collection_add_context(
                &EXTENSION_MODULE_SHARED_LIBRARY_ONLY.deref().into(),
            );
            add_context.location = ConcreteResourceLocation::InMemory;
            let res = builder.add_python_extension_module(
                &EXTENSION_MODULE_SHARED_LIBRARY_ONLY,
                Some(add_context),
            );
            assert!(res.is_err());
            assert_eq!(
                        res.err().unwrap().to_string(),
                        "rejecting request to load extension module shared_only from memory since it is not supported"
                    );

            let mut add_context = builder
                .packaging_policy
                .derive_collection_add_context(&EXTENSION_MODULE_OBJECT_FILES_ONLY.deref().into());
            add_context.location = ConcreteResourceLocation::InMemory;
            builder.add_python_extension_module(
                &EXTENSION_MODULE_OBJECT_FILES_ONLY,
                Some(add_context),
            )?;
            assert_extension_builtin(&builder, &EXTENSION_MODULE_OBJECT_FILES_ONLY)?;

            let mut add_context = builder.packaging_policy.derive_collection_add_context(
                &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES
                    .deref()
                    .into(),
            );
            add_context.location = ConcreteResourceLocation::InMemory;
            builder.add_python_extension_module(
                &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES,
                Some(add_context),
            )?;
            assert_extension_builtin(&builder, &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES)?;
        }
        Ok(())
    }

    #[test]
    fn test_linux_extension_prefer_in_memory_policy() -> Result<()> {
        for libpython_link_mode in vec![
            BinaryLibpythonLinkMode::Static,
            BinaryLibpythonLinkMode::Dynamic,
        ] {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                extension_module_filter: ExtensionModuleFilter::Minimal,
                libpython_link_mode,
                resources_policy: PythonResourcesPolicy::PreferInMemoryFallbackFilesystemRelative(
                    "prefix_policy".to_string(),
                ),
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            let mut builder = options.new_builder()?;

            builder.add_python_extension_module(&EXTENSION_MODULE_SHARED_LIBRARY_ONLY, None)?;
            assert_extension_shared_library(
                &builder,
                &EXTENSION_MODULE_SHARED_LIBRARY_ONLY,
                ConcreteResourceLocation::RelativePath("prefix_policy".to_string()),
            )?;

            builder.add_python_extension_module(&EXTENSION_MODULE_OBJECT_FILES_ONLY, None)?;
            assert_extension_builtin(&builder, &EXTENSION_MODULE_OBJECT_FILES_ONLY)?;

            builder.add_python_extension_module(
                &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES,
                None,
            )?;
            assert_extension_builtin(&builder, &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES)?;
        }
        Ok(())
    }

    #[test]
    fn test_linux_distribution_extension_relative_path_policy() -> Result<()> {
        for libpython_link_mode in vec![
            BinaryLibpythonLinkMode::Static,
            BinaryLibpythonLinkMode::Dynamic,
        ] {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                extension_module_filter: ExtensionModuleFilter::Minimal,
                libpython_link_mode,
                resources_policy: PythonResourcesPolicy::FilesystemRelativeOnly(
                    "prefix_policy".to_string(),
                ),
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            let mut builder = options.new_builder()?;

            let ext = builder
                .distribution
                .extension_modules
                .get("_sqlite3")
                .unwrap()
                .default_variant()
                .clone();

            // The distribution extension can only be materialized as a built-in.
            // So it is added as such.
            builder.add_python_extension_module(&ext, None)?;

            assert_eq!(
                builder.extension_build_contexts.get("_sqlite3"),
                Some(&LibPythonBuildContext {
                    object_files: ext.object_file_data,
                    static_libraries: BTreeSet::from_iter(["sqlite3".to_string()].iter().cloned()),
                    init_functions: BTreeMap::from_iter(
                        [("_sqlite3".to_string(), "PyInit__sqlite3".to_string())]
                            .iter()
                            .cloned()
                    ),
                    license_infos: BTreeMap::from_iter(
                        [(
                            "_sqlite3".to_string(),
                            builder
                                .distribution
                                .license_infos
                                .get("_sqlite3")
                                .unwrap()
                                .clone()
                        )]
                        .iter()
                        .cloned()
                    ),
                    ..LibPythonBuildContext::default()
                })
            );

            assert_eq!(
                builder
                    .iter_resources()
                    .find_map(|(name, r)| if *name == "_sqlite3" { Some(r) } else { None }),
                Some(&PrePackagedResource {
                    flavor: ResourceFlavor::BuiltinExtensionModule,
                    name: "_sqlite3".to_string(),
                    ..PrePackagedResource::default()
                })
            );
        }

        Ok(())
    }

    #[test]
    fn test_linux_distribution_extension_relative_path_explicit() -> Result<()> {
        for libpython_link_mode in vec![
            BinaryLibpythonLinkMode::Static,
            BinaryLibpythonLinkMode::Dynamic,
        ] {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                extension_module_filter: ExtensionModuleFilter::Minimal,
                libpython_link_mode,
                resources_policy: PythonResourcesPolicy::PreferInMemoryFallbackFilesystemRelative(
                    "prefix_policy".to_string(),
                ),
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            let mut builder = options.new_builder()?;

            let ext = builder
                .distribution
                .extension_modules
                .get("_sqlite3")
                .unwrap()
                .default_variant()
                .clone();

            let mut add_context = builder
                .packaging_policy
                .derive_collection_add_context(&(&ext).into());
            add_context.location =
                ConcreteResourceLocation::RelativePath("prefix_policy".to_string());
            // TODO this should probably fail since an explicit, invalid location was requested.
            builder.add_python_extension_module(&ext, Some(add_context))?;

            assert_eq!(
                builder.extension_build_contexts.get("_sqlite3"),
                Some(&LibPythonBuildContext {
                    object_files: ext.object_file_data,
                    static_libraries: BTreeSet::from_iter(["sqlite3".to_string()].iter().cloned()),
                    init_functions: BTreeMap::from_iter(
                        [("_sqlite3".to_string(), "PyInit__sqlite3".to_string())]
                            .iter()
                            .cloned()
                    ),
                    license_infos: BTreeMap::from_iter(
                        [(
                            "_sqlite3".to_string(),
                            builder
                                .distribution
                                .license_infos
                                .get("_sqlite3")
                                .unwrap()
                                .clone()
                        )]
                        .iter()
                        .cloned()
                    ),
                    ..LibPythonBuildContext::default()
                })
            );

            assert_eq!(
                builder
                    .iter_resources()
                    .find_map(|(name, r)| if *name == "_sqlite3" { Some(r) } else { None }),
                Some(&PrePackagedResource {
                    flavor: ResourceFlavor::BuiltinExtensionModule,
                    name: "_sqlite3".to_string(),
                    ..PrePackagedResource::default()
                })
            );
        }

        Ok(())
    }

    #[test]
    fn test_linux_extension_relative_path_policy() -> Result<()> {
        for libpython_link_mode in vec![
            BinaryLibpythonLinkMode::Static,
            BinaryLibpythonLinkMode::Dynamic,
        ] {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                extension_module_filter: ExtensionModuleFilter::Minimal,
                libpython_link_mode,
                resources_policy: PythonResourcesPolicy::FilesystemRelativeOnly(
                    "prefix_policy".to_string(),
                ),
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            let mut builder = options.new_builder()?;

            builder.add_python_extension_module(&EXTENSION_MODULE_SHARED_LIBRARY_ONLY, None)?;
            assert_extension_shared_library(
                &builder,
                &EXTENSION_MODULE_SHARED_LIBRARY_ONLY,
                ConcreteResourceLocation::RelativePath("prefix_policy".to_string()),
            )?;

            builder.add_python_extension_module(&EXTENSION_MODULE_OBJECT_FILES_ONLY, None)?;
            assert_extension_builtin(&builder, &EXTENSION_MODULE_OBJECT_FILES_ONLY)?;

            builder.add_python_extension_module(
                &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES,
                None,
            )?;
            assert_extension_shared_library(
                &builder,
                &EXTENSION_MODULE_SHARED_LIBRARY_ONLY,
                ConcreteResourceLocation::RelativePath("prefix_policy".to_string()),
            )?;
        }

        Ok(())
    }

    #[test]
    fn test_linux_extension_relative_path_explicit() -> Result<()> {
        for libpython_link_mode in vec![
            BinaryLibpythonLinkMode::Static,
            BinaryLibpythonLinkMode::Dynamic,
        ] {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                extension_module_filter: ExtensionModuleFilter::Minimal,
                libpython_link_mode,
                resources_policy: PythonResourcesPolicy::PreferInMemoryFallbackFilesystemRelative(
                    "prefix_policy".to_string(),
                ),
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            let mut builder = options.new_builder()?;

            let mut add_context = builder.packaging_policy.derive_collection_add_context(
                &EXTENSION_MODULE_SHARED_LIBRARY_ONLY.deref().into(),
            );
            add_context.location =
                ConcreteResourceLocation::RelativePath("prefix_explicit".to_string());
            builder.add_python_extension_module(
                &EXTENSION_MODULE_SHARED_LIBRARY_ONLY,
                Some(add_context),
            )?;
            assert_extension_shared_library(
                &builder,
                &EXTENSION_MODULE_SHARED_LIBRARY_ONLY,
                ConcreteResourceLocation::RelativePath("prefix_explicit".to_string()),
            )?;

            let mut add_context = builder
                .packaging_policy
                .derive_collection_add_context(&EXTENSION_MODULE_OBJECT_FILES_ONLY.deref().into());
            add_context.location =
                ConcreteResourceLocation::RelativePath("prefix_explicit".to_string());
            builder.add_python_extension_module(
                &EXTENSION_MODULE_OBJECT_FILES_ONLY,
                Some(add_context),
            )?;
            assert_extension_builtin(&builder, &EXTENSION_MODULE_OBJECT_FILES_ONLY)?;

            let mut add_context = builder.packaging_policy.derive_collection_add_context(
                &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES
                    .deref()
                    .into(),
            );
            add_context.location =
                ConcreteResourceLocation::RelativePath("prefix_explicit".to_string());
            builder.add_python_extension_module(
                &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES,
                Some(add_context),
            )?;
            assert_extension_shared_library(
                &builder,
                &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES,
                ConcreteResourceLocation::RelativePath("prefix_explicit".to_string()),
            )?;
        }

        Ok(())
    }

    #[test]
    fn test_linux_musl_distribution_dynamic() -> Result<()> {
        let options = StandalonePythonExecutableBuilderOptions {
            target_triple: "x86_64-unknown-linux-musl".to_string(),
            extension_module_filter: ExtensionModuleFilter::Minimal,
            libpython_link_mode: BinaryLibpythonLinkMode::Dynamic,
            ..StandalonePythonExecutableBuilderOptions::default()
        };

        // Dynamic libpython on musl is not supported.
        let err = options.new_builder().err();
        assert!(err.is_some());
        assert_eq!(
            err.unwrap().to_string(),
            "Python distribution does not support dynamically linking libpython"
        );

        Ok(())
    }

    #[test]
    fn test_linux_musl_extensions_sanity() -> Result<()> {
        let options = StandalonePythonExecutableBuilderOptions {
            target_triple: "x86_64-unknown-linux-musl".to_string(),
            extension_module_filter: ExtensionModuleFilter::All,
            ..StandalonePythonExecutableBuilderOptions::default()
        };

        let builder = options.new_builder()?;

        // All extensions for musl Linux are built-in because dynamic linking
        // not possible.
        for name in builder.distribution.extension_modules.keys() {
            assert!(builder.extension_build_contexts.keys().any(|e| name == e));
        }

        Ok(())
    }

    #[test]
    fn test_linux_musl_distribution_extension_static() -> Result<()> {
        let options = StandalonePythonExecutableBuilderOptions {
            target_triple: "x86_64-unknown-linux-musl".to_string(),
            extension_module_filter: ExtensionModuleFilter::Minimal,
            libpython_link_mode: BinaryLibpythonLinkMode::Static,
            ..StandalonePythonExecutableBuilderOptions::default()
        };

        let mut builder = options.new_builder()?;

        // When adding an extension module in static link mode, it gets
        // added as a built-in and linked with libpython.

        let sqlite = builder
            .distribution
            .extension_modules
            .get("_sqlite3")
            .unwrap()
            .default_variant()
            .clone();

        builder.add_python_extension_module(&sqlite, None)?;

        assert_eq!(
            builder.extension_build_contexts.get("_sqlite3"),
            Some(&LibPythonBuildContext {
                object_files: sqlite.object_file_data,
                static_libraries: BTreeSet::from_iter(["sqlite3".to_string()].iter().cloned()),
                init_functions: BTreeMap::from_iter(
                    [("_sqlite3".to_string(), "PyInit__sqlite3".to_string())]
                        .iter()
                        .cloned()
                ),
                license_infos: BTreeMap::from_iter(
                    [(
                        "_sqlite3".to_string(),
                        builder
                            .distribution
                            .license_infos
                            .get("_sqlite3")
                            .unwrap()
                            .clone()
                    )]
                    .iter()
                    .cloned()
                ),
                ..LibPythonBuildContext::default()
            })
        );

        assert_eq!(
            builder
                .iter_resources()
                .find_map(|(name, r)| if *name == "_sqlite3" { Some(r) } else { None }),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::BuiltinExtensionModule,
                name: "_sqlite3".to_string(),
                ..PrePackagedResource::default()
            })
        );

        Ok(())
    }

    #[test]
    fn test_linux_musl_distribution_extension_relative_path_policy() -> Result<()> {
        let options = StandalonePythonExecutableBuilderOptions {
            target_triple: "x86_64-unknown-linux-musl".to_string(),
            extension_module_filter: ExtensionModuleFilter::Minimal,
            libpython_link_mode: BinaryLibpythonLinkMode::Static,
            resources_policy: PythonResourcesPolicy::FilesystemRelativeOnly(
                "prefix_policy".to_string(),
            ),
            ..StandalonePythonExecutableBuilderOptions::default()
        };

        let mut builder = options.new_builder()?;

        let ext = builder
            .distribution
            .extension_modules
            .get("_sqlite3")
            .unwrap()
            .default_variant()
            .clone();

        // The distribution extension can only be materialized as a built-in.
        // So it is added as such.
        builder.add_python_extension_module(&ext, None)?;

        assert_eq!(
            builder.extension_build_contexts.get("_sqlite3"),
            Some(&LibPythonBuildContext {
                object_files: ext.object_file_data,
                static_libraries: BTreeSet::from_iter(["sqlite3".to_string()].iter().cloned()),
                init_functions: BTreeMap::from_iter(
                    [("_sqlite3".to_string(), "PyInit__sqlite3".to_string())]
                        .iter()
                        .cloned()
                ),
                license_infos: BTreeMap::from_iter(
                    [(
                        "_sqlite3".to_string(),
                        builder
                            .distribution
                            .license_infos
                            .get("_sqlite3")
                            .unwrap()
                            .clone()
                    )]
                    .iter()
                    .cloned()
                ),
                ..LibPythonBuildContext::default()
            })
        );

        assert_eq!(
            builder
                .iter_resources()
                .find_map(|(name, r)| if *name == "_sqlite3" { Some(r) } else { None }),
            Some(&PrePackagedResource {
                flavor: ResourceFlavor::BuiltinExtensionModule,
                name: "_sqlite3".to_string(),
                ..PrePackagedResource::default()
            })
        );

        Ok(())
    }

    #[test]
    fn test_linux_musl_distribution_extension_relative_path_explicit() -> Result<()> {
        let options = StandalonePythonExecutableBuilderOptions {
            target_triple: "x86_64-unknown-linux-musl".to_string(),
            extension_module_filter: ExtensionModuleFilter::Minimal,
            libpython_link_mode: BinaryLibpythonLinkMode::Static,
            resources_policy: PythonResourcesPolicy::PreferInMemoryFallbackFilesystemRelative(
                "prefix_policy".to_string(),
            ),
            ..StandalonePythonExecutableBuilderOptions::default()
        };

        let mut builder = options.new_builder()?;

        let ext = builder
            .distribution
            .extension_modules
            .get("_sqlite3")
            .unwrap()
            .default_variant()
            .clone();

        let mut add_context = builder
            .packaging_policy
            .derive_collection_add_context(&(&ext).into());
        add_context.location =
            ConcreteResourceLocation::RelativePath("prefix_explicit".to_string());
        let err = builder
            .add_python_extension_module(&ext, Some(add_context))
            .err();
        assert!(err.is_some());
        assert_eq!(
            err.unwrap().to_string(),
            "explicit request to load extension module _sqlite3 from the filesystem is not supported by this Python distribution"
        );

        Ok(())
    }

    #[test]
    fn test_linux_musl_extension_in_memory_policy() -> Result<()> {
        let options = StandalonePythonExecutableBuilderOptions {
            target_triple: "x86_64-unknown-linux-musl".to_string(),
            extension_module_filter: ExtensionModuleFilter::Minimal,
            libpython_link_mode: BinaryLibpythonLinkMode::Static,
            resources_policy: PythonResourcesPolicy::InMemoryOnly,
            ..StandalonePythonExecutableBuilderOptions::default()
        };

        let mut builder = options.new_builder()?;

        let res = builder.add_python_extension_module(&EXTENSION_MODULE_SHARED_LIBRARY_ONLY, None);
        assert!(res.is_err());
        assert_eq!(
            res.err().unwrap().to_string(),
            "extension module shared_only cannot be loaded from memory but memory loading required"
        );

        builder.add_python_extension_module(&EXTENSION_MODULE_OBJECT_FILES_ONLY, None)?;
        assert_extension_builtin(&builder, &EXTENSION_MODULE_OBJECT_FILES_ONLY)?;

        builder
            .add_python_extension_module(&EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES, None)?;
        assert_extension_builtin(&builder, &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES)?;

        Ok(())
    }

    #[test]
    fn test_linux_musl_extension_prefer_in_memory_policy() -> Result<()> {
        let options = StandalonePythonExecutableBuilderOptions {
            target_triple: "x86_64-unknown-linux-musl".to_string(),
            extension_module_filter: ExtensionModuleFilter::Minimal,
            libpython_link_mode: BinaryLibpythonLinkMode::Static,
            resources_policy: PythonResourcesPolicy::PreferInMemoryFallbackFilesystemRelative(
                "prefix_policy".to_string(),
            ),
            ..StandalonePythonExecutableBuilderOptions::default()
        };

        let mut builder = options.new_builder()?;

        let res = builder.add_python_extension_module(&EXTENSION_MODULE_SHARED_LIBRARY_ONLY, None);
        assert!(res.is_err());
        assert_eq!(
            res.err().unwrap().to_string(),
            "extension module shared_only cannot be materialized as a shared library because distribution does not support loading extension module shared libraries"
        );

        builder.add_python_extension_module(&EXTENSION_MODULE_OBJECT_FILES_ONLY, None)?;
        assert_extension_builtin(&builder, &EXTENSION_MODULE_OBJECT_FILES_ONLY)?;

        builder
            .add_python_extension_module(&EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES, None)?;
        assert_extension_builtin(&builder, &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES)?;

        Ok(())
    }

    #[test]
    fn test_macos_extensions_sanity() -> Result<()> {
        for libpython_link_mode in vec![
            BinaryLibpythonLinkMode::Static,
            BinaryLibpythonLinkMode::Dynamic,
        ] {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: "x86_64-apple-darwin".to_string(),
                libpython_link_mode,
                extension_module_filter: ExtensionModuleFilter::All,
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            let builder = options.new_builder()?;

            let builtin_names = builder.extension_build_contexts.keys().collect::<Vec<_>>();

            // All extensions compiled as built-ins by default.
            for (name, _) in builder.distribution.extension_modules.iter() {
                assert!(builtin_names.contains(&name));
            }
        }

        Ok(())
    }

    #[test]
    fn test_macos_distribution_extension_static() -> Result<()> {
        for libpython_link_mode in vec![
            BinaryLibpythonLinkMode::Static,
            BinaryLibpythonLinkMode::Dynamic,
        ] {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: "x86_64-apple-darwin".to_string(),
                extension_module_filter: ExtensionModuleFilter::Minimal,
                libpython_link_mode,
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            let mut builder = options.new_builder()?;

            // When adding an extension module in static link mode, it gets
            // added as a built-in and linked with libpython.

            let sqlite = builder
                .distribution
                .extension_modules
                .get("_sqlite3")
                .unwrap()
                .default_variant()
                .clone();

            builder.add_python_extension_module(&sqlite, None)?;

            assert_eq!(
                builder.extension_build_contexts.get("_sqlite3"),
                Some(&LibPythonBuildContext {
                    object_files: sqlite.object_file_data,
                    system_libraries: BTreeSet::from_iter(["iconv".to_string()].iter().cloned()),
                    static_libraries: BTreeSet::from_iter(
                        ["intl".to_string(), "sqlite3".to_string()].iter().cloned()
                    ),
                    init_functions: BTreeMap::from_iter(
                        [("_sqlite3".to_string(), "PyInit__sqlite3".to_string())]
                            .iter()
                            .cloned()
                    ),
                    license_infos: BTreeMap::from_iter(
                        [(
                            "_sqlite3".to_string(),
                            builder
                                .distribution
                                .license_infos
                                .get("_sqlite3")
                                .unwrap()
                                .clone()
                        )]
                        .iter()
                        .cloned()
                    ),
                    ..LibPythonBuildContext::default()
                })
            );

            assert_eq!(
                builder
                    .iter_resources()
                    .find_map(|(name, r)| if *name == "_sqlite3" { Some(r) } else { None }),
                Some(&PrePackagedResource {
                    flavor: ResourceFlavor::BuiltinExtensionModule,
                    name: "_sqlite3".to_string(),
                    ..PrePackagedResource::default()
                })
            );
        }

        Ok(())
    }

    #[test]
    fn test_macos_distribution_extension_relative_path_policy() -> Result<()> {
        for libpython_link_mode in vec![
            BinaryLibpythonLinkMode::Static,
            BinaryLibpythonLinkMode::Dynamic,
        ] {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: "x86_64-apple-darwin".to_string(),
                extension_module_filter: ExtensionModuleFilter::Minimal,
                libpython_link_mode,
                resources_policy: PythonResourcesPolicy::FilesystemRelativeOnly(
                    "prefix_policy".to_string(),
                ),
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            let mut builder = options.new_builder()?;

            let ext = builder
                .distribution
                .extension_modules
                .get("_sqlite3")
                .unwrap()
                .default_variant()
                .clone();

            // Distribution extensions can only be materialized as built-ins.
            builder.add_python_extension_module(&ext, None)?;

            assert_eq!(
                builder.extension_build_contexts.get("_sqlite3"),
                Some(&LibPythonBuildContext {
                    object_files: ext.object_file_data,
                    system_libraries: BTreeSet::from_iter(["iconv".to_string()].iter().cloned()),
                    static_libraries: BTreeSet::from_iter(
                        ["intl".to_string(), "sqlite3".to_string()].iter().cloned()
                    ),
                    init_functions: BTreeMap::from_iter(
                        [("_sqlite3".to_string(), "PyInit__sqlite3".to_string())]
                            .iter()
                            .cloned()
                    ),
                    license_infos: BTreeMap::from_iter(
                        [(
                            "_sqlite3".to_string(),
                            builder
                                .distribution
                                .license_infos
                                .get("_sqlite3")
                                .unwrap()
                                .clone()
                        )]
                        .iter()
                        .cloned()
                    ),
                    ..LibPythonBuildContext::default()
                })
            );

            assert_eq!(
                builder
                    .iter_resources()
                    .find_map(|(name, r)| if *name == "_sqlite3" { Some(r) } else { None }),
                Some(&PrePackagedResource {
                    flavor: ResourceFlavor::BuiltinExtensionModule,
                    name: "_sqlite3".to_string(),
                    ..PrePackagedResource::default()
                })
            );
        }

        Ok(())
    }

    #[test]
    fn test_macos_distribution_extension_relative_path_explicit() -> Result<()> {
        for libpython_link_mode in vec![
            BinaryLibpythonLinkMode::Static,
            BinaryLibpythonLinkMode::Dynamic,
        ] {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: "x86_64-apple-darwin".to_string(),
                extension_module_filter: ExtensionModuleFilter::Minimal,
                libpython_link_mode,
                resources_policy: PythonResourcesPolicy::PreferInMemoryFallbackFilesystemRelative(
                    "prefix_policy".to_string(),
                ),
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            let mut builder = options.new_builder()?;

            let ext = builder
                .distribution
                .extension_modules
                .get("_sqlite3")
                .unwrap()
                .default_variant()
                .clone();

            let mut add_context = builder
                .packaging_policy
                .derive_collection_add_context(&(&ext).into());
            add_context.location =
                ConcreteResourceLocation::RelativePath("prefix_explicit".to_string());
            // TODO this should probably fail due location request not being valid.
            builder.add_python_extension_module(&ext, Some(add_context))?;

            assert_eq!(
                builder.extension_build_contexts.get("_sqlite3"),
                Some(&LibPythonBuildContext {
                    object_files: ext.object_file_data,
                    system_libraries: BTreeSet::from_iter(["iconv".to_string()].iter().cloned()),
                    static_libraries: BTreeSet::from_iter(
                        ["intl".to_string(), "sqlite3".to_string()].iter().cloned()
                    ),
                    init_functions: BTreeMap::from_iter(
                        [("_sqlite3".to_string(), "PyInit__sqlite3".to_string())]
                            .iter()
                            .cloned()
                    ),
                    license_infos: BTreeMap::from_iter(
                        [(
                            "_sqlite3".to_string(),
                            builder
                                .distribution
                                .license_infos
                                .get("_sqlite3")
                                .unwrap()
                                .clone()
                        )]
                        .iter()
                        .cloned()
                    ),
                    ..LibPythonBuildContext::default()
                })
            );

            assert_eq!(
                builder
                    .iter_resources()
                    .find_map(|(name, r)| if *name == "_sqlite3" { Some(r) } else { None }),
                Some(&PrePackagedResource {
                    flavor: ResourceFlavor::BuiltinExtensionModule,
                    name: "_sqlite3".to_string(),
                    ..PrePackagedResource::default()
                })
            );
        }

        Ok(())
    }

    #[test]
    fn test_macos_extension_in_memory_policy() -> Result<()> {
        for libpython_link_mode in vec![
            BinaryLibpythonLinkMode::Static,
            BinaryLibpythonLinkMode::Dynamic,
        ] {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: "x86_64-apple-darwin".to_string(),
                extension_module_filter: ExtensionModuleFilter::Minimal,
                libpython_link_mode,
                resources_policy: PythonResourcesPolicy::InMemoryOnly,
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            let mut builder = options.new_builder()?;

            let res =
                builder.add_python_extension_module(&EXTENSION_MODULE_SHARED_LIBRARY_ONLY, None);
            assert!(res.is_err());
            assert_eq!(
                        res.err().unwrap().to_string(),
                        "extension module shared_only cannot be loaded from memory but memory loading required"
                    );

            builder.add_python_extension_module(&EXTENSION_MODULE_OBJECT_FILES_ONLY, None)?;
            assert_extension_builtin(&builder, &EXTENSION_MODULE_OBJECT_FILES_ONLY)?;

            builder.add_python_extension_module(
                &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES,
                None,
            )?;
            assert_extension_builtin(&builder, &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES)?;
        }

        Ok(())
    }

    #[test]
    fn test_macos_extension_in_memory_explicit() -> Result<()> {
        for libpython_link_mode in vec![
            BinaryLibpythonLinkMode::Static,
            BinaryLibpythonLinkMode::Dynamic,
        ] {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: "x86_64-apple-darwin".to_string(),
                extension_module_filter: ExtensionModuleFilter::Minimal,
                libpython_link_mode,
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            let mut builder = options.new_builder()?;

            let mut add_context = builder.packaging_policy.derive_collection_add_context(
                &EXTENSION_MODULE_SHARED_LIBRARY_ONLY.deref().into(),
            );
            add_context.location = ConcreteResourceLocation::InMemory;
            let res = builder.add_python_extension_module(
                &EXTENSION_MODULE_SHARED_LIBRARY_ONLY,
                Some(add_context),
            );
            assert!(res.is_err());
            assert_eq!(
                        res.err().unwrap().to_string(),
                        "rejecting request to load extension module shared_only from memory since it is not supported"
                    );

            let mut add_context = builder
                .packaging_policy
                .derive_collection_add_context(&EXTENSION_MODULE_OBJECT_FILES_ONLY.deref().into());
            add_context.location = ConcreteResourceLocation::InMemory;
            builder.add_python_extension_module(
                &EXTENSION_MODULE_OBJECT_FILES_ONLY,
                Some(add_context),
            )?;
            assert_extension_builtin(&builder, &EXTENSION_MODULE_OBJECT_FILES_ONLY)?;

            let mut add_context = builder.packaging_policy.derive_collection_add_context(
                &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES
                    .deref()
                    .into(),
            );
            add_context.location = ConcreteResourceLocation::InMemory;
            builder.add_python_extension_module(
                &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES,
                Some(add_context),
            )?;
            assert_extension_builtin(&builder, &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES)?;
        }

        Ok(())
    }

    #[test]
    fn test_macos_extension_relative_path_policy() -> Result<()> {
        for libpython_link_mode in vec![
            BinaryLibpythonLinkMode::Static,
            BinaryLibpythonLinkMode::Dynamic,
        ] {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: "x86_64-apple-darwin".to_string(),
                extension_module_filter: ExtensionModuleFilter::Minimal,
                libpython_link_mode,
                resources_policy: PythonResourcesPolicy::FilesystemRelativeOnly(
                    "prefix_policy".to_string(),
                ),
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            let mut builder = options.new_builder()?;

            builder.add_python_extension_module(&EXTENSION_MODULE_SHARED_LIBRARY_ONLY, None)?;
            assert_extension_shared_library(
                &builder,
                &EXTENSION_MODULE_SHARED_LIBRARY_ONLY,
                ConcreteResourceLocation::RelativePath("prefix_policy".to_string()),
            )?;

            builder.add_python_extension_module(&EXTENSION_MODULE_OBJECT_FILES_ONLY, None)?;
            assert_extension_builtin(&builder, &EXTENSION_MODULE_OBJECT_FILES_ONLY)?;

            builder.add_python_extension_module(
                &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES,
                None,
            )?;
            assert_extension_shared_library(
                &builder,
                &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES,
                ConcreteResourceLocation::RelativePath("prefix_policy".to_string()),
            )?;
        }

        Ok(())
    }

    #[test]
    fn test_macos_extension_relative_path_explicit() -> Result<()> {
        for libpython_link_mode in vec![
            BinaryLibpythonLinkMode::Static,
            BinaryLibpythonLinkMode::Dynamic,
        ] {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: "x86_64-apple-darwin".to_string(),
                extension_module_filter: ExtensionModuleFilter::Minimal,
                libpython_link_mode,
                resources_policy: PythonResourcesPolicy::PreferInMemoryFallbackFilesystemRelative(
                    "prefix_policy".to_string(),
                ),
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            let mut builder = options.new_builder()?;

            let mut add_context = builder.packaging_policy.derive_collection_add_context(
                &EXTENSION_MODULE_SHARED_LIBRARY_ONLY.deref().into(),
            );
            add_context.location =
                ConcreteResourceLocation::RelativePath("prefix_explicit".to_string());
            builder.add_python_extension_module(
                &EXTENSION_MODULE_SHARED_LIBRARY_ONLY,
                Some(add_context),
            )?;
            assert_extension_shared_library(
                &builder,
                &EXTENSION_MODULE_SHARED_LIBRARY_ONLY,
                ConcreteResourceLocation::RelativePath("prefix_explicit".to_string()),
            )?;

            let mut add_context = builder
                .packaging_policy
                .derive_collection_add_context(&EXTENSION_MODULE_OBJECT_FILES_ONLY.deref().into());
            add_context.location =
                ConcreteResourceLocation::RelativePath("prefix_explicit".to_string());
            builder.add_python_extension_module(
                &EXTENSION_MODULE_OBJECT_FILES_ONLY,
                Some(add_context),
            )?;
            assert_extension_builtin(&builder, &EXTENSION_MODULE_OBJECT_FILES_ONLY)?;

            let mut add_context = builder.packaging_policy.derive_collection_add_context(
                &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES
                    .deref()
                    .into(),
            );
            add_context.location =
                ConcreteResourceLocation::RelativePath("prefix_explicit".to_string());
            builder.add_python_extension_module(
                &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES,
                Some(add_context),
            )?;
            assert_extension_shared_library(
                &builder,
                &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES,
                ConcreteResourceLocation::RelativePath("prefix_explicit".to_string()),
            )?;
        }

        Ok(())
    }

    #[test]
    fn test_macos_extension_prefer_in_memory_policy() -> Result<()> {
        for libpython_link_mode in vec![
            BinaryLibpythonLinkMode::Static,
            BinaryLibpythonLinkMode::Dynamic,
        ] {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: "x86_64-apple-darwin".to_string(),
                extension_module_filter: ExtensionModuleFilter::Minimal,
                libpython_link_mode,
                resources_policy: PythonResourcesPolicy::PreferInMemoryFallbackFilesystemRelative(
                    "prefix_policy".to_string(),
                ),
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            let mut builder = options.new_builder()?;

            builder.add_python_extension_module(&EXTENSION_MODULE_SHARED_LIBRARY_ONLY, None)?;
            assert_extension_shared_library(
                &builder,
                &EXTENSION_MODULE_SHARED_LIBRARY_ONLY,
                ConcreteResourceLocation::RelativePath("prefix_policy".to_string()),
            )?;

            builder.add_python_extension_module(&EXTENSION_MODULE_OBJECT_FILES_ONLY, None)?;
            assert_extension_builtin(&builder, &EXTENSION_MODULE_OBJECT_FILES_ONLY)?;

            builder.add_python_extension_module(
                &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES,
                None,
            )?;
            assert_extension_builtin(&builder, &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES)?;
        }

        Ok(())
    }

    #[test]
    fn test_windows_dynamic_static_mismatch() -> Result<()> {
        for target_triple in WINDOWS_TARGET_TRIPLES.iter() {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: target_triple.to_string(),
                distribution_flavor: DistributionFlavor::StandaloneDynamic,
                extension_module_filter: ExtensionModuleFilter::Minimal,
                libpython_link_mode: BinaryLibpythonLinkMode::Static,
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            // We can't request static libpython with a dynamic distribution.
            let err = options.new_builder().err();
            assert!(err.is_some());
            assert_eq!(
                err.unwrap().to_string(),
                "Python distribution does not support statically linking libpython"
            );
        }

        Ok(())
    }

    #[test]
    fn test_windows_static_dynamic_mismatch() -> Result<()> {
        for target_triple in WINDOWS_TARGET_TRIPLES.iter() {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: target_triple.to_string(),
                distribution_flavor: DistributionFlavor::StandaloneStatic,
                extension_module_filter: ExtensionModuleFilter::Minimal,
                libpython_link_mode: BinaryLibpythonLinkMode::Dynamic,
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            // We can't request dynamic libpython with a static distribution.
            assert!(options.new_builder().is_err());
        }

        Ok(())
    }

    #[test]
    fn test_windows_dynamic_extensions_sanity() -> Result<()> {
        for target in WINDOWS_TARGET_TRIPLES.iter() {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: target.to_string(),
                distribution_flavor: DistributionFlavor::StandaloneDynamic,
                extension_module_filter: ExtensionModuleFilter::All,
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            let builder = options.new_builder()?;

            let builtin_names = builder.extension_build_contexts.keys().collect::<Vec<_>>();

            // Stdlib extensions are compiled as built-ins.
            for (name, _variants) in builder.distribution.extension_modules.iter() {
                assert!(builtin_names.contains(&name));
            }

            // Required extensions are compiled as built-in.
            // This assumes that are extensions annotated as required are built-in.
            // But this is an implementation detail. If this fails, it might be OK.
            for (name, variants) in builder.distribution.extension_modules.iter() {
                // !required does not mean it is missing, however!
                if variants.iter().any(|e| e.required) {
                    assert!(builtin_names.contains(&name));
                }
            }
        }

        Ok(())
    }

    #[test]
    fn test_windows_distribution_extension_static() -> Result<()> {
        for target_triple in WINDOWS_TARGET_TRIPLES.iter() {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: target_triple.to_string(),
                distribution_flavor: DistributionFlavor::StandaloneStatic,
                extension_module_filter: ExtensionModuleFilter::Minimal,
                libpython_link_mode: BinaryLibpythonLinkMode::Static,
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            let mut builder = options.new_builder()?;

            // When adding an extension module in static link mode, it gets
            // added as a built-in and linked with libpython.

            let sqlite = builder
                .distribution
                .extension_modules
                .get("_sqlite3")
                .unwrap()
                .default_variant()
                .clone();

            builder.add_python_extension_module(&sqlite, None)?;

            assert_eq!(
                builder.extension_build_contexts.get("_sqlite3"),
                Some(&LibPythonBuildContext {
                    object_files: sqlite.object_file_data.clone(),
                    static_libraries: BTreeSet::from_iter(["sqlite3".to_string()].iter().cloned()),
                    init_functions: BTreeMap::from_iter(
                        [("_sqlite3".to_string(), "PyInit__sqlite3".to_string())]
                            .iter()
                            .cloned()
                    ),
                    license_infos: BTreeMap::from_iter(
                        [(
                            "_sqlite3".to_string(),
                            builder
                                .distribution
                                .license_infos
                                .get("_sqlite3")
                                .unwrap()
                                .clone()
                        )]
                        .iter()
                        .cloned()
                    ),
                    ..LibPythonBuildContext::default()
                })
            );

            assert_eq!(
                builder
                    .iter_resources()
                    .find_map(|(name, r)| if *name == "_sqlite3" { Some(r) } else { None }),
                Some(&PrePackagedResource {
                    flavor: ResourceFlavor::BuiltinExtensionModule,
                    name: "_sqlite3".to_string(),
                    ..PrePackagedResource::default()
                })
            );
        }

        Ok(())
    }

    #[test]
    fn test_windows_distribution_extension_dynamic() -> Result<()> {
        for target_triple in WINDOWS_TARGET_TRIPLES.iter() {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: target_triple.to_string(),
                distribution_flavor: DistributionFlavor::StandaloneDynamic,
                extension_module_filter: ExtensionModuleFilter::Minimal,
                libpython_link_mode: BinaryLibpythonLinkMode::Dynamic,
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            let mut builder = options.new_builder()?;

            // When adding an extension module in static link mode, it gets
            // added as a built-in and linked with libpython.

            let sqlite = builder
                .distribution
                .extension_modules
                .get("_sqlite3")
                .unwrap()
                .default_variant()
                .clone();

            builder.add_python_extension_module(&sqlite, None)?;

            assert_eq!(
                builder.extension_build_contexts.get("_sqlite3"),
                Some(&LibPythonBuildContext {
                    object_files: sqlite.object_file_data.clone(),
                    dynamic_libraries: BTreeSet::from_iter(["sqlite3".to_string()].iter().cloned()),
                    init_functions: BTreeMap::from_iter(
                        [("_sqlite3".to_string(), "PyInit__sqlite3".to_string())]
                            .iter()
                            .cloned()
                    ),
                    license_infos: BTreeMap::from_iter(
                        [(
                            "_sqlite3".to_string(),
                            builder
                                .distribution
                                .license_infos
                                .get("_sqlite3")
                                .unwrap()
                                .clone()
                        )]
                        .iter()
                        .cloned()
                    ),
                    ..LibPythonBuildContext::default()
                })
            );

            assert_eq!(
                builder
                    .iter_resources()
                    .find_map(|(name, r)| if *name == "_sqlite3" { Some(r) } else { None }),
                Some(&PrePackagedResource {
                    flavor: ResourceFlavor::BuiltinExtensionModule,
                    name: "_sqlite3".to_string(),
                    ..PrePackagedResource::default()
                })
            );
        }

        Ok(())
    }

    #[test]
    fn test_windows_dynamic_distribution_dynamic_extension_files() -> Result<()> {
        for target in WINDOWS_TARGET_TRIPLES.iter() {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: target.to_string(),
                extension_module_filter: ExtensionModuleFilter::Minimal,
                resources_policy: PythonResourcesPolicy::FilesystemRelativeOnly("lib".to_string()),
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            let mut builder = options.new_builder()?;

            // When loading resources from the filesystem, dynamically linked
            // extension modules should be manifested as filesystem files and
            // library dependencies should be captured.

            let ssl_extension = builder
                .distribution
                .extension_modules
                .get("_ssl")
                .unwrap()
                .default_variant()
                .clone();
            builder.add_python_extension_module(&ssl_extension, None)?;

            let extensions = builder
                .iter_resources()
                .filter_map(|(_, r)| {
                    if r.relative_path_extension_module_shared_library.is_some() {
                        Some(r)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();

            assert_eq!(
                extensions.len(),
                1,
                "only manually added extension present when using minimal extension mode"
            );
            let ssl = &extensions[0];
            assert_eq!(ssl.name, "_ssl");

            let (path, _) = ssl
                .relative_path_extension_module_shared_library
                .as_ref()
                .unwrap();
            assert_eq!(path, &PathBuf::from("lib/_ssl"));

            let shared_libraries = builder
                .iter_resources()
                .filter_map(|(_, r)| {
                    if r.relative_path_shared_library.is_some() {
                        Some(r)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();

            assert_eq!(
                shared_libraries.len(),
                2,
                "pulled in shared library dependencies for _ssl"
            );

            let lib_suffix = match *target {
                "i686-pc-windows-msvc" => "",
                "x86_64-pc-windows-msvc" => "-x64",
                _ => panic!("unexpected target: {}", target),
            };

            assert_eq!(
                shared_libraries[0].name,
                format!("libcrypto-1_1{}", lib_suffix)
            );
            assert_eq!(
                shared_libraries[0]
                    .relative_path_shared_library
                    .as_ref()
                    .unwrap()
                    .0,
                "lib"
            );

            assert_eq!(
                shared_libraries[1].name,
                format!("libssl-1_1{}", lib_suffix)
            );
        }
        Ok(())
    }

    #[test]
    fn test_windows_static_extensions_sanity() -> Result<()> {
        for target in WINDOWS_TARGET_TRIPLES.iter() {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: target.to_string(),
                distribution_flavor: DistributionFlavor::StandaloneStatic,
                extension_module_filter: ExtensionModuleFilter::All,
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            let builder = options.new_builder()?;

            let builtin_names = builder.extension_build_contexts.keys().collect::<Vec<_>>();

            // All distribution extensions are built-ins in static Windows
            // distributions.
            for name in builder.distribution.extension_modules.keys() {
                assert!(builtin_names.contains(&name));
            }
        }

        Ok(())
    }

    #[test]
    fn test_windows_dynamic_extension_in_memory_policy() -> Result<()> {
        for target_triple in WINDOWS_TARGET_TRIPLES.iter() {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: target_triple.to_string(),
                distribution_flavor: DistributionFlavor::StandaloneDynamic,
                extension_module_filter: ExtensionModuleFilter::Minimal,
                libpython_link_mode: BinaryLibpythonLinkMode::Dynamic,
                resources_policy: PythonResourcesPolicy::InMemoryOnly,
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            let mut builder = options.new_builder()?;

            builder.add_python_extension_module(&EXTENSION_MODULE_SHARED_LIBRARY_ONLY, None)?;
            assert_extension_shared_library(
                &builder,
                &EXTENSION_MODULE_SHARED_LIBRARY_ONLY,
                ConcreteResourceLocation::InMemory,
            )?;

            builder.add_python_extension_module(&EXTENSION_MODULE_OBJECT_FILES_ONLY, None)?;
            assert_extension_builtin(&builder, &EXTENSION_MODULE_OBJECT_FILES_ONLY)?;

            builder.add_python_extension_module(
                &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES,
                None,
            )?;
            assert_extension_builtin(&builder, &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES)?;
        }

        Ok(())
    }

    #[test]
    fn test_windows_static_extension_in_memory_policy() -> Result<()> {
        for target_triple in WINDOWS_TARGET_TRIPLES.iter() {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: target_triple.to_string(),
                distribution_flavor: DistributionFlavor::StandaloneStatic,
                extension_module_filter: ExtensionModuleFilter::Minimal,
                libpython_link_mode: BinaryLibpythonLinkMode::Static,
                resources_policy: PythonResourcesPolicy::InMemoryOnly,
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            let mut builder = options.new_builder()?;

            let res =
                builder.add_python_extension_module(&EXTENSION_MODULE_SHARED_LIBRARY_ONLY, None);
            assert!(res.is_err());
            assert_eq!(
                res.err().unwrap().to_string(),
                "extension module shared_only cannot be loaded from memory but memory loading required"
            );

            builder.add_python_extension_module(&EXTENSION_MODULE_OBJECT_FILES_ONLY, None)?;
            assert_extension_builtin(&builder, &EXTENSION_MODULE_OBJECT_FILES_ONLY)?;

            builder.add_python_extension_module(
                &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES,
                None,
            )?;
            assert_extension_builtin(&builder, &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES)?;
        }

        Ok(())
    }

    #[test]
    fn test_windows_dynamic_extension_relative_path_policy() -> Result<()> {
        for target_triple in WINDOWS_TARGET_TRIPLES.iter() {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: target_triple.to_string(),
                distribution_flavor: DistributionFlavor::StandaloneDynamic,
                extension_module_filter: ExtensionModuleFilter::Minimal,
                libpython_link_mode: BinaryLibpythonLinkMode::Dynamic,
                resources_policy: PythonResourcesPolicy::FilesystemRelativeOnly(
                    "prefix_policy".to_string(),
                ),
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            let mut builder = options.new_builder()?;

            builder.add_python_extension_module(&EXTENSION_MODULE_SHARED_LIBRARY_ONLY, None)?;
            assert_extension_shared_library(
                &builder,
                &EXTENSION_MODULE_SHARED_LIBRARY_ONLY,
                ConcreteResourceLocation::RelativePath("prefix_policy".to_string()),
            )?;

            builder.add_python_extension_module(&EXTENSION_MODULE_OBJECT_FILES_ONLY, None)?;
            assert_extension_builtin(&builder, &EXTENSION_MODULE_OBJECT_FILES_ONLY)?;

            builder.add_python_extension_module(
                &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES,
                None,
            )?;
            assert_extension_shared_library(
                &builder,
                &EXTENSION_MODULE_SHARED_LIBRARY_ONLY,
                ConcreteResourceLocation::RelativePath("prefix_policy".to_string()),
            )?;
        }

        Ok(())
    }

    #[test]
    fn test_windows_static_extension_relative_path_policy() -> Result<()> {
        for target_triple in WINDOWS_TARGET_TRIPLES.iter() {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: target_triple.to_string(),
                distribution_flavor: DistributionFlavor::StandaloneStatic,
                extension_module_filter: ExtensionModuleFilter::Minimal,
                libpython_link_mode: BinaryLibpythonLinkMode::Static,
                resources_policy: PythonResourcesPolicy::FilesystemRelativeOnly(
                    "prefix_policy".to_string(),
                ),
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            let mut builder = options.new_builder()?;

            let res =
                builder.add_python_extension_module(&EXTENSION_MODULE_SHARED_LIBRARY_ONLY, None);
            assert!(res.is_err());
            assert_eq!(res.err().unwrap().to_string(),
                "extension module shared_only cannot be materialized as a shared library because distribution does not support loading extension module shared libraries"
            );

            builder.add_python_extension_module(&EXTENSION_MODULE_OBJECT_FILES_ONLY, None)?;
            assert_extension_builtin(&builder, &EXTENSION_MODULE_OBJECT_FILES_ONLY)?;

            builder.add_python_extension_module(
                &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES,
                None,
            )?;
            assert_extension_builtin(&builder, &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES)?;
        }

        Ok(())
    }

    #[test]
    fn test_windows_dynamic_extension_prefer_in_memory_policy() -> Result<()> {
        for target_triple in WINDOWS_TARGET_TRIPLES.iter() {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: target_triple.to_string(),
                distribution_flavor: DistributionFlavor::StandaloneDynamic,
                extension_module_filter: ExtensionModuleFilter::Minimal,
                libpython_link_mode: BinaryLibpythonLinkMode::Dynamic,
                resources_policy: PythonResourcesPolicy::PreferInMemoryFallbackFilesystemRelative(
                    "prefix_policy".to_string(),
                ),
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            let mut builder = options.new_builder()?;

            builder.add_python_extension_module(&EXTENSION_MODULE_SHARED_LIBRARY_ONLY, None)?;
            assert_extension_shared_library(
                &builder,
                &EXTENSION_MODULE_SHARED_LIBRARY_ONLY,
                ConcreteResourceLocation::InMemory,
            )?;

            builder.add_python_extension_module(&EXTENSION_MODULE_OBJECT_FILES_ONLY, None)?;
            assert_extension_builtin(&builder, &EXTENSION_MODULE_OBJECT_FILES_ONLY)?;

            builder.add_python_extension_module(
                &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES,
                None,
            )?;
            assert_extension_builtin(&builder, &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES)?;
        }

        Ok(())
    }

    #[test]
    fn test_windows_static_extension_prefer_in_memory_policy() -> Result<()> {
        for target_triple in WINDOWS_TARGET_TRIPLES.iter() {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: target_triple.to_string(),
                distribution_flavor: DistributionFlavor::StandaloneStatic,
                extension_module_filter: ExtensionModuleFilter::Minimal,
                libpython_link_mode: BinaryLibpythonLinkMode::Static,
                resources_policy: PythonResourcesPolicy::PreferInMemoryFallbackFilesystemRelative(
                    "prefix_policy".to_string(),
                ),
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            let mut builder = options.new_builder()?;

            let res =
                builder.add_python_extension_module(&EXTENSION_MODULE_SHARED_LIBRARY_ONLY, None);
            assert!(res.is_err());
            assert_eq!(res.err().unwrap().to_string(),
                "extension module shared_only cannot be materialized as a shared library because distribution does not support loading extension module shared libraries"
            );

            builder.add_python_extension_module(&EXTENSION_MODULE_OBJECT_FILES_ONLY, None)?;
            assert_extension_builtin(&builder, &EXTENSION_MODULE_OBJECT_FILES_ONLY)?;

            builder.add_python_extension_module(
                &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES,
                None,
            )?;
            assert_extension_builtin(&builder, &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES)?;
        }

        Ok(())
    }
}
