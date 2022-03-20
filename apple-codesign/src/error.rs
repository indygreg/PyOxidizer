// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    cryptographic_message_syntax::CmsError,
    std::path::PathBuf,
    thiserror::Error,
    tugger_apple::UniversalMachOError,
    x509_certificate::{KeyAlgorithm, X509CertificateError},
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

    #[error("invalid Mach-O binary: {0}")]
    InvalidBinary(String),

    #[error("binary does not have code signature data")]
    BinaryNoCodeSignature,

    #[error("X.509 certificate handler error: {0}")]
    X509(#[from] X509CertificateError),

    #[error("CMS error: {0}")]
    Cms(#[from] CmsError),

    #[error("JSON serialization error: {0}")]
    SerdeJson(#[from] serde_json::Error),

    #[error("problems reported during verification")]
    VerificationProblems,

    #[error("certificate decode error: {0}")]
    CertificateDecode(bcder::decode::Error),

    #[error("PEM error: {0}")]
    CertificatePem(pem::PemError),

    #[error("X.509 certificate parsing error: {0}")]
    X509Parse(String),

    #[error("unsupported key algorithm in certificate: {0:?}")]
    CertificateUnsupportedKeyAlgorithm(KeyAlgorithm),

    #[error("unspecified cryptography error in certificate")]
    CertificateRing(ring::error::Unspecified),

    #[error("bad string value in certificate: {0:?}")]
    CertificateCharset(bcder::string::CharSetError),

    #[error("error parsing version string: {0}")]
    VersionParse(#[from] semver::Error),

    #[error("unable to locate __TEXT segment")]
    MissingText,

    #[error("unable to locate __LINKEDIT segment")]
    MissingLinkedit,

    #[error("bad header magic in {0}")]
    BadMagic(&'static str),

    #[error("data structure parse error: {0}")]
    Scroll(#[from] scroll::Error),

    #[error("error parsing plist XML: {0}")]
    PlistParseXml(plist::Error),

    #[error("error serializing plist to XML: {0}")]
    PlistSerializeXml(plist::Error),

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

    #[error("error when encoding entitlements to DER: {0}")]
    EntitlementsDerEncode(String),

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

    #[error("insufficient room to write code signature load command")]
    LoadCommandNoRoom,

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

    #[error("the given OID does not match a recognized Apple certificate authority extension")]
    OidIsntCertificateAuthority,

    #[error("the given OID does not match a recognized Apple extended key usage extension")]
    OidIsntExtendedKeyUsage,

    #[error("the given OID does not match a recognized Apple code signing extension")]
    OidIsntCodeSigningExtension,

    #[error("error building certificate: {0}")]
    CertificateBuildError(String),

    #[error("unknown certificate profile: {0}")]
    UnknownCertificateProfile(String),

    #[error("unknown code execution policy: {0}")]
    UnknownPolicy(String),

    #[error("unable to generate code requirement policy: {0}")]
    PolicyFormulationError(String),

    #[error("error producing universal Mach-O binary: {0}")]
    UniversalMachO(#[from] UniversalMachOError),

    #[error("notarization record not in response: {0}")]
    NotarizationRecordNotInResponse(String),

    #[error("signed ticket data not found in ticket lookup response (this should not happen)")]
    NotarizationRecordNoSignedTicket,

    #[error("signedTicket in notarization ticket lookup response is not BYTES: {0}")]
    NotarizationRecordSignedTicketNotBytes(String),

    #[error("notarization ticket lookup failure: {0}: {1}")]
    NotarizationLookupFailure(String, String),

    #[error("error decoding base64 in notarization ticket: {0}")]
    NotarizationRecordDecodeFailure(base64::DecodeError),

    #[error("do not support stapling {0:?} bundles")]
    StapleUnsupportedBundleType(apple_bundles::BundlePackageType),

    #[error("failed to find main executable in bundle")]
    StapleMainExecutableNotFound,

    #[error("do not know how to staple {0}")]
    StapleUnsupportedPath(PathBuf),
}
