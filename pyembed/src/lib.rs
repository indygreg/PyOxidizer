// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Control an embedded Python interpreter.

The `pyembed` crate contains functionality for controlling an embedded
Python interpreter running in the current process.

`pyembed` provides significant additional functionality over what is covered
by the official
[Embedding Python in Another Application](https://docs.python.org/3.7/extending/embedding.html)
docs and provided by the [CPython C API](https://docs.python.org/3.7/c-api/).
For example, `pyembed` defines a custom Python *meta path importer* that can
import Python module bytecode from memory using 0-copy.

While this crate is conceptually generic and can be used as a high-level
manager of an embedded Python interpreter (it has a high-level API that
makes running an embedded Python interpreter relatively easy), the crate
was designed for use with [PyOxidizer](https://github.com/indygreg/PyOxidizer).
If you are leveraging the advanced features like the module importer that can
import modules from memory using 0-copy, you probably want to use this crate
with `PyOxidizer`.

The most important types in this crate are
[`OxidizedPythonInterpreterConfig`](struct.OxidizedPythonInterpreterConfig.html)
and [`MainPythonInterpreter`](struct.MainPythonInterpreter.html). An
`OxidizedPythonInterpreterConfig` defines how a Python interpreter is to
behave. A `MainPythonInterpreter` creates and manages that interpreter and
serves as a high-level interface for running code in the interpreter.

# Dependencies

Under the hood, `pyembed` makes direct use of the `python3-sys` crate for
low-level Python FFI bindings as well as the `cpython` crate for higher-level
interfacing.

**It is an explicit goal of this crate to rely on as few external dependencies
as possible.** This is because we want to minimize bloat in produced binaries.
At this time, we have required direct dependencies on published versions of the
`anyhow`, `lazy_static`, `libc`, `memmap`, `python-packed-resources`, and `uuid`
crates. On Windows, this list is extended by `memory-module-sys` and `winapi`,
which are required to support loading DLLs from memory. We also have an optional
direct dependency on the `jemalloc-sys` crate.

This crate requires linking against a library providing CPython C symbols.
(This dependency is via the `python3-sys` crate.) On Windows, this library
must be named `pythonXY`.

# Features

The optional `jemalloc` feature controls support for using
[jemalloc](http://jemalloc.net/) as Python's memory allocator. Use of Jemalloc
from Python is a run-time configuration option controlled by the
`PythonConfig` type and having `jemalloc` compiled into the binary does not
mean it is being used!

There exist mutually exclusive `build-mode-*` features to control how the
`build.rs` build script works.

`build-mode-standalone` (the default) builds the crate as a standalone crate
and doesn't attempt to do anything special at build time.

`build-mode-pyoxidizer-exe` attempts to invoke a `pyoxidizer` executable
to build required artifacts.

`build-mode-prebuilt-artifacts` will attempt to use artifacts produced by
`PyOxidizer` out-of-band. In this mode, the `PYOXIDIZER_ARTIFACT_DIR`
environment variable can refer to the directory containing build artifacts
that this crate needs. If not set, `OUT_DIR` will be used.

The exist mutually exclusive `cpython-link-*` features to control how
the `cpython`/`python3-sys` crates are built.

`cpython-link-unresolved-static` instructs to leave the Python symbols
as unresolved. This crate will provide a static library providing the
symbols.

`cpython-link-default` builds `cpython` with default link mode control.
That crate's build script will attempt to find a `libpython` from the
`python` defined by `PYTHON_SYS_EXECUTABLE` or present on `PATH`.

*/

#[cfg(not(library_mode = "extension"))]
mod config;
mod conversion;
mod importer;
#[cfg(not(library_mode = "extension"))]
mod interpreter;
#[cfg(not(library_mode = "extension"))]
mod interpreter_config;
#[cfg(windows)]
mod memory_dll;
#[cfg(not(library_mode = "extension"))]
mod osutils;
mod package_metadata;
#[cfg(not(library_mode = "extension"))]
mod pyalloc;
#[cfg(not(library_mode = "extension"))]
mod python_eval;
#[allow(unused_variables)]
mod python_resource_collector;
mod python_resource_types;
mod python_resources;
mod resource_scanning;
#[cfg(not(library_mode = "extension"))]
pub mod technotes;
#[cfg(test)]
mod test;

#[cfg(not(library_mode = "extension"))]
#[allow(unused_imports)]
pub use crate::config::{
    Allocator, CheckHashPYCsMode, CoerceCLocale, ExtensionModule, OptimizationLevel,
    OxidizedPythonInterpreterConfig, PythonConfig, PythonInterpreterConfig,
    PythonInterpreterProfile, PythonRawAllocator, PythonRunMode, TerminfoResolution,
};

#[cfg(not(library_mode = "extension"))]
#[allow(unused_imports)]
pub use crate::interpreter::{MainPythonInterpreter, NewInterpreterError};

#[cfg(not(library_mode = "extension"))]
#[allow(unused_imports)]
pub use crate::python_eval::{
    run, run_and_handle_error, run_code, run_file, run_module_as_main, run_repl,
};

#[cfg(library_mode = "extension")]
pub use crate::importer::PyInit_oxidized_importer;
