// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Control an embedded Python interpreter.

The `pyembed` crate contains functionality for controlling an embedded
Python interpreter running in the current process.

`pyembed` provides significant additional functionality over what is covered
by the official
[Embedding Python in Another Application](https://docs.python.org/3/extending/embedding.html)
docs and provided by the [CPython C API](https://docs.python.org/3/c-api/).
For example, `pyembed` defines a custom Python *meta path importer* that can
import Python module bytecode from memory using 0-copy.

This crate was initially designed for and is maintained as part of the
[PyOxidizer](https://github.com/indygreg/PyOxidizer) project. However,
the crate is generic and can be used outside the PyOxidizer project.

The most important types in this crate are
[`OxidizedPythonInterpreterConfig`](struct.OxidizedPythonInterpreterConfig.html)
and [`MainPythonInterpreter`](struct.MainPythonInterpreter.html). An
`OxidizedPythonInterpreterConfig` defines how a Python interpreter is to
behave. A `MainPythonInterpreter` creates and manages that interpreter and
serves as a high-level interface for running code in the interpreter.

# Dependencies

Under the hood, `pyembed` makes direct use of the `pyo3` crate for
low-level Python FFI bindings as well as higher-level interfacing.

**It is an explicit goal of this crate to rely on as few external dependencies
as possible.** This is because we want to minimize bloat in produced binaries.
At this time, we have required direct dependencies on published versions of the
`anyhow`, `dunce`, `libc`, `memmap`, `once_cell`, `python-packed-resources`,
`python-packaging`, `tugger-file-manifest`, and `uuid` crates. On Windows, this
list is extended by `memory-module-sys` and `winapi`, which are required to
support loading DLLs from memory. We also have an optional direct dependency
on the `jemalloc-sys`, `libmimalloc-sys`, and `snmalloc-sys` crates for custom
memory allocators.

# Features

The optional `jemalloc` feature controls support for using
[jemalloc](http://jemalloc.net/) as Python's memory allocator. Use of Jemalloc
from Python is a run-time configuration option controlled by the
`OxidizedPythonInterpreterConfig` type and having `jemalloc` compiled into the
binary does not mean it is being used!

The optional `mimalloc` feature controls support for using
[mimalloc](https://github.com/microsoft/mimalloc) as Python's memory allocator.
The feature behaves similarly to `jemalloc`, which is documented above.

The optional `snmalloc` feature controls support for using
[snmalloc](https://github.com/microsoft/snmalloc) as Python's memory allocator.
The feature behaves similarly to `jemalloc`, which is documented above.

The optional `extension-module` feature changes the way the crate is built
so that the built crate can be used as a Python extension module.
*/

#[allow(unused)]
mod config;
mod conversion;
mod error;
#[allow(clippy::manual_strip, clippy::transmute_ptr_to_ptr, clippy::zero_ptr)]
mod extension;
#[allow(clippy::manual_strip, clippy::transmute_ptr_to_ptr, clippy::zero_ptr)]
mod importer;
#[cfg(not(library_mode = "extension"))]
mod interpreter;
#[cfg(not(library_mode = "extension"))]
mod interpreter_config;
#[cfg(windows)]
mod memory_dll;
#[cfg(not(library_mode = "extension"))]
mod osutils;
#[allow(clippy::manual_strip, clippy::transmute_ptr_to_ptr, clippy::zero_ptr)]
mod package_metadata;
#[allow(clippy::manual_strip, clippy::transmute_ptr_to_ptr, clippy::zero_ptr)]
mod pkg_resources;
#[cfg(not(library_mode = "extension"))]
mod pyalloc;
#[allow(
    unused_variables,
    clippy::manual_strip,
    clippy::transmute_ptr_to_ptr,
    clippy::zero_ptr
)]
mod python_resource_collector;
#[allow(clippy::manual_strip, clippy::transmute_ptr_to_ptr, clippy::zero_ptr)]
mod python_resource_types;
#[allow(clippy::manual_strip, clippy::transmute_ptr_to_ptr, clippy::zero_ptr)]
mod python_resources;
mod resource_scanning;
#[cfg(not(library_mode = "extension"))]
pub mod technotes;
#[cfg(test)]
mod test;

pub use crate::{config::PackedResourcesSource, error::NewInterpreterError};

#[cfg(not(library_mode = "extension"))]
#[allow(unused_imports)]
pub use crate::{
    config::{
        ExtensionModule, OxidizedPythonInterpreterConfig, ResolvedOxidizedPythonInterpreterConfig,
    },
    interpreter::MainPythonInterpreter,
    python_resources::PythonResourcesState,
};

#[cfg(library_mode = "extension")]
pub use crate::extension::PyInit_oxidized_importer;

#[cfg(not(library_mode = "extension"))]
#[allow(unused_imports)]
pub use python_packaging::{
    interpreter::{
        Allocator, BytesWarning, CheckHashPycsMode, CoerceCLocale, MemoryAllocatorBackend,
        MultiprocessingStartMethod, PythonInterpreterConfig, PythonInterpreterProfile,
        TerminfoResolution,
    },
    resource::BytecodeOptimizationLevel,
};
