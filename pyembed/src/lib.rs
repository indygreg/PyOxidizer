// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

extern crate byteorder;
#[macro_use]
extern crate cpython;
extern crate libc;
extern crate python3_sys as pyffi;

mod data;
mod pyalloc;
mod pyinterp;
mod pymodules_module;
mod pystr;

#[allow(unused)]
pub use crate::pyinterp::{MainPythonInterpreter, PythonConfig};
