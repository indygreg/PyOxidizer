// Copyright 2022 Gregory Szorc.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

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
#[cfg(test)]
mod testutil;
#[cfg(feature = "wheel")]
pub mod wheel;
#[cfg(feature = "wheel")]
pub mod wheel_builder;
#[cfg(feature = "zip")]
pub mod zip_app_builder;
