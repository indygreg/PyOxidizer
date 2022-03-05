// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{importer::OxidizedFinder, python_resources::name_at_package_hierarchy},
    pyo3::{
        prelude::*,
        types::{PyList, PyString},
        PyTraverseError, PyVisit,
    },
};

/// A (mostly compliant) `importlib.abc.PathEntryFinder` that delegates paths
/// within the current executable to the `OxidizedFinder` whose `path_hook`
/// method created it.
#[pyclass(module = "oxidized_importer")]
pub(crate) struct OxidizedPathEntryFinder {
    /// A clone of the meta path finder from which we came.
    pub(crate) finder: Py<OxidizedFinder>,

    /// The sys.path value this instance was created with.
    pub(crate) source_path: Py<PyString>,

    /// Name of package being targeted.
    ///
    /// None is the top-level. Some(T) is a specific package in the hierarchy.
    pub(crate) target_package: Option<String>,
}

impl OxidizedPathEntryFinder {
    pub(crate) fn get_finder(&self) -> &Py<OxidizedFinder> {
        &self.finder
    }

    pub(crate) fn get_source_path(&self) -> &Py<PyString> {
        &self.source_path
    }

    pub(crate) fn get_target_package(&self) -> &Option<String> {
        &self.target_package
    }
}

#[pymethods]
impl OxidizedPathEntryFinder {
    fn __traverse__(&self, visit: PyVisit) -> Result<(), PyTraverseError> {
        visit.call(&self.finder)?;

        Ok(())
    }

    #[args(target = "None")]
    fn find_spec(
        &self,
        py: Python,
        fullname: &str,
        target: Option<&PyModule>,
    ) -> PyResult<Py<PyAny>> {
        if !name_at_package_hierarchy(fullname, self.target_package.as_deref()) {
            return Ok(py.None());
        }

        self.finder.call_method(
            py,
            "find_spec",
            (
                fullname,
                PyList::new(py, &[self.source_path.clone_ref(py)]),
                target,
            ),
            None,
        )
    }

    fn invalidate_caches(&self, py: Python) -> PyResult<Py<PyAny>> {
        self.finder.call_method0(py, "invalidate_caches")
    }

    #[args(prefix = "\"\"")]
    fn iter_modules<'p>(&self, py: Python<'p>, prefix: &str) -> PyResult<&'p PyList> {
        let finder = self.finder.borrow(py);

        finder.state.get_resources_state().pkgutil_modules_infos(
            py,
            self.target_package.as_deref(),
            Some(prefix.to_string()),
            finder.state.optimize_level,
        )
    }

    /// Private getter. Just for testing.
    #[getter]
    fn _package(&self) -> Option<String> {
        self.target_package.clone()
    }
}
