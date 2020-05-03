// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Python functionality for resource collection. */

use {
    crate::conversion::pyobject_to_pathbuf,
    crate::python_resource_types::PythonModuleSource,
    crate::python_resources::resource_to_pyobject,
    cpython::exc::{TypeError, ValueError},
    cpython::{
        py_class, py_class_prop_getter, ObjectProtocol, PyErr, PyObject, PyResult, Python,
        PythonObject, ToPyObject,
    },
    python_packaging::resource_collection::{
        PreparedPythonResources, PythonResourceCollector, PythonResourcesPolicy,
    },
    std::cell::RefCell,
    std::convert::TryFrom,
};

py_class!(pub class OxidizedResourceCollector |py| {
    data collector: RefCell<PythonResourceCollector>;

    def __new__(_cls, policy: String) -> PyResult<OxidizedResourceCollector> {
        OxidizedResourceCollector::new(py, policy)
    }

    def __repr__(&self) -> PyResult<String> {
        Ok("<OxidizedResourceCollector>".to_string())
    }

    @property def policy(&self) -> PyResult<String> {
        Ok(self.collector(py).borrow().get_policy().into())
    }

    def add_in_memory(&self, resource: PyObject) -> PyResult<PyObject> {
        self.add_in_memory_impl(py, resource)
    }

    def oxidize(&self) -> PyResult<PyObject> {
        self.oxidize_impl(py)
    }
});

impl OxidizedResourceCollector {
    pub fn new(py: Python, policy: String) -> PyResult<Self> {
        let policy = PythonResourcesPolicy::try_from(policy.as_ref())
            .or_else(|e| Err(PyErr::new::<ValueError, _>(py, e.to_string())))?;

        let sys_module = py.import("sys")?;
        let cache_tag = sys_module
            .get(py, "implementation")?
            .getattr(py, "cache_tag")?
            .extract::<String>(py)?;

        let collector = PythonResourceCollector::new(&policy, &cache_tag);

        OxidizedResourceCollector::create_instance(py, RefCell::new(collector))
    }

    fn add_in_memory_impl(&self, py: Python, resource: PyObject) -> PyResult<PyObject> {
        let mut collector = self.collector(py).borrow_mut();
        let typ = resource.get_type(py);

        match typ.name(py).as_ref() {
            "PythonModuleSource" => {
                let module = resource.cast_into::<PythonModuleSource>(py)?;
                collector
                    .add_in_memory_python_module_source(&module.get_resource(py))
                    .or_else(|e| Err(PyErr::new::<ValueError, _>(py, e.to_string())))?;

                Ok(py.None())
            }
            _ => Err(PyErr::new::<TypeError, _>(
                py,
                format!("cannot operate on {} values", typ.name(py)),
            )),
        }
    }

    fn oxidize_impl(&self, py: Python) -> PyResult<PyObject> {
        let sys_module = py.import("sys")?;
        let executable = sys_module.get(py, "executable")?;

        let python_exe = pyobject_to_pathbuf(py, executable)?;

        let collector = self.collector(py).borrow();

        let prepared: PreparedPythonResources = collector
            .to_prepared_python_resources(&python_exe)
            .or_else(|e| {
                Err(PyErr::new::<ValueError, _>(
                    py,
                    format!("error oxidizing: {}", e),
                ))
            })?;

        let mut resources = Vec::new();

        for resource in prepared.resources.values() {
            resources.push(resource_to_pyobject(py, resource)?);
        }

        Ok(resources.into_py_object(py).into_object())
    }
}
