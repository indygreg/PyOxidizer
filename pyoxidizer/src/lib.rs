// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Tools for embedding and packaging Python.

PyOxidizer provides a myriad of functionality for packaging a Python
distribution and embedding it in a larger binary, oftentimes an executable.
*/

pub mod analyze;
pub mod environment;
pub mod projectmgmt;
pub mod pyrepackager;
pub mod python_distributions;

pub use pyrepackager::repackage::run_from_build;
