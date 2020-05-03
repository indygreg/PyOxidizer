// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Defines Python type objects that represent Python resources. */

use {
    cpython::{py_class, PyResult, Python},
    python_packaging::resource::{
        PythonExtensionModule as RawPythonExtensionModule,
        PythonModuleBytecode as RawPythonModuleBytecode,
        PythonModuleSource as RawPythonModuleSource,
        PythonPackageDistributionResource as RawPythonPackageDistributionResource,
        PythonPackageResource as RawPythonPackageResource,
    },
    std::cell::RefCell,
};

py_class!(pub class PythonModuleSource |py| {
    data source: RefCell<RawPythonModuleSource>;

    def __repr__(&self) -> PyResult<String> {
        Ok(format!("<PythonModuleSource module=\"{}\">", self.source(py).borrow().name))
    }
});

impl PythonModuleSource {
    pub fn new(py: Python, source: RawPythonModuleSource) -> PyResult<Self> {
        PythonModuleSource::create_instance(py, RefCell::new(source))
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
