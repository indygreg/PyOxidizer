// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{
        importer::{ImporterState, OxidizedFinder},
        package_metadata::find_pkg_resources_distributions,
        path_entry_finder::OxidizedPathEntryFinder,
    },
    pyo3::{
        exceptions::{PyIOError, PyNotImplementedError, PyTypeError, PyUnicodeDecodeError},
        prelude::*,
        types::{PyList, PyString},
    },
    std::sync::Arc,
};

#[pyclass(module = "oxidized_importer")]
pub(crate) struct OxidizedPkgResourcesProvider {
    state: Arc<ImporterState>,
    package: String,
}

#[pymethods]
impl OxidizedPkgResourcesProvider {
    /// OxidizedPkgResourcesProvider.__new__(module)
    #[new]
    fn new(py: Python, module: &PyAny) -> PyResult<Self> {
        let loader = module.getattr("__loader__")?;
        let package = module.getattr("__package__")?;

        let loader_type = loader.get_type();

        if !loader_type.is(py.get_type::<OxidizedFinder>()) {
            return Err(PyTypeError::new_err("__loader__ is not an OxidizedFinder"));
        }

        let finder = loader.cast_as::<PyCell<OxidizedFinder>>()?;
        let state = finder.borrow().get_state();

        Ok(Self {
            state,
            package: package.to_string(),
        })
    }

    // Begin IMetadataProvider interface.

    fn has_metadata(&self, name: &str) -> PyResult<bool> {
        let resources_state = self.state.get_resources_state();

        let data = resources_state
            .resolve_package_distribution_resource(&self.package, name)
            .unwrap_or(None);

        Ok(data.is_some())
    }

    fn get_metadata(&self, name: &str) -> PyResult<String> {
        let resources_state = self.state.get_resources_state();

        let data = resources_state
            .resolve_package_distribution_resource(&self.package, name)
            .map_err(|e| PyIOError::new_err(format!("error obtaining metadata: {}", e)))?
            .ok_or_else(|| PyIOError::new_err("metadata does not exist"))?;

        String::from_utf8(data.to_vec())
            .map_err(|_| PyUnicodeDecodeError::new_err("metadata is not UTF-8"))
    }

    fn get_metadata_lines<'p>(&self, py: Python<'p>, name: &str) -> PyResult<&'p PyAny> {
        let s = self.get_metadata(name)?;

        let pkg_resources = py.import("pkg_resources")?;

        pkg_resources.getattr("yield_lines")?.call((s,), None)
    }

    fn metadata_isdir(&self, name: &str) -> PyResult<bool> {
        let resources_state = self.state.get_resources_state();

        Ok(resources_state.package_distribution_resource_name_is_directory(&self.package, name))
    }

    fn metadata_listdir<'p>(&self, py: Python<'p>, name: &str) -> PyResult<&'p PyList> {
        let resources_state = self.state.get_resources_state();

        let entries = resources_state
            .package_distribution_resources_list_directory(&self.package, name)
            .into_iter()
            .map(|s| PyString::new(py, s))
            .collect::<Vec<_>>();

        Ok(PyList::new(py, &entries))
    }

    #[allow(unused)]
    fn run_script(&self, script_name: &PyAny, namespace: &PyAny) -> PyResult<&PyAny> {
        Err(PyNotImplementedError::new_err(()))
    }

    // End IMetadataProvider interface.

    // Begin IResourceProvider interface.

    #[allow(unused)]
    fn get_resource_filename(&self, manager: &PyAny, resource_name: &PyAny) -> PyResult<&PyAny> {
        // Raising NotImplementedError seems allowed per the implementation of
        // pkg_resources.ZipProvider, which also raises this error when resources
        // aren't backed by the filesystem.
        //
        // We could potentially expose the filename if the resource is backed
        // by a file. But we keep things simple for now.
        Err(PyNotImplementedError::new_err(()))
    }

    #[allow(unused)]
    fn get_resource_stream<'p>(
        &self,
        py: Python<'p>,
        manager: &PyAny,
        resource_name: &str,
    ) -> PyResult<&'p PyAny> {
        self.state
            .get_resources_state()
            .get_package_resource_file(py, &self.package, resource_name)?
            .ok_or_else(|| PyIOError::new_err("resource does not exist"))
    }

    fn get_resource_string<'p>(
        &self,
        py: Python<'p>,
        manager: &PyAny,
        resource_name: &str,
    ) -> PyResult<&'p PyAny> {
        let fh = self.get_resource_stream(py, manager, resource_name)?;

        fh.call_method0("read")
    }

    fn has_resource(&self, py: Python, resource_name: &str) -> PyResult<bool> {
        Ok(self
            .state
            .get_resources_state()
            .get_package_resource_file(py, &self.package, resource_name)
            .unwrap_or(None)
            .is_some())
    }

    fn resource_isdir(&self, resource_name: &str) -> PyResult<bool> {
        Ok(self
            .state
            .get_resources_state()
            .is_package_resource_directory(&self.package, resource_name))
    }

    fn resource_listdir<'p>(&self, py: Python<'p>, resource_name: &str) -> PyResult<&'p PyList> {
        let entries = self
            .state
            .get_resources_state()
            .package_resources_list_directory(&self.package, resource_name)
            .into_iter()
            .map(|s| PyString::new(py, &s))
            .collect::<Vec<_>>();

        Ok(PyList::new(py, &entries))
    }

    // End IResourceProvider interface.
}

pub(crate) fn create_oxidized_pkg_resources_provider(
    state: Arc<ImporterState>,
    package: String,
) -> PyResult<OxidizedPkgResourcesProvider> {
    Ok(OxidizedPkgResourcesProvider { state, package })
}

/// Registers our types/callbacks with `pkg_resources`.
pub(crate) fn register_pkg_resources_with_module(
    py: Python,
    pkg_resources: &PyAny,
) -> PyResult<()> {
    pkg_resources.call_method(
        "register_finder",
        (
            py.get_type::<OxidizedPathEntryFinder>(),
            wrap_pyfunction!(pkg_resources_find_distributions)(py)?,
        ),
        None,
    )?;

    pkg_resources.call_method(
        "register_loader_type",
        (
            py.get_type::<OxidizedFinder>(),
            py.get_type::<OxidizedPkgResourcesProvider>(),
        ),
        None,
    )?;

    Ok(())
}

/// pkg_resources distribution finder for sys.path items.
#[pyfunction(only = false)]
pub(crate) fn pkg_resources_find_distributions<'p>(
    py: Python<'p>,
    importer: &PyAny,
    path_item: &PyString,
    only: bool,
) -> PyResult<&'p PyAny> {
    let importer_type = importer.get_type();

    // This shouldn't happen since that path hook type is mapped to this function.
    // But you never know.
    if !importer_type.is(py.get_type::<OxidizedPathEntryFinder>()) {
        return Ok(PyList::empty(py));
    }

    let finder_cell = importer.cast_as::<PyCell<OxidizedPathEntryFinder>>()?;
    let finder = finder_cell.borrow();

    // The path_item we're handling should match what was registered to this path
    // entry finder. Reject if that's not the case.
    if path_item.compare(finder.get_source_path())? != std::cmp::Ordering::Equal {
        return Ok(PyList::empty(py));
    }

    let meta_finder = finder.get_finder().borrow(py);
    let state = meta_finder.get_state();

    let dists = find_pkg_resources_distributions(
        py,
        state,
        &path_item.to_string_lossy(),
        only,
        finder.get_target_package().as_ref().map(|s| s.as_str()),
    )?;

    dists.call_method0("__iter__")
}

pub(crate) fn init_module(m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(pkg_resources_find_distributions, m)?)?;

    Ok(())
}
