// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Support for importing from zip archives. */

use {
    crate::{conversion::pyobject_to_pathbuf, decode_source},
    anyhow::{anyhow, Result},
    pyo3::{
        buffer::PyBuffer,
        exceptions::{PyImportError, PyValueError},
        ffi as pyffi,
        prelude::*,
        types::{PyBytes, PyDict, PyType},
        PyNativeType, PyTraverseError, PyVisit,
    },
    std::{
        collections::HashMap,
        io::{BufReader, Cursor, Read, Seek},
        path::{Path, PathBuf},
    },
    zip::read::ZipArchive,
};

/// Represents a handle on a Python module within a [ZipImporter].
pub struct ZipPythonModule {
    /// Whether this module is a package.
    pub is_package: bool,

    /// Indexed path in [ZipImporter] of module source code.
    pub source_path: Option<PathBuf>,

    /// Indexed path in [ZipImporter] of module bytecode.
    pub bytecode_path: Option<PathBuf>,
}

/// Read Python resources from a zip file.
///
/// Instances are bound to a handle on a zip archive. At open time, the zip
/// archive index is read and an index of files within is constructed.
/// File content is resolved when individual members are accessed.
pub struct ZipIndex<R: Read + Seek> {
    /// Handle on zip archive that we're reading from.
    archive: ZipArchive<R>,
    prefix: Option<PathBuf>,
    members: HashMap<PathBuf, usize>,
}

impl<R: Read + Seek> ZipIndex<R> {
    /// Construct a new instance from a reader of zip data and an optional path prefix.
    ///
    /// The path prefix denotes the path within the zip file to look for content.
    /// All paths outside this prefix are ignored.
    pub fn new(reader: R, prefix: Option<&Path>) -> Result<Self> {
        let prefix = prefix.map(|p| p.to_path_buf());

        let mut archive = ZipArchive::new(reader)?;

        let mut members = HashMap::with_capacity(archive.len());

        for index in 0..archive.len() {
            let zf = archive.by_index_raw(index)?;

            // Ignore entries that don't have a valid filename within the archive.
            if let Some(name) = zf.enclosed_name() {
                // If told to look at paths within a subdirectory of the zip, ignore
                // all files outside this prefix and strip the leading path accordingly.
                let name = if let Some(prefix) = &prefix {
                    if !name.starts_with(prefix) {
                        continue;
                    }

                    name.strip_prefix(prefix)?
                } else {
                    name
                };

                members.insert(name.to_path_buf(), index);
            }
        }

        Ok(Self {
            archive,
            prefix,
            members,
        })
    }

    /// Attempt to locate a Python module within the zip archive.
    ///
    /// `full_name` is the fully qualified / dotted Python module name.
    pub fn find_python_module(&mut self, full_name: &str) -> Option<ZipPythonModule> {
        let mut common_path = self.prefix.clone().unwrap_or_default();
        common_path.extend(full_name.split('.'));

        let package_py_path = common_path.join("__init__").with_extension("py");
        let package_pyc_path = common_path.join("__init__").with_extension("pyc");

        let non_package_py_path = common_path.with_extension("py");
        let non_package_pyc_path = common_path.with_extension("pyc");

        let mut is_package = false;
        let mut source_path = None;
        let mut bytecode_path = None;

        if self.members.contains_key(&package_py_path) {
            is_package = true;
            source_path = Some(package_py_path);
        }

        if self.members.contains_key(&package_pyc_path) {
            is_package = true;
            bytecode_path = Some(package_pyc_path);
        }

        if is_package {
            return Some(ZipPythonModule {
                is_package,
                source_path,
                bytecode_path,
            });
        }

        if self.members.contains_key(&non_package_py_path) {
            source_path = Some(non_package_py_path);
        }
        if self.members.contains_key(&non_package_pyc_path) {
            bytecode_path = Some(non_package_pyc_path);
        }

        if source_path.is_some() || bytecode_path.is_some() {
            Some(ZipPythonModule {
                is_package,
                source_path,
                bytecode_path,
            })
        } else {
            None
        }
    }

    /// Resolve the byte content for a given path.
    ///
    /// Errors if the path does not exist.
    pub fn resolve_path_content(&mut self, path: &Path) -> Result<Vec<u8>> {
        let index = self
            .members
            .get(path)
            .ok_or_else(|| anyhow!("path {} not present in archive", path.display()))?;

        let mut zf = self.archive.by_index(*index)?;

        let mut buffer = Vec::<u8>::with_capacity(zf.size() as _);
        zf.read_to_end(&mut buffer)?;

        Ok(buffer)
    }
}

pub trait SeekableReader: Read + Seek + Send {}

impl SeekableReader for Cursor<Vec<u8>> {}
impl SeekableReader for Cursor<&[u8]> {}
impl SeekableReader for BufReader<std::fs::File> {}

/// A meta path finder that reads from zip archives.
///
/// Known incompatibilities with `zipimporter`:
///
/// * ResourceReader interface not implemented.
/// * ResourceLoader interface not implemented.
/// * Bytecode isn't validated.
#[pyclass(module = "oxidized_importer")]
pub struct OxidizedZipFinder {
    /// A PyObject backing storage of data.
    ///
    /// This exists to hold a reference to the PyObject backing storage to a memory slice.
    backing_pyobject: Option<Py<PyAny>>,

    /// The interface to the zip file.
    ///
    /// We can't have generic type parameters. So we need to define an explicit
    /// type backing the zip file. We choose `Vec<u8>` because we can always
    /// capture arbitrary data to a `Vec<u8>`.
    index: ZipIndex<Box<dyn SeekableReader>>,

    /// Path to advertise for this zip archive.
    ///
    /// This becomes the prefix for `__file__`. It is also used by the path hooks mechanism
    /// to identify this zip archive.
    ///
    /// May point to the current executable for in-memory zip archives.
    zip_path: PathBuf,

    /// `importlib._boostrap.ModuleSpec` type.
    module_spec_type: Py<PyAny>,

    /// `_io` Python module.
    io_module: Py<PyModule>,

    /// `marshal.loads` function.
    marshal_loads: Py<PyAny>,

    /// `builtins.compile` function.
    builtins_compile: Py<PyAny>,

    /// `builtins.exec` function.
    builtins_exec: Py<PyAny>,
}

impl OxidizedZipFinder {
    /// Construct a new instance from zip data.
    pub fn new_from_data(
        py: Python,
        zip_path: PathBuf,
        data: Vec<u8>,
        prefix: Option<&Path>,
    ) -> PyResult<Self> {
        let reader: Box<dyn SeekableReader> = Box::new(Cursor::new(data));

        let index = ZipIndex::new(reader, prefix)
            .map_err(|e| PyValueError::new_err(format!("error indexing zip data: {}", e)))?;

        Self::new_internal(py, index, zip_path, None)
    }

    /// Construct a new instance from a PyObject conforming to the buffer protocol.
    pub fn new_from_pyobject(
        py: Python,
        zip_path: PathBuf,
        source: &PyAny,
        prefix: Option<&Path>,
    ) -> PyResult<Self> {
        let buffer = PyBuffer::<u8>::get(source)?;

        let data = unsafe {
            std::slice::from_raw_parts::<u8>(buffer.buf_ptr() as *const _, buffer.len_bytes())
        };

        let reader: Box<dyn SeekableReader> = Box::new(Cursor::new(data));

        let index = ZipIndex::new(reader, prefix)
            .map_err(|e| PyValueError::new_err(format!("error indexing zip data: {}", e)))?;

        Self::new_internal(py, index, zip_path, Some(source.into_py(py)))
    }

    /// Construct a new instance from a reader.
    ///
    /// The full content of the reader will be read to an in-memory buffer.
    pub fn new_from_reader(
        py: Python,
        zip_path: PathBuf,
        reader: Box<dyn SeekableReader>,

        prefix: Option<&Path>,
    ) -> PyResult<Self> {
        let index = ZipIndex::new(reader, prefix)
            .map_err(|e| PyValueError::new_err(format!("error indexing zip data: {}", e)))?;

        Self::new_internal(py, index, zip_path, None)
    }

    fn new_internal(
        py: Python,
        index: ZipIndex<Box<dyn SeekableReader>>,
        zip_path: PathBuf,
        backing_pyobject: Option<Py<PyAny>>,
    ) -> PyResult<Self> {
        let importlib_bootstrap = py.import("_frozen_importlib")?;
        let module_spec_type = importlib_bootstrap.getattr("ModuleSpec")?.into_py(py);
        let io_module = py.import("_io")?.into_py(py);
        let marshal_module = py.import("marshal")?;
        let marshal_loads = marshal_module.getattr("loads")?.into_py(py);
        let builtins_module = py.import("builtins")?;
        let builtins_compile = builtins_module.getattr("compile")?.into_py(py);
        let builtins_exec = builtins_module.getattr("exec")?.into_py(py);

        Ok(Self {
            backing_pyobject,
            index,
            zip_path,
            module_spec_type,
            io_module,
            marshal_loads,
            builtins_compile,
            builtins_exec,
        })
    }

    fn resolve_python_module(
        slf: &mut PyRefMut<Self>,
        full_name: &str,
    ) -> PyResult<ZipPythonModule> {
        if let Some(module) = slf.index.find_python_module(full_name) {
            Ok(module)
        } else {
            Err(PyImportError::new_err((
                "module not found in zip archive",
                full_name.to_string(),
            )))
        }
    }
}

#[pymethods]
impl OxidizedZipFinder {
    fn __traverse__(&self, visit: PyVisit) -> Result<(), PyTraverseError> {
        if let Some(o) = &self.backing_pyobject {
            visit.call(o)?;
        }

        visit.call(&self.module_spec_type)?;
        visit.call(&self.io_module)?;
        visit.call(&self.marshal_loads)?;
        visit.call(&self.builtins_compile)?;
        visit.call(&self.builtins_exec)?;

        Ok(())
    }

    #[classmethod]
    #[allow(unused)]
    fn from_path(cls: &PyType, py: Python, path: &PyAny) -> PyResult<Self> {
        let path = pyobject_to_pathbuf(py, path)?;

        let f = Box::new(BufReader::new(std::fs::File::open(&path).map_err(|e| {
            PyValueError::new_err(format!("failed to open path {}: {}", path.display(), e))
        })?));

        Self::new_from_reader(py, path, f, None)
    }

    #[classmethod]
    #[args(path = "None")]
    #[allow(unused)]
    fn from_zip_data(
        cls: &PyType,
        py: Python,
        source: &PyAny,
        path: Option<&PyAny>,
    ) -> PyResult<Self> {
        let path = if let Some(o) = path {
            o
        } else {
            let sys_module = py.import("sys")?;
            sys_module.getattr("executable")?
        };

        let zip_path = pyobject_to_pathbuf(py, path)?;

        Self::new_from_pyobject(py, zip_path, source, None)
    }

    // Start of importlib.abc.MetaPathFinder interface.
    #[args(target = "None")]
    #[allow(unused)]
    fn find_spec<'p>(
        slf: &'p PyCell<Self>,
        fullname: String,
        path: &PyAny,
        target: Option<&PyAny>,
    ) -> PyResult<&'p PyAny> {
        // TODO support namespace packages for parity with zipimporter.

        let py = slf.py();
        let mut importer = slf.try_borrow_mut()?;

        let module = if let Some(module) = importer.index.find_python_module(&fullname) {
            module
        } else {
            return Ok(py.None().into_ref(py));
        };

        let module_spec_type = importer.module_spec_type.clone_ref(py);

        let kwargs = PyDict::new(py);
        kwargs.set_item("is_package", module.is_package)?;

        // origin is the path to the zip archive + the path within the archive.
        let mut origin = importer.zip_path.clone();
        if let Some(prefix) = &importer.index.prefix {
            origin = origin.join(prefix);
        }

        if let Some(path) = module.source_path {
            origin = origin.join(path);
        } else if let Some(path) = module.bytecode_path {
            origin = origin.join(path);
        }

        kwargs.set_item("origin", (&origin).into_py(py))?;

        let spec = module_spec_type
            .call(py, (&fullname, slf), Some(kwargs))?
            .into_ref(py);

        spec.setattr("has_location", true)?;
        spec.setattr("cached", py.None())?;

        // __path__ MUST be set on packages.
        // __path__ is an iterable of strings, which can be empty.
        if module.is_package {
            let parent = origin.parent().ok_or_else(|| {
                PyValueError::new_err(
                    "unable to determine dirname(origin); this should never happen",
                )
            })?;

            let locations = vec![parent.into_py(py)];
            spec.setattr("submodule_search_locations", locations)?;
        }

        Ok(spec)
    }

    #[allow(unused)]
    #[args(path = "None")]
    fn find_module<'p>(
        slf: &'p PyCell<Self>,
        fullname: String,
        path: Option<&PyAny>,
    ) -> PyResult<&'p PyAny> {
        // TODO support namespace packages for parity with zipimporter.

        let find_spec = slf.getattr("find_spec")?;
        let spec = find_spec.call((fullname, path), None)?;

        if spec.is_none() {
            Ok(slf.py().None().into_ref(slf.py()))
        } else {
            spec.getattr("loader")
        }
    }

    fn invalidate_caches(&self) -> PyResult<()> {
        Ok(())
    }

    // End of importlib.abc.MetaPathFinder interface.

    // Start of importlib.abc.Loader interface.

    #[allow(unused)]
    fn create_module(&self, py: Python, spec: &PyAny) -> PyResult<Py<PyAny>> {
        // Use default module creation semantics.
        Ok(py.None())
    }

    fn exec_module(slf: &PyCell<Self>, module: &PyAny) -> PyResult<Py<PyAny>> {
        let py = slf.py();

        let name = module.getattr("__name__")?;
        let full_name = name.extract::<String>()?;
        let dict = module.getattr("__dict__")?;

        let code = Self::get_code(slf, &full_name)?;

        let importer = slf.try_borrow()?;
        // Executing the module can lead to imports and nested borrows. So drop our
        // borrow before calling.
        let builtins_exec = importer.builtins_exec.clone_ref(py);
        std::mem::drop(importer);

        builtins_exec.call(py, (code, dict), None)
    }

    // End of importlib.abc.Loader interface.

    // Start of importlib.abc.InspectLoader interface.

    fn get_code(slf: &PyCell<Self>, fullname: &str) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let mut importer = slf.try_borrow_mut()?;

        let module: ZipPythonModule = Self::resolve_python_module(&mut importer, fullname)?;

        if let Some(path) = module.bytecode_path {
            let bytecode_data = importer.index.resolve_path_content(&path).map_err(|e| {
                PyImportError::new_err((
                    format!("error reading module bytecode from zip: {}", e),
                    fullname.to_string(),
                ))
            })?;

            // Minimize potential for nested borrow by dropping borrow as soon as possible.
            let marshal_loads = importer.marshal_loads.clone_ref(py);
            std::mem::drop(importer);

            // TODO validate the .pyc header.

            let bytecode = &bytecode_data[16..];
            let ptr = unsafe {
                pyffi::PyMemoryView_FromMemory(
                    bytecode.as_ptr() as _,
                    bytecode.len() as _,
                    pyffi::PyBUF_READ,
                )
            };

            let bytecode_obj = if ptr.is_null() {
                return Err(PyImportError::new_err((
                    "error coercing bytecode to memoryview",
                    fullname.to_string(),
                )));
            } else {
                unsafe { PyObject::from_owned_ptr(py, ptr) }
            };

            marshal_loads.call1(py, (bytecode_obj,))
        } else if let Some(path) = module.source_path {
            let source_bytes: Vec<u8> =
                importer.index.resolve_path_content(&path).map_err(|e| {
                    PyImportError::new_err((
                        format!("error reading module source from zip: {}", e),
                        fullname.to_string(),
                    ))
                })?;

            // Minimize potential for nested borrow by dropping borrow as soon as possible.
            let builtins_compile = importer.builtins_compile.clone_ref(py);
            std::mem::drop(importer);

            let source_bytes = PyBytes::new(py, &source_bytes);

            let crlf = PyBytes::new(py, b"\r\n");
            let lf = PyBytes::new(py, b"\n");
            let cr = PyBytes::new(py, b"\r");

            let source_bytes = source_bytes.call_method("replace", (crlf, lf), None)?;
            let source_bytes = source_bytes.call_method("replace", (cr, lf), None)?;

            let kwargs = PyDict::new(py);
            kwargs.set_item("dont_inherit", true)?;

            builtins_compile.call(py, (source_bytes, path, "exec"), Some(kwargs))
        } else {
            Err(PyImportError::new_err((
                "unable to resolve bytecode for module",
                fullname.to_string(),
            )))
        }
    }

    fn get_source(slf: &PyCell<Self>, fullname: &str) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let mut importer = slf.try_borrow_mut()?;

        let module = Self::resolve_python_module(&mut importer, fullname)?;

        let source_bytes = if let Some(source_path) = module.source_path {
            importer
                .index
                .resolve_path_content(&source_path)
                .map_err(|e| {
                    PyImportError::new_err((
                        format!("error reading module source from zip: {}", e),
                        fullname.to_string(),
                    ))
                })?
        } else {
            return Ok(py.None());
        };

        let source_bytes = PyBytes::new(py, &source_bytes);

        let source = decode_source(py, importer.io_module.as_ref(py), source_bytes)?;

        Ok(source.into_py(py))
    }

    fn is_package(slf: &PyCell<Self>, fullname: &str) -> PyResult<bool> {
        let mut importer = slf.try_borrow_mut()?;

        let module = Self::resolve_python_module(&mut importer, fullname)?;
        Ok(module.is_package)
    }

    // End of importlib.abc.InspectLoader interface.
}
