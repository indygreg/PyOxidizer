// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Python functionality for resource collection. */

use {
    crate::{
        conversion::{path_to_pathlib_path, pyobject_to_pathbuf},
        python_resource_types::{
            PythonExtensionModule, PythonModuleBytecode, PythonModuleSource,
            PythonPackageDistributionResource, PythonPackageResource,
        },
        python_resources::resource_to_pyobject,
    },
    anyhow::Context,
    pyo3::{
        exceptions::{PyTypeError, PyValueError},
        ffi as pyffi,
        prelude::*,
        types::{PyBytes, PyList, PyTuple},
        AsPyPointer,
    },
    python_packaging::{
        bytecode::BytecodeCompiler,
        location::{AbstractResourceLocation, ConcreteResourceLocation},
        resource_collection::{CompiledResourcesCollection, PythonResourceCollector},
    },
    std::{
        cell::RefCell,
        path::{Path, PathBuf},
    },
};

#[pyclass(module = "oxidized_importer")]
pub struct PyTempDir {
    cleanup: Py<PyAny>,
    path: PathBuf,
}

impl PyTempDir {
    pub fn new(py: Python) -> PyResult<Self> {
        let temp_dir = py
            .import("tempfile")?
            .getattr("TemporaryDirectory")?
            .call0()?;
        let cleanup = temp_dir.getattr("cleanup")?.into_py(py);
        let path = pyobject_to_pathbuf(py, temp_dir.getattr("name")?)?;

        Ok(Self { cleanup, path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for PyTempDir {
    fn drop(&mut self) {
        Python::with_gil(|py| {
            if self.cleanup.call0(py).is_err() {
                let cleanup = self.cleanup.as_ptr();
                unsafe { pyffi::PyErr_WriteUnraisable(cleanup) }
            }
        });
    }
}

#[pyclass(module = "oxidized_importer")]
pub(crate) struct OxidizedResourceCollector {
    collector: RefCell<PythonResourceCollector>,
}

#[pymethods]
impl OxidizedResourceCollector {
    fn __repr__(&self) -> &'static str {
        "<OxidizedResourceCollector>"
    }

    #[new]
    fn new(allowed_locations: Vec<String>) -> PyResult<Self> {
        let allowed_locations = allowed_locations
            .iter()
            .map(|location| AbstractResourceLocation::try_from(location.as_str()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(PyValueError::new_err)?;

        let collector =
            PythonResourceCollector::new(allowed_locations.clone(), allowed_locations, true, true);

        Ok(Self {
            collector: RefCell::new(collector),
        })
    }

    #[getter]
    fn allowed_locations<'p>(&self, py: Python<'p>) -> PyResult<&'p PyList> {
        let values = self
            .collector
            .borrow()
            .allowed_locations()
            .iter()
            .map(|l| l.to_string().into_py(py))
            .collect::<Vec<Py<PyAny>>>();

        Ok(PyList::new(py, &values))
    }

    fn add_in_memory(&self, resource: &PyAny) -> PyResult<()> {
        let mut collector = self.collector.borrow_mut();
        let typ = resource.get_type();
        let repr = resource.repr()?;

        match typ.name()? {
            "PythonExtensionModule" => {
                let module_cell = resource.cast_as::<PyCell<PythonExtensionModule>>()?;
                let module = module_cell.borrow();
                let resource = module.get_resource();

                if resource.shared_library.is_some() {
                    collector
                        .add_python_extension_module(&resource, &ConcreteResourceLocation::InMemory)
                        .with_context(|| format!("adding {}", repr))
                        .map_err(|e| PyValueError::new_err(format!("{:?}", e)))?;

                    Ok(())
                } else {
                    Err(PyValueError::new_err(
                        "PythonExtensionModule lacks a shared library",
                    ))
                }
            }
            "PythonModuleBytecode" => {
                let module = resource.cast_as::<PyCell<PythonModuleBytecode>>()?;
                collector
                    .add_python_module_bytecode(
                        &module.borrow().get_resource(),
                        &ConcreteResourceLocation::InMemory,
                    )
                    .with_context(|| format!("adding {}", repr))
                    .map_err(|e| PyValueError::new_err(format!("{:?}", e)))?;

                Ok(())
            }
            "PythonModuleSource" => {
                let module = resource.cast_as::<PyCell<PythonModuleSource>>()?;
                collector
                    .add_python_module_source(
                        &module.borrow().get_resource(),
                        &ConcreteResourceLocation::InMemory,
                    )
                    .with_context(|| format!("adding {}", repr))
                    .map_err(|e| PyValueError::new_err(format!("{:?}", e)))?;

                Ok(())
            }
            "PythonPackageResource" => {
                let resource = resource.cast_as::<PyCell<PythonPackageResource>>()?;
                collector
                    .add_python_package_resource(
                        &resource.borrow().get_resource(),
                        &ConcreteResourceLocation::InMemory,
                    )
                    .with_context(|| format!("adding {}", repr))
                    .map_err(|e| PyValueError::new_err(format!("{:?}", e)))?;

                Ok(())
            }
            "PythonPackageDistributionResource" => {
                let resource = resource.cast_as::<PyCell<PythonPackageDistributionResource>>()?;
                collector
                    .add_python_package_distribution_resource(
                        &resource.borrow().get_resource(),
                        &ConcreteResourceLocation::InMemory,
                    )
                    .with_context(|| format!("adding {}", repr))
                    .map_err(|e| PyValueError::new_err(format!("{:?}", e)))?;

                Ok(())
            }
            type_name => Err(PyTypeError::new_err(format!(
                "cannot operate on {} values",
                type_name
            ))),
        }
    }

    fn add_filesystem_relative(&self, prefix: String, resource: &PyAny) -> PyResult<()> {
        let mut collector = self.collector.borrow_mut();

        let repr = resource.repr()?;

        match resource.get_type().name()? {
            "PythonExtensionModule" => {
                let module_cell = resource.cast_as::<PyCell<PythonExtensionModule>>()?;
                let module = module_cell.borrow();
                let resource = module.get_resource();

                collector
                    .add_python_extension_module(
                        &resource,
                        &ConcreteResourceLocation::RelativePath(prefix),
                    )
                    .with_context(|| format!("adding {}", repr))
                    .map_err(|e| PyValueError::new_err(format!("{:?}", e)))?;

                Ok(())
            }
            "PythonModuleBytecode" => {
                let module = resource.cast_as::<PyCell<PythonModuleBytecode>>()?;

                collector
                    .add_python_module_bytecode(
                        &module.borrow().get_resource(),
                        &ConcreteResourceLocation::RelativePath(prefix),
                    )
                    .with_context(|| format!("adding {}", repr))
                    .map_err(|e| PyValueError::new_err(format!("{:?}", e)))?;

                Ok(())
            }
            "PythonModuleSource" => {
                let module = resource.cast_as::<PyCell<PythonModuleSource>>()?;

                collector
                    .add_python_module_source(
                        &module.borrow().get_resource(),
                        &ConcreteResourceLocation::RelativePath(prefix),
                    )
                    .with_context(|| format!("adding {}", repr))
                    .map_err(|e| PyValueError::new_err(format!("{:?}", e)))?;

                Ok(())
            }
            "PythonPackageResource" => {
                let resource = resource.cast_as::<PyCell<PythonPackageResource>>()?;

                collector
                    .add_python_package_resource(
                        &resource.borrow().get_resource(),
                        &ConcreteResourceLocation::RelativePath(prefix),
                    )
                    .with_context(|| format!("adding {}", repr))
                    .map_err(|e| PyValueError::new_err(format!("{:?}", e)))?;

                Ok(())
            }
            "PythonPackageDistributionResource" => {
                let resource = resource.cast_as::<PyCell<PythonPackageDistributionResource>>()?;

                collector
                    .add_python_package_distribution_resource(
                        &resource.borrow().get_resource(),
                        &ConcreteResourceLocation::RelativePath(prefix),
                    )
                    .with_context(|| format!("adding {}", repr))
                    .map_err(|e| PyValueError::new_err(format!("{:?}", e)))?;

                Ok(())
            }
            name => Err(PyTypeError::new_err(format!(
                "cannot operate on {} values",
                name
            ))),
        }
    }

    #[args(python_exe = "None")]
    fn oxidize<'p>(&self, py: Python<'p>, python_exe: Option<&PyAny>) -> PyResult<&'p PyTuple> {
        let python_exe = match python_exe {
            Some(p) => p,
            None => {
                let sys_module = py.import("sys")?;
                sys_module.getattr("executable")?
            }
        };
        let python_exe = pyobject_to_pathbuf(py, python_exe)?;
        let temp_dir = PyTempDir::new(py)?;
        let collector = self.collector.borrow();

        let mut compiler = BytecodeCompiler::new(&python_exe, temp_dir.path()).map_err(|e| {
            PyValueError::new_err(format!("error constructing bytecode compiler: {:?}", e))
        })?;

        let prepared: CompiledResourcesCollection = collector
            .compile_resources(&mut compiler)
            .context("compiling resources")
            .map_err(|e| PyValueError::new_err(format!("error oxidizing: {:?}", e)))?;

        let mut resources = Vec::new();

        for resource in prepared.resources.values() {
            resources.push(resource_to_pyobject(py, resource)?);
        }

        let mut file_installs = Vec::new();

        for (path, location, executable) in &prepared.extra_files {
            let path = path_to_pathlib_path(py, path)?;
            let data = location
                .resolve_content()
                .map_err(|e| PyValueError::new_err(e.to_string()))?;
            let data = PyBytes::new(py, &data);
            let executable = executable.to_object(py);

            file_installs.push((path, data, executable).to_object(py));
        }

        Ok(PyTuple::new(
            py,
            &[resources.to_object(py), file_installs.to_object(py)],
        ))
    }
}
