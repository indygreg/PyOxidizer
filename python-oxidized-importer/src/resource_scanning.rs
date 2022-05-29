// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Functionality for scanning the filesystem for Python resources. */

use {
    crate::{
        conversion::pyobject_to_pathbuf,
        python_resource_types::{
            PythonExtensionModule, PythonModuleBytecode, PythonModuleSource,
            PythonPackageDistributionResource, PythonPackageResource,
        },
    },
    pyo3::{exceptions::PyValueError, prelude::*, types::PyList},
    python_packaging::{
        filesystem_scanning::find_python_resources, module_util::PythonModuleSuffixes,
        resource::PythonResource,
    },
};

/// Scans a filesystem path for Python resources and turns them into Python types.
#[pyfunction]
pub(crate) fn find_resources_in_path<'p>(py: Python<'p>, path: &PyAny) -> PyResult<&'p PyList> {
    let path = pyobject_to_pathbuf(py, path)?;

    if !path.is_dir() {
        return Err(PyValueError::new_err(format!(
            "path is not a directory: {}",
            path.display()
        )));
    }

    let sys_module = py.import("sys")?;
    let implementation = sys_module.getattr("implementation")?;
    let cache_tag = implementation.getattr("cache_tag")?.extract::<String>()?;

    let importlib_machinery = py.import("importlib.machinery")?;

    let source = importlib_machinery
        .getattr("SOURCE_SUFFIXES")?
        .extract::<Vec<String>>()?;
    let bytecode = importlib_machinery
        .getattr("BYTECODE_SUFFIXES")?
        .extract::<Vec<String>>()?;
    let debug_bytecode = importlib_machinery
        .getattr("DEBUG_BYTECODE_SUFFIXES")?
        .extract::<Vec<String>>()?;
    let optimized_bytecode = importlib_machinery
        .getattr("OPTIMIZED_BYTECODE_SUFFIXES")?
        .extract::<Vec<String>>()?;
    let extension = importlib_machinery
        .getattr("EXTENSION_SUFFIXES")?
        .extract::<Vec<String>>()?;

    let suffixes = PythonModuleSuffixes {
        source,
        bytecode,
        debug_bytecode,
        optimized_bytecode,
        extension,
    };

    let mut res: Vec<Py<PyAny>> = Vec::new();

    let iter = find_python_resources(&path, &cache_tag, &suffixes, false, true)
        .map_err(|e| PyValueError::new_err(format!("error scanning filesystem: {}", e)))?;

    for resource in iter {
        let resource = resource
            .map_err(|e| PyValueError::new_err(format!("error scanning filesystem: {}", e)))?;

        match resource {
            PythonResource::ModuleSource(source) => {
                res.push(PythonModuleSource::new(py, source.into_owned())?.to_object(py));
            }
            PythonResource::ModuleBytecode(bytecode) => {
                res.push(PythonModuleBytecode::new(py, bytecode.into_owned())?.to_object(py));
            }
            PythonResource::ExtensionModule(extension) => {
                res.push(PythonExtensionModule::new(py, extension.into_owned())?.to_object(py));
            }
            PythonResource::PackageResource(resource) => {
                res.push(PythonPackageResource::new(py, resource.into_owned())?.to_object(py));
            }
            PythonResource::PackageDistributionResource(resource) => res.push(
                PythonPackageDistributionResource::new(py, resource.into_owned())?.to_object(py),
            ),
            PythonResource::ModuleBytecodeRequest(_) => {}
            PythonResource::EggFile(_) => {}
            PythonResource::PathExtension(_) => {}
            PythonResource::File(_) => {}
        }
    }

    Ok(PyList::new(py, &res))
}

pub(crate) fn init_module(m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(find_resources_in_path, m)?)?;

    Ok(())
}
