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
//!
//! # Features
//!
//! * Parse X.509 certificates from BER, DER, and PEM.
//! * Access and manipulation of low-level ASN.1 data structures defining
//!   certificates. See [rfc5280::Certificate] for the main X.509 certificate type.
//! * Serialize X.509 certificates to BER, DER, and PEM.
//! * Higher-level APIs for interfacing with [rfc3280::Name] types, which
//!   define subject and issuer fields but have a very difficult to work with
//!   data structure.
//! * Rust enums defining key algorithms [KeyAlgorithm], signature algorithms
//!   [SignatureAlgorithm], and digest algorithms [DigestAlgorithm] commonly
//!   found in X.509 certificates. These can be converted to/from OIDs as well
//!   as to their respective ASN.1 types that express them in X.509 certificates.
//! * Verification of cryptographic signatures in certificates. If you have a
//!   parsed X.509 certificate and a public key (which is embedded in the
//!   issuing certificate), we can tell you if that certificate was signed
//!   by that key/certificate.
//! * Generating new X.509 certificates with an easy-to-use builder type. See
//!   [X509CertificateBuilder].
//!
//! # Security Disclaimer
//!
//! This crate has not been audited by a security professional. It may contain
//! severe bugs. Use in some security sensitive contexts is not advised.
//!
//! In particular, the ASN.1 parser isn't hardened against malicious inputs.
//! And there are some ASN.1 types in the parsing code that will result in
//! panics.
//!
//! # Known Isuses
//!
//! This code was originally developed as part of the [cryptographic-message-syntax]
//! crate, which was developed to support implement Apple code signing in pure Rust.
//! After reinventing X.509 certificate handling logic in multiple crates, it was
//! decided to create this crate as a unified interface to managing X.509 certificates.
//! While an attempt has been made to make the APIs useful in a standalone context,
//! some of the history of this crate's intent may leak into its design. PRs that
//! pass GitHub Actions to improve matters are gladly accepted!
//!
//! Not all ASN.1 types are implemented. You may encounter panics for some
//! less tested code paths. Patches to improve the situation are much appreciated!
//!
//! We are using the bcder crate for ASN.1. Use of the yasna crate would be preferred,
//! as it seems to be more popular. However, the author initially couldn't get yasna
//! working with RFC 5652 ASN.1. However, this was likely due to his lack of knowledge
//! of ASN.1 at the time. A port to yasna (or any other ASN.1 parser) might be in the
//! future.
//!
//! Because of the history of this crate, many tests covering its functionality exist
//! elsewhere in the repo. Overall test coverage could also likely be improved.
//! There is no fuzzing or corpora of X.509 certificates that we're testing against,
//! for example.

pub mod algorithm;
pub use algorithm::{DigestAlgorithm, EcdsaCurve, KeyAlgorithm, SignatureAlgorithm};
pub mod asn1time;
pub mod certificate;
pub use certificate::{
    CapturedX509Certificate, MutableX509Certificate, X509Certificate, X509CertificateBuilder,
};
pub mod rfc2986;
pub mod rfc3280;
pub mod rfc3447;
pub mod rfc4519;
pub mod rfc5280;
pub mod rfc5480;
pub mod rfc5652;
pub mod rfc5915;
pub mod rfc5958;
pub mod rfc8017;
pub mod signing;
pub use signing::{InMemorySigningKeyPair, KeyInfoSigner, Sign, Signature};
#[cfg(any(feature = "test", test))]
pub mod testutil;

use thiserror::Error;

pub use signature::Signer;

/// Errors related to X.509 certificate handling.
#[derive(Debug, Error)]
pub enum X509CertificateError {
    #[error("unknown digest algorithm: {0}")]
    UnknownDigestAlgorithm(String),

    #[error("unknown signature algorithm: {0}")]
    UnknownSignatureAlgorithm(String),

    #[error("unknown key algorithm: {0}")]
    UnknownKeyAlgorithm(String),

    #[error("unknown elliptic curve: {0}")]
    UnknownEllipticCurve(String),

    #[error("KeyAlgorithm encountered unexpected algorithm parameters: {0}")]
    UnhandledKeyAlgorithmParameters(&'static str),

    #[error("can not verify {1:?} signatures made with key algorithm {0:?}")]
    UnsupportedSignatureVerification(KeyAlgorithm, SignatureAlgorithm),

    #[error("ring rejected loading private key: {0}")]
    PrivateKeyRejected(&'static str),

    #[error("error when decoding ASN.1 data: {0}")]
    Asn1Parse(bcder::decode::DecodeError<std::convert::Infallible>),

    #[error("I/O error occurred: {0}")]
    Io(#[from] std::io::Error),

    #[error("error decoding PEM data: {0}")]
    PemDecode(pem::PemError),

    #[error("error creating signature: {0}")]
    SigningError(#[from] signature::Error),

    #[error("error creating cryptographic signature with memory-backed key-pair")]
    SignatureCreationInMemoryKey,

    #[error("certificate signature verification failed")]
    CertificateSignatureVerificationFailed,

    #[error("error generating key pair")]
    KeyPairGenerationError,

    #[error("RSA key generation is not supported")]
    RsaKeyGenerationNotSupported,

    #[error("target length for PKCS#1 padding to too short")]
    PkcsEncodeTooShort,

    #[error("unhandled error: {0}")]
    Other(String),
}

impl From<ring::error::KeyRejected> for X509CertificateError {
    fn from(e: ring::error::KeyRejected) -> Self {
        Self::PrivateKeyRejected(e.description_())
    }
}

impl From<bcder::decode::DecodeError<std::convert::Infallible>> for X509CertificateError {
    fn from(e: bcder::decode::DecodeError<std::convert::Infallible>) -> Self {
        Self::Asn1Parse(e)
    }
}
