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
pub mod certificate;
pub use certificate::{CapturedX509Certificate, MutableX509Certificate, X509Certificate};
pub mod rfc3280;
pub mod rfc4519;
pub mod rfc5280;
pub mod rfc5480;
pub mod rfc5652;
pub mod rfc5915;
pub mod rfc5958;
pub mod signing;
pub use signing::InMemorySigningKeyPair;

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

    #[error("ring rejected loading private key: {0}")]
    PrivateKeyRejected(&'static str),

    #[error("error when decoding ASN.1 data: {0}")]
    Asn1Parse(bcder::decode::Error),

    #[error("I/O error occurred: {0}")]
    Io(#[from] std::io::Error),

    #[error("error decoding PEM data: {0}")]
    PemDecode(pem::PemError),

    #[error("error creating cryptographic signature with memory-backed key-pair")]
    SignatureCreationInMemoryKey,

    #[error("certificate signature verification failed")]
    CertificateSignatureVerificationFailed,
}

impl From<ring::error::KeyRejected> for X509CertificateError {
    fn from(e: ring::error::KeyRejected) -> Self {
        Self::PrivateKeyRejected(e.description_())
    }
}

impl From<bcder::decode::Error> for X509CertificateError {
    fn from(e: bcder::decode::Error) -> Self {
        Self::Asn1Parse(e)
    }
}
