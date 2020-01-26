// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
The `starlark` module and related sub-modules define the
[Starlark](https://github.com/bazelbuild/starlark) dialect used to
define Oxidized Python binaries.
*/

pub mod env;
pub mod eval;
pub mod file_resource;
pub mod python_distribution;
pub mod python_embedded_resources;
pub mod python_executable;
pub mod python_interpreter_config;
pub mod python_resource;
pub mod target;
#[cfg(test)]
mod testutil;
pub mod util;
