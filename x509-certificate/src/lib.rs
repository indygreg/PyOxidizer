// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Interface with X.509 certificates.
//!
//! This crate provides an interface to X.509 certificates.
//!
//! Low-level ASN.1 primitives are defined in modules having the name of the
//! RFC in which they are defined.

pub mod asn1time;
pub mod rfc3280;
pub mod rfc4519;
pub mod rfc5280;
