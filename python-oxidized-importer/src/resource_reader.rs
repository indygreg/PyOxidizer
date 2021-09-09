// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::importer::ImporterState,
    pyo3::{exceptions::PyFileNotFoundError, prelude::*},
    std::sync::Arc,
};

/// Implements in-memory reading of resource data.
///
/// Implements importlib.abc.ResourceReader.
#[pyclass(module = "oxidized_importer")]
pub(crate) struct OxidizedResourceReader {
    state: Arc<ImporterState>,
    package: String,
}

impl OxidizedResourceReader {
    pub(crate) fn new(state: Arc<ImporterState>, package: String) -> Self {
        Self { state, package }
    }
}

#[pymethods]
impl OxidizedResourceReader {
    /// Returns an opened, file-like object for binary reading of the resource.
    ///
    /// If the resource cannot be found, FileNotFoundError is raised.
    fn open_resource<'p>(&self, py: Python<'p>, resource: &str) -> PyResult<&'p PyAny> {
        if let Some(file) = self.state.get_resources_state().get_package_resource_file(
            py,
            &self.package,
            resource,
        )? {
            Ok(file)
        } else {
            Err(PyFileNotFoundError::new_err("resource not found"))
        }
    }

    /// Returns the file system path to the resource.
    ///
    /// If the resource does not concretely exist on the file system, raise
    /// FileNotFoundError.
    #[allow(unused)]
    fn resource_path(&self, resource: &PyAny) -> PyResult<()> {
        Err(PyFileNotFoundError::new_err(
            "in-memory resources do not have filesystem paths",
        ))
    }

    /// Returns True if the named name is considered a resource. FileNotFoundError
    /// is raised if name does not exist.
    fn is_resource(&self, name: &str) -> PyResult<bool> {
        if self
            .state
            .get_resources_state()
            .is_package_resource(&self.package, name)
        {
            Ok(true)
        } else {
            Err(PyFileNotFoundError::new_err("resource not found"))
        }
    }

    /// Returns an iterable of strings over the contents of the package.
    ///
    /// Do note that it is not required that all names returned by the iterator be actual resources,
    /// e.g. it is acceptable to return names for which is_resource() would be false.
    ///
    /// Allowing non-resource names to be returned is to allow for situations where how a package
    /// and its resources are stored are known a priori and the non-resource names would be useful.
    /// For instance, returning subdirectory names is allowed so that when it is known that the
    /// package and resources are stored on the file system then those subdirectory names can be
    /// used directly.
    fn contents<'p>(&self, py: Python<'p>) -> PyResult<&'p PyAny> {
        self.state
            .get_resources_state()
            .package_resource_names(py, &self.package)
    }
}
