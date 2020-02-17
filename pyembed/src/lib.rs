// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Control an embedded Python interpreter.

The `pyembed` crate contains functionality for controlling an embedded
Python interpreter running in the current process.

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
*/

mod config;
mod importer;
mod osutils;
mod pyalloc;
mod pyinterp;
mod pystr;
pub mod specifications;

#[allow(unused_imports)]
pub use crate::config::{
    ExtensionModule, PythonConfig, PythonRawAllocator, PythonRunMode, TerminfoResolution,
};

#[allow(unused_imports)]
pub use crate::pyinterp::MainPythonInterpreter;
