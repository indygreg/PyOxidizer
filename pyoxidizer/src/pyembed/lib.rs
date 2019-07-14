// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Manage an embedded Python interpreter.

The `pyembed` crate contains functionality for managing a Python interpreter
embedded in the current binary. This crate is typically used along with
[PyOxidizer](https://github.com/indygreg/PyOxidizer) for producing
self-contained binaries containing Python.

The most important types are [`PythonConfig`](struct.PythonConfig.html) and
[`MainPythonInterpreter`](struct.MainPythonInterpreter.html). A `PythonConfig`
defines how a Python interpreter is to behave. A `MainPythonInterpreter`
creates and manages that interpreter and serves as a high-level interface for
running code in the interpreter.
*/

mod config;
mod data;
mod importer;
mod osutils;
mod pyalloc;
mod pyinterp;
mod pystr;

#[allow(unused_imports)]
pub use crate::config::PythonConfig;

#[allow(unused_imports)]
pub use crate::data::default_python_config;

#[allow(unused_imports)]
pub use crate::pyinterp::MainPythonInterpreter;
