// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Python Packaging Implemented in Rust

This crate exposes functionality for interacting with Python resources
and packaging facilities.
*/

pub mod bytecode;
pub mod filesystem_scanning;
pub mod interpreter;
pub mod libpython;
pub mod licensing;
pub mod location;
pub mod module_util;
pub mod package_metadata;
pub mod policy;
pub mod python_source;
pub mod resource;
pub mod resource_collection;

#[cfg(feature = "wheel")]
pub mod wheel;
#[cfg(feature = "wheel")]
pub mod wheel_builder;
