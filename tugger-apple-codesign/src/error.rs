// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    cryptographic_message_syntax::{CertificateKeyAlgorithm, CmsError},
    thiserror::Error,
};

/// Unified error type for Apple code signing.
#[derive(Debug, Error)]
pub enum AppleCodesignError {
    #[error("unknown command")]
    CliUnknownCommand,

    #[error("bad argument")]
    CliBadArgument,

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("binary parsing error: {0}")]
    Goblin(#[from] goblin::error::Error),

    #[error("Mach-O parsing error: {0}")]
    MachO(#[from] crate::macho::MachOError),

    #[error("binary does not have code signature data")]
    BinaryNoCodeSignature,

    #[error("digest error: {0}")]
    Digest(#[from] crate::macho::DigestError),

    #[error("CMS error: {0}")]
    Cms(#[from] CmsError),

    #[error("binary not signable: {0}")]
    NotSignable(#[from] crate::signing::NotSignableError),

    #[error("signing error: {0}")]
    Signing(#[from] crate::signing::SigningError),

    #[error("problems reported during verification")]
    VerificationProblems,

    #[error("code requirement error: {0}")]
    CodeRequirement(#[from] crate::code_requirement::CodeRequirementError),

    #[error("certificate decode error: {0}")]
    CertificateDecode(bcder::decode::Error),

    #[error("unsupported key algorithm in certificate: {0:?}")]
    CertificateUnsupportedKeyAlgorithm(CertificateKeyAlgorithm),

    #[error("unspecified cryptography error in certificate")]
    CertificateRing(ring::error::Unspecified),

    #[error("bad string value in certificate: {0:?}")]
    CertificateCharset(bcder::string::CharSetError),
}
