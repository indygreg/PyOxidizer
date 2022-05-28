// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Functionality for embedding and packaging Python.

PyOxidizer provides a myriad of functionality for packaging a Python
distribution and embedding it in a larger binary, oftentimes an executable.

This library exposes that functionality to other tools.
*/

mod default_python_distributions;
pub mod environment;
pub mod licensing;
pub mod project_building;
pub mod project_layout;
pub mod projectmgmt;
pub mod py_packaging;
pub mod python_distributions;
pub mod starlark;

#[cfg(test)]
mod testutil;
