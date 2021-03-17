// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Binary signing for Apple platforms.

This crate contains code for interfacing with binary code signing on Apple
platforms.

*/

pub mod code_hash;
pub mod macho;
pub mod signing;
pub mod specification;

pub use crate::signing::{check_signing_capability, MachOSignatureBuilder, SigningError};
