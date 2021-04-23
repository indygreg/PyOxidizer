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
    cryptographic_message_syntax::{Certificate, CmsError, SigningKey},
    reqwest::{IntoUrl, Url},
    std::{convert::TryFrom, path::Path},
    thiserror::Error,
    tugger_apple_codesign::{AppleCodesignError, MachOSigner},
    tugger_windows_codesign::SystemStore,
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

    #[error("cryptography error: {0}")]
    Cms(#[from] CmsError),

    #[error("no certificate data was found")]
    NoCertificateData,

    #[error("incorrect decryption password")]
    BadDecryptionPassword,

    #[error("PFX reading error: {0}")]
    PfxRead(String),

    #[error("{0}")]
    BadWindowsCertificateStore(String),

    #[error("bad URL: {0}")]
    BadUrl(reqwest::Error),

    #[error("macOS keychain integration only supported on macOS")]
    MacOsKeychainNotSupported,

    #[error("failed to resolve signing certificate: {0}")]
    CertificateResolutionFailure(String),

    #[error("error resolving certificate chain: {0}")]
    MacOsCertificateChainResolveFailure(AppleCodesignError),
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
#[derive(Debug)]
pub enum SigningCertificate {
    /// A parsed certificate and signing key stored in memory.
    ///
    /// The private key is managed by the `ring` crate.
    Memory((Certificate, SigningKey)),

    /// Use an automatically chosen certificate in the Windows certificate store.
    WindowsStoreAuto,

    /// A certificate stored in a Windows certificate store with a subject name string.
    ///
    /// See [SystemStore] for the possible system stores. [SystemStore::My] (the
    /// current user's store) is typically where code signing certificates are
    /// located.
    ///
    /// The string defines a value to match against in the certificate's `subject`
    /// field to locate the certificate.
    WindowsStoreSubject((SystemStore, String)),
}

impl SigningCertificate {
    /// Obtain an instance by parsing PFX / PKCS #12 data.
    ///
    /// PFX data is commonly encountered in `.pfx` or `.p12` files, such as
    /// those created when exporting certificates from Apple's `Keychain Access`
    /// or Windows' `certmgr`.
    ///
    /// The contents of the file require a password to decrypt. However, if no
    /// password was provided to create the data, this password may be the
    /// empty string.
    pub fn from_pfx_data(data: &[u8], password: &str) -> Result<Self, SigningError> {
        let (cert, key) = tugger_apple_codesign::parse_pfx_data(data, password)
            .map_err(|e| SigningError::PfxRead(format!("{:?}", e)))?;

        Ok(Self::Memory((cert, key)))
    }

    /// Construct an instance referring to a named certificate in a Windows certificate store.
    ///
    /// `store` is the name of a Windows certificate store to open. See
    /// [SystemStore] for possible values. The `My` store (the store for the current
    /// user) is likely where code signing certificates live.
    ///
    /// `subject` is a string to match against the certificate's `subject` field
    /// to locate the certificate.
    pub fn windows_store_with_subject(
        store: &str,
        subject: impl ToString,
    ) -> Result<Self, SigningError> {
        let store =
            SystemStore::try_from(store).map_err(SigningError::BadWindowsCertificateStore)?;

        Ok(Self::WindowsStoreSubject((store, subject.to_string())))
    }
}

/// An entity for performing code signing.
///
/// This contains the [SigningCertificate] as well as other fields to control
/// how signing is performed.
#[derive(Debug)]
pub struct Signer {
    /// The signing certificate to use.
    signing_certificate: SigningCertificate,

    /// The certificates that signed the signing certificate.
    ///
    /// Ideally this contains the full certificate chain, leading to the
    /// root CA.
    certificate_chain: Vec<Certificate>,

    /// URL of Time-Stamp Protocol server to use.
    time_stamp_url: Option<Url>,
}

impl Signer {
    /// Construct a new instance given a [SigningCertificate].
    pub fn new(signing_certificate: SigningCertificate) -> Self {
        Self {
            signing_certificate,
            certificate_chain: vec![],
            time_stamp_url: None,
        }
    }

    /// Add an X.509 certificate to the certificate chain.
    ///
    /// When signing, it is common to include the chain of certificates
    /// that signed the signing certificate in the signature. This can
    /// facilitate with validation of the signature.
    ///
    /// This function can be called to register addition certificates
    /// into the signing chain.
    pub fn chain_certificate(&mut self, certificate: Certificate) {
        self.certificate_chain.push(certificate);
    }

    /// Add PEM encoded X.509 certificates to the certificate chain.
    ///
    /// This is like [Self::chain_certificate] except the certificate is specified as
    /// PEM encoded data. This is a human readable string like
    /// `-----BEGIN CERTIFICATE-----` and is a common method for encoding
    /// certificate data. The specified data can contain multiple certificates.
    pub fn chain_certificates_pem(&mut self, data: impl AsRef<[u8]>) -> Result<(), SigningError> {
        let certs = Certificate::from_pem_multiple(data)?;

        if certs.is_empty() {
            Err(SigningError::NoCertificateData)
        } else {
            self.certificate_chain.extend(certs);
            Ok(())
        }
    }

    /// Add multiple X.509 certificates to the certificate chain.
    ///
    /// See [Self::chain_certificate] for details.
    pub fn chain_certificates(&mut self, certificates: impl Iterator<Item = Certificate>) {
        self.certificate_chain.extend(certificates);
    }

    /// Chain X.509 certificates by searching for them in the macOS keychain.
    ///
    /// This function will access the macOS keychain and attempt to locate
    /// the certificates composing the signing chain of the currently configured
    /// signing certificate.
    ///
    /// This function only works when run on macOS.
    ///
    /// This function will error if the signing certificate wasn't self-signed
    /// and its issuer chain could not be resolved.
    #[cfg(target_os = "macos")]
    pub fn chain_certificates_macos_keychain(&mut self) -> Result<(), SigningError> {
        let cert: &Certificate = match &self.signing_certificate {
            SigningCertificate::Memory((cert, _)) => Ok(cert),
            _ => Err(SigningError::CertificateResolutionFailure(
                "can only operate on signing certificates loaded into memory".to_string(),
            )),
        }?;

        if cert.is_self_signed() {
            return Ok(());
        }

        let subject_rdn = cert.subject_dn()?;
        let user_id = subject_rdn
            .find_attribute_string(bcder::Oid(tugger_apple_codesign::OID_UID.as_ref().into()))
            .map_err(|e| {
                SigningError::CertificateResolutionFailure(format!(
                    "failed to decode UID field in signing certificate: {:?}",
                    e
                ))
            })?
            .ok_or_else(|| {
                SigningError::CertificateResolutionFailure(
                    "could not find UID in signing certificate".to_string(),
                )
            })?;

        let domain = tugger_apple_codesign::KeychainDomain::User;

        let certs =
            tugger_apple_codesign::macos_keychain_find_certificate_chain(domain, None, &user_id)
                .map_err(SigningError::MacOsCertificateChainResolveFailure)?;

        if certs.is_empty() {
            return Err(SigningError::CertificateResolutionFailure(
                "issuing certificates not found in macOS keychain".to_string(),
            ));
        }

        if !certs[certs.len() - 1].is_self_signed() {
            return Err(SigningError::CertificateResolutionFailure(
                "unable to resolve entire signing certificate chain; root certificate not found"
                    .to_string(),
            ));
        }

        self.certificate_chain.extend(certs);
        Ok(())
    }

    /// Chain X.509 certificates by searching for them in the macOS keychain.
    ///
    /// This function will access the macOS keychain and attempt to locate
    /// the certificates composing the signing chain of the currently configured
    /// signing certificate.
    ///
    /// This function only works when run on macOS.
    ///
    /// This function will error if the signing certificate wasn't self-signed
    /// and its issuer chain could not be resolved.
    #[cfg(not(target_os = "macos"))]
    pub fn chain_certificates_macos_keychain(&self) -> Result<(), SigningError> {
        Err(SigningError::MacOsKeychainNotSupported)
    }

    /// Set the URL of a Time-Stamp Protocol server to use.
    ///
    /// If specified, the server will always be used. In some cases, a
    /// Time-Stamp Protocol server will be used automatically if one is
    /// not specified.
    pub fn time_stamp_url(&mut self, url: impl IntoUrl) -> Result<(), SigningError> {
        let url = url.into_url().map_err(SigningError::BadUrl)?;
        self.time_stamp_url = Some(url);
        Ok(())
    }

    /// Compute signability of a given filesystem path.
    pub fn path_signability(&self, path: impl AsRef<Path>) -> Result<Signability, SigningError> {
        path_signable(path)
    }

    /// Compute signability of a given slice of data.
    pub fn data_signability(&self, data: impl AsRef<[u8]>) -> Result<Signability, SigningError> {
        data_signable(data.as_ref())
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

    #[test]
    fn windows_store_with_subject() {
        let cert = SigningCertificate::windows_store_with_subject("my", "test user").unwrap();
        assert!(matches!(cert, SigningCertificate::WindowsStoreSubject(_)));
    }
}
