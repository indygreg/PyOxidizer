// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{
        importer::ImporterState,
        pkg_resources::create_oxidized_pkg_resources_provider,
        python_resources::{name_at_package_hierarchy, name_within_package_hierarchy},
    },
    pyo3::{
        exceptions::{PyIOError, PyNotImplementedError, PyValueError},
        prelude::*,
        types::{PyBytes, PyDict, PyList, PyString, PyTuple, PyType},
    },
    std::{collections::BTreeMap, sync::Arc},
};

// Emulates importlib.metadata.Distribution._discover_resolvers().
fn discover_resolvers(py: Python) -> PyResult<&PyList> {
    let sys_module = py.import("sys")?;
    let meta_path = sys_module.getattr("meta_path")?.cast_as::<PyList>()?;

    let mut resolvers = vec![];

    for finder in meta_path.iter() {
        if let Ok(find_distributions) = finder.getattr("find_distributions") {
            if !find_distributions.is_none() {
                resolvers.push(find_distributions);
            }
        }
    }

    Ok(PyList::new(py, resolvers))
}

/// A importlib.metadata.Distribution allowing access to package distribution data.
#[pyclass(module = "oxidized_importer")]
pub(crate) struct OxidizedDistribution {
    state: Arc<ImporterState>,
    package: String,
}

impl OxidizedDistribution {
    pub(crate) fn new(state: Arc<ImporterState>, package: String) -> Self {
        Self { state, package }
    }
}

#[pymethods]
impl OxidizedDistribution {
    #[allow(unused)]
    #[classmethod]
    fn from_name<'p>(cls: &PyType, py: Python<'p>, name: &PyString) -> PyResult<&'p PyAny> {
        let importlib_metadata = py.import("importlib.metadata")?;
        let finder = importlib_metadata.getattr("DistributionFinder")?;
        let context_type = finder.getattr("Context")?;

        for resolver in discover_resolvers(py)?.iter() {
            let kwargs = PyDict::new(py);
            kwargs.set_item("name", name)?;
            let context = context_type.call((), Some(kwargs))?;

            let dists = resolver.call((context,), None)?;

            let mut it = dists.iter()?;

            if let Some(dist) = it.next() {
                let dist = dist?;

                return Ok(dist);
            }
        }

        let package_not_found_error = importlib_metadata.getattr("PackageNotFoundError")?;

        Err(PyErr::from_value(
            package_not_found_error.call((name,), None)?,
        ))
    }

    #[allow(unused)]
    #[classmethod]
    #[args(py_args = "*", py_kwargs = "**")]
    fn discover<'p>(
        cls: &PyType,
        py: Python<'p>,
        py_args: &PyTuple,
        py_kwargs: Option<&PyDict>,
    ) -> PyResult<&'p PyAny> {
        let importlib_metadata = py.import("importlib.metadata")?;
        let distribution_finder = importlib_metadata.getattr("DistributionFinder")?;
        let context_type = distribution_finder.getattr("Context")?;

        let context = if let Some(kwargs) = py_kwargs {
            let context = kwargs.call_method("pop", ("context", py.None()), None)?;

            if !context.is_none() && !kwargs.is_empty() {
                return Err(PyValueError::new_err("cannot accept context and kwargs"));
            }

            if context.is_none() {
                context_type.call((), Some(kwargs))?
            } else {
                context
            }
        } else {
            context_type.call0()?
        };

        let mut distributions = vec![];

        for resolver in discover_resolvers(py)?.iter() {
            for distribution in resolver.call((context,), None)?.iter()? {
                distributions.push(distribution?);
            }
        }

        // Return an iterator for compatibility with older standard library
        // versions.
        PyList::new(py, &distributions).call_method0("__iter__")
    }

    /// Attempt to load metadata file given by the filename.
    fn read_text<'p>(&self, py: Python<'p>, filename: String) -> PyResult<&'p PyAny> {
        let resources_state = self.state.get_resources_state();

        let data = resources_state
            .resolve_package_distribution_resource(&self.package, &filename)
            .map_err(|e| PyIOError::new_err(format!("error when resolving resource: {}", e)))?;

        // Missing resource returns None.
        let data = if let Some(data) = data {
            data
        } else {
            return Ok(py.None().into_ref(py));
        };

        let data = PyBytes::new(py, &data);

        let io = py.import("io")?;

        let bytes_io = io.getattr("BytesIO")?.call((data,), None)?;
        let text_wrapper = io
            .getattr("TextIOWrapper")?
            .call((bytes_io, "utf-8"), None)?;

        text_wrapper.call_method0("read")
    }

    /// Return the parsed metadata for this Distribution.
    ///
    /// The returned object will have keys that name the various bits of
    /// metadata.
    #[getter]
    fn metadata<'p>(&self, py: Python<'p>) -> PyResult<&'p PyAny> {
        let resources_state = self.state.get_resources_state();

        let data = resources_state
            .resolve_package_distribution_resource(&self.package, "METADATA")
            .map_err(|e| PyIOError::new_err(format!("error when resolving resource: {}", e)))?;

        let data = if let Some(data) = data {
            data
        } else {
            resources_state
                .resolve_package_distribution_resource(&self.package, "PKG-INFO")
                .map_err(|e| PyIOError::new_err(format!("error when resolving resource: {}", e)))?
                .ok_or_else(|| PyIOError::new_err("package metadata not found"))?
        };

        let data = PyBytes::new(py, &data);
        let email = py.import("email")?;

        let message = email.getattr("message_from_bytes")?.call((data,), None)?;

        // Python 3.10+ has an adapter class for the raw email Message.
        if let Ok(adapters) = py.import("importlib.metadata._adapters") {
            let adapter_cls = adapters.getattr("Message")?;
            adapter_cls.call1((message,))
        } else {
            Ok(message)
        }
    }

    /// Return the `Name` metadata for the distribution package.
    #[getter]
    fn name<'p>(&self, py: Python<'p>) -> PyResult<&'p PyAny> {
        let metadata = self.metadata(py)?;

        metadata.get_item("Name")
    }

    /// Return a normalized version of the name.
    #[getter]
    fn _normalized_name<'p>(&self, py: Python<'p>) -> PyResult<&'p PyAny> {
        let name = self.name(py)?;
        let re = py.import("re")?;

        // PEP 503 normalization plus dashes as underscores.
        let value = re.call_method("sub", ("[-_.]+", "-", name), None)?;
        let value = value.call_method0("lower")?;
        let value = value.call_method("replace", ("-", "_"), None)?;

        Ok(value)
    }

    #[getter]
    fn version<'p>(self_: PyRef<Self>, py: Python<'p>) -> PyResult<&'p PyAny> {
        let metadata = self_.metadata(py)?;

        metadata.get_item("Version")
    }

    #[getter]
    fn entry_points<'p>(self_: PyRef<Self>, py: Python<'p>) -> PyResult<&'p PyAny> {
        let importlib_metadata = py.import("importlib.metadata")?;

        let text = self_.read_text(py, "entry_points.txt".into())?;

        if let Ok(entry_points) = importlib_metadata.getattr("EntryPoints") {
            entry_points.call_method("_from_text_for", (text, self_), None)
        } else {
            let entry_point = importlib_metadata.getattr("EntryPoint")?;

            entry_point.call_method("_from_text", (text,), None)
        }
    }

    #[getter]
    fn files(&self) -> PyResult<()> {
        Err(PyNotImplementedError::new_err(()))
    }

    #[getter]
    fn requires<'p>(&self, py: Python<'p>) -> PyResult<&'p PyAny> {
        let requires = self
            .metadata(py)?
            .call_method("get_all", ("Requires-Dist",), None)?;

        let requires = if requires.is_none() {
            // Fall back to reading from requires.txt.
            let source = self.read_text(py, "requires.txt".into())?;

            if source.is_none() {
                py.None().into_ref(py)
            } else {
                let importlib_metadata = py.import("importlib.metadata")?;
                let distribution = importlib_metadata.getattr("Distribution")?;

                distribution.call_method("_deps_from_requires_text", (source,), None)?
            }
        } else {
            requires
        };

        if requires.is_none() {
            Ok(py.None().into_ref(py))
        } else {
            let res = PyList::empty(py);
            res.call_method("extend", (requires,), None)?;

            Ok(res.into())
        }
    }
}

/// Find package metadata distributions given search criteria.
pub(crate) fn find_distributions<'p>(
    py: Python<'p>,
    state: Arc<ImporterState>,
    name: Option<&PyAny>,
    _path: Option<&PyAny>,
) -> PyResult<&'p PyList> {
    let distributions = state
        .get_resources_state()
        .package_distribution_names(|match_name| {
            if let Some(name) = name {
                // Python normalizes the name. We do the same.
                let name = name.to_string();
                let name = name.to_lowercase().replace('-', "_");

                let match_name = match_name.to_lowercase().replace('-', "_");

                match_name == name
            } else {
                true
            }
        })
        .into_iter()
        .map(|name| {
            PyCell::new(
                py,
                OxidizedDistribution::new(state.clone(), name.to_string()),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(PyList::new(py, &distributions))
}

/// pkg_resources distribution finder for sys.path entries.
///
/// `state` meta path importer state.
/// `search_path` is the `sys.path` item being evaluated.
/// `only` if True only yield items that would be importable if `search_path` were
/// on `sys.path`. Otherwise yields items that are in or under `search_path`.
/// `package_target` is the package target from the `OxidizedPathEntryFinder`.
pub(crate) fn find_pkg_resources_distributions<'p>(
    py: Python<'p>,
    state: Arc<ImporterState>,
    search_path: &str,
    only: bool,
    package_target: Option<&str>,
) -> PyResult<&'p PyList> {
    let resources = &state.get_resources_state();

    let pkg_resources = py.import("pkg_resources")?;
    let distribution_type = pkg_resources.getattr("Distribution")?;

    let distributions = resources
        .package_distribution_names(|name| {
            if only {
                name_at_package_hierarchy(name, package_target)
            } else {
                name_within_package_hierarchy(name, package_target)
            }
        })
        .into_iter()
        .map(|name| {
            let oxidized_distribution = OxidizedDistribution::new(state.clone(), name.to_string());

            let metadata = oxidized_distribution.metadata(py)?;

            let project_name = metadata.get_item("Name")?;
            let version = metadata.get_item("Version")?;

            let location = format!("{}/{}", search_path, name.replace('.', "/"));

            let provider = create_oxidized_pkg_resources_provider(state.clone(), name.to_string())?;

            let kwargs = PyDict::new(py);
            kwargs.set_item("location", PyString::new(py, &location))?;
            kwargs.set_item("metadata", PyCell::new(py, provider)?)?;
            kwargs.set_item("project_name", project_name)?;
            kwargs.set_item("version", version)?;

            Ok((name, distribution_type.call((), Some(kwargs))?))
        })
        // Collect into a BTreeMap to deduplicate and facilitate deterministic output.
        .filter_map(|kv: PyResult<(_, &PyAny)>| kv.ok())
        .collect::<BTreeMap<_, &PyAny>>();

    Ok(PyList::new(
        py,
        &distributions.into_values()
            .collect::<Vec<_>>(),
    ))
}
