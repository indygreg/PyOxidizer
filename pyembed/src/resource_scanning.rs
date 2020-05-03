// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Functionality for scanning the filesystem for Python resources. */

use {
    crate::conversion::pyobject_to_pathbuf,
    crate::python_resource_types::{
        PythonExtensionModule, PythonModuleBytecode, PythonModuleSource,
        PythonPackageDistributionResource, PythonPackageResource,
    },
    cpython::exc::ValueError,
    cpython::{ObjectProtocol, PyErr, PyObject, PyResult, Python, PythonObject, ToPyObject},
    python_packaging::filesystem_scanning::find_python_resources,
    python_packaging::module_util::PythonModuleSuffixes,
    python_packaging::resource::PythonResource,
};

/// Scans a filesystem path for Python resources and turns them into Python types.
pub(crate) fn find_resources_in_path(py: Python, path: PyObject) -> PyResult<PyObject> {
    let path = pyobject_to_pathbuf(py, path)?;

    if !path.is_dir() {
        return Err(PyErr::new::<ValueError, _>(
            py,
            format!("path is not a directory: {}", path.display()),
        ));
    }

    let sys_module = py.import("sys")?;
    let implementation = sys_module.get(py, "implementation")?;
    let cache_tag = implementation
        .getattr(py, "cache_tag")?
        .extract::<String>(py)?;

    let importlib_machinery = py.import("importlib.machinery")?;

    let source = importlib_machinery
        .get(py, "SOURCE_SUFFIXES")?
        .extract::<Vec<String>>(py)?;
    let bytecode = importlib_machinery
        .get(py, "BYTECODE_SUFFIXES")?
        .extract::<Vec<String>>(py)?;
    let debug_bytecode = importlib_machinery
        .get(py, "DEBUG_BYTECODE_SUFFIXES")?
        .extract::<Vec<String>>(py)?;
    let optimized_bytecode = importlib_machinery
        .get(py, "OPTIMIZED_BYTECODE_SUFFIXES")?
        .extract::<Vec<String>>(py)?;
    let extension = importlib_machinery
        .get(py, "EXTENSION_SUFFIXES")?
        .extract::<Vec<String>>(py)?;

    let suffixes = PythonModuleSuffixes {
        source,
        bytecode,
        debug_bytecode,
        optimized_bytecode,
        extension,
    };

    let mut res: Vec<PyObject> = Vec::new();

    let iter = find_python_resources(&path, &cache_tag, &suffixes);

    for resource in iter {
        let resource = resource.or_else(|e| {
            Err(PyErr::new::<ValueError, _>(
                py,
                format!("error scanning filesystem: {}", e),
            ))
        })?;

        match resource {
            PythonResource::ModuleSource(source) => {
                res.push(PythonModuleSource::new(py, source)?.into_object());
            }
            PythonResource::ModuleBytecode(bytecode) => {
                res.push(PythonModuleBytecode::new(py, bytecode)?.into_object());
            }
            PythonResource::ExtensionModuleDynamicLibrary(extension) => {
                res.push(PythonExtensionModule::new(py, extension)?.into_object());
            }
            PythonResource::Resource(resource) => {
                res.push(PythonPackageResource::new(py, resource)?.into_object());
            }
            PythonResource::DistributionResource(resource) => {
                res.push(PythonPackageDistributionResource::new(py, resource)?.into_object())
            }
            _ => {}
        }
    }

    Ok(res.into_py_object(py).into_object())
}
