// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
The `starlark` module and related sub-modules define the
[Starlark](https://github.com/bazelbuild/starlark) dialect used to
define Oxidized Python binaries.
*/

pub mod config;
pub mod distribution;
pub mod embedded_python_config;
pub mod env;
pub mod eval;
pub mod file_resource;
pub mod python_distribution;
pub mod python_packaging;
pub mod python_resource;
pub mod python_run_mode;
#[cfg(test)]
mod testutil;
