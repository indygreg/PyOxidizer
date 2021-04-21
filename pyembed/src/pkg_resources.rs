// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{
        importer::{ImporterState, OxidizedFinder, OxidizedPathEntryFinder},
        package_metadata::{
            find_pkg_resources_distributions, metadata_list_directory, metadata_name_is_directory,
            resolve_package_distribution_resource,
        },
    },
    cpython::{
        exc::{IOError, NotImplementedError, TypeError, UnicodeDecodeError},
        {
            py_class, py_fn, NoArgs, ObjectProtocol, PyErr, PyList, PyObject, PyResult, PyString,
            Python, PythonObject,
        },
    },
    std::sync::Arc,
};

py_class!(pub(crate) class OxidizedPkgResourcesProvider |py| {
    data state: Arc<ImporterState>;
    data package: String;

    def __new__(_cls, module: PyObject) -> PyResult<OxidizedPkgResourcesProvider> {
        oxidized_pkg_resources_provider_new(py, module)
    }

    // Begin IMetadataProvider interface.

    def has_metadata(&self, name: PyString) -> PyResult<bool> {
        Ok(self.has_metadata_impl(py, name))
    }

    def get_metadata(&self, name: PyString) -> PyResult<PyString> {
        self.get_metadata_impl(py, name)
    }

    def get_metadata_lines(&self, name: PyString) -> PyResult<PyObject> {
        self.get_metadata_lines_impl(py, name)
    }

    def metadata_isdir(&self, name: PyString) -> PyResult<bool> {
        Ok(self.metadata_isdir_impl(py, name))
    }

    def metadata_listdir(&self, name: PyString) -> PyResult<PyList> {
        Ok(self.metadata_listdir_impl(py, name))
    }

    def run_script(&self, script_name: PyString, namespace: PyObject) -> PyResult<PyObject> {
        self.run_script_impl(py, script_name, namespace)
    }

    // End IMetadataProvider interface.

    // Begin IResourceProvider interface.

    def get_resource_filename(&self, manager: PyObject, resource_name: PyString) -> PyResult<PyObject> {
        self.get_resource_filename_impl(py, manager, resource_name)
    }

    def get_resource_stream(&self, manager: PyObject, resource_name: PyString) -> PyResult<PyObject> {
        self.get_resource_stream_impl(py, manager, resource_name)
    }

    def get_resource_string(&self, manager: PyObject, resource_name: PyString) -> PyResult<PyObject> {
        self.get_resource_string_impl(py, manager, resource_name)
    }

    def has_resource(&self, resource_name: PyString) -> PyResult<bool> {
        Ok(self.has_resource_impl(py, resource_name))
    }

    def resource_isdir(&self, resource_name: PyString) -> PyResult<bool> {
        Ok(self.resource_isdir_impl(py, resource_name))
    }

    def resource_listdir(&self, resource_name: PyString) -> PyResult<PyList> {
        Ok(self.resource_listdir_impl(py, resource_name))
    }

    // End IResourceProvider interface.
});

/// OxidizedPkgResourcesProvider.__new__(module)
fn oxidized_pkg_resources_provider_new(
    py: Python,
    module: PyObject,
) -> PyResult<OxidizedPkgResourcesProvider> {
    let loader = module.getattr(py, "__loader__")?;
    let package = module.getattr(py, "__package__")?;

    let loader_type = loader.get_type(py);

    if loader_type != py.get_type::<OxidizedFinder>() {
        return Err(PyErr::new::<TypeError, _>(
            py,
            "__loader__ is not an OxidizedFinder",
        ));
    }

    let finder = loader.cast_as::<OxidizedFinder>(py)?;
    let state = finder.get_state(py);

    OxidizedPkgResourcesProvider::create_instance(py, state, package.to_string())
}

pub(crate) fn create_oxidized_pkg_resources_provider(
    py: Python,
    state: Arc<ImporterState>,
    package: String,
) -> PyResult<PyObject> {
    Ok(OxidizedPkgResourcesProvider::create_instance(py, state, package)?.into_object())
}

// pkg_resources.IMetadataProvider
impl OxidizedPkgResourcesProvider {
    fn has_metadata_impl(&self, py: Python, name: PyString) -> bool {
        let state = self.state(py);
        let package = self.package(py);
        let resources_state = state.get_resources_state();

        let name = name.to_string_lossy(py);

        let data = resolve_package_distribution_resource(
            &resources_state.resources,
            &resources_state.origin,
            package,
            &name,
        )
        .unwrap_or(None);

        data.is_some()
    }

    fn get_metadata_impl(&self, py: Python, name: PyString) -> PyResult<PyString> {
        let state = self.state(py);
        let package = self.package(py);
        let resources_state = state.get_resources_state();

        let name = name.to_string_lossy(py);

        let data = resolve_package_distribution_resource(
            &resources_state.resources,
            &resources_state.origin,
            package,
            &name,
        )
        .map_err(|e| PyErr::new::<IOError, _>(py, format!("error obtaining metadata: {}", e)))?
        .ok_or_else(|| PyErr::new::<IOError, _>(py, "metadata does not exist"))?;

        let data = String::from_utf8(data.to_vec())
            .map_err(|_| PyErr::new::<UnicodeDecodeError, _>(py, "metadata is not UTF-8"))?;

        Ok(PyString::new(py, &data))
    }

    fn get_metadata_lines_impl(&self, py: Python, name: PyString) -> PyResult<PyObject> {
        let s = self.get_metadata(py, name)?;

        let pkg_resources = py.import("pkg_resources")?;

        pkg_resources.call(py, "yield_lines", (s,), None)
    }

    fn metadata_isdir_impl(&self, py: Python, name: PyString) -> bool {
        let state = self.state(py);
        let package = self.package(py);
        let resources_state = state.get_resources_state();

        let name = name.to_string_lossy(py);

        metadata_name_is_directory(&resources_state.resources, &package, &name)
    }

    fn metadata_listdir_impl(&self, py: Python, name: PyString) -> PyList {
        let state = self.state(py);
        let package = self.package(py);
        let resources_state = state.get_resources_state();

        let name = name.to_string_lossy(py);

        let entries = metadata_list_directory(&resources_state.resources, &package, &name)
            .into_iter()
            .map(|s| PyString::new(py, s).into_object())
            .collect::<Vec<_>>();

        PyList::new(py, &entries)
    }

    fn run_script_impl(
        &self,
        py: Python,
        _script_name: PyString,
        _namespace: PyObject,
    ) -> PyResult<PyObject> {
        Err(PyErr::new::<NotImplementedError, _>(py, NoArgs))
    }
}

// pkg_resources.IResourceProvider
impl OxidizedPkgResourcesProvider {
    fn get_resource_filename_impl(
        &self,
        py: Python,
        _manager: PyObject,
        _resource_name: PyString,
    ) -> PyResult<PyObject> {
        // Raising NotImplementedError seems allowed per the implementation of
        // pkg_resources.ZipProvider, which also raises this error when resources
        // aren't backed by the filesystem.
        //
        // We could potentially expose the filename if the resource is backed
        // by a file. But we keep things simple for now.
        Err(PyErr::new::<NotImplementedError, _>(py, NoArgs))
    }

    fn get_resource_stream_impl(
        &self,
        py: Python,
        _manager: PyObject,
        resource_name: PyString,
    ) -> PyResult<PyObject> {
        let state = self.state(py);
        let package = self.package(py);
        let resource_name = resource_name.to_string_lossy(py);

        state
            .get_resources_state()
            .get_package_resource_file(py, &package, &resource_name)?
            .ok_or_else(|| PyErr::new::<IOError, _>(py, "resource does not exist"))
    }

    fn get_resource_string_impl(
        &self,
        py: Python,
        manager: PyObject,
        resource_name: PyString,
    ) -> PyResult<PyObject> {
        let fh = self.get_resource_stream_impl(py, manager, resource_name)?;

        fh.call_method(py, "read", NoArgs, None)
    }

    fn has_resource_impl(&self, py: Python, resource_name: PyString) -> bool {
        let state = self.state(py);
        let package = self.package(py);
        let resource_name = resource_name.to_string_lossy(py);

        state
            .get_resources_state()
            .get_package_resource_file(py, &package, &resource_name)
            .unwrap_or(None)
            .is_some()
    }

    fn resource_isdir_impl(&self, py: Python, resource_name: PyString) -> bool {
        let state = self.state(py);
        let package = self.package(py);
        let resource_name = resource_name.to_string_lossy(py);

        state
            .get_resources_state()
            .is_package_resource_directory(&package, &resource_name)
    }

    fn resource_listdir_impl(&self, py: Python, resource_name: PyString) -> PyList {
        let state = self.state(py);
        let package = self.package(py);
        let resource_name = resource_name.to_string_lossy(py);

        let entries = state
            .get_resources_state()
            .package_resources_list_directory(&package, &resource_name)
            .into_iter()
            .map(|s| PyString::new(py, &s).into_object())
            .collect::<Vec<_>>();

        PyList::new(py, &entries)
    }
}

/// Registers our types/callbacks with `pkg_resources`.
pub(crate) fn register_pkg_resources_with_module(
    py: Python,
    pkg_resources: &PyObject,
) -> PyResult<PyObject> {
    pkg_resources.call_method(
        py,
        "register_finder",
        (
            py.get_type::<OxidizedPathEntryFinder>(),
            py_fn!(
                py,
                pkg_resources_find_distributions(
                    importer: PyObject,
                    path_item: PyString,
                    only: Option<bool> = false
                )
            ),
        ),
        None,
    )?;

    pkg_resources.call_method(
        py,
        "register_loader_type",
        (
            py.get_type::<OxidizedFinder>(),
            py.get_type::<OxidizedPkgResourcesProvider>(),
        ),
        None,
    )?;

    Ok(py.None())
}

/// pkg_resources distribution finder for sys.path items.
pub(crate) fn pkg_resources_find_distributions(
    py: Python,
    importer: PyObject,
    path_item: PyString,
    only: bool,
) -> PyResult<PyList> {
    let importer_type = importer.get_type(py);

    // This shouldn't happen since that path hook type is mapped to this function.
    // But you never know.
    if importer_type != py.get_type::<OxidizedPathEntryFinder>() {
        return Ok(PyList::new(py, &[]));
    }

    let finder = importer.cast_as::<OxidizedPathEntryFinder>(py)?;

    // The path_item we're handling should match what was registered to this path
    // entry finder. Reject if that's not the case.
    if finder
        .get_source_path(py)
        .as_object()
        .compare(py, path_item.as_object())?
        != std::cmp::Ordering::Equal
    {
        return Ok(PyList::new(py, &[]));
    }

    let meta_finder = finder.get_finder(py);
    let state = meta_finder.get_state(py);

    find_pkg_resources_distributions(
        py,
        state.clone(),
        &path_item.to_string_lossy(py),
        only,
        finder.get_target_package(py).as_ref().map(|s| s.as_str()),
    )
}
