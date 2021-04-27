// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Interface with X.509 certificates.
//!
//! This crate provides an interface to X.509 certificates.
//!
//! Low-level ASN.1 primitives are defined in modules having the name of the
//! RFC in which they are defined.
//!
//! Higher-level primitives that most end-users will want to use are defined
//! in sub-modules but exported from the main crate.

pub mod algorithm;
pub use algorithm::{DigestAlgorithm, KeyAlgorithm, SignatureAlgorithm};
pub mod asn1time;
pub mod rfc3280;
pub mod rfc4519;
pub mod rfc5280;

use thiserror::Error;

/// Errors related to X.509 certificate handling.
#[derive(Debug, Error)]
pub enum X509CertificateError {
    #[error("unknown digest algorithm: {0}")]
    UnknownDigestAlgorithm(String),

    #[error("unknown signature algorithm: {0}")]
    UnknownSignatureAlgorithm(String),

    #[error("unknown key algorithm: {0}")]
    UnknownKeyAlgorithm(String),
}
