// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Defines Python type objects that represent Python resources. */

use {
    crate::conversion::pyobject_to_owned_bytes,
    cpython::py_class,
    python_packaging::resource::{
        BytecodeOptimizationLevel, PythonExtensionModule as RawPythonExtensionModule,
        PythonModuleBytecode as RawPythonModuleBytecode,
        PythonModuleSource as RawPythonModuleSource,
        PythonPackageDistributionResource as RawPythonPackageDistributionResource,
        PythonPackageResource as RawPythonPackageResource,
    },
    std::cell::{Ref, RefCell},
    std::convert::TryFrom,
    tugger_file_manifest::FileData,
};

py_class!(pub(crate) class PythonModuleSource |py| {
    data resource: RefCell<RawPythonModuleSource>;

    def __repr__(&self) -> cpython::PyResult<String> {
        Ok(format!("<PythonModuleSource module=\"{}\">", self.resource(py).borrow().name))
    }

    @property def module(&self) -> cpython::PyResult<String> {
        Ok(self.resource(py).borrow().name.to_string())
    }

    @module.setter def set_module(&self, value: Option<&str>) -> cpython::PyResult<()> {
        if let Some(value) = value {
            self.resource(py).borrow_mut().name = value.to_string();

            Ok(())
        } else {
            Err(cpython::PyErr::new::<cpython::exc::TypeError, _>(py, "cannot delete module"))
        }
    }

    @property def source(&self) -> cpython::PyResult<cpython::PyBytes> {
        let source = self.resource(py).borrow().source.resolve_content().map_err(|_| cpython::PyErr::new::<cpython::exc::ValueError, _>(py, "error resolving source code"))?;

        Ok(cpython::PyBytes::new(py, &source))
    }

    @source.setter def set_source(&self, value: Option<cpython::PyObject>) -> cpython::PyResult<()> {
        if let Some(value) = value {
            self.resource(py).borrow_mut().source = FileData::Memory(
                pyobject_to_owned_bytes(py, &value)?
            );

            Ok(())
        } else {
            Err(cpython::PyErr::new::<cpython::exc::TypeError, _>(py, "cannot delete source"))
        }
    }

    @property def is_package(&self) -> cpython::PyResult<bool> {
        Ok(self.resource(py).borrow().is_package)
    }

    @is_package.setter def set_is_package(&self, value: Option<bool>) -> cpython::PyResult<()> {
        if let Some(value) = value {
            self.resource(py).borrow_mut().is_package = value;

            Ok(())
        } else {
            Err(cpython::PyErr::new::<cpython::exc::TypeError, _>(py, "cannot delete is_package"))
        }
    }
});

impl PythonModuleSource {
    pub fn new(py: cpython::Python, resource: RawPythonModuleSource) -> cpython::PyResult<Self> {
        PythonModuleSource::create_instance(py, RefCell::new(resource))
    }

    pub fn get_resource<'a>(&'a self, py: cpython::Python<'a>) -> Ref<'a, RawPythonModuleSource> {
        self.resource(py).borrow()
    }
}

py_class!(pub(crate) class PythonModuleBytecode |py| {
    data resource: RefCell<RawPythonModuleBytecode>;

    def __repr__(&self) -> cpython::PyResult<String> {
        Ok(format!("<PythonModuleBytecode module=\"{}\">", self.resource(py).borrow().name))
    }

    @property def module(&self) -> cpython::PyResult<String> {
        Ok(self.resource(py).borrow().name.to_string())
    }

    @module.setter def set_module(&self, value: Option<&str>) -> cpython::PyResult<()> {
        if let Some(value) = value {
            self.resource(py).borrow_mut().name = value.to_string();

            Ok(())
        } else {
            Err(cpython::PyErr::new::<cpython::exc::TypeError, _>(py, "cannot delete module"))
        }
    }

    @property def bytecode(&self) -> cpython::PyResult<cpython::PyBytes> {
        let bytecode = self.resource(py).borrow().resolve_bytecode().map_err(|_| cpython::PyErr::new::<cpython::exc::ValueError, _>(py, "error resolving bytecode"))?;

        Ok(cpython::PyBytes::new(py, &bytecode))
    }

    @bytecode.setter def set_bytecode(&self, value: Option<cpython::PyObject>) -> cpython::PyResult<()> {
        if let Some(value) = value {
            self.resource(py).borrow_mut().set_bytecode(
                &pyobject_to_owned_bytes(py, &value)?
            );

            Ok(())
        } else {
            Err(cpython::PyErr::new::<cpython::exc::TypeError, _>(py, "cannot delete bytecode"))
        }
    }

    @property def optimize_level(&self) -> cpython::PyResult<i32> {
        Ok(self.resource(py).borrow().optimize_level.into())
    }

    @optimize_level.setter def set_optimize_level(&self, value: Option<i32>) -> cpython::PyResult<()> {
        if let Some(value) = value {
            let value = BytecodeOptimizationLevel::try_from(value).map_err(|_| cpython::PyErr::new::<cpython::exc::ValueError, _>(py, "invalid bytecode optimization level"))?;

            self.resource(py).borrow_mut().optimize_level = value;

            Ok(())
        } else {
            Err(cpython::PyErr::new::<cpython::exc::TypeError, _>(py, "cannot delete optimize_level"))
        }
    }

    @property def is_package(&self) -> cpython::PyResult<bool> {
        Ok(self.resource(py).borrow().is_package)
    }

    @is_package.setter def set_is_package(&self, value: Option<bool>) -> cpython::PyResult<()> {
        if let Some(value) = value {
            self.resource(py).borrow_mut().is_package = value;

            Ok(())
        } else {
            Err(cpython::PyErr::new::<cpython::exc::TypeError, _>(py, "cannot delete is_package"))
        }
    }
});

impl PythonModuleBytecode {
    pub fn new(py: cpython::Python, resource: RawPythonModuleBytecode) -> cpython::PyResult<Self> {
        PythonModuleBytecode::create_instance(py, RefCell::new(resource))
    }

    pub fn get_resource<'a>(&'a self, py: cpython::Python<'a>) -> Ref<'a, RawPythonModuleBytecode> {
        self.resource(py).borrow()
    }
}

py_class!(pub(crate) class PythonPackageResource |py| {
    data resource: RefCell<RawPythonPackageResource>;

    def __repr__(&self) -> cpython::PyResult<String> {
        let resource = self.resource(py).borrow();
        Ok(format!("<PythonPackageResource package=\"{}\", path=\"{}\">",
            resource.leaf_package, resource.relative_name
        ))
    }

    @property def package(&self) -> cpython::PyResult<String> {
        Ok(self.resource(py).borrow().leaf_package.clone())
    }

    @package.setter def set_package(&self, value: Option<&str>) -> cpython::PyResult<()> {
        if let Some(value) = value {
            self.resource(py).borrow_mut().leaf_package = value.to_string();

            Ok(())
        } else {
            Err(cpython::PyErr::new::<cpython::exc::TypeError, _>(py, "cannot delete package"))
        }
    }

    @property def name(&self) -> cpython::PyResult<String> {
        Ok(self.resource(py).borrow().relative_name.clone())
    }

    @name.setter def set_name(&self, value: Option<&str>) -> cpython::PyResult<()> {
        if let Some(value) = value {
            self.resource(py).borrow_mut().relative_name = value.to_string();

            Ok(())
        } else {
            Err(cpython::PyErr::new::<cpython::exc::TypeError, _>(py, "cannot delete name"))
        }
    }

    @property def data(&self) -> cpython::PyResult<cpython::PyBytes> {
        let data = self.resource(py).borrow().data.resolve_content().map_err(|_| cpython::PyErr::new::<cpython::exc::ValueError, _>(py, "error resolving data"))?;

        Ok(cpython::PyBytes::new(py, &data))
    }

    @data.setter def set_data(&self, value: Option<cpython::PyObject>) -> cpython::PyResult<()> {
        if let Some(value) = value {
            self.resource(py).borrow_mut().data = FileData::Memory(
                pyobject_to_owned_bytes(py, &value)?
            );

            Ok(())
        } else {
            Err(cpython::PyErr::new::<cpython::exc::TypeError, _>(py, "cannot delete data"))
        }
    }
});

impl PythonPackageResource {
    pub fn new(py: cpython::Python, resource: RawPythonPackageResource) -> cpython::PyResult<Self> {
        PythonPackageResource::create_instance(py, RefCell::new(resource))
    }

    pub fn get_resource<'a>(
        &'a self,
        py: cpython::Python<'a>,
    ) -> Ref<'a, RawPythonPackageResource> {
        self.resource(py).borrow()
    }
}

py_class!(pub(crate) class PythonPackageDistributionResource |py| {
    data resource: RefCell<RawPythonPackageDistributionResource>;

    def __repr__(&self) -> cpython::PyResult<String> {
        let resource = self.resource(py).borrow();
        Ok(format!("<PythonPackageDistributionResource package=\"{}\", path=\"{}\">",
            resource.package, resource.name
        ))
    }

    @property def package(&self) -> cpython::PyResult<String> {
        Ok(self.resource(py).borrow().package.clone())
    }

    @package.setter def set_package(&self, value: Option<&str>) -> cpython::PyResult<()> {
        if let Some(value) = value {
            self.resource(py).borrow_mut().package = value.to_string();

            Ok(())
        } else {
            Err(cpython::PyErr::new::<cpython::exc::TypeError, _>(py, "cannot delete package"))
        }
    }

    @property def version(&self) -> cpython::PyResult<String> {
        Ok(self.resource(py).borrow().version.clone())
    }

    @version.setter def set_version(&self, value: Option<&str>) -> cpython::PyResult<()> {
        if let Some(value) = value {
            self.resource(py).borrow_mut().version = value.to_string();

            Ok(())
        } else {
            Err(cpython::PyErr::new::<cpython::exc::TypeError, _>(py, "cannot delete version"))
        }
    }

    @property def name(&self) -> cpython::PyResult<String> {
        Ok(self.resource(py).borrow().name.clone())
    }

    @name.setter def set_name(&self, value: Option<&str>) -> cpython::PyResult<()> {
        if let Some(value) = value {
            self.resource(py).borrow_mut().name = value.to_string();

            Ok(())
        } else {
            Err(cpython::PyErr::new::<cpython::exc::TypeError, _>(py, "cannot delete name"))
        }
    }

    @property def data(&self) -> cpython::PyResult<cpython::PyBytes> {
        let data = self.resource(py).borrow().data.resolve_content().map_err(|_| cpython::PyErr::new::<cpython::exc::ValueError, _>(py, "error resolving data"))?;

        Ok(cpython::PyBytes::new(py, &data))
    }

    @data.setter def set_data(&self, value: Option<cpython::PyObject>) -> cpython::PyResult<()> {
        if let Some(value) = value {
            self.resource(py).borrow_mut().data = FileData::Memory(
                pyobject_to_owned_bytes(py, &value)?
            );

            Ok(())
        } else {
            Err(cpython::PyErr::new::<cpython::exc::TypeError, _>(py, "cannot delete data"))
        }
    }
});

impl PythonPackageDistributionResource {
    pub fn new(
        py: cpython::Python,
        resource: RawPythonPackageDistributionResource,
    ) -> cpython::PyResult<Self> {
        PythonPackageDistributionResource::create_instance(py, RefCell::new(resource))
    }

    pub fn get_resource<'a>(
        &'a self,
        py: cpython::Python<'a>,
    ) -> Ref<'a, RawPythonPackageDistributionResource> {
        self.resource(py).borrow()
    }
}

py_class!(pub(crate) class PythonExtensionModule |py| {
    data resource: RefCell<RawPythonExtensionModule>;

    def __repr__(&self) -> cpython::PyResult<String> {
        Ok(format!("<PythonExtensionModule module=\"{}\">",
            self.resource(py).borrow().name))
    }

    @property def name(&self) -> cpython::PyResult<String> {
        Ok(self.resource(py).borrow().name.clone())
    }
});

impl PythonExtensionModule {
    pub fn new(py: cpython::Python, resource: RawPythonExtensionModule) -> cpython::PyResult<Self> {
        PythonExtensionModule::create_instance(py, RefCell::new(resource))
    }

    pub fn get_resource<'a>(
        &'a self,
        py: cpython::Python<'a>,
    ) -> Ref<'a, RawPythonExtensionModule> {
        self.resource(py).borrow()
    }
}
