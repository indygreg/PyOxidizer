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
    cpython::{ObjectProtocol, PythonObject, ToPyObject},
    python_packaging::{
        filesystem_scanning::find_python_resources, module_util::PythonModuleSuffixes,
        resource::PythonResource,
    },
};

/// Scans a filesystem path for Python resources and turns them into Python types.
pub(crate) fn find_resources_in_path(
    py: cpython::Python,
    path: cpython::PyObject,
) -> cpython::PyResult<cpython::PyObject> {
    let path = pyobject_to_pathbuf(py, path)?;

    if !path.is_dir() {
        return Err(cpython::PyErr::new::<cpython::exc::ValueError, _>(
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

    let mut res: Vec<cpython::PyObject> = Vec::new();

    let iter = find_python_resources(&path, &cache_tag, &suffixes, false, true);

    for resource in iter {
        let resource = resource.map_err(|e| {
            cpython::PyErr::new::<cpython::exc::ValueError, _>(
                py,
                format!("error scanning filesystem: {}", e),
            )
        })?;

        match resource {
            PythonResource::ModuleSource(source) => {
                res.push(PythonModuleSource::new(py, source.into_owned())?.into_object());
            }
            PythonResource::ModuleBytecode(bytecode) => {
                res.push(PythonModuleBytecode::new(py, bytecode.into_owned())?.into_object());
            }
            PythonResource::ExtensionModule(extension) => {
                res.push(PythonExtensionModule::new(py, extension.into_owned())?.into_object());
            }
            PythonResource::PackageResource(resource) => {
                res.push(PythonPackageResource::new(py, resource.into_owned())?.into_object());
            }
            PythonResource::PackageDistributionResource(resource) => res.push(
                PythonPackageDistributionResource::new(py, resource.into_owned())?.into_object(),
            ),
            PythonResource::ModuleBytecodeRequest(_) => {}
            PythonResource::EggFile(_) => {}
            PythonResource::PathExtension(_) => {}
            PythonResource::File(_) => {}
        }
    }

    Ok(res.into_py_object(py).into_object())
}
