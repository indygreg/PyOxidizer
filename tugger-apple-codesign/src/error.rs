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

    #[error("certificate decode error: {0}")]
    CertificateDecode(bcder::decode::Error),

    #[error("unsupported key algorithm in certificate: {0:?}")]
    CertificateUnsupportedKeyAlgorithm(CertificateKeyAlgorithm),

    #[error("unspecified cryptography error in certificate")]
    CertificateRing(ring::error::Unspecified),

    #[error("bad string value in certificate: {0:?}")]
    CertificateCharset(bcder::string::CharSetError),

    #[error("unable to locate __LINKEDIT segment")]
    MissingLinkedit,

    #[error("bad header magic in {0}")]
    BadMagic(&'static str),

    #[error("data structure parse error: {0}")]
    Scroll(#[from] scroll::Error),

    #[error("malformed identifier string in code directory")]
    CodeDirectoryMalformedIdentifier,

    #[error("malformed team name string in code directory")]
    CodeDirectoryMalformedTeam,

    #[error("SuperBlob data is malformed")]
    SuperblobMalformed,

    #[error("functionality not implemented: {0}")]
    Unimplemented(&'static str),

    #[error("unknown code signature flag: {0}")]
    CodeSignatureUnknownFlag(String),

    #[error("entitlements data not valid UTF-8: {0}")]
    EntitlementsBadUtf8(std::str::Utf8Error),

    #[error("unknown code requirement opcode: {0}")]
    RequirementUnknownOpcode(u32),

    #[error("unknown code requirement match expression: {0}")]
    RequirementUnknownMatchExpression(u32),

    #[error("code requirement data malformed: {0}")]
    RequirementMalformed(&'static str),
}
