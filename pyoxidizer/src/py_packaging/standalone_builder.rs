// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    super::{
        binary::{
            pyembed_licenses, EmbeddedPythonContext, LibpythonLinkMode, PackedResourcesLoadMode,
            PythonBinaryBuilder, PythonLinkingInfo, ResourceAddCollectionContextCallback,
            WindowsRuntimeDllsMode,
        },
        config::{PyembedPackedResourcesSource, PyembedPythonInterpreterConfig},
        distribution::{AppleSdkInfo, BinaryLibpythonLinkMode, PythonDistribution},
        filtering::{filter_btreemap, resolve_resource_names_from_files},
        libpython::link_libpython,
        packaging_tool::{
            find_resources, pip_download, pip_install, read_virtualenv, setup_py_install,
        },
        standalone_distribution::StandaloneDistribution,
    },
    anyhow::{anyhow, Context, Result},
    once_cell::sync::Lazy,
    python_packaging::{
        bytecode::BytecodeCompiler,
        interpreter::MemoryAllocatorBackend,
        libpython::LibPythonBuildContext,
        licensing::derive_package_license_infos,
        location::AbstractResourceLocation,
        policy::PythonPackagingPolicy,
        resource::{
            PythonExtensionModule, PythonModuleSource, PythonPackageDistributionResource,
            PythonPackageResource, PythonResource,
        },
        resource_collection::{
            PrePackagedResource, PythonResourceAddCollectionContext, PythonResourceCollector,
        },
    },
    slog::warn,
    std::{
        collections::{BTreeMap, BTreeSet, HashMap},
        convert::TryInto,
        path::{Path, PathBuf},
        sync::Arc,
    },
    tugger_file_manifest::{File, FileData, FileEntry, FileManifest},
    tugger_licensing::{ComponentFlavor, LicensedComponent},
    tugger_windows::{find_visual_cpp_redistributable, VcRedistributablePlatform},
};

/// Libraries that we should not link against on Linux.
static LINUX_IGNORE_LIBRARIES: Lazy<Vec<&'static str>> = Lazy::new(|| vec!["dl", "m"]);

/// Libraries that we should not link against on macOS.
static MACOS_IGNORE_LIBRARIES: Lazy<Vec<&'static str>> = Lazy::new(|| vec!["dl", "m"]);

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
#[derive(Clone)]
pub struct StandalonePythonExecutableBuilder {
    /// The target triple we are running on.
    host_triple: String,

    /// The target triple we are building for.
    target_triple: String,

    /// The name of the executable to build.
    exe_name: String,

    /// The Python distribution being used to build this executable.
    host_distribution: Arc<dyn PythonDistribution>,

    /// The Python distribution this executable is targeting.
    target_distribution: Arc<StandaloneDistribution>,

    /// How libpython should be linked.
    link_mode: LibpythonLinkMode,

    /// Whether the built binary is capable of loading dynamically linked
    /// extension modules from memory.
    supports_in_memory_dynamically_linked_extension_loading: bool,

    /// Policy to apply to added resources.
    packaging_policy: PythonPackagingPolicy,

    /// Python resources to be embedded in the binary.
    resources_collector: PythonResourceCollector,

    /// How packed resources will be loaded at run-time.
    resources_load_mode: PackedResourcesLoadMode,

    /// Holds state necessary to link libpython.
    core_build_context: LibPythonBuildContext,

    /// Holds linking context for individual extensions.
    ///
    /// We need to track per-extension state separately since we need
    /// to support filtering extensions as part of building.
    extension_build_contexts: BTreeMap<String, LibPythonBuildContext>,

    /// Configuration of the embedded Python interpreter.
    config: PyembedPythonInterpreterConfig,

    /// Path to python executable that can be invoked at build time.
    host_python_exe: PathBuf,

    /// Value for the `windows_subsystem` Rust attribute for generated Rust projects.
    windows_subsystem: String,

    /// Path to install tcl/tk files into.
    tcl_files_path: Option<String>,

    /// Describes how Windows runtime DLLs should be handled during builds.
    windows_runtime_dlls_mode: WindowsRuntimeDllsMode,
}

impl StandalonePythonExecutableBuilder {
    #[allow(clippy::too_many_arguments)]
    pub fn from_distribution(
        host_distribution: Arc<dyn PythonDistribution>,
        target_distribution: Arc<StandaloneDistribution>,
        host_triple: String,
        target_triple: String,
        exe_name: String,
        link_mode: BinaryLibpythonLinkMode,
        packaging_policy: PythonPackagingPolicy,
        config: PyembedPythonInterpreterConfig,
    ) -> Result<Box<Self>> {
        let host_python_exe = host_distribution.python_exe_path().to_path_buf();
        let cache_tag = target_distribution.cache_tag.clone();

        let (supports_static_libpython, supports_dynamic_libpython) =
            target_distribution.libpython_link_support();

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
            target_distribution.supports_in_memory_shared_library_loading();

        let mut allowed_locations = vec![AbstractResourceLocation::from(
            packaging_policy.resources_location(),
        )];
        if let Some(fallback) = packaging_policy.resources_location_fallback() {
            allowed_locations.push(AbstractResourceLocation::from(fallback));
        }

        let mut allowed_extension_module_locations = vec![];

        if supports_in_memory_dynamically_linked_extension_loading
            && packaging_policy.allow_in_memory_shared_library_loading()
        {
            allowed_extension_module_locations.push(AbstractResourceLocation::InMemory);
        }

        if target_distribution.is_extension_module_file_loadable() {
            allowed_extension_module_locations.push(AbstractResourceLocation::RelativePath);
        }

        let allow_new_builtin_extension_modules = link_mode == LibpythonLinkMode::Static;

        let mut builder = Box::new(Self {
            host_triple,
            target_triple,
            exe_name,
            host_distribution,
            target_distribution,
            link_mode,
            supports_in_memory_dynamically_linked_extension_loading,
            packaging_policy: packaging_policy.clone(),
            resources_collector: PythonResourceCollector::new(
                allowed_locations,
                allowed_extension_module_locations,
                allow_new_builtin_extension_modules,
                packaging_policy.allow_files(),
                &cache_tag,
            ),
            resources_load_mode: PackedResourcesLoadMode::EmbeddedInBinary(
                "packed-resources".to_string(),
            ),
            core_build_context: LibPythonBuildContext::default(),
            extension_build_contexts: BTreeMap::new(),
            config,
            host_python_exe,
            windows_subsystem: "console".to_string(),
            tcl_files_path: None,
            windows_runtime_dlls_mode: WindowsRuntimeDllsMode::WhenPresent,
        });

        builder.add_distribution_core_state()?;

        Ok(builder)
    }

    fn add_distribution_core_state(&mut self) -> Result<()> {
        for component in pyembed_licenses().context("deriving pyembed component licenses")? {
            self.resources_collector.add_licensed_component(component)?;
        }

        self.core_build_context.inittab_cflags =
            Some(self.target_distribution.inittab_cflags.clone());

        for (name, path) in &self.target_distribution.includes {
            self.core_build_context
                .includes
                .insert(PathBuf::from(name), FileData::Path(path.clone()));
        }

        // Add the distribution's object files from Python core to linking context.
        for fs_path in self.target_distribution.objs_core.values() {
            // libpython generation derives its own `_PyImport_Inittab`. So ignore
            // the object file containing it.
            if fs_path == &self.target_distribution.inittab_object {
                continue;
            }

            self.core_build_context
                .object_files
                .push(FileData::Path(fs_path.clone()));
        }

        for entry in &self.target_distribution.links_core {
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

        for location in self.target_distribution.libraries.values() {
            let path = match location {
                FileData::Path(p) => p,
                FileData::Memory(_) => {
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

        if let Some(component) = &self.target_distribution.core_license {
            self.core_build_context
                .licensed_components
                .add_component(component.clone());
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
                let temp_dir = tempfile::Builder::new()
                    .prefix("pyoxidizer-build-exe-packaging")
                    .tempdir()?;
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
                libpython_filename = self.target_distribution.libpython_shared_library.clone();
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

    /// Resolves Windows runtime DLLs file needed for this binary given current settings.
    fn resolve_windows_runtime_dll_files(&self) -> Result<FileManifest> {
        let mut manifest = FileManifest::default();

        // If we require Windows CRT DLLs and we're told to install them, do that.
        if let Some((version, platform)) = self.vc_runtime_requirements() {
            if matches!(
                self.windows_runtime_dlls_mode(),
                WindowsRuntimeDllsMode::WhenPresent | WindowsRuntimeDllsMode::Always
            ) {
                match find_visual_cpp_redistributable(&version, platform) {
                    Ok(paths) => {
                        for path in paths {
                            let file_name = PathBuf::from(
                                path.file_name()
                                    .ok_or_else(|| anyhow!("could not determine file name"))?,
                            );
                            manifest.add_file_entry(
                                file_name,
                                FileEntry {
                                    data: FileData::Path(path),
                                    executable: true,
                                },
                            )?;
                        }
                    }
                    Err(err) => {
                        // Non-fatal in WhenPresent mode.
                        if matches!(
                            self.windows_runtime_dlls_mode(),
                            WindowsRuntimeDllsMode::Always
                        ) {
                            return Err(anyhow!(
                                "Windows Runtime DLLs mode of 'always' failed to locate files: {}",
                                err
                            ));
                        }
                    }
                }
            }
        }

        Ok(manifest)
    }
}

impl PythonBinaryBuilder for StandalonePythonExecutableBuilder {
    fn clone_trait(&self) -> Arc<dyn PythonBinaryBuilder> {
        Arc::new(self.clone())
    }

    fn name(&self) -> String {
        self.exe_name.clone()
    }

    fn libpython_link_mode(&self) -> LibpythonLinkMode {
        self.link_mode
    }

    fn target_triple(&self) -> &str {
        &self.target_triple
    }

    fn vc_runtime_requirements(&self) -> Option<(String, VcRedistributablePlatform)> {
        let platform = if self.target_triple.starts_with("i686-") {
            VcRedistributablePlatform::X86
        } else if self.target_triple.starts_with("x86_64-") {
            VcRedistributablePlatform::X64
        } else if self.target_triple.starts_with("aarch64-") {
            VcRedistributablePlatform::Arm64
        } else {
            return None;
        };

        if let Some(s) = self
            .target_distribution
            .crt_features
            .iter()
            .find(|s| s.starts_with("vcruntime:"))
        {
            Some((s.split(':').nth(1).unwrap()[0..2].to_string(), platform))
        } else {
            None
        }
    }

    fn cache_tag(&self) -> &str {
        self.target_distribution.cache_tag()
    }

    fn python_packaging_policy(&self) -> &PythonPackagingPolicy {
        &self.packaging_policy
    }

    fn host_python_exe_path(&self) -> &Path {
        &self.host_python_exe
    }

    fn target_python_exe_path(&self) -> &Path {
        &self.target_distribution.python_exe_path()
    }

    fn apple_sdk_info(&self) -> Option<&AppleSdkInfo> {
        self.target_distribution.apple_sdk_info()
    }

    fn windows_runtime_dlls_mode(&self) -> &WindowsRuntimeDllsMode {
        &self.windows_runtime_dlls_mode
    }

    fn set_windows_runtime_dlls_mode(&mut self, value: WindowsRuntimeDllsMode) {
        self.windows_runtime_dlls_mode = value;
    }

    fn tcl_files_path(&self) -> &Option<String> {
        &self.tcl_files_path
    }

    fn set_tcl_files_path(&mut self, value: Option<String>) {
        self.tcl_files_path = value;

        self.config.tcl_library = if let Some(path) = &self.tcl_files_path {
            Some(
                PathBuf::from("$ORIGIN").join(path).join(
                    self.target_distribution
                        .tcl_library_path_directory()
                        .expect("should have a tcl library path directory"),
                ),
            )
        } else {
            None
        };
    }

    fn windows_subsystem(&self) -> &str {
        &self.windows_subsystem
    }

    fn set_windows_subsystem(&mut self, value: &str) -> Result<()> {
        self.windows_subsystem = value.to_string();

        Ok(())
    }

    fn packed_resources_load_mode(&self) -> &PackedResourcesLoadMode {
        &self.resources_load_mode
    }

    fn set_packed_resources_load_mode(&mut self, load_mode: PackedResourcesLoadMode) {
        self.resources_load_mode = load_mode;
    }

    fn iter_resources<'a>(
        &'a self,
    ) -> Box<dyn Iterator<Item = (&'a String, &'a PrePackagedResource)> + 'a> {
        Box::new(self.resources_collector.iter_resources())
    }

    fn index_package_license_info_from_resources<'a>(
        &mut self,
        resources: &[PythonResource<'a>],
    ) -> Result<()> {
        for info in derive_package_license_infos(resources.iter())? {
            self.resources_collector
                .add_licensed_component(info.try_into()?)?;
        }

        Ok(())
    }

    fn pip_download(
        &mut self,
        logger: &slog::Logger,
        verbose: bool,
        args: &[String],
    ) -> Result<Vec<PythonResource>> {
        let resources = pip_download(
            logger,
            &*self.host_distribution,
            &*self.target_distribution,
            self.python_packaging_policy(),
            verbose,
            args,
        )
        .context("calling pip download")?;

        self.index_package_license_info_from_resources(&resources)
            .context("indexing package license metadata")?;

        Ok(resources)
    }

    fn pip_install(
        &mut self,
        logger: &slog::Logger,
        verbose: bool,
        install_args: &[String],
        extra_envs: &HashMap<String, String>,
    ) -> Result<Vec<PythonResource>> {
        let resources = pip_install(
            logger,
            &*self.target_distribution,
            self.python_packaging_policy(),
            self.link_mode,
            verbose,
            install_args,
            extra_envs,
        )
        .context("calling pip install")?;

        self.index_package_license_info_from_resources(&resources)
            .context("indexing package license metadata")?;

        Ok(resources)
    }

    fn read_package_root(
        &mut self,
        _logger: &slog::Logger,
        path: &Path,
        packages: &[String],
    ) -> Result<Vec<PythonResource>> {
        let resources = find_resources(
            &*self.target_distribution,
            self.python_packaging_policy(),
            path,
            None,
        )
        .context("finding resources")?
        .iter()
        .filter_map(|x| {
            if x.is_in_packages(packages) {
                Some(x.clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

        self.index_package_license_info_from_resources(&resources)
            .context("indexing package license metadata")?;

        Ok(resources)
    }

    fn read_virtualenv(
        &mut self,
        _logger: &slog::Logger,
        path: &Path,
    ) -> Result<Vec<PythonResource>> {
        let resources = read_virtualenv(
            &*self.target_distribution,
            self.python_packaging_policy(),
            path,
        )
        .context("reading virtualenv")?;

        self.index_package_license_info_from_resources(&resources)
            .context("indexing package license metadata")?;

        Ok(resources)
    }

    fn setup_py_install(
        &mut self,
        logger: &slog::Logger,
        package_path: &Path,
        verbose: bool,
        extra_envs: &HashMap<String, String>,
        extra_global_arguments: &[String],
    ) -> Result<Vec<PythonResource>> {
        let resources = setup_py_install(
            logger,
            &*self.target_distribution,
            self.python_packaging_policy(),
            self.link_mode,
            package_path,
            verbose,
            extra_envs,
            extra_global_arguments,
        )
        .context("running setup.py install")?;

        self.index_package_license_info_from_resources(&resources)
            .context("indexing package license metadata")?;

        Ok(resources)
    }

    fn add_distribution_resources(
        &mut self,
        callback: Option<ResourceAddCollectionContextCallback>,
    ) -> Result<()> {
        let core_component = self
            .target_distribution
            .core_license
            .clone()
            .ok_or_else(|| anyhow!("could not resolve Python standard library license"))?;

        self.resources_collector
            .add_licensed_component(core_component.clone())?;

        // TODO consolidate into loop below.
        for ext in self.packaging_policy.resolve_python_extension_modules(
            self.target_distribution.extension_modules.values(),
            &self.target_triple,
        )? {
            let resource = (&ext).into();
            let mut add_context = self
                .packaging_policy
                .derive_add_collection_context(&resource);

            if let Some(callback) = &callback {
                callback(&self.packaging_policy, &resource, &mut add_context)?;
            }

            if let Some(component) = &ext.license {
                self.resources_collector
                    .add_licensed_component(component.clone())?;
            }

            self.add_python_extension_module(&ext, Some(add_context))?;
        }

        for resource in self
            .target_distribution
            .python_resources()
            .iter()
            .filter(|r| match r {
                PythonResource::ModuleSource(_) => true,
                PythonResource::PackageResource(_) => true,
                PythonResource::ModuleBytecode(_) => false,
                PythonResource::ModuleBytecodeRequest(_) => false,
                PythonResource::ExtensionModule(_) => false,
                PythonResource::PackageDistributionResource(_) => false,
                PythonResource::EggFile(_) => false,
                PythonResource::PathExtension(_) => false,
                PythonResource::File(_) => false,
            })
        {
            let mut add_context = self
                .packaging_policy
                .derive_add_collection_context(&resource);

            if let Some(callback) = &callback {
                callback(&self.packaging_policy, resource, &mut add_context)?;
            }

            match resource {
                PythonResource::ModuleSource(source) => {
                    let mut component = LicensedComponent::new_spdx(
                        source.top_level_package(),
                        &core_component
                            .spdx_expression()
                            .ok_or_else(|| anyhow!("should have resolved SPDX expression"))?
                            .to_string(),
                    )?;
                    component.set_flavor(ComponentFlavor::PythonPackage);

                    self.resources_collector.add_licensed_component(component)?;
                    self.add_python_module_source(source, Some(add_context))?;
                }
                PythonResource::PackageResource(r) => {
                    self.add_python_package_resource(r, Some(add_context))?;
                }
                _ => panic!("should not get here since resources should be filtered above"),
            }
        }

        Ok(())
    }

    fn add_python_module_source(
        &mut self,
        module: &PythonModuleSource,
        add_context: Option<PythonResourceAddCollectionContext>,
    ) -> Result<()> {
        let add_context = add_context.unwrap_or_else(|| {
            self.packaging_policy
                .derive_add_collection_context(&module.into())
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
                .derive_add_collection_context(&resource.into())
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
                .derive_add_collection_context(&resource.into())
        });

        self.resources_collector
            .add_python_package_distribution_resource_with_context(resource, &add_context)
    }

    fn add_python_extension_module(
        &mut self,
        extension_module: &PythonExtensionModule,
        add_context: Option<PythonResourceAddCollectionContext>,
    ) -> Result<()> {
        let add_context = add_context.unwrap_or_else(|| {
            self.packaging_policy
                .derive_add_collection_context(&extension_module.into())
        });

        if let Some(mut build_context) = self
            .resources_collector
            .add_python_extension_module_with_context(extension_module, &add_context)?
        {
            // Resources collector doesn't doesn't know about ignored libraries. So filter
            // them here.
            build_context.static_libraries = build_context
                .static_libraries
                .iter()
                .filter(|x| {
                    !ignored_libraries_for_target(&self.target_triple).contains(&x.as_str())
                })
                .cloned()
                .collect::<BTreeSet<_>>();
            build_context.dynamic_libraries = build_context
                .dynamic_libraries
                .iter()
                .filter(|x| {
                    !ignored_libraries_for_target(&self.target_triple).contains(&x.as_str())
                })
                .cloned()
                .collect::<BTreeSet<_>>();

            self.extension_build_contexts
                .insert(extension_module.name.clone(), build_context);
        }

        Ok(())
    }

    fn add_file_data(
        &mut self,
        file: &File,
        add_context: Option<PythonResourceAddCollectionContext>,
    ) -> Result<()> {
        let add_context = add_context.unwrap_or_else(|| {
            self.packaging_policy
                .derive_add_collection_context(&file.into())
        });

        self.resources_collector
            .add_file_data_with_context(file, &add_context)
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
        self.config.allocator_backend == MemoryAllocatorBackend::Jemalloc
    }

    fn requires_mimalloc(&self) -> bool {
        self.config.allocator_backend == MemoryAllocatorBackend::Mimalloc
    }

    fn requires_snmalloc(&self) -> bool {
        self.config.allocator_backend == MemoryAllocatorBackend::Snmalloc
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

        let license_report = self.resources_collector.generate_license_report()?;
        if license_report.no_license_packages.is_empty() {
            warn!(logger, "All Python packages have license metadata");
        } else {
            warn!(
                logger,
                "{} Python packages lack software licenses: {:?}",
                license_report.no_license_packages.len(),
                license_report.no_license_packages
            );
        }

        if license_report.non_spdx_by_package.is_empty() {
            warn!(logger, "No Python packages with non-SPDX licenses");
        } else {
            warn!(
                logger,
                "{} non-SPDX licenses seen",
                license_report.non_spdx_by_package.len()
            );
            for (license, packages) in &license_report.non_spdx_by_package {
                warn!(logger, "license: {}; packages: {:?}", license, packages);
            }
        }

        warn!(
            logger,
            "{} SPDX licenses encountered:",
            license_report.spdx_by_package.len()
        );
        for (license, packages) in &license_report.spdx_by_package {
            warn!(logger, "license: {}; packages: {:?}", license, packages);
        }

        let compiled_resources = {
            let temp_dir = tempfile::TempDir::new()?;
            let mut compiler = BytecodeCompiler::new(self.host_python_exe_path(), temp_dir.path())?;
            self.resources_collector.compile_resources(&mut compiler)?
        };

        let mut pending_resources = vec![];
        let mut extra_files = FileManifest::default();

        for (path, location, executable) in &compiled_resources.extra_files {
            extra_files.add_file_entry(
                path,
                FileEntry {
                    data: location.resolve()?.into(),
                    executable: *executable,
                },
            )?;
        }

        let mut config = self.config.clone();

        match &self.resources_load_mode {
            PackedResourcesLoadMode::None => {}
            PackedResourcesLoadMode::EmbeddedInBinary(filename) => {
                pending_resources.push((compiled_resources, PathBuf::from(filename)));
                config
                    .packed_resources
                    .push(PyembedPackedResourcesSource::MemoryIncludeBytes(
                        PathBuf::from(filename),
                    ));
            }
            PackedResourcesLoadMode::BinaryRelativePathMemoryMapped(path) => {
                // We need to materialize the file in extra_files. So compile now.
                let mut buffer = vec![];
                compiled_resources
                    .write_packed_resources(&mut buffer)
                    .context("serializing packed resources")?;
                extra_files.add_file_entry(
                    Path::new(path),
                    FileEntry {
                        data: FileData::Memory(buffer),
                        executable: false,
                    },
                )?;

                config
                    .packed_resources
                    .push(PyembedPackedResourcesSource::MemoryMappedPath(
                        PathBuf::from("$ORIGIN").join(path),
                    ));
            }
        }

        let linking_info = self.resolve_python_linking_info(logger, opt_level)?;

        if self.link_mode == LibpythonLinkMode::Dynamic {
            if let Some(p) = &self.target_distribution.libpython_shared_library {
                let manifest_path = Path::new(p.file_name().unwrap());
                let content = FileEntry {
                    data: std::fs::read(&p)?.into(),
                    executable: false,
                };

                extra_files.add_file_entry(&manifest_path, content)?;

                // Always look for and add the python3.dll variant if it exists. This DLL
                // exports the stable subset of the Python ABI and it is required by some
                // extensions.
                let python3_dll_path = p.with_file_name("python3.dll");
                let manifest_path = Path::new(python3_dll_path.file_name().unwrap());
                if python3_dll_path.exists() {
                    let content = FileEntry {
                        data: std::fs::read(&python3_dll_path)?.into(),
                        executable: false,
                    };

                    extra_files.add_file_entry(&manifest_path, content)?;
                }
            }
        }

        if let Some(tcl_files_path) = self.tcl_files_path() {
            for (path, location) in self.target_distribution.tcl_files()? {
                let install_path = PathBuf::from(tcl_files_path).join(path);

                extra_files.add_file_entry(
                    &install_path,
                    FileEntry {
                        data: location.resolve()?.into(),
                        executable: false,
                    },
                )?;
            }
        }

        // Install Windows runtime DLLs if told to do so.
        extra_files.add_manifest(&self.resolve_windows_runtime_dll_files()?)?;

        Ok(EmbeddedPythonContext {
            config,
            linking_info,
            pending_resources,
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
        once_cell::sync::Lazy,
        python_packaging::{location::ConcreteResourceLocation, policy::ExtensionModuleFilter},
        std::ops::DerefMut,
        tugger_licensing::LicensedComponents,
    };

    #[cfg(target_os = "linux")]
    use python_packaging::resource::LibraryDependency;

    pub static WINDOWS_TARGET_TRIPLES: Lazy<Vec<&'static str>> =
        Lazy::new(|| vec!["i686-pc-windows-msvc", "x86_64-pc-windows-msvc"]);

    pub static MACOS_TARGET_TRIPLES: [&'static str; 2] =
        ["aarch64-apple-darwin", "x86_64-apple-darwin"];

    /// An extension module represented by a shared library file.
    pub static EXTENSION_MODULE_SHARED_LIBRARY_ONLY: Lazy<PythonExtensionModule> =
        Lazy::new(|| PythonExtensionModule {
            name: "shared_only".to_string(),
            init_fn: Some("PyInit_shared_only".to_string()),
            extension_file_suffix: ".so".to_string(),
            shared_library: Some(FileData::Memory(vec![42])),
            object_file_data: vec![],
            is_package: false,
            link_libraries: vec![],
            is_stdlib: false,
            builtin_default: false,
            required: false,
            variant: None,
            license: None,
        });

    /// An extension module represented by only object files.
    pub static EXTENSION_MODULE_OBJECT_FILES_ONLY: Lazy<PythonExtensionModule> =
        Lazy::new(|| PythonExtensionModule {
            name: "object_files_only".to_string(),
            init_fn: Some("PyInit_object_files_only".to_string()),
            extension_file_suffix: ".so".to_string(),
            shared_library: None,
            object_file_data: vec![FileData::Memory(vec![0]), FileData::Memory(vec![1])],
            is_package: false,
            link_libraries: vec![],
            is_stdlib: false,
            builtin_default: false,
            required: false,
            variant: None,
            license: None,
        });

    /// An extension module with both a shared library and object files.
    pub static EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES: Lazy<PythonExtensionModule> =
        Lazy::new(|| PythonExtensionModule {
            name: "shared_and_object_files".to_string(),
            init_fn: Some("PyInit_shared_and_object_files".to_string()),
            extension_file_suffix: ".so".to_string(),
            shared_library: Some(FileData::Memory(b"shared".to_vec())),
            object_file_data: vec![FileData::Memory(vec![0]), FileData::Memory(vec![1])],
            is_package: false,
            link_libraries: vec![],
            is_stdlib: false,
            builtin_default: false,
            required: false,
            variant: None,
            license: None,
        });

    /// Defines construction options for a `StandalonePythonExecutableBuilder`.
    ///
    /// This is mostly intended to be used by tests, to reduce boilerplate for
    /// constructing instances.
    pub struct StandalonePythonExecutableBuilderOptions {
        pub host_triple: String,
        pub target_triple: String,
        pub distribution_version: Option<String>,
        pub distribution_flavor: DistributionFlavor,
        pub app_name: String,
        pub libpython_link_mode: BinaryLibpythonLinkMode,
        pub extension_module_filter: Option<ExtensionModuleFilter>,
        pub resources_location: Option<ConcreteResourceLocation>,
        pub resources_location_fallback: Option<Option<ConcreteResourceLocation>>,
        pub allow_in_memory_shared_library_loading: Option<bool>,
        pub config: PyembedPythonInterpreterConfig,
    }

    impl Default for StandalonePythonExecutableBuilderOptions {
        fn default() -> Self {
            Self {
                host_triple: env!("HOST").to_string(),
                target_triple: env!("HOST").to_string(),
                distribution_version: None,
                distribution_flavor: DistributionFlavor::Standalone,
                app_name: "testapp".to_string(),
                libpython_link_mode: BinaryLibpythonLinkMode::Default,
                extension_module_filter: None,
                resources_location: None,
                resources_location_fallback: None,
                allow_in_memory_shared_library_loading: None,
                config: PyembedPythonInterpreterConfig::default(),
            }
        }
    }

    impl StandalonePythonExecutableBuilderOptions {
        pub fn new_builder(&self) -> Result<Box<StandalonePythonExecutableBuilder>> {
            let target_record = PYTHON_DISTRIBUTIONS
                .find_distribution(
                    &self.target_triple,
                    &self.distribution_flavor,
                    self.distribution_version.as_deref(),
                )
                .ok_or_else(|| anyhow!("could not find target Python distribution"))?;

            let target_distribution = get_distribution(&target_record.location)?;

            let host_distribution = if target_distribution
                .compatible_host_triples()
                .contains(&self.host_triple)
            {
                target_distribution.clone_trait()
            } else {
                let host_record = PYTHON_DISTRIBUTIONS
                    .find_distribution(&self.host_triple, &DistributionFlavor::Standalone, None)
                    .ok_or_else(|| anyhow!("could not find host Python distribution"))?;

                get_distribution(&host_record.location)?.clone_trait()
            };

            let mut policy = target_distribution.create_packaging_policy()?;
            if let Some(filter) = &self.extension_module_filter {
                policy.set_extension_module_filter(filter.clone());
            }
            if let Some(location) = &self.resources_location {
                policy.set_resources_location(location.clone());
            }
            if let Some(location) = &self.resources_location_fallback {
                policy.set_resources_location_fallback(location.clone());
            }
            if let Some(value) = &self.allow_in_memory_shared_library_loading {
                policy.set_allow_in_memory_shared_library_loading(*value);
            }

            let mut builder = StandalonePythonExecutableBuilder::from_distribution(
                host_distribution,
                target_distribution,
                self.host_triple.clone(),
                self.target_triple.clone(),
                self.app_name.clone(),
                self.libpython_link_mode.clone(),
                policy,
                self.config.clone(),
            )?;

            builder.add_distribution_resources(None)?;

            Ok(builder)
        }
    }

    fn assert_extension_builtin(
        builder: &StandalonePythonExecutableBuilder,
        extension: &PythonExtensionModule,
    ) {
        assert_eq!(
            builder.iter_resources().find_map(|(name, r)| {
                if *name == extension.name {
                    Some(r)
                } else {
                    None
                }
            }),
            Some(&PrePackagedResource {
                is_builtin_extension_module: true,
                name: extension.name.clone(),
                ..PrePackagedResource::default()
            }),
            "extension module {} is built-in",
            extension.name,
        );

        assert_eq!(
            builder.extension_build_contexts.get(&extension.name),
            Some(&LibPythonBuildContext {
                object_files: extension.object_file_data.clone(),
                init_functions: [(
                    extension.name.to_string(),
                    extension.init_fn.as_ref().clone().unwrap().to_string()
                )]
                .iter()
                .cloned()
                .collect(),
                ..LibPythonBuildContext::default()
            }),
            "build context for extension module {} is present",
            extension.name
        );
    }

    fn assert_extension_shared_library(
        builder: &StandalonePythonExecutableBuilder,
        extension: &PythonExtensionModule,
        location: ConcreteResourceLocation,
    ) {
        let mut entry = PrePackagedResource {
            is_extension_module: true,
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
    }

    fn licensed_components_from_extension(ext: &PythonExtensionModule) -> LicensedComponents {
        let mut r = LicensedComponents::default();

        if let Some(component) = &ext.license {
            r.add_component(component.clone());
        }

        r
    }

    #[test]
    fn test_write_embedded_files() -> Result<()> {
        let logger = get_logger()?;
        let options = StandalonePythonExecutableBuilderOptions::default();
        let exe = options.new_builder()?;
        let embedded = exe.to_embedded_python_context(&logger, "0")?;

        let temp_dir = tempfile::Builder::new()
            .prefix("pyoxidizer-test")
            .tempdir()?;

        embedded.write_files(temp_dir.path())?;

        let resources_path = temp_dir.path().join("packed-resources");
        assert!(resources_path.exists(), "packed-resources file exists");

        Ok(())
    }

    #[test]
    fn test_memory_mapped_file_resources() -> Result<()> {
        let logger = get_logger()?;
        let options = StandalonePythonExecutableBuilderOptions::default();
        let mut exe = options.new_builder()?;
        exe.resources_load_mode =
            PackedResourcesLoadMode::BinaryRelativePathMemoryMapped("resources".into());

        let embedded = exe.to_embedded_python_context(&logger, "0")?;

        assert_eq!(
            &embedded.config.packed_resources,
            &vec![PyembedPackedResourcesSource::MemoryMappedPath(
                "$ORIGIN/resources".into()
            )],
            "load mode should have mapped to MemoryMappedPath"
        );

        assert!(
            embedded.extra_files.has_path(Path::new("resources")),
            "resources file should be present in extra files manifest"
        );

        Ok(())
    }

    #[test]
    fn test_minimal_extensions_present() -> Result<()> {
        let options = StandalonePythonExecutableBuilderOptions::default();
        let builder = options.new_builder()?;

        let expected = builder
            .target_distribution
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

        // Spot check.
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
    fn test_linux_distribution_extensions() -> Result<()> {
        for libpython_link_mode in vec![
            BinaryLibpythonLinkMode::Static,
            BinaryLibpythonLinkMode::Dynamic,
        ] {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                extension_module_filter: Some(ExtensionModuleFilter::All),
                libpython_link_mode,
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            let builder = options.new_builder()?;

            let builtin_names = builder.extension_build_contexts.keys().collect::<Vec<_>>();

            // All extensions compiled as built-ins by default.
            for (name, _) in builder.target_distribution.extension_modules.iter() {
                if builder
                    .python_packaging_policy()
                    .broken_extensions_for_triple(&builder.target_triple)
                    .unwrap_or(&vec![])
                    .contains(name)
                {
                    assert!(!builtin_names.contains(&name))
                } else {
                    assert!(builtin_names.contains(&name));
                }
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
                extension_module_filter: Some(ExtensionModuleFilter::Minimal),
                libpython_link_mode,
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            let mut builder = options.new_builder()?;

            // When adding an extension module in static link mode, it gets
            // added as a built-in and linked with libpython.

            let sqlite = builder
                .target_distribution
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
                    static_libraries: ["sqlite3".to_string()].iter().cloned().collect(),
                    init_functions: [("_sqlite3".to_string(), "PyInit__sqlite3".to_string())]
                        .iter()
                        .cloned()
                        .collect(),
                    licensed_components: licensed_components_from_extension(&sqlite),
                    ..LibPythonBuildContext::default()
                })
            );

            assert_eq!(
                builder
                    .iter_resources()
                    .find_map(|(name, r)| if *name == "_sqlite3" { Some(r) } else { None }),
                Some(&PrePackagedResource {
                    is_builtin_extension_module: true,
                    name: "_sqlite3".to_string(),
                    ..PrePackagedResource::default()
                })
            );
        }

        Ok(())
    }

    #[test]
    fn test_linux_extension_in_memory_only() -> Result<()> {
        for libpython_link_mode in vec![
            BinaryLibpythonLinkMode::Static,
            BinaryLibpythonLinkMode::Dynamic,
        ] {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                extension_module_filter: Some(ExtensionModuleFilter::Minimal),
                libpython_link_mode: libpython_link_mode.clone(),
                resources_location: Some(ConcreteResourceLocation::InMemory),
                resources_location_fallback: Some(None),
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

            let res =
                builder.add_python_extension_module(&EXTENSION_MODULE_OBJECT_FILES_ONLY, None);
            match libpython_link_mode {
                BinaryLibpythonLinkMode::Static => {
                    assert!(res.is_ok());
                    assert_extension_builtin(&builder, &EXTENSION_MODULE_OBJECT_FILES_ONLY);
                }
                BinaryLibpythonLinkMode::Dynamic => {
                    assert!(res.is_err());
                    assert_eq!(res.err().unwrap().to_string(), "extension module object_files_only cannot be loaded from memory but memory loading required");
                }
                BinaryLibpythonLinkMode::Default => {
                    panic!("should not get here");
                }
            }

            let res = builder.add_python_extension_module(
                &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES,
                None,
            );
            match libpython_link_mode {
                BinaryLibpythonLinkMode::Static => {
                    assert!(res.is_ok());
                    assert_extension_builtin(
                        &builder,
                        &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES,
                    );
                }
                BinaryLibpythonLinkMode::Dynamic => {
                    assert!(res.is_err());
                    assert_eq!(res.err().unwrap().to_string(), "extension module shared_and_object_files cannot be loaded from memory but memory loading required")
                }
                BinaryLibpythonLinkMode::Default => {
                    panic!("should not get here");
                }
            }
        }

        Ok(())
    }

    #[test]
    fn test_linux_extension_prefer_in_memory() -> Result<()> {
        for libpython_link_mode in vec![
            BinaryLibpythonLinkMode::Static,
            BinaryLibpythonLinkMode::Dynamic,
        ] {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                extension_module_filter: Some(ExtensionModuleFilter::Minimal),
                libpython_link_mode: libpython_link_mode.clone(),
                resources_location: Some(ConcreteResourceLocation::InMemory),
                resources_location_fallback: Some(Some(ConcreteResourceLocation::RelativePath(
                    "prefix_policy".to_string(),
                ))),
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            let mut builder = options.new_builder()?;

            builder.add_python_extension_module(&EXTENSION_MODULE_SHARED_LIBRARY_ONLY, None)?;
            assert_extension_shared_library(
                &builder,
                &EXTENSION_MODULE_SHARED_LIBRARY_ONLY,
                ConcreteResourceLocation::RelativePath("prefix_policy".to_string()),
            );

            let res =
                builder.add_python_extension_module(&EXTENSION_MODULE_OBJECT_FILES_ONLY, None);
            match libpython_link_mode {
                BinaryLibpythonLinkMode::Static => {
                    assert!(res.is_ok());
                    assert_extension_builtin(&builder, &EXTENSION_MODULE_OBJECT_FILES_ONLY);
                }
                BinaryLibpythonLinkMode::Dynamic => {
                    assert!(res.is_err());
                    assert_eq!(
                        res.err().unwrap().to_string(),
                        "no shared library data present"
                    );
                }
                BinaryLibpythonLinkMode::Default => {
                    panic!("should not get here");
                }
            }

            builder.add_python_extension_module(
                &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES,
                None,
            )?;
            match libpython_link_mode {
                BinaryLibpythonLinkMode::Static => {
                    assert_extension_builtin(
                        &builder,
                        &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES,
                    );
                }
                BinaryLibpythonLinkMode::Dynamic => {
                    assert_extension_shared_library(
                        &builder,
                        &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES,
                        ConcreteResourceLocation::RelativePath("prefix_policy".to_string()),
                    );
                }
                BinaryLibpythonLinkMode::Default => {
                    panic!("should not get here");
                }
            }
        }
        Ok(())
    }

    #[test]
    fn test_linux_distribution_extension_filesystem_relative_only() -> Result<()> {
        for libpython_link_mode in vec![
            BinaryLibpythonLinkMode::Static,
            BinaryLibpythonLinkMode::Dynamic,
        ] {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                extension_module_filter: Some(ExtensionModuleFilter::Minimal),
                libpython_link_mode,
                resources_location: Some(ConcreteResourceLocation::RelativePath(
                    "prefix_policy".to_string(),
                )),
                resources_location_fallback: Some(None),
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            let mut builder = options.new_builder()?;

            let ext = builder
                .target_distribution
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
                    object_files: ext.object_file_data.clone(),
                    static_libraries: ["sqlite3".to_string()].iter().cloned().collect(),
                    init_functions: [("_sqlite3".to_string(), "PyInit__sqlite3".to_string())]
                        .iter()
                        .cloned()
                        .collect(),
                    licensed_components: licensed_components_from_extension(&ext),
                    ..LibPythonBuildContext::default()
                })
            );

            assert_eq!(
                builder
                    .iter_resources()
                    .find_map(|(name, r)| if *name == "_sqlite3" { Some(r) } else { None }),
                Some(&PrePackagedResource {
                    is_builtin_extension_module: true,
                    name: "_sqlite3".to_string(),
                    ..PrePackagedResource::default()
                })
            );
        }

        Ok(())
    }

    #[test]
    fn test_linux_extension_filesystem_relative_only() -> Result<()> {
        for libpython_link_mode in vec![
            BinaryLibpythonLinkMode::Static,
            BinaryLibpythonLinkMode::Dynamic,
        ] {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                extension_module_filter: Some(ExtensionModuleFilter::Minimal),
                libpython_link_mode: libpython_link_mode.clone(),
                resources_location: Some(ConcreteResourceLocation::RelativePath(
                    "prefix_policy".to_string(),
                )),
                resources_location_fallback: Some(None),
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            let mut builder = options.new_builder()?;

            builder.add_python_extension_module(&EXTENSION_MODULE_SHARED_LIBRARY_ONLY, None)?;
            assert_extension_shared_library(
                &builder,
                &EXTENSION_MODULE_SHARED_LIBRARY_ONLY,
                ConcreteResourceLocation::RelativePath("prefix_policy".to_string()),
            );

            let res =
                builder.add_python_extension_module(&EXTENSION_MODULE_OBJECT_FILES_ONLY, None);
            match libpython_link_mode {
                BinaryLibpythonLinkMode::Static => {
                    assert!(res.is_ok());
                    assert_extension_builtin(&builder, &EXTENSION_MODULE_OBJECT_FILES_ONLY);
                }
                BinaryLibpythonLinkMode::Dynamic => {
                    assert!(res.is_err());
                    assert_eq!(res.err().unwrap().to_string(), "extension module object_files_only cannot be materialized as a shared library extension but filesystem loading required");
                }
                BinaryLibpythonLinkMode::Default => {
                    panic!("should not get here");
                }
            }

            builder.add_python_extension_module(
                &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES,
                None,
            )?;
            assert_extension_shared_library(
                &builder,
                &EXTENSION_MODULE_SHARED_LIBRARY_ONLY,
                ConcreteResourceLocation::RelativePath("prefix_policy".to_string()),
            );
        }

        Ok(())
    }

    #[test]
    fn test_linux_musl_distribution_dynamic() {
        let options = StandalonePythonExecutableBuilderOptions {
            target_triple: "x86_64-unknown-linux-musl".to_string(),
            extension_module_filter: Some(ExtensionModuleFilter::Minimal),
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
    }

    #[test]
    fn test_linux_musl_distribution_extensions() -> Result<()> {
        let options = StandalonePythonExecutableBuilderOptions {
            target_triple: "x86_64-unknown-linux-musl".to_string(),
            extension_module_filter: Some(ExtensionModuleFilter::All),
            ..StandalonePythonExecutableBuilderOptions::default()
        };

        let builder = options.new_builder()?;

        // All extensions for musl Linux are built-in because dynamic linking
        // not possible.
        for name in builder.target_distribution.extension_modules.keys() {
            if builder
                .python_packaging_policy()
                .broken_extensions_for_triple(&builder.target_triple)
                .unwrap_or(&vec![])
                .contains(name)
            {
                assert!(!builder.extension_build_contexts.keys().any(|e| name == e));
            } else {
                assert!(builder.extension_build_contexts.keys().any(|e| name == e));
            }
        }

        Ok(())
    }

    #[test]
    fn test_linux_musl_distribution_extension_static() -> Result<()> {
        let options = StandalonePythonExecutableBuilderOptions {
            target_triple: "x86_64-unknown-linux-musl".to_string(),
            extension_module_filter: Some(ExtensionModuleFilter::Minimal),
            libpython_link_mode: BinaryLibpythonLinkMode::Static,
            ..StandalonePythonExecutableBuilderOptions::default()
        };

        let mut builder = options.new_builder()?;

        // When adding an extension module in static link mode, it gets
        // added as a built-in and linked with libpython.

        let sqlite = builder
            .target_distribution
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
                static_libraries: ["sqlite3".to_string()].iter().cloned().collect(),
                init_functions: [("_sqlite3".to_string(), "PyInit__sqlite3".to_string())]
                    .iter()
                    .cloned()
                    .collect(),
                licensed_components: licensed_components_from_extension(&sqlite),
                ..LibPythonBuildContext::default()
            })
        );

        assert_eq!(
            builder
                .iter_resources()
                .find_map(|(name, r)| if *name == "_sqlite3" { Some(r) } else { None }),
            Some(&PrePackagedResource {
                is_builtin_extension_module: true,
                name: "_sqlite3".to_string(),
                ..PrePackagedResource::default()
            })
        );

        Ok(())
    }

    #[test]
    fn test_linux_musl_distribution_extension_filesystem_relative_only() -> Result<()> {
        let options = StandalonePythonExecutableBuilderOptions {
            target_triple: "x86_64-unknown-linux-musl".to_string(),
            extension_module_filter: Some(ExtensionModuleFilter::Minimal),
            libpython_link_mode: BinaryLibpythonLinkMode::Static,
            resources_location: Some(ConcreteResourceLocation::RelativePath(
                "prefix_policy".to_string(),
            )),
            resources_location_fallback: Some(None),
            ..StandalonePythonExecutableBuilderOptions::default()
        };

        let mut builder = options.new_builder()?;

        let ext = builder
            .target_distribution
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
                object_files: ext.object_file_data.clone(),
                static_libraries: ["sqlite3".to_string()].iter().cloned().collect(),
                init_functions: [("_sqlite3".to_string(), "PyInit__sqlite3".to_string())]
                    .iter()
                    .cloned()
                    .collect(),
                licensed_components: licensed_components_from_extension(&ext),
                ..LibPythonBuildContext::default()
            })
        );

        assert_eq!(
            builder
                .iter_resources()
                .find_map(|(name, r)| if *name == "_sqlite3" { Some(r) } else { None }),
            Some(&PrePackagedResource {
                is_builtin_extension_module: true,
                name: "_sqlite3".to_string(),
                ..PrePackagedResource::default()
            })
        );

        Ok(())
    }

    #[test]
    fn test_linux_musl_extension_in_memory_only() -> Result<()> {
        let options = StandalonePythonExecutableBuilderOptions {
            target_triple: "x86_64-unknown-linux-musl".to_string(),
            extension_module_filter: Some(ExtensionModuleFilter::Minimal),
            libpython_link_mode: BinaryLibpythonLinkMode::Static,
            resources_location: Some(ConcreteResourceLocation::InMemory),
            resources_location_fallback: Some(None),
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
        assert_extension_builtin(&builder, &EXTENSION_MODULE_OBJECT_FILES_ONLY);

        builder
            .add_python_extension_module(&EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES, None)?;
        assert_extension_builtin(&builder, &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES);

        Ok(())
    }

    #[test]
    fn test_linux_musl_extension_prefer_in_memory() -> Result<()> {
        let options = StandalonePythonExecutableBuilderOptions {
            target_triple: "x86_64-unknown-linux-musl".to_string(),
            extension_module_filter: Some(ExtensionModuleFilter::Minimal),
            libpython_link_mode: BinaryLibpythonLinkMode::Static,
            resources_location: Some(ConcreteResourceLocation::InMemory),
            resources_location_fallback: Some(Some(ConcreteResourceLocation::RelativePath(
                "prefix_policy".to_string(),
            ))),
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
        assert_extension_builtin(&builder, &EXTENSION_MODULE_OBJECT_FILES_ONLY);

        builder
            .add_python_extension_module(&EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES, None)?;
        assert_extension_builtin(&builder, &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES);

        Ok(())
    }

    #[test]
    fn test_macos_distribution_extensions() -> Result<()> {
        for target_triple in MACOS_TARGET_TRIPLES.iter() {
            for libpython_link_mode in vec![
                BinaryLibpythonLinkMode::Static,
                BinaryLibpythonLinkMode::Dynamic,
            ] {
                let options = StandalonePythonExecutableBuilderOptions {
                    target_triple: target_triple.to_string(),
                    libpython_link_mode,
                    extension_module_filter: Some(ExtensionModuleFilter::All),
                    ..StandalonePythonExecutableBuilderOptions::default()
                };

                let builder = options.new_builder()?;

                let builtin_names = builder.extension_build_contexts.keys().collect::<Vec<_>>();

                // All extensions compiled as built-ins by default.
                for (name, _) in builder.target_distribution.extension_modules.iter() {
                    if builder
                        .python_packaging_policy()
                        .broken_extensions_for_triple(&builder.target_triple)
                        .unwrap_or(&vec![])
                        .contains(name)
                    {
                        assert!(!builtin_names.contains(&name))
                    } else {
                        assert!(builtin_names.contains(&name));
                    }
                }
            }
        }

        Ok(())
    }

    #[test]
    fn test_macos_distribution_extension_static() -> Result<()> {
        for target_triple in MACOS_TARGET_TRIPLES.iter() {
            for libpython_link_mode in vec![
                BinaryLibpythonLinkMode::Static,
                BinaryLibpythonLinkMode::Dynamic,
            ] {
                let options = StandalonePythonExecutableBuilderOptions {
                    target_triple: target_triple.to_string(),
                    extension_module_filter: Some(ExtensionModuleFilter::Minimal),
                    libpython_link_mode,
                    ..StandalonePythonExecutableBuilderOptions::default()
                };

                let mut builder = options.new_builder()?;

                // When adding an extension module in static link mode, it gets
                // added as a built-in and linked with libpython.

                let sqlite = builder
                    .target_distribution
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
                        static_libraries: ["sqlite3".to_string()].iter().cloned().collect(),
                        init_functions: [("_sqlite3".to_string(), "PyInit__sqlite3".to_string())]
                            .iter()
                            .cloned()
                            .collect(),
                        licensed_components: licensed_components_from_extension(&sqlite),
                        ..LibPythonBuildContext::default()
                    })
                );

                assert_eq!(
                    builder
                        .iter_resources()
                        .find_map(|(name, r)| if *name == "_sqlite3" { Some(r) } else { None }),
                    Some(&PrePackagedResource {
                        is_builtin_extension_module: true,
                        name: "_sqlite3".to_string(),
                        ..PrePackagedResource::default()
                    })
                );
            }
        }

        Ok(())
    }

    #[test]
    fn test_macos_distribution_extension_filesystem_relative_only() -> Result<()> {
        for target_triple in MACOS_TARGET_TRIPLES.iter() {
            for libpython_link_mode in vec![
                BinaryLibpythonLinkMode::Static,
                BinaryLibpythonLinkMode::Dynamic,
            ] {
                let options = StandalonePythonExecutableBuilderOptions {
                    target_triple: target_triple.to_string(),
                    extension_module_filter: Some(ExtensionModuleFilter::Minimal),
                    libpython_link_mode,
                    resources_location: Some(ConcreteResourceLocation::RelativePath(
                        "prefix_policy".to_string(),
                    )),
                    resources_location_fallback: Some(None),
                    ..StandalonePythonExecutableBuilderOptions::default()
                };

                let mut builder = options.new_builder()?;

                let ext = builder
                    .target_distribution
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
                        object_files: ext.object_file_data.clone(),
                        static_libraries: ["sqlite3".to_string()].iter().cloned().collect(),
                        init_functions: [("_sqlite3".to_string(), "PyInit__sqlite3".to_string())]
                            .iter()
                            .cloned()
                            .collect(),
                        licensed_components: licensed_components_from_extension(&ext),
                        ..LibPythonBuildContext::default()
                    })
                );

                assert_eq!(
                    builder
                        .iter_resources()
                        .find_map(|(name, r)| if *name == "_sqlite3" { Some(r) } else { None }),
                    Some(&PrePackagedResource {
                        is_builtin_extension_module: true,
                        name: "_sqlite3".to_string(),
                        ..PrePackagedResource::default()
                    })
                );
            }
        }

        Ok(())
    }

    #[test]
    fn test_macos_extension_in_memory_only() -> Result<()> {
        for target_triple in MACOS_TARGET_TRIPLES.iter() {
            for libpython_link_mode in vec![
                BinaryLibpythonLinkMode::Static,
                BinaryLibpythonLinkMode::Dynamic,
            ] {
                let options = StandalonePythonExecutableBuilderOptions {
                    target_triple: target_triple.to_string(),
                    extension_module_filter: Some(ExtensionModuleFilter::Minimal),
                    libpython_link_mode: libpython_link_mode.clone(),
                    resources_location: Some(ConcreteResourceLocation::InMemory),
                    resources_location_fallback: Some(None),
                    ..StandalonePythonExecutableBuilderOptions::default()
                };

                let mut builder = options.new_builder()?;

                let res = builder
                    .add_python_extension_module(&EXTENSION_MODULE_SHARED_LIBRARY_ONLY, None);
                assert!(res.is_err());
                assert_eq!(
                    res.err().unwrap().to_string(),
                    "extension module shared_only cannot be loaded from memory but memory loading required"
                );

                let res =
                    builder.add_python_extension_module(&EXTENSION_MODULE_OBJECT_FILES_ONLY, None);
                match libpython_link_mode {
                    BinaryLibpythonLinkMode::Static => {
                        assert!(res.is_ok());
                        assert_extension_builtin(&builder, &EXTENSION_MODULE_OBJECT_FILES_ONLY);
                    }
                    BinaryLibpythonLinkMode::Dynamic => {
                        assert!(res.is_err());
                        assert_eq!(res.err().unwrap().to_string(), "extension module object_files_only cannot be loaded from memory but memory loading required");
                    }
                    BinaryLibpythonLinkMode::Default => {
                        panic!("should not get here");
                    }
                }

                let res = builder.add_python_extension_module(
                    &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES,
                    None,
                );
                match libpython_link_mode {
                    BinaryLibpythonLinkMode::Static => {
                        assert!(res.is_ok());
                        assert_extension_builtin(
                            &builder,
                            &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES,
                        );
                    }
                    BinaryLibpythonLinkMode::Dynamic => {
                        assert!(res.is_err());
                        assert_eq!(res.err().unwrap().to_string(), "extension module shared_and_object_files cannot be loaded from memory but memory loading required")
                    }
                    BinaryLibpythonLinkMode::Default => {
                        panic!("should not get here");
                    }
                }
            }
        }

        Ok(())
    }

    #[test]
    fn test_macos_extension_filesystem_relative_only() -> Result<()> {
        for target_triple in MACOS_TARGET_TRIPLES.iter() {
            for libpython_link_mode in vec![
                BinaryLibpythonLinkMode::Static,
                BinaryLibpythonLinkMode::Dynamic,
            ] {
                let options = StandalonePythonExecutableBuilderOptions {
                    target_triple: target_triple.to_string(),
                    extension_module_filter: Some(ExtensionModuleFilter::Minimal),
                    libpython_link_mode: libpython_link_mode.clone(),
                    resources_location: Some(ConcreteResourceLocation::RelativePath(
                        "prefix_policy".to_string(),
                    )),
                    resources_location_fallback: Some(None),
                    ..StandalonePythonExecutableBuilderOptions::default()
                };

                let mut builder = options.new_builder()?;

                builder.add_python_extension_module(&EXTENSION_MODULE_SHARED_LIBRARY_ONLY, None)?;
                assert_extension_shared_library(
                    &builder,
                    &EXTENSION_MODULE_SHARED_LIBRARY_ONLY,
                    ConcreteResourceLocation::RelativePath("prefix_policy".to_string()),
                );

                let res =
                    builder.add_python_extension_module(&EXTENSION_MODULE_OBJECT_FILES_ONLY, None);
                match libpython_link_mode {
                    BinaryLibpythonLinkMode::Static => {
                        assert!(res.is_ok());
                        assert_extension_builtin(&builder, &EXTENSION_MODULE_OBJECT_FILES_ONLY);
                    }
                    BinaryLibpythonLinkMode::Dynamic => {
                        assert!(res.is_err());
                        assert_eq!(res.err().unwrap().to_string(), "extension module object_files_only cannot be materialized as a shared library extension but filesystem loading required");
                    }
                    BinaryLibpythonLinkMode::Default => {
                        panic!("should not get here");
                    }
                }

                builder.add_python_extension_module(
                    &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES,
                    None,
                )?;
                assert_extension_shared_library(
                    &builder,
                    &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES,
                    ConcreteResourceLocation::RelativePath("prefix_policy".to_string()),
                );
            }
        }

        Ok(())
    }

    #[test]
    fn test_macos_extension_prefer_in_memory() -> Result<()> {
        for target_triple in MACOS_TARGET_TRIPLES.iter() {
            for libpython_link_mode in vec![
                BinaryLibpythonLinkMode::Static,
                BinaryLibpythonLinkMode::Dynamic,
            ] {
                let options = StandalonePythonExecutableBuilderOptions {
                    target_triple: target_triple.to_string(),
                    extension_module_filter: Some(ExtensionModuleFilter::Minimal),
                    libpython_link_mode: libpython_link_mode.clone(),
                    resources_location: Some(ConcreteResourceLocation::InMemory),
                    resources_location_fallback: Some(Some(
                        ConcreteResourceLocation::RelativePath("prefix_policy".to_string()),
                    )),
                    ..StandalonePythonExecutableBuilderOptions::default()
                };

                let mut builder = options.new_builder()?;

                builder.add_python_extension_module(&EXTENSION_MODULE_SHARED_LIBRARY_ONLY, None)?;
                assert_extension_shared_library(
                    &builder,
                    &EXTENSION_MODULE_SHARED_LIBRARY_ONLY,
                    ConcreteResourceLocation::RelativePath("prefix_policy".to_string()),
                );

                let res =
                    builder.add_python_extension_module(&EXTENSION_MODULE_OBJECT_FILES_ONLY, None);
                match libpython_link_mode {
                    BinaryLibpythonLinkMode::Static => {
                        assert!(res.is_ok());
                        assert_extension_builtin(&builder, &EXTENSION_MODULE_OBJECT_FILES_ONLY);
                    }
                    BinaryLibpythonLinkMode::Dynamic => {
                        assert!(res.is_err());
                        assert_eq!(
                            res.err().unwrap().to_string(),
                            "no shared library data present"
                        );
                    }
                    BinaryLibpythonLinkMode::Default => {
                        panic!("should not get here");
                    }
                }

                builder.add_python_extension_module(
                    &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES,
                    None,
                )?;
                match libpython_link_mode {
                    BinaryLibpythonLinkMode::Static => {
                        assert_extension_builtin(
                            &builder,
                            &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES,
                        );
                    }
                    BinaryLibpythonLinkMode::Dynamic => {
                        assert_extension_shared_library(
                            &builder,
                            &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES,
                            ConcreteResourceLocation::RelativePath("prefix_policy".to_string()),
                        );
                    }
                    BinaryLibpythonLinkMode::Default => {
                        panic!("should not get here");
                    }
                }
            }
        }

        Ok(())
    }

    #[test]
    fn test_windows_dynamic_static_mismatch() {
        for target_triple in WINDOWS_TARGET_TRIPLES.iter() {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: target_triple.to_string(),
                distribution_flavor: DistributionFlavor::StandaloneDynamic,
                extension_module_filter: Some(ExtensionModuleFilter::Minimal),
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
    }

    #[test]
    fn test_windows_static_dynamic_mismatch() {
        for target_triple in WINDOWS_TARGET_TRIPLES.iter() {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: target_triple.to_string(),
                distribution_flavor: DistributionFlavor::StandaloneStatic,
                extension_module_filter: Some(ExtensionModuleFilter::Minimal),
                libpython_link_mode: BinaryLibpythonLinkMode::Dynamic,
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            // We can't request dynamic libpython with a static distribution.
            assert!(options.new_builder().is_err());
        }
    }

    #[test]
    fn test_windows_dynamic_distribution_extensions() -> Result<()> {
        for target in WINDOWS_TARGET_TRIPLES.iter() {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: target.to_string(),
                distribution_flavor: DistributionFlavor::StandaloneDynamic,
                extension_module_filter: Some(ExtensionModuleFilter::All),
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            let builder = options.new_builder()?;

            let builtin_names = builder.extension_build_contexts.keys().collect::<Vec<_>>();
            let relative_path_extension_names = builder
                .iter_resources()
                .filter_map(|(name, r)| {
                    if r.relative_path_extension_module_shared_library.is_some() {
                        Some(name)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            let in_memory_extension_names = builder
                .iter_resources()
                .filter_map(|(name, r)| {
                    if r.in_memory_extension_module_shared_library.is_some() {
                        Some(name)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();

            // Required extensions are compiled as built-in.
            // This assumes that our extensions annotated as required are built-in.
            // But this is an implementation detail. If this fails, it might be OK.
            for (name, variants) in builder.target_distribution.extension_modules.iter() {
                // !required does not mean it is missing, however!
                if variants.iter().any(|e| e.required) {
                    assert!(builtin_names.contains(&name));
                }
            }

            // Builtin/default extensions are compiled as built-in.
            for (name, variants) in builder.target_distribution.extension_modules.iter() {
                if variants.iter().any(|e| e.builtin_default) {
                    assert!(builtin_names.contains(&name));
                }
            }

            // Non-builtin/default extensions are compiled as standalone files.
            for (name, variants) in builder.target_distribution.extension_modules.iter() {
                if variants.iter().all(|e| !e.builtin_default) {
                    assert!(!builtin_names.contains(&name));
                    assert!(relative_path_extension_names.contains(&name));
                    assert!(!in_memory_extension_names.contains(&name));
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
                extension_module_filter: Some(ExtensionModuleFilter::Minimal),
                libpython_link_mode: BinaryLibpythonLinkMode::Static,
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            let mut builder = options.new_builder()?;

            // When adding an extension module in static link mode, it gets
            // added as a built-in and linked with libpython.

            let sqlite = builder
                .target_distribution
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
                    static_libraries: ["sqlite3".to_string()].iter().cloned().collect(),
                    init_functions: [("_sqlite3".to_string(), "PyInit__sqlite3".to_string())]
                        .iter()
                        .cloned()
                        .collect(),
                    licensed_components: licensed_components_from_extension(&sqlite),
                    ..LibPythonBuildContext::default()
                })
            );

            assert_eq!(
                builder
                    .iter_resources()
                    .find_map(|(name, r)| if *name == "_sqlite3" { Some(r) } else { None }),
                Some(&PrePackagedResource {
                    is_builtin_extension_module: true,
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
                extension_module_filter: Some(ExtensionModuleFilter::Minimal),
                libpython_link_mode: BinaryLibpythonLinkMode::Dynamic,
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            let mut builder = options.new_builder()?;

            // When adding an extension module in dynamic link mode and it isn't
            // already a built-in, it should be preserved as a standalone extension
            // module file.

            let sqlite = builder
                .target_distribution
                .extension_modules
                .get("_sqlite3")
                .unwrap()
                .default_variant()
                .clone();

            builder.add_python_extension_module(&sqlite, None)?;

            assert!(!builder.extension_build_contexts.contains_key("_sqlite3"));

            assert_eq!(
                builder
                    .iter_resources()
                    .find_map(|(name, r)| if *name == "_sqlite3" { Some(r) } else { None }),
                Some(&PrePackagedResource {
                    name: "_sqlite3".to_string(),
                    is_extension_module: true,
                    relative_path_extension_module_shared_library: Some((
                        PathBuf::from("lib/_sqlite3.pyd"),
                        sqlite.shared_library.as_ref().unwrap().to_memory()?
                    )),
                    shared_library_dependency_names: Some(vec!["sqlite3".to_string()]),
                    ..PrePackagedResource::default()
                })
            );

            let library = builder
                .iter_resources()
                .find_map(|(name, r)| if *name == "sqlite3" { Some(r) } else { None })
                .unwrap();
            assert!(library.is_shared_library);
            assert!(library.relative_path_shared_library.is_some());
        }

        Ok(())
    }

    #[test]
    fn test_windows_dynamic_distribution_dynamic_extension_files() -> Result<()> {
        for target in WINDOWS_TARGET_TRIPLES.iter() {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: target.to_string(),
                extension_module_filter: Some(ExtensionModuleFilter::Minimal),
                resources_location: Some(ConcreteResourceLocation::RelativePath("lib".to_string())),
                resources_location_fallback: Some(None),
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            let mut builder = options.new_builder()?;

            // When loading resources from the filesystem, dynamically linked
            // extension modules should be manifested as filesystem files and
            // library dependencies should be captured.

            let ssl_extension = builder
                .target_distribution
                .extension_modules
                .get("_ssl")
                .unwrap()
                .default_variant()
                .clone();
            assert_eq!(ssl_extension.extension_file_suffix, ".pyd");
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
            assert_eq!(path, &PathBuf::from("lib/_ssl.pyd"));

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

            let mut compiler = builder.host_distribution.create_bytecode_compiler()?;

            let resources = shared_libraries
                .iter()
                .map(|r| r.to_resource(compiler.deref_mut()))
                .collect::<Result<Vec<_>>>()?;
            assert_eq!(resources.len(), 2);

            assert_eq!(
                &resources[0].1,
                &vec![(
                    PathBuf::from(format!("lib/libcrypto-1_1{}.dll", lib_suffix)),
                    FileData::Path(
                        builder
                            .target_distribution
                            .base_dir
                            .join("python")
                            .join("install")
                            .join("DLLs")
                            .join(format!("libcrypto-1_1{}.dll", lib_suffix))
                    ),
                    true
                )]
            );
            assert_eq!(
                &resources[1].1,
                &vec![(
                    PathBuf::from(format!("lib/libssl-1_1{}.dll", lib_suffix)),
                    FileData::Path(
                        builder
                            .target_distribution
                            .base_dir
                            .join("python")
                            .join("install")
                            .join("DLLs")
                            .join(format!("libssl-1_1{}.dll", lib_suffix))
                    ),
                    true
                )]
            );
        }

        Ok(())
    }

    #[test]
    fn test_windows_static_distribution_extensions() -> Result<()> {
        for target in WINDOWS_TARGET_TRIPLES.iter() {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: target.to_string(),
                distribution_flavor: DistributionFlavor::StandaloneStatic,
                extension_module_filter: Some(ExtensionModuleFilter::All),
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            let builder = options.new_builder()?;

            let builtin_names = builder.extension_build_contexts.keys().collect::<Vec<_>>();

            // All distribution extensions are built-ins in static Windows
            // distributions.
            for name in builder.target_distribution.extension_modules.keys() {
                assert!(builtin_names.contains(&name));
            }
        }

        Ok(())
    }

    #[test]
    fn test_windows_dynamic_extension_in_memory_only() -> Result<()> {
        for target_triple in WINDOWS_TARGET_TRIPLES.iter() {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: target_triple.to_string(),
                distribution_flavor: DistributionFlavor::StandaloneDynamic,
                extension_module_filter: Some(ExtensionModuleFilter::Minimal),
                libpython_link_mode: BinaryLibpythonLinkMode::Dynamic,
                resources_location: Some(ConcreteResourceLocation::InMemory),
                resources_location_fallback: Some(None),
                allow_in_memory_shared_library_loading: Some(true),
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            let mut builder = options.new_builder()?;

            builder.add_python_extension_module(&EXTENSION_MODULE_SHARED_LIBRARY_ONLY, None)?;
            assert_extension_shared_library(
                &builder,
                &EXTENSION_MODULE_SHARED_LIBRARY_ONLY,
                ConcreteResourceLocation::InMemory,
            );

            let res =
                builder.add_python_extension_module(&EXTENSION_MODULE_OBJECT_FILES_ONLY, None);
            assert!(res.is_err());
            assert_eq!(
                res.err().unwrap().to_string(),
                "no shared library data present"
            );

            builder.add_python_extension_module(
                &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES,
                None,
            )?;
            assert_extension_shared_library(
                &builder,
                &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES,
                ConcreteResourceLocation::InMemory,
            );
        }

        Ok(())
    }

    #[test]
    fn test_windows_static_extension_in_memory_only() -> Result<()> {
        for target_triple in WINDOWS_TARGET_TRIPLES.iter() {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: target_triple.to_string(),
                distribution_flavor: DistributionFlavor::StandaloneStatic,
                extension_module_filter: Some(ExtensionModuleFilter::Minimal),
                libpython_link_mode: BinaryLibpythonLinkMode::Static,
                resources_location: Some(ConcreteResourceLocation::InMemory),
                resources_location_fallback: Some(None),
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
            assert_extension_builtin(&builder, &EXTENSION_MODULE_OBJECT_FILES_ONLY);

            builder.add_python_extension_module(
                &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES,
                None,
            )?;
            assert_extension_builtin(&builder, &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES);
        }

        Ok(())
    }

    #[test]
    fn test_windows_dynamic_extension_filesystem_relative_only() -> Result<()> {
        for target_triple in WINDOWS_TARGET_TRIPLES.iter() {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: target_triple.to_string(),
                distribution_flavor: DistributionFlavor::StandaloneDynamic,
                extension_module_filter: Some(ExtensionModuleFilter::Minimal),
                libpython_link_mode: BinaryLibpythonLinkMode::Dynamic,
                resources_location: Some(ConcreteResourceLocation::RelativePath(
                    "prefix_policy".to_string(),
                )),
                resources_location_fallback: Some(None),
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            let mut builder = options.new_builder()?;

            builder.add_python_extension_module(&EXTENSION_MODULE_SHARED_LIBRARY_ONLY, None)?;
            assert_extension_shared_library(
                &builder,
                &EXTENSION_MODULE_SHARED_LIBRARY_ONLY,
                ConcreteResourceLocation::RelativePath("prefix_policy".to_string()),
            );

            let res =
                builder.add_python_extension_module(&EXTENSION_MODULE_OBJECT_FILES_ONLY, None);
            assert!(res.is_err());
            assert_eq!(res.err().unwrap().to_string(), "extension module object_files_only cannot be materialized as a shared library extension but filesystem loading required");

            builder.add_python_extension_module(
                &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES,
                None,
            )?;
            assert_extension_shared_library(
                &builder,
                &EXTENSION_MODULE_SHARED_LIBRARY_ONLY,
                ConcreteResourceLocation::RelativePath("prefix_policy".to_string()),
            );
        }

        Ok(())
    }

    #[test]
    fn test_windows_static_extension_filesystem_relative_only() -> Result<()> {
        for target_triple in WINDOWS_TARGET_TRIPLES.iter() {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: target_triple.to_string(),
                distribution_flavor: DistributionFlavor::StandaloneStatic,
                extension_module_filter: Some(ExtensionModuleFilter::Minimal),
                libpython_link_mode: BinaryLibpythonLinkMode::Static,
                resources_location: Some(ConcreteResourceLocation::RelativePath(
                    "prefix_policy".to_string(),
                )),
                resources_location_fallback: Some(None),
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
            assert_extension_builtin(&builder, &EXTENSION_MODULE_OBJECT_FILES_ONLY);

            builder.add_python_extension_module(
                &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES,
                None,
            )?;
            assert_extension_builtin(&builder, &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES);
        }

        Ok(())
    }

    #[test]
    fn test_windows_dynamic_extension_prefer_in_memory() -> Result<()> {
        for target_triple in WINDOWS_TARGET_TRIPLES.iter() {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: target_triple.to_string(),
                distribution_flavor: DistributionFlavor::StandaloneDynamic,
                extension_module_filter: Some(ExtensionModuleFilter::Minimal),
                libpython_link_mode: BinaryLibpythonLinkMode::Dynamic,
                resources_location: Some(ConcreteResourceLocation::InMemory),
                resources_location_fallback: Some(Some(ConcreteResourceLocation::RelativePath(
                    "prefix_policy".to_string(),
                ))),
                allow_in_memory_shared_library_loading: Some(true),
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            let mut builder = options.new_builder()?;

            builder.add_python_extension_module(&EXTENSION_MODULE_SHARED_LIBRARY_ONLY, None)?;
            assert_extension_shared_library(
                &builder,
                &EXTENSION_MODULE_SHARED_LIBRARY_ONLY,
                ConcreteResourceLocation::InMemory,
            );

            // Cannot link new builtins in dynamic libpython link mode.
            let res =
                builder.add_python_extension_module(&EXTENSION_MODULE_OBJECT_FILES_ONLY, None);
            assert!(res.is_err());
            assert_eq!(
                res.err().unwrap().to_string(),
                "no shared library data present"
            );

            builder.add_python_extension_module(
                &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES,
                None,
            )?;
            assert_extension_shared_library(
                &builder,
                &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES,
                ConcreteResourceLocation::InMemory,
            );
        }

        Ok(())
    }

    #[test]
    fn test_windows_static_extension_prefer_in_memory() -> Result<()> {
        for target_triple in WINDOWS_TARGET_TRIPLES.iter() {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: target_triple.to_string(),
                distribution_flavor: DistributionFlavor::StandaloneStatic,
                extension_module_filter: Some(ExtensionModuleFilter::Minimal),
                libpython_link_mode: BinaryLibpythonLinkMode::Static,
                resources_location: Some(ConcreteResourceLocation::InMemory),
                resources_location_fallback: Some(Some(ConcreteResourceLocation::RelativePath(
                    "prefix_policy".to_string(),
                ))),
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
            assert_extension_builtin(&builder, &EXTENSION_MODULE_OBJECT_FILES_ONLY);

            builder.add_python_extension_module(
                &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES,
                None,
            )?;
            assert_extension_builtin(&builder, &EXTENSION_MODULE_SHARED_LIBRARY_AND_OBJECT_FILES);
        }

        Ok(())
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_linux_extension_build_with_library() -> Result<()> {
        for libpython_link_mode in vec![
            BinaryLibpythonLinkMode::Static,
            BinaryLibpythonLinkMode::Dynamic,
        ] {
            let options = StandalonePythonExecutableBuilderOptions {
                target_triple: "x86_64-unknown-linux-gnu".to_string(),
                extension_module_filter: Some(ExtensionModuleFilter::All),
                libpython_link_mode: libpython_link_mode.clone(),
                resources_location: Some(ConcreteResourceLocation::InMemory),
                ..StandalonePythonExecutableBuilderOptions::default()
            };

            let mut builder = options.new_builder()?;
            let logger = get_logger()?;

            let resources = builder.pip_install(
                &logger,
                false,
                &["pyyaml==5.3.1".to_string()],
                &HashMap::new(),
            )?;

            let extensions = resources
                .iter()
                .filter_map(|r| match r {
                    PythonResource::ExtensionModule(e) => Some(e),
                    _ => None,
                })
                .collect::<Vec<_>>();

            assert_eq!(extensions.len(), 1);

            let mut orig = extensions[0].clone();
            assert!(orig.shared_library.is_some());

            let (objects_len, link_libraries) = match libpython_link_mode {
                BinaryLibpythonLinkMode::Dynamic => (0, vec![]),
                BinaryLibpythonLinkMode::Static => (
                    1,
                    vec![LibraryDependency {
                        name: "yaml".to_string(),
                        static_library: None,
                        static_filename: None,
                        dynamic_library: None,
                        dynamic_filename: None,
                        framework: false,
                        system: false,
                    }],
                ),
                BinaryLibpythonLinkMode::Default => {
                    panic!("should not get here");
                }
            };

            assert_eq!(orig.object_file_data.len(), objects_len);

            // Makes compare easier.
            let mut e = orig.to_mut();
            e.shared_library = None;
            e.object_file_data = vec![];

            assert_eq!(
                e,
                &PythonExtensionModule {
                    name: "_yaml".to_string(),
                    init_fn: Some("PyInit__yaml".to_string()),
                    extension_file_suffix: ".cpython-39-x86_64-linux-gnu.so".to_string(),
                    shared_library: None,
                    object_file_data: vec![],
                    is_package: false,
                    link_libraries,
                    is_stdlib: false,
                    builtin_default: false,
                    required: false,
                    variant: None,
                    license: None,
                },
                "PythonExtensionModule for {:?}",
                libpython_link_mode
            );
        }

        Ok(())
    }

    #[test]
    fn test_vcruntime_requirements() -> Result<()> {
        let host_distribution = get_default_distribution()?;

        for dist in get_all_standalone_distributions()? {
            let builder = StandalonePythonExecutableBuilder::from_distribution(
                host_distribution.clone(),
                dist.clone(),
                host_distribution.target_triple().to_string(),
                dist.target_triple().to_string(),
                "myapp".to_string(),
                BinaryLibpythonLinkMode::Default,
                dist.create_packaging_policy()?,
                dist.create_python_interpreter_config()?,
            )?;

            let reqs = builder.vc_runtime_requirements();

            if dist.target_triple().contains("windows") && dist.libpython_shared_library.is_some() {
                let platform = match dist.target_triple() {
                    "i686-pc-windows-msvc" => VcRedistributablePlatform::X86,
                    "x86_64-pc-windows-msvc" => VcRedistributablePlatform::X64,
                    triple => {
                        return Err(anyhow!("unexpected distribution triple: {}", triple));
                    }
                };

                assert_eq!(reqs, Some(("14".to_string(), platform)));
            } else {
                assert!(reqs.is_none());
            }
        }

        Ok(())
    }

    #[test]
    fn test_install_windows_runtime_dlls() -> Result<()> {
        let host_distribution = get_default_distribution()?;

        for dist in get_all_standalone_distributions()? {
            let mut builder = StandalonePythonExecutableBuilder::from_distribution(
                host_distribution.clone(),
                dist.clone(),
                host_distribution.target_triple().to_string(),
                dist.target_triple().to_string(),
                "myapp".to_string(),
                BinaryLibpythonLinkMode::Default,
                dist.create_packaging_policy()?,
                dist.create_python_interpreter_config()?,
            )?;

            // In Never mode, the set of extra files should always be empty.
            builder.set_windows_runtime_dlls_mode(WindowsRuntimeDllsMode::Never);
            let manifest = builder.resolve_windows_runtime_dll_files()?;
            assert!(
                manifest.is_empty(),
                "target triple: {}",
                dist.target_triple()
            );

            // In WhenPresent mode, we resolve files when the binary requires
            // them and when the host machine can locate them.
            builder.set_windows_runtime_dlls_mode(WindowsRuntimeDllsMode::WhenPresent);

            if let Some((version, platform)) = builder.vc_runtime_requirements() {
                let can_locate_runtime =
                    find_visual_cpp_redistributable(&version, platform).is_ok();

                let manifest = builder.resolve_windows_runtime_dll_files()?;

                if can_locate_runtime {
                    assert!(
                        !manifest.is_empty(),
                        "target triple: {}",
                        dist.target_triple()
                    );
                } else {
                    assert!(
                        manifest.is_empty(),
                        "target triple: {}",
                        dist.target_triple()
                    );
                }
            } else {
                assert!(
                    builder.resolve_windows_runtime_dll_files()?.is_empty(),
                    "target triple: {}",
                    dist.target_triple()
                );
            }

            // In Always mode, we error if we can't locate the runtime files.
            builder.set_windows_runtime_dlls_mode(WindowsRuntimeDllsMode::Always);

            if let Some((version, platform)) = builder.vc_runtime_requirements() {
                let can_locate_runtime =
                    find_visual_cpp_redistributable(&version, platform).is_ok();

                let res = builder.resolve_windows_runtime_dll_files();

                if can_locate_runtime {
                    assert!(!res?.is_empty(), "target triple: {}", dist.target_triple());
                } else {
                    assert!(res.is_err());
                }
            } else {
                assert!(
                    builder.resolve_windows_runtime_dll_files()?.is_empty(),
                    "target triple: {}",
                    dist.target_triple()
                );
            }
        }

        Ok(())
    }
}
