// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Control an embedded Python interpreter.

The `pyembed` crate contains functionality for controlling an embedded
Python interpreter running in the current process.

`pyembed` provides additional functionality over what is covered by the official
[Embedding Python in Another Application](https://docs.python.org/3/extending/embedding.html)
docs and provided by the [CPython C API](https://docs.python.org/3/c-api/).
For example, `pyembed` can utilize a custom Python *meta path importer* that
can import Python module bytecode from memory using 0-copy.

This crate was initially designed for and is maintained as part of the
[PyOxidizer](https://github.com/indygreg/PyOxidizer) project. However,
the crate is generic and can be used outside the PyOxidizer project.

The most important types in this crate are
[OxidizedPythonInterpreterConfig] and [MainPythonInterpreter]. An
[OxidizedPythonInterpreterConfig] defines how a Python interpreter is to
behave. A [MainPythonInterpreter] creates and manages that interpreter and
serves as a high-level interface for running code in the interpreter.

# Dependencies

Under the hood, `pyembed` makes direct use of the `pyo3` crate for
low-level Python FFI bindings as well as higher-level interfacing.

**It is an explicit goal of this crate to rely on as few external dependencies
as possible.** This is because we want to minimize bloat in produced binaries.

# Features

The optional `allocator-jemalloc` feature controls support for using
[jemalloc](http://jemalloc.net/) as Python's memory allocator. Use of Jemalloc
from Python is a run-time configuration option controlled by the
[OxidizedPythonInterpreterConfig] type and having `jemalloc` compiled into the
binary does not mean it is being used!

The optional `allocator-mimalloc` feature controls support for using
[mimalloc](https://github.com/microsoft/mimalloc) as Python's memory allocator.
The feature behaves similarly to `jemalloc`, which is documented above.

The optional `allocator-snmalloc` feature controls support for using
[snmalloc](https://github.com/microsoft/snmalloc) as Python's memory allocator.
The feature behaves similarly to `jemalloc`, which is documented above.

The optional `serialization` feature controls whether configuration types
(such as [OxidizedPythonInterpreterConfig]) implement `Serialize` and
`Deserialize`.
*/

#[allow(unused)]
mod config;
mod conversion;
mod error;
mod interpreter;
mod interpreter_config;
mod osutils;
mod pyalloc;
pub mod technotes;
#[cfg(test)]
mod test;

#[allow(unused_imports)]
pub use {
    crate::{
        config::{
            ExtensionModule, OxidizedPythonInterpreterConfig,
            ResolvedOxidizedPythonInterpreterConfig,
        },
        error::NewInterpreterError,
        interpreter::MainPythonInterpreter,
        pyalloc::PythonMemoryAllocator,
    },
    oxidized_importer::{PackedResourcesSource, PythonResourcesState},
    python_packaging::{
        interpreter::{
            Allocator, BytesWarning, CheckHashPycsMode, CoerceCLocale, MemoryAllocatorBackend,
            MultiprocessingStartMethod, PythonInterpreterConfig, PythonInterpreterProfile,
            TerminfoResolution,
        },
        resource::BytecodeOptimizationLevel,
    },
};
