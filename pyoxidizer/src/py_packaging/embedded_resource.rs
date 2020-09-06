// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Embedded Python resources in a binary.
*/

use {
    super::standalone_builder::ExtensionModuleBuildState,
    anyhow::Result,
    python_packaging::resource::DataLocation,
    python_packaging::resource_collection::CompiledResourcesCollection,
    slog::{info, warn},
    std::collections::{BTreeMap, BTreeSet},
    std::io::Write,
};

/// Holds state necessary to link libpython.
pub struct LibpythonLinkingInfo {
    /// Object files that need to be linked.
    pub object_files: Vec<DataLocation>,

    pub link_libraries: BTreeSet<String>,
    pub link_frameworks: BTreeSet<String>,
    pub link_system_libraries: BTreeSet<String>,
    pub link_libraries_external: BTreeSet<String>,
}

/// Represents Python resources to embed in a binary.
#[derive(Debug, Default, Clone)]
pub struct EmbeddedPythonResources<'a> {
    /// Resources to write to a packed resources data structure.
    pub resources: CompiledResourcesCollection<'a>,

    /// Holds state needed for adding extension modules to libpython.
    pub extension_modules: BTreeMap<String, ExtensionModuleBuildState>,
}

impl<'a> EmbeddedPythonResources<'a> {
    /// Write entities defining resources.
    pub fn write_blobs<W: Write>(&self, module_names: &mut W, resources: &mut W) -> Result<()> {
        for name in self.resources.resources.keys() {
            module_names
                .write_all(name.as_bytes())
                .expect("failed to write");
            module_names.write_all(b"\n").expect("failed to write");
        }

        self.resources.write_packed_resources_v1(resources)
    }

    /// Obtain a list of built-in extensions.
    ///
    /// The returned list will likely make its way to PyImport_Inittab.
    pub fn builtin_extensions(&self) -> Vec<(String, String)> {
        self.extension_modules
            .iter()
            .filter_map(|(name, state)| {
                if let Some(init_fn) = &state.init_fn {
                    Some((name.clone(), init_fn.clone()))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Resolve state needed to link a libpython.
    pub fn resolve_libpython_linking_info(
        &self,
        logger: &slog::Logger,
    ) -> Result<LibpythonLinkingInfo> {
        let mut object_files = Vec::new();
        let mut link_libraries = BTreeSet::new();
        let mut link_frameworks = BTreeSet::new();
        let mut link_system_libraries = BTreeSet::new();
        let mut link_libraries_external = BTreeSet::new();

        warn!(
            logger,
            "resolving inputs for {} extension modules...",
            self.extension_modules.len()
        );

        for (name, state) in &self.extension_modules {
            if !state.link_object_files.is_empty() {
                info!(
                    logger,
                    "adding {} object files for {} extension module",
                    state.link_object_files.len(),
                    name
                );
                object_files.extend(state.link_object_files.iter().cloned());
            }

            for framework in &state.link_frameworks {
                warn!(logger, "framework {} required by {}", framework, name);
                link_frameworks.insert(framework.clone());
            }

            for library in &state.link_system_libraries {
                warn!(logger, "system library {} required by {}", library, name);
                link_system_libraries.insert(library.clone());
            }

            for library in &state.link_static_libraries {
                warn!(logger, "static library {} required by {}", library, name);
                link_libraries.insert(library.clone());
            }

            for library in &state.link_dynamic_libraries {
                warn!(logger, "dynamic library {} required by {}", library, name);
                link_libraries.insert(library.clone());
            }

            for library in &state.link_external_libraries {
                warn!(logger, "dynamic library {} required by {}", library, name);
                link_libraries_external.insert(library.clone());
            }
        }

        Ok(LibpythonLinkingInfo {
            object_files,
            link_libraries,
            link_frameworks,
            link_system_libraries,
            link_libraries_external,
        })
    }
}
