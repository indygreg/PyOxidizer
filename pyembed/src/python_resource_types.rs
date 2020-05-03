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
        BytecodeOptimizationLevel, DataLocation, PythonExtensionModule as RawPythonExtensionModule,
        PythonModuleBytecode as RawPythonModuleBytecode,
        PythonModuleSource as RawPythonModuleSource,
        PythonPackageDistributionResource as RawPythonPackageDistributionResource,
        PythonPackageResource as RawPythonPackageResource,
    },
    std::cell::{Ref, RefCell},
    std::convert::TryFrom,
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

    pub fn get_resource<'a>(&'a self, py: Python<'a>) -> Ref<'a, RawPythonModuleSource> {
        self.resource(py).borrow()
    }
}

py_class!(pub class PythonModuleBytecode |py| {
    data resource: RefCell<RawPythonModuleBytecode>;

    def __repr__(&self) -> PyResult<String> {
        Ok(format!("<PythonModuleBytecode module=\"{}\">", self.resource(py).borrow().name))
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

    @property def bytecode(&self) -> PyResult<PyBytes> {
        let bytecode = self.resource(py).borrow().resolve_bytecode().or_else(|_| {
            Err(PyErr::new::<ValueError, _>(py, "error resolving bytecode"))
        })?;

        Ok(PyBytes::new(py, &bytecode))
    }

    @bytecode.setter def set_bytecode(&self, value: Option<PyObject>) -> PyResult<()> {
        if let Some(value) = value {
            self.resource(py).borrow_mut().set_bytecode(
                &pyobject_to_owned_bytes(py, &value)?
            );

            Ok(())
        } else {
            Err(PyErr::new::<TypeError, _>(py, "cannot delete bytecode"))
        }
    }

    @property def optimize_level(&self) -> PyResult<i32> {
        Ok(self.resource(py).borrow().optimize_level.into())
    }

    @optimize_level.setter def set_optimize_level(&self, value: Option<i32>) -> PyResult<()> {
        if let Some(value) = value {
            let value = BytecodeOptimizationLevel::try_from(value).or_else(|_| {
                Err(PyErr::new::<ValueError, _>(py, "invalid bytecode optimization level"))
            })?;

            self.resource(py).borrow_mut().optimize_level = value;

            Ok(())
        } else {
            Err(PyErr::new::<TypeError, _>(py, "cannot delete optimize_level"))
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

impl PythonModuleBytecode {
    pub fn new(py: Python, resource: RawPythonModuleBytecode) -> PyResult<Self> {
        PythonModuleBytecode::create_instance(py, RefCell::new(resource))
    }

    pub fn get_resource<'a>(&'a self, py: Python<'a>) -> Ref<'a, RawPythonModuleBytecode> {
        self.resource(py).borrow()
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

    @property def package(&self) -> PyResult<String> {
        Ok(self.resource(py).borrow().leaf_package.clone())
    }

    @package.setter def set_package(&self, value: Option<&str>) -> PyResult<()> {
        if let Some(value) = value {
            self.resource(py).borrow_mut().leaf_package = value.to_string();

            Ok(())
        } else {
            Err(PyErr::new::<TypeError, _>(py, "cannot delete package"))
        }
    }

    @property def name(&self) -> PyResult<String> {
        Ok(self.resource(py).borrow().relative_name.clone())
    }

    @name.setter def set_name(&self, value: Option<&str>) -> PyResult<()> {
        if let Some(value) = value {
            self.resource(py).borrow_mut().relative_name = value.to_string();

            Ok(())
        } else {
            Err(PyErr::new::<TypeError, _>(py, "cannot delete name"))
        }
    }

    @property def data(&self) -> PyResult<PyBytes> {
        let data = self.resource(py).borrow().data.resolve().or_else(|_| {
            Err(PyErr::new::<ValueError, _>(py, "error resolving data"))
        })?;

        Ok(PyBytes::new(py, &data))
    }

    @data.setter def set_data(&self, value: Option<PyObject>) -> PyResult<()> {
        if let Some(value) = value {
            self.resource(py).borrow_mut().data = DataLocation::Memory(
                pyobject_to_owned_bytes(py, &value)?
            );

            Ok(())
        } else {
            Err(PyErr::new::<TypeError, _>(py, "cannot delete data"))
        }
    }
});

impl PythonPackageResource {
    pub fn new(py: Python, resource: RawPythonPackageResource) -> PyResult<Self> {
        PythonPackageResource::create_instance(py, RefCell::new(resource))
    }

    pub fn get_resource<'a>(&'a self, py: Python<'a>) -> Ref<'a, RawPythonPackageResource> {
        self.resource(py).borrow()
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

    @property def package(&self) -> PyResult<String> {
        Ok(self.resource(py).borrow().package.clone())
    }

    @package.setter def set_package(&self, value: Option<&str>) -> PyResult<()> {
        if let Some(value) = value {
            self.resource(py).borrow_mut().package = value.to_string();

            Ok(())
        } else {
            Err(PyErr::new::<TypeError, _>(py, "cannot delete package"))
        }
    }

    @property def version(&self) -> PyResult<String> {
        Ok(self.resource(py).borrow().version.clone())
    }

    @version.setter def set_version(&self, value: Option<&str>) -> PyResult<()> {
        if let Some(value) = value {
            self.resource(py).borrow_mut().version = value.to_string();

            Ok(())
        } else {
            Err(PyErr::new::<TypeError, _>(py, "cannot delete version"))
        }
    }

    @property def name(&self) -> PyResult<String> {
        Ok(self.resource(py).borrow().name.clone())
    }

    @name.setter def set_name(&self, value: Option<&str>) -> PyResult<()> {
        if let Some(value) = value {
            self.resource(py).borrow_mut().name = value.to_string();

            Ok(())
        } else {
            Err(PyErr::new::<TypeError, _>(py, "cannot delete name"))
        }
    }

    @property def data(&self) -> PyResult<PyBytes> {
        let data = self.resource(py).borrow().data.resolve().or_else(|_| {
            Err(PyErr::new::<ValueError, _>(py, "error resolving data"))
        })?;

        Ok(PyBytes::new(py, &data))
    }

    @data.setter def set_data(&self, value: Option<PyObject>) -> PyResult<()> {
        if let Some(value) = value {
            self.resource(py).borrow_mut().data = DataLocation::Memory(
                pyobject_to_owned_bytes(py, &value)?
            );

            Ok(())
        } else {
            Err(PyErr::new::<TypeError, _>(py, "cannot delete data"))
        }
    }
});

impl PythonPackageDistributionResource {
    pub fn new(py: Python, resource: RawPythonPackageDistributionResource) -> PyResult<Self> {
        PythonPackageDistributionResource::create_instance(py, RefCell::new(resource))
    }

    pub fn get_resource<'a>(
        &'a self,
        py: Python<'a>,
    ) -> Ref<'a, RawPythonPackageDistributionResource> {
        self.resource(py).borrow()
    }
}

py_class!(pub class PythonExtensionModule |py| {
    data resource: RefCell<RawPythonExtensionModule>;

    def __repr__(&self) -> PyResult<String> {
        Ok(format!("<PythonExtensionModule module=\"{}\">",
            self.resource(py).borrow().name))
    }

    @property def name(&self) -> PyResult<String> {
        Ok(self.resource(py).borrow().name.clone())
    }
});

impl PythonExtensionModule {
    pub fn new(py: Python, resource: RawPythonExtensionModule) -> PyResult<Self> {
        PythonExtensionModule::create_instance(py, RefCell::new(resource))
    }

    pub fn get_resource<'a>(&'a self, py: Python<'a>) -> Ref<'a, RawPythonExtensionModule> {
        self.resource(py).borrow()
    }
}
