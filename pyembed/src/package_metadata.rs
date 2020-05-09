// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    super::importer::ImporterState,
    cpython::exc::{IOError, NotImplementedError},
    cpython::{
        py_class, py_class_prop_getter, NoArgs, ObjectProtocol, PyBytes, PyErr, PyList, PyObject,
        PyResult, PyString, Python, PythonObject, ToPyObject,
    },
    python_packed_resources::data::Resource,
    std::borrow::Cow,
    std::collections::HashMap,
    std::path::Path,
    std::sync::Arc,
};

// A importlib.metadata.Distribution allowing access to package distribution data.
py_class!(class OxidizedDistribution |py| {
    data state: Arc<Box<ImporterState>>;
    data package: String;

    def read_text(&self, filename: &PyString) -> PyResult<PyObject> {
        self.read_text_impl(py, filename)
    }

    @property def metadata(&self) -> PyResult<PyObject> {
        self.metadata_impl(py)
    }

    @property def version(&self) -> PyResult<PyObject> {
        self.version_impl(py)
    }

    @property def entry_points(&self) -> PyResult<PyObject> {
        self.entry_points_impl(py)
    }

    @property def files(&self) -> PyResult<PyObject> {
        self.files_impl(py)
    }

    @property def requires(&self) -> PyResult<PyObject> {
        self.requires_impl(py)
    }
});

impl OxidizedDistribution {
    fn read_text_impl(&self, py: Python, filename: &PyString) -> PyResult<PyObject> {
        let state: &Arc<Box<ImporterState>> = self.state(py);
        let package: &str = self.package(py);
        let resources_state = state.get_resources_state();

        let filename = filename.to_string_lossy(py);

        let data = resolve_package_distribution_resource(
            &resources_state.resources,
            &resources_state.origin,
            package,
            &filename,
        )
        .or_else(|e| {
            Err(PyErr::new::<IOError, _>(
                py,
                format!("error when resolving resource: {}", e),
            ))
        })?;

        // Missing resource returns None.
        let data = if let Some(data) = data {
            data
        } else {
            return Ok(py.None());
        };

        let data = PyBytes::new(py, &data);

        let io = py.import("io")?;

        let bytes_io = io.call(py, "BytesIO", (data,), None)?;
        let text_wrapper = io.call(py, "TextIOWrapper", (bytes_io, "utf-8"), None)?;

        text_wrapper.call_method(py, "read", NoArgs, None)
    }

    /// Return the parsed metadata for this Distribution.
    ///
    /// The returned object will have keys that name the various bits of
    /// metadata.
    fn metadata_impl(&self, py: Python) -> PyResult<PyObject> {
        let state: &Arc<Box<ImporterState>> = self.state(py);
        let package: &str = self.package(py);
        let resources_state = state.get_resources_state();

        let data = resolve_package_distribution_resource(
            &resources_state.resources,
            &resources_state.origin,
            package,
            "METADATA",
        )
        .or_else(|e| {
            Err(PyErr::new::<IOError, _>(
                py,
                format!("error when resolving resource: {}", e),
            ))
        })?;

        let data = if let Some(data) = data {
            data
        } else {
            resolve_package_distribution_resource(
                &resources_state.resources,
                &resources_state.origin,
                package,
                "PKG-INFO",
            )
            .or_else(|e| {
                Err(PyErr::new::<IOError, _>(
                    py,
                    format!("error when resolving resource: {}", e),
                ))
            })?
            .ok_or_else(|| PyErr::new::<IOError, _>(py, ("package metadata not found",)))?
        };

        let data = PyBytes::new(py, &data);
        let email = py.import("email")?;

        email.call(py, "message_from_bytes", (data,), None)
    }

    fn version_impl(&self, py: Python) -> PyResult<PyObject> {
        let distribution = self.as_object();

        let metadata = distribution.getattr(py, "metadata")?;

        metadata.get_item(py, "Version")
    }

    fn entry_points_impl(&self, py: Python) -> PyResult<PyObject> {
        let importlib_metadata = py.import("importlib.metadata")?;

        let entry_point = importlib_metadata.get(py, "EntryPoint")?;

        let text = self.read_text_impl(py, &"entry_points.txt".to_py_object(py))?;

        entry_point.call_method(py, "_from_text", (text,), None)
    }

    fn files_impl(&self, py: Python) -> PyResult<PyObject> {
        Err(PyErr::new::<NotImplementedError, _>(py, NoArgs))
    }

    fn requires_impl(&self, py: Python) -> PyResult<PyObject> {
        let requires: PyObject =
            self.metadata_impl(py)?
                .call_method(py, "get_all", ("Requires-Dist",), None)?;

        let requires = if requires == py.None() {
            // Fall back to reading from requires.txt.
            let source = self.read_text_impl(py, &"requires.txt".to_py_object(py))?;

            if source == py.None() {
                py.None()
            } else {
                let importlib_metadata = py.import("importlib.metadata")?;
                let distribution = importlib_metadata.get(py, "Distribution")?;

                distribution.call_method(py, "_deps_from_requires_text", (source,), None)?
            }
        } else {
            requires
        };

        if requires == py.None() {
            Ok(py.None())
        } else {
            let res = PyList::new(py, &[]).into_object();
            res.call_method(py, "extend", (requires,), None)?;

            Ok(res)
        }
    }
}

/// Find package metadata distributions given search criteria.
pub(crate) fn find_distributions<'a>(
    py: Python,
    state: Arc<Box<ImporterState>>,
    name: Option<PyObject>,
    _path: Option<PyObject>,
) -> PyResult<PyObject> {
    let resources = &state.get_resources_state().resources;

    let distributions = if let Some(name) = name {
        // Python normalizes the name. We do the same.
        let name = name.str(py)?.to_string(py)?.to_string();
        let name = name.to_lowercase().replace('-', "_");
        let name_cow = Cow::Borrowed::<str>(&name);

        if let Some(resource) = resources.get(&name_cow) {
            if resource.is_package
                && (resource.in_memory_distribution_resources.is_some()
                    || resource.relative_path_distribution_resources.is_some())
            {
                vec![OxidizedDistribution::create_instance(py, state.clone(), name)?.into_object()]
            } else {
                vec![]
            }
        } else {
            vec![]
        }
    } else {
        // Return all distributions.
        let mut distributions = Vec::new();

        for (k, v) in resources.iter() {
            if v.is_package
                && (v.in_memory_distribution_resources.is_some()
                    || v.relative_path_distribution_resources.is_some())
            {
                distributions.push(
                    OxidizedDistribution::create_instance(py, state.clone(), k.to_string())?
                        .into_object(),
                );
            }
        }

        distributions
    };

    Ok(PyList::new(py, &distributions).into_object())
}

fn resolve_package_distribution_resource<'a>(
    resources: &'a HashMap<Cow<'a, str>, Resource<'a, u8>>,
    origin: &Path,
    package: &str,
    name: &str,
) -> anyhow::Result<Option<Cow<'a, [u8]>>> {
    if let Some(entry) = resources.get(package) {
        if let Some(resources) = &entry.in_memory_distribution_resources {
            if let Some(data) = resources.get(name) {
                return Ok(Some(Cow::Borrowed(data.as_ref())));
            }
        }

        if let Some(resources) = &entry.relative_path_distribution_resources {
            if let Some(path) = resources.get(name) {
                let path = origin.join(path);
                let data = std::fs::read(&path)?;

                return Ok(Some(Cow::Owned(data)));
            }
        }

        Ok(None)
    } else {
        Ok(None)
    }
}
