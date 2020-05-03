// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Defines Python type objects that represent Python resources. */

use {
    crate::conversion::pyobject_to_owned_bytes,
    cpython::exc::{TypeError, ValueError},
    cpython::{
        py_class, py_class_call_slot_impl_with_ref, py_class_prop_getter, py_class_prop_setter,
        PyBytes, PyErr, PyObject, PyResult, Python,
    },
    python_packaging::resource::{
        DataLocation, PythonExtensionModule as RawPythonExtensionModule,
        PythonModuleBytecode as RawPythonModuleBytecode,
        PythonModuleSource as RawPythonModuleSource,
        PythonPackageDistributionResource as RawPythonPackageDistributionResource,
        PythonPackageResource as RawPythonPackageResource,
    },
    std::cell::RefCell,
};

py_class!(pub class PythonModuleSource |py| {
    data resource: RefCell<RawPythonModuleSource>;

    def __repr__(&self) -> PyResult<String> {
        Ok(format!("<PythonModuleSource module=\"{}\">", self.resource(py).borrow().name))
    }

    @property def module(&self) -> PyResult<String> {
        Ok(self.resource(py).borrow().name.to_string())
    }

    @module.setter def set_module(&self, value: Option<&str>) -> PyResult<()> {
        if let Some(value) = value {
            self.resource(py).borrow_mut().name = value.to_string();

            Ok(())
        } else {
            Err(PyErr::new::<TypeError, _>(py, "cannot delete module"))
        }
    }

    @property def source(&self) -> PyResult<PyBytes> {
        let source = self.resource(py).borrow().source.resolve().or_else(|_| {
            Err(PyErr::new::<ValueError, _>(py, "error resolving source code"))
        })?;

        Ok(PyBytes::new(py, &source))
    }

    @source.setter def set_source(&self, value: Option<PyObject>) -> PyResult<()> {
        if let Some(value) = value {
            self.resource(py).borrow_mut().source = DataLocation::Memory(
                pyobject_to_owned_bytes(py, &value)?
            );

            Ok(())
        } else {
            Err(PyErr::new::<TypeError, _>(py, "cannot delete source"))
        }
    }

    @property def is_package(&self) -> PyResult<bool> {
        Ok(self.resource(py).borrow().is_package)
    }

    @is_package.setter def set_is_package(&self, value: Option<bool>) -> PyResult<()> {
        if let Some(value) = value {
            self.resource(py).borrow_mut().is_package = value;

            Ok(())
        } else {
            Err(PyErr::new::<TypeError, _>(py, "cannot delete is_package"))
        }
    }
});

impl PythonModuleSource {
    pub fn new(py: Python, resource: RawPythonModuleSource) -> PyResult<Self> {
        PythonModuleSource::create_instance(py, RefCell::new(resource))
    }
}

py_class!(pub class PythonModuleBytecode |py| {
    data bytecode: RefCell<RawPythonModuleBytecode>;

    def __repr__(&self) -> PyResult<String> {
        Ok(format!("<PythonModuleBytecode module=\"{}\">", self.bytecode(py).borrow().name))
    }
});

impl PythonModuleBytecode {
    pub fn new(py: Python, bytecode: RawPythonModuleBytecode) -> PyResult<Self> {
        PythonModuleBytecode::create_instance(py, RefCell::new(bytecode))
    }
}

py_class!(pub class PythonPackageResource |py| {
    data resource: RefCell<RawPythonPackageResource>;

    def __repr__(&self) -> PyResult<String> {
        let resource = self.resource(py).borrow();
        Ok(format!("<PythonPackageResource package=\"{}\", path=\"{}\">",
            resource.leaf_package, resource.relative_name
        ))
    }
});

impl PythonPackageResource {
    pub fn new(py: Python, resource: RawPythonPackageResource) -> PyResult<Self> {
        PythonPackageResource::create_instance(py, RefCell::new(resource))
    }
}

py_class!(pub class PythonPackageDistributionResource |py| {
    data resource: RefCell<RawPythonPackageDistributionResource>;

    def __repr__(&self) -> PyResult<String> {
        let resource = self.resource(py).borrow();
        Ok(format!("<PythonPackageDistributionResource package=\"{}\", path=\"{}\">",
            resource.package, resource.name
        ))
    }
});

impl PythonPackageDistributionResource {
    pub fn new(py: Python, resource: RawPythonPackageDistributionResource) -> PyResult<Self> {
        PythonPackageDistributionResource::create_instance(py, RefCell::new(resource))
    }
}

py_class!(pub class PythonExtensionModule |py| {
    data extension: RefCell<RawPythonExtensionModule>;

    def __repr__(&self) -> PyResult<String> {
        Ok(format!("<PythonExtensionModule module=\"{}\">",
            self.extension(py).borrow().name))
    }
});

impl PythonExtensionModule {
    pub fn new(py: Python, extension: RawPythonExtensionModule) -> PyResult<Self> {
        PythonExtensionModule::create_instance(py, RefCell::new(extension))
    }
}
