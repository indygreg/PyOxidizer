// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    cryptographic_message_syntax::{CertificateKeyAlgorithm, CmsError},
    std::path::PathBuf,
    thiserror::Error,
};

/// Unified error type for Apple code signing.
#[derive(Debug, Error)]
pub enum AppleCodesignError {
    #[error("unknown command")]
    CliUnknownCommand,

    #[error("bad argument")]
    CliBadArgument,

    #[error("{0}")]
    CliGeneralError(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("binary parsing error: {0}")]
    Goblin(#[from] goblin::error::Error),

    #[error("binary does not have code signature data")]
    BinaryNoCodeSignature,

    #[error("CMS error: {0}")]
    Cms(#[from] CmsError),

    #[error("problems reported during verification")]
    VerificationProblems,

    #[error("certificate decode error: {0}")]
    CertificateDecode(bcder::decode::Error),

    #[error("PEM error: {0}")]
    CertificatePem(pem::PemError),

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

    #[error("plist error in code directory: {0}")]
    CodeDirectoryPlist(plist::Error),

    #[error("SuperBlob data is malformed")]
    SuperblobMalformed,

    #[error("functionality not implemented: {0}")]
    Unimplemented(&'static str),

    #[error("unknown code signature flag: {0}")]
    CodeSignatureUnknownFlag(String),

    #[error("entitlements data not valid UTF-8: {0}")]
    EntitlementsBadUtf8(std::str::Utf8Error),

    #[error("unknown executable segment flag: {0}")]
    ExecutableSegmentUnknownFlag(String),

    #[error("unknown code requirement opcode: {0}")]
    RequirementUnknownOpcode(u32),

    #[error("unknown code requirement match expression: {0}")]
    RequirementUnknownMatchExpression(u32),

    #[error("code requirement data malformed: {0}")]
    RequirementMalformed(&'static str),

    #[error("plist error in code resources: {0}")]
    ResourcesPlist(plist::Error),

    #[error("base64 error in code resources: {0}")]
    ResourcesBase64(base64::DecodeError),

    #[error("plist parse error in code resources: {0}")]
    ResourcesPlistParse(String),

    #[error("bad regular expression in code resources: {0}; {1}")]
    ResourcesBadRegex(String, regex::Error),

    #[error("__LINKEDIT isn't final Mach-O segment")]
    LinkeditNotLast,

    #[error("__LINKEDIT segment contains data after signature")]
    DataAfterSignature,

    #[error("no identifier string provided")]
    NoIdentifier,

    #[error("no signing certificate")]
    NoSigningCertificate,

    #[error("signature data too large (please report this issue)")]
    SignatureDataTooLarge,

    #[error("HTTP error: {0}")]
    Reqwest(#[from] reqwest::Error),

    #[error("unknown digest algorithm")]
    DigestUnknownAlgorithm,

    #[error("unsupported digest algorithm")]
    DigestUnsupportedAlgorithm,

    #[error("unspecified digest error")]
    DigestUnspecified,

    #[error("error interfacing with directory-based bundle: {0}")]
    DirectoryBundle(anyhow::Error),

    #[error("nested bundle does not exist: {0}")]
    BundleUnknown(String),

    #[error("bundle Info.plist does not define CFBundleIdentifier: {0}")]
    BundleNoIdentifier(PathBuf),

    #[error("bundle Info.plist does not define CFBundleExecutable: {0}")]
    BundleNoMainExecutable(PathBuf),

    #[error("unable to parse settings scope: {0}")]
    ParseSettingsScope(String),

    #[error("incorrect password given when decrypting PFX data")]
    PfxBadPassword,

    #[error("error parsing PFX data: {0}")]
    PfxParseError(String),

    #[cfg(target_os = "macos")]
    #[error("SecurityFramework error: {0}")]
    SecurityFramework(#[from] security_framework::base::Error),

    #[error("error interfacing with macOS keychain: {0}")]
    KeychainError(String),

    #[error("failed to find certificate satisfying requirements: {0}")]
    CertificateNotFound(String),
}
