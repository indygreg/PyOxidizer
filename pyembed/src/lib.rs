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
[`PythonConfig`](struct.PythonConfig.html) and
[`MainPythonInterpreter`](struct.MainPythonInterpreter.html). A `PythonConfig`
defines how a Python interpreter is to behave. A `MainPythonInterpreter`
creates and manages that interpreter and serves as a high-level interface for
running code in the interpreter.

# Dependencies

Under the hood, `pyembed` makes direct use of the `python3-sys` crate for
low-level Python FFI bindings as well as the `cpython` crate for higher-level
interfacing.

**It is an explicit goal of this crate to rely on as few external dependencies
as possible.** This is because we want to minimize bloat in produced binaries.
At this time, we have required direct dependencies on published versions of the
`byteorder`, `libc`, and `uuid` crates. We also have an optional direct
dependency on the `jemalloc-sys` crate. Via the `cpython` crate, we also
have an indirect dependency on the `num-traits` crate.

This crate requires linking against a library providing CPython C symbols.
(This dependency is via the `python3-sys` crate.) On Windows, this library
must be named `pythonXY`.

# Features

The optional `jemalloc-sys` feature controls support for using
[jemalloc](http://jemalloc.net/) as Python's memory allocator. Use of Jemalloc
from Python is a run-time configuration option controlled by the
`PythonConfig` type and having `jemalloc` compiled into the binary does not
mean it is being used!


*/

mod config;
mod importer;
mod osutils;
mod pyalloc;
mod pyinterp;
mod pystr;
pub mod specifications;
pub mod technotes;

#[allow(unused_imports)]
pub use crate::config::{
    ExtensionModule, PythonConfig, PythonRawAllocator, PythonRunMode, TerminfoResolution,
};

#[allow(unused_imports)]
pub use crate::pyinterp::MainPythonInterpreter;
