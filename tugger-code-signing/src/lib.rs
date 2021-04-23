// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Cross-platform gateway to code signing.
//!
//! This crate implements functionality for performing code signing in
//! a platform-agnostic manner. It attempts to abstract over platform
//! differences so users don't care what platform they are running on or
//! what type of entity they are signing. It achieves varying levels
//! of success, depending on limitations of underlying crates.

use {
    cryptographic_message_syntax::{Certificate, SigningKey},
    std::path::Path,
    thiserror::Error,
    tugger_apple_codesign::{AppleCodesignError, MachOSigner},
    yasna::ASN1Error,
};

/// Represents a signing error.
#[derive(Debug, Error)]
pub enum SigningError {
    #[error("could not determine if path is signable: {0}")]
    SignableTestError(String),

    /// General I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("error reading ASN.1 data: {0}")]
    Asn1(#[from] ASN1Error),

    #[error("incorrect decryption password")]
    BadDecryptionPassword,

    #[error("PFX reading error: {0}")]
    PfxRead(String),
}

/// Represents the results of a signability test.
#[derive(Debug)]
pub enum Signability {
    /// The entity is a signable entity on Windows.
    SignableWindows,
    /// The entity is a signable Mach-O file.
    SignableMachO,
    /// The entity is a signable Apple bundle.
    SignableAppleBundle,
    /// The entity is not signable for undetermined reason.
    Unsignable,
    /// The entity is a Mach-O binary that cannot be signed.
    UnsignableMachoError(AppleCodesignError),
    /// The entity is signable, but not from this platform. Details of the
    /// limitation are stored in a string.
    PlatformUnsupported(&'static str),
}

impl Signability {
    /// Whether we are capable of signing.
    pub fn is_signable(&self) -> bool {
        match self {
            Self::SignableWindows | Self::SignableMachO | Self::SignableAppleBundle => true,
            Self::Unsignable | Self::PlatformUnsupported(_) | Self::UnsignableMachoError(_) => {
                false
            }
        }
    }
}

/// Resolve signability information given an input path.
///
/// The path can be to a file or directory.
///
/// Returns `Err` if we could not fully test the path. This includes
/// I/O failures.
pub fn path_signable(path: impl AsRef<Path>) -> Result<Signability, SigningError> {
    let path = path.as_ref();

    match tugger_windows_codesign::is_file_signable(path) {
        Ok(true) => {
            // But we can only sign Windows binaries on Windows since we call out to
            // signtool.exe.
            return if cfg!(target_family = "windows") {
                Ok(Signability::SignableWindows)
            } else {
                Ok(Signability::PlatformUnsupported(
                    "Windows signing requires running on Windows",
                ))
            };
        }
        Ok(false) => {}
        Err(e) => return Err(SigningError::SignableTestError(format!("{:?}", e))),
    }

    // Apple is a bit more complicated. If the path is a file, we test for Mach-O.
    // If a directory, we test for a bundle.

    if path.is_file() {
        let data = std::fs::read(path)?;

        if goblin::mach::Mach::parse(&data).is_ok() {
            // Try to construct a signer to see if the binary is compatible.
            return Ok(match MachOSigner::new(&data) {
                Ok(_) => Signability::SignableMachO,
                Err(e) => Signability::UnsignableMachoError(e),
            });
        }
    } else if path.is_dir() && tugger_apple_bundle::DirectoryBundle::new_from_path(path).is_ok() {
        return Ok(Signability::SignableAppleBundle);
    }

    Ok(Signability::Unsignable)
}

/// Resolve signability information given a data slice.
pub fn data_signable(data: &[u8]) -> Result<Signability, SigningError> {
    if tugger_windows_codesign::is_signable_binary_header(data) {
        // But we can only sign Windows binaries on Windows since we call out to
        // signtool.exe.
        return if cfg!(target_family = "windows") {
            Ok(Signability::SignableWindows)
        } else {
            Ok(Signability::PlatformUnsupported(
                "Windows signing requires running on Windows",
            ))
        };
    }

    if goblin::mach::Mach::parse(&data).is_ok() {
        // Try to construct a signer to see if the binary is compatible.
        return Ok(match MachOSigner::new(&data) {
            Ok(_) => Signability::SignableMachO,
            Err(e) => Signability::UnsignableMachoError(e),
        });
    }

    Ok(Signability::Unsignable)
}

/// Represents a signing key and public certificate to sign something.
pub enum SigningCertificate {
    /// A parsed certificate and signing key stored in memory.
    ///
    /// The private key is managed by the `ring` crate.
    Memory((Certificate, SigningKey)),
}

impl SigningCertificate {
    /// Obtain an instance by parsing PFX data.
    ///
    /// PFX data is commonly encountered in `.p12` files, such as those
    /// created when exporting certificates from Apple's `Keychain Access`
    /// application.
    ///
    /// The contents of the PFX file require a password to decrypt. However,
    /// if no password was provided to create the data, this password
    /// may be the empty string.
    pub fn from_pfx_data(data: &[u8], password: &str) -> Result<Self, SigningError> {
        let (cert, key) = tugger_apple_codesign::parse_pfx_data(data, password)
            .map_err(|e| SigningError::PfxRead(format!("{:?}", e)))?;

        Ok(Self::Memory((cert, key)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const APPLE_P12_DATA: &[u8] =
        include_bytes!("../../tugger-apple-codesign/src/apple-codesign-testuser.p12");

    const WINDOWS_PFX_DEFAULT_DATA: &[u8] = include_bytes!("windows-testuser-default.pfx");
    const WINDOWS_PFX_NO_EXTRAS_DATA: &[u8] = include_bytes!("windows-testuser-no-extras.pfx");

    #[test]
    fn parse_apple_p12() {
        SigningCertificate::from_pfx_data(APPLE_P12_DATA, "password123").unwrap();
    }

    #[test]
    fn parse_windows_pfx() {
        SigningCertificate::from_pfx_data(WINDOWS_PFX_DEFAULT_DATA, "password123").unwrap();
        SigningCertificate::from_pfx_data(WINDOWS_PFX_NO_EXTRAS_DATA, "password123").unwrap();
    }

    #[test]
    fn parse_windows_pfx_dynamic() {
        let cert =
            tugger_windows_codesign::create_self_signed_code_signing_certificate("test user")
                .unwrap();
        let pfx_data =
            tugger_windows_codesign::certificate_to_pfx(&cert, "password", "name").unwrap();

        SigningCertificate::from_pfx_data(&pfx_data, "password").unwrap();
    }
}
