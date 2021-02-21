// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Python functionality for resource collection. */

use {
	rayon::prelude::*,
    crate::{
        conversion::{path_to_pathlib_path, pyobject_to_pathbuf},
        python_resource_types::{
            PythonExtensionModule, PythonModuleBytecode, PythonModuleSource,
            PythonPackageDistributionResource, PythonPackageResource,
        },
        python_resources::resource_to_pyobject,
    },
    anyhow::Context,
    cpython::{
        exc::{TypeError, ValueError},
        py_class, NoArgs, ObjectProtocol, PyBytes, PyErr, PyList, PyObject, PyResult, Python,
        PythonObject, ToPyObject,
    },
    python_packaging::{
        bytecode::BytecodeCompiler,
        location::{AbstractResourceLocation, ConcreteResourceLocation},
        resource_collection::{CompiledResourcesCollection, PythonResourceCollector},
    },
    std::{cell::RefCell, convert::TryFrom},
};

py_class!(pub class OxidizedResourceCollector |py| {
    data collector: RefCell<PythonResourceCollector>;

    def __new__(_cls, allowed_locations: Vec<String>) -> PyResult<OxidizedResourceCollector> {
        OxidizedResourceCollector::new(py, allowed_locations)
    }

    def __repr__(&self) -> PyResult<String> {
        Ok("<OxidizedResourceCollector>".to_string())
    }

    @property def allowed_locations(&self) -> PyResult<PyObject> {
        self.allowed_locations_impl(py)
    }

    def add_in_memory(&self, resource: PyObject) -> PyResult<PyObject> {
        self.add_in_memory_impl(py, resource)
    }

    def add_filesystem_relative(&self, prefix: String, resource: PyObject) -> PyResult<PyObject> {
        self.add_filesystem_relative_impl(py, prefix, resource)
    }

    def oxidize(&self, python_exe: Option<PyObject> = None) -> PyResult<PyObject> {
        self.oxidize_impl(py, python_exe)
    }
});

impl OxidizedResourceCollector {
    pub fn new(py: Python<'_>, allowed_locations: Vec<String>) -> PyResult<Self> {
        let allowed_locations = allowed_locations
            .par_iter()
            .map(|location| AbstractResourceLocation::try_from(location.as_str()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| PyErr::new::<ValueError, _>(py, e))?;

        let sys_module = py.import("sys")?;
        let cache_tag = sys_module
            .get(py, "implementation")?
            .getattr(py, "cache_tag")?
            .extract::<String>(py)?;

        let collector = PythonResourceCollector::new(
            allowed_locations.clone(),
            allowed_locations,
            true,
            true,
            &cache_tag,
        );

        OxidizedResourceCollector::create_instance(py, RefCell::new(collector))
    }

    fn allowed_locations_impl(&self, py: Python<'_>) -> PyResult<PyObject> {
        let values = self
            .collector(py)
            .borrow()
            .allowed_locations()
            .iter()
            .map(|l| l.to_string().to_py_object(py).into_object())
            .collect::<Vec<PyObject>>();

        Ok(PyList::new(py, &values).into_object())
    }

    fn add_in_memory_impl(&self, py: Python<'_>, resource: PyObject) -> PyResult<PyObject> {
        let mut collector = self.collector(py).borrow_mut();
        let typ = resource.get_type(py);

        match typ.name(py).as_ref() {
            "PythonExtensionModule" => {
                let module = resource.cast_into::<PythonExtensionModule>(py)?;
                let repr = module.__repr__(py)?;

                let resource = module.get_resource(py);

                if let Some(location) = &resource.shared_library {
                    collector
                        .add_python_extension_module(&resource, &ConcreteResourceLocation::InMemory)
                        .with_context(|| format!("adding {}", repr))
                        .map_err(|e| PyErr::new::<ValueError, _>(py, format!("{:?}", e)))?;

                    Ok(py.None())
                } else {
                    Err(PyErr::new::<ValueError, _>(
                        py,
                        "PythonExtensionModule lacks a shared library",
                    ))
                }
            }
            "PythonModuleBytecode" => {
                let module = resource.cast_into::<PythonModuleBytecode>(py)?;
                let repr = module.__repr__(py)?;
                collector
                    .add_python_module_bytecode(
                        &module.get_resource(py),
                        &ConcreteResourceLocation::InMemory,
                    )
                    .with_context(|| format!("adding {}", repr))
                    .map_err(|e| PyErr::new::<ValueError, _>(py, format!("{:?}", e)))?;

                Ok(py.None())
            }
            "PythonModuleSource" => {
                let module = resource.cast_into::<PythonModuleSource>(py)?;
                let repr = module.__repr__(py)?;
                collector
                    .add_python_module_source(
                        &module.get_resource(py),
                        &ConcreteResourceLocation::InMemory,
                    )
                    .with_context(|| format!("adding {}", repr))
                    .map_err(|e| PyErr::new::<ValueError, _>(py, format!("{:?}", e)))?;

                Ok(py.None())
            }
            "PythonPackageResource" => {
                let resource = resource.cast_into::<PythonPackageResource>(py)?;
                let repr = resource.__repr__(py)?;
                collector
                    .add_python_package_resource(
                        &resource.get_resource(py),
                        &ConcreteResourceLocation::InMemory,
                    )
                    .with_context(|| format!("adding {}", repr))
                    .map_err(|e| PyErr::new::<ValueError, _>(py, format!("{:?}", e)))?;

                Ok(py.None())
            }
            "PythonPackageDistributionResource" => {
                let resource = resource.cast_into::<PythonPackageDistributionResource>(py)?;
                let repr = resource.__repr__(py)?;
                collector
                    .add_python_package_distribution_resource(
                        &resource.get_resource(py),
                        &ConcreteResourceLocation::InMemory,
                    )
                    .with_context(|| format!("adding {}", repr))
                    .map_err(|e| PyErr::new::<ValueError, _>(py, format!("{:?}", e)))?;

                Ok(py.None())
            }
            _ => Err(PyErr::new::<TypeError, _>(
                py,
                format!("cannot operate on {} values", typ.name(py)),
            )),
        }
    }

    fn add_filesystem_relative_impl(
        &self,
        py: Python<'_>,
        prefix: String,
        resource: PyObject,
    ) -> PyResult<PyObject> {
        let mut collector = self.collector(py).borrow_mut();

        match resource.get_type(py).name(py).as_ref() {
            "PythonExtensionModule" => {
                let module = resource.cast_into::<PythonExtensionModule>(py)?;
                let repr = module.__repr__(py)?;
                let resource = module.get_resource(py);

                collector
                    .add_python_extension_module(
                        &resource,
                        &ConcreteResourceLocation::RelativePath(prefix),
                    )
                    .with_context(|| format!("adding {}", repr))
                    .map_err(|e| PyErr::new::<ValueError, _>(py, format!("{:?}", e)))?;

                Ok(py.None())
            }
            "PythonModuleBytecode" => {
                let module = resource.cast_into::<PythonModuleBytecode>(py)?;
                let repr = module.__repr__(py)?;
                collector
                    .add_python_module_bytecode(
                        &module.get_resource(py),
                        &ConcreteResourceLocation::RelativePath(prefix),
                    )
                    .with_context(|| format!("adding {}", repr))
                    .map_err(|e| PyErr::new::<ValueError, _>(py, format!("{:?}", e)))?;

                Ok(py.None())
            }
            "PythonModuleSource" => {
                let module = resource.cast_into::<PythonModuleSource>(py)?;
                let repr = module.__repr__(py)?;
                collector
                    .add_python_module_source(
                        &module.get_resource(py),
                        &ConcreteResourceLocation::RelativePath(prefix),
                    )
                    .with_context(|| format!("adding {}", repr))
                    .map_err(|e| PyErr::new::<ValueError, _>(py, format!("{:?}", e)))?;

                Ok(py.None())
            }
            "PythonPackageResource" => {
                let resource = resource.cast_into::<PythonPackageResource>(py)?;
                let repr = resource.__repr__(py)?;
                collector
                    .add_python_package_resource(
                        &resource.get_resource(py),
                        &ConcreteResourceLocation::RelativePath(prefix),
                    )
                    .with_context(|| format!("adding {}", repr))
                    .map_err(|e| PyErr::new::<ValueError, _>(py, format!("{:?}", e)))?;

                Ok(py.None())
            }
            "PythonPackageDistributionResource" => {
                let resource = resource.cast_into::<PythonPackageDistributionResource>(py)?;
                let repr = resource.__repr__(py)?;
                collector
                    .add_python_package_distribution_resource(
                        &resource.get_resource(py),
                        &ConcreteResourceLocation::RelativePath(prefix),
                    )
                    .with_context(|| format!("adding {}", repr))
                    .map_err(|e| PyErr::new::<ValueError, _>(py, format!("{:?}", e)))?;

                Ok(py.None())
            }
            name => Err(PyErr::new::<TypeError, _>(
                py,
                format!("cannot operate on {} values", name),
            )),
        }
    }

    fn oxidize_impl(&self, py: Python<'_>, python_exe: Option<PyObject>) -> PyResult<PyObject> {
        let python_exe = match python_exe {
            Some(p) => p,
            None => {
                let sys_module = py.import("sys")?;
                let executable = sys_module.get(py, "executable")?;

                executable
            }
        };
        let python_exe = pyobject_to_pathbuf(py, python_exe)?;

        let tempfile = py.import("tempfile")?;
        let temp_dir = tempfile.call(py, "TemporaryDirectory", NoArgs, None)?;
        let temp_dir_name = temp_dir.getattr(py, "name")?;
        let temp_dir_path = pyobject_to_pathbuf(py, temp_dir_name)?;

        let collector = self.collector(py).borrow();

        let mut compiler = BytecodeCompiler::new(&python_exe, &temp_dir_path).map_err(|e| {
            PyErr::new::<ValueError, _>(
                py,
                format!("error constructing bytecode compiler: {:?}", e),
            )
        })?;

        let prepared: CompiledResourcesCollection<'_> = collector
            .compile_resources(&mut compiler)
            .context("compiling resources")
            .map_err(|e| PyErr::new::<ValueError, _>(py, format!("error oxidizing: {:?}", e)))?;

        let mut resources = Vec::new();

        for resource in prepared.resources.values() {
            resources.push(resource_to_pyobject(py, resource)?);
        }

        let mut file_installs = Vec::new();

        for (path, location, executable) in &prepared.extra_files {
            let path = path_to_pathlib_path(py, path)?;
            let data = location
                .resolve()
                .map_err(|e| PyErr::new::<ValueError, _>(py, e.to_string()))?;
            let data = PyBytes::new(py, &data);
            let executable = executable.to_py_object(py);

            file_installs.push((path, data, executable).into_py_object(py));
        }

        Ok((resources.into_py_object(py), file_installs)
            .into_py_object(py)
            .into_object())
    }
}
