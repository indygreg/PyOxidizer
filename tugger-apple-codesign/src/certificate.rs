// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Functionality related to certificates.

use {
    crate::error::AppleCodesignError,
    bcder::{
        encode::{PrimitiveContent, Values},
        ConstOid, Mode, Oid,
    },
    bytes::Bytes,
    x509_certificate::{
        CapturedX509Certificate, InMemorySigningKeyPair, KeyAlgorithm, X509CertificateBuilder,
    },
};

/// Key Usage extension.
///
/// 2.5.29.15
const OID_EXTENSION_KEY_USAGE: ConstOid = Oid(&[85, 29, 15]);

/// Extended Key Usage extension.
///
/// 2.5.29.37
const OID_EXTENSION_EXTENDED_KEY_USAGE: ConstOid = Oid(&[85, 29, 37]);

/// Extended Key Usage purpose for code signing.
///
/// 1.3.6.1.5.5.7.3.3
const OID_EKU_PURPOSE_CODE_SIGNING: ConstOid = Oid(&[43, 6, 1, 5, 5, 7, 3, 3]);

/// Extended Key Usage for purpose of `Safari Developer`.
///
/// 1.2.840.113635.100.4.8
const OID_EKU_PURPOSE_SAFARI_DEVELOPER: ConstOid = Oid(&[42, 134, 72, 134, 247, 99, 100, 4, 8]);

/// Extended Key Usage for purpose of `3rd Party Mac Developer Installer`.
///
/// 1.2.840.113635.100.4.9
const OID_EKU_PURPOSE_3RD_PARTY_MAC_DEVELOPER_INSTALLER: ConstOid =
    Oid(&[42, 134, 72, 134, 247, 99, 100, 4, 9]);

/// Extended Key Usage for purpose of `Developer ID Installer`.
///
/// 1.2.840.113635.100.4.13
const OID_EKU_PURPOSE_DEVELOPER_ID_INSTALLER: ConstOid =
    Oid(&[42, 134, 72, 134, 247, 99, 100, 4, 13]);

/// Extension for `Apple Signing`.
///
/// 1.2.840.113635.100.6.1.1
const OID_EXTENSION_APPLE_SIGNING: ConstOid = Oid(&[42, 134, 72, 134, 247, 99, 100, 6, 1, 1]);

/// Extension for `iPhone Developer`.
///
/// 1.2.840.113635.100.6.1.2
const OID_EXTENSION_IPHONE_DEVELOPER: ConstOid = Oid(&[42, 134, 72, 134, 247, 99, 100, 6, 1, 2]);

/// Extension for `Apple iPhone OS Application Signing`
///
/// 1.2.840.113635.100.6.1.3
const OID_EXTENSION_IPHONE_OS_APPLICATION_SIGNING: ConstOid =
    Oid(&[42, 134, 72, 134, 247, 99, 100, 6, 1, 3]);

/// Extension for `Apple Developer Certificate (Submission)`.
///
/// May also be referred to as `iPhone Distribution`.
///
/// 1.2.840.113635.100.6.1.4
const OID_EXTENSION_APPLE_DEVELOPER_CERTIFICATE_SUBMISSION: ConstOid =
    Oid(&[42, 134, 72, 134, 247, 99, 100, 6, 1, 4]);

/// Extension for `Safari Developer`.
///
/// 1.2.840.113635.100.6.1.5
const OID_EXTENSION_SAFARI_DEVELOPER: ConstOid = Oid(&[42, 134, 72, 134, 247, 99, 100, 6, 1, 5]);

/// Extension for `Apple iPhone OS VPN Signing`
///
/// 1.2.840.113635.100.6.1.6
const OID_EXTENSION_IPHONE_OS_VPN_SIGNING: ConstOid =
    Oid(&[42, 134, 72, 134, 247, 99, 100, 6, 1, 6]);

/// Extension for `Apple Mac App Signing (Development)`.
///
/// May also appear as `3rd Party Mac Developer Application`.
///
/// 1.2.840.113635.100.6.1.7
const OID_EXTENSION_APPLE_MAC_APP_SIGNING_DEVELOPMENT: ConstOid =
    Oid(&[42, 134, 72, 134, 247, 99, 100, 6, 1, 7]);

/// Extension for `Apple Mac App Signing Submission`.
///
/// 1.2.840.113635.100.6.1.8
const OID_EXTENSION_APPLE_MAC_APP_SIGNING_SUBMISSION: ConstOid =
    Oid(&[42, 134, 72, 134, 247, 99, 100, 6, 1, 8]);

/// Extension for `Mac App Store Code Signing`.
///
/// 1.2.840.113635.100.6.1.9
const OID_EXTENSION_APPLE_MAC_APP_STORE_CODE_SIGNING: ConstOid =
    Oid(&[42, 134, 72, 134, 247, 99, 100, 6, 1, 9]);

/// Extension for `Mac App Store Installer Signing`.
///
/// 1.2.840.113635.100.6.1.10
const OID_EXTENSION_APPLE_MAC_APP_STORE_INSTALLER_SIGNING: ConstOid =
    Oid(&[42, 134, 72, 134, 247, 99, 100, 6, 1, 10]);

// 1.2.840.113635.100.6.1.11 is unknown.

/// Extension for `Mac Developer`.
///
/// 1.2.840.113635.100.6.1.12
const OID_EXTENSION_MAC_DEVELOPER: ConstOid = Oid(&[42, 134, 72, 134, 247, 99, 100, 6, 1, 12]);

/// Extension for `Developer ID Application`.
///
/// 1.2.840.113635.100.6.1.13
const OID_EXTENSION_DEVELOPER_ID_APPLICATION: ConstOid =
    Oid(&[42, 134, 72, 134, 247, 99, 100, 6, 1, 13]);

/// Extension for `Developer ID Installer`.
///
/// 1.2.840.113635.100.6.1.14
const OID_EXTENSION_DEVELOPER_ID_INSTALLER: ConstOid =
    Oid(&[42, 134, 72, 134, 247, 99, 100, 6, 1, 14]);

// 1.2.840.113635.100.6.1.15 looks to have something to do with core OS functionality,
// as it appears in search results for hacking Apple OS booting.

/// Extension for `Apple Pay Passbook Signing`
///
/// 1.2.840.113635.100.6.1.16
const OID_EXTENSION_PASSBOOK_SIGNING: ConstOid = Oid(&[42, 134, 72, 134, 247, 99, 100, 6, 1, 16]);

/// Extension for `Web Site Push Notifications Signing`
///
/// 1.2.840.113635.100.6.1.17
const OID_EXTENSION_WEBSITE_PUSH_NOTIFICATION_SIGNING: ConstOid =
    Oid(&[42, 134, 72, 134, 247, 99, 100, 6, 1, 17]);

/// Extension for `Developer ID Kernel`.
///
/// 1.2.840.113635.100.6.1.18
const OID_EXTENSION_DEVELOPER_ID_KERNEL: ConstOid =
    Oid(&[42, 134, 72, 134, 247, 99, 100, 6, 1, 18]);

/// Extension for `Developer ID Date`.
///
/// This OID doesn't have a description in Apple tooling. But it
/// holds a UtcDate (with hours, minutes, and seconds all set to 0) and seems to
/// denote a date constraint to apply to validation. This is likely used
/// to validating timestamping constrains for certificate validity.
///
/// 1.2.840.113635.100.6.1.33
const OID_EXTENSION_DEVELOPER_ID_DATE: ConstOid = Oid(&[42, 134, 72, 134, 247, 99, 100, 6, 1, 33]);

/// Extension for `TestFlight`.
///
/// 1.2.840.113635.100.6.1.25.1
const OID_EXTENSION_TEST_FLIGHT: ConstOid = Oid(&[42, 134, 72, 134, 247, 99, 100, 6, 1, 25, 1]);

/// OID used for email address in RDN in Apple generated code signing certificates.
const OID_EMAIL_ADDRESS: ConstOid = Oid(&[42, 134, 72, 134, 247, 13, 1, 9, 1]);

/// Apple Worldwide Developer Relations.
///
/// 1.2.840.113635.100.6.2.1
const OID_CA_EXTENSION_APPLE_WORLDWIDE_DEVELOPER_RELATIONS: ConstOid =
    Oid(&[42, 134, 72, 134, 247, 99, 100, 6, 2, 1]);

/// Apple Application Integration.
///
/// 1.2.840.113635.100.6.2.3
const OID_CA_EXTENSION_APPLE_APPLICATION_INTEGRATION: ConstOid =
    Oid(&[42, 134, 72, 134, 247, 99, 100, 6, 2, 3]);

/// Developer ID Certification Authority
///
/// 1.2.840.113635.100.6.2.6
const OID_CA_EXTENSION_DEVELOPER_ID: ConstOid = Oid(&[42, 134, 72, 134, 247, 99, 100, 6, 2, 6]);

/// Apple Timestamp.
///
/// 1.2.840.113635.100.6.2.9
const OID_CA_EXTENSION_APPLE_TIMESTAMP: ConstOid = Oid(&[42, 134, 72, 134, 247, 99, 100, 6, 2, 9]);

/// Developer Authentication Certification Authority.
///
/// 1.2.840.113635.100.6.2.11
const OID_CA_EXTENSION_DEVELOPER_AUTHENTICATION: ConstOid =
    Oid(&[42, 134, 72, 134, 247, 99, 100, 6, 2, 11]);

/// Apple Worldwide Developer Relations CA - G2
///
/// 1.2.840.113635.100.6.2.15
const OID_CA_EXTENSION_APPLE_WORLDWIDE_DEVELOPER_RELATIONS_G2: ConstOid =
    Oid(&[42, 134, 72, 134, 247, 99, 100, 6, 2, 15]);

/// Apple Software Update Certification.
///
/// 1.2.840.113635.100.6.2.19
const OID_CA_EXTENSION_APPLE_SOFTWARE_UPDATE_CERTIFICATION: ConstOid =
    Oid(&[42, 134, 72, 134, 247, 99, 100, 6, 2, 19]);

const ALL_OID_CA_EXTENSIONS: &[&ConstOid; 7] = &[
    &OID_CA_EXTENSION_APPLE_WORLDWIDE_DEVELOPER_RELATIONS,
    &OID_CA_EXTENSION_APPLE_APPLICATION_INTEGRATION,
    &OID_CA_EXTENSION_DEVELOPER_ID,
    &OID_CA_EXTENSION_APPLE_TIMESTAMP,
    &OID_CA_EXTENSION_DEVELOPER_AUTHENTICATION,
    &OID_CA_EXTENSION_APPLE_WORLDWIDE_DEVELOPER_RELATIONS_G2,
    &OID_CA_EXTENSION_APPLE_SOFTWARE_UPDATE_CERTIFICATION,
];

/// Describes the type of code signing that a certificate is authorized to perform.
///
/// Code signing certificates are issued with extended key usage (EKU) attributes
/// denoting what that certificate will be used for. They basically say *I'm authorized
/// to sign X*.
///
/// This type describes the different code signing key usages defined on Apple
/// platforms.
pub enum ExtendedKeyUsagePurpose {
    /// Code signing.
    CodeSigning,

    /// Safari Developer.
    SafariDeveloper,

    /// 3rd Party Mac Developer Installer Packaging Signing.
    ///
    /// The certificate can be used to sign Mac installer packages.
    ThirdPartyMacDeveloperInstaller,

    /// Developer ID Installer.
    DeveloperIdInstaller,
}

impl ExtendedKeyUsagePurpose {
    pub fn as_oid(&self) -> ConstOid {
        match self {
            Self::CodeSigning => OID_EKU_PURPOSE_CODE_SIGNING,
            Self::SafariDeveloper => OID_EKU_PURPOSE_SAFARI_DEVELOPER,
            Self::ThirdPartyMacDeveloperInstaller => {
                OID_EKU_PURPOSE_3RD_PARTY_MAC_DEVELOPER_INSTALLER
            }
            Self::DeveloperIdInstaller => OID_EKU_PURPOSE_DEVELOPER_ID_INSTALLER,
        }
    }
}

/// Describes one of the many X.509 certificate extensions found on Apple code signing certificates.
pub enum CodeSigningCertificateExtension {
    /// Apple Signing.
    ///
    /// (Appears to be deprecated).
    AppleSigning,

    /// iPhone Developer.
    IPhoneDeveloper,

    /// Apple iPhone OS Application Signing.
    IPhoneOsApplicationSigning,

    /// Apple Developer Certificate (Submission).
    ///
    /// May also be referred to as `iPhone Distribution`.
    AppleDeveloperCertificateSubmission,

    /// Safari Developer.
    SafariDeveloper,

    /// Apple iPhone OS VPN Signing.
    IPhoneOsVpnSigning,

    /// Apple Mac App Signing (Development).
    ///
    /// Also known as `3rd Party Mac Developer Application`.
    AppleMacAppSigningDevelopment,

    /// Apple Mac App Signing Submission.
    AppleMacAppSigningSubmission,

    /// Mac App Store Code Signing.
    AppleMacAppStoreCodeSigning,

    /// Mac App Store Installer Signing.
    AppleMacAppStoreInstallerSigning,

    /// Mac Developer.
    MacDeveloper,

    /// Developer ID Application.
    DeveloperIdApplication,

    /// Developer ID Date.
    DeveloperIdDate,

    /// Developer ID Installer.
    DeveloperIdInstaller,

    /// Apple Pay Passbook Signing.
    ApplePayPassbookSigning,

    /// Web Site Push Notifications Signing.
    WebsitePushNotificationSigning,

    /// Developer ID Kernel.
    DeveloperIdKernel,

    /// TestFlight.
    TestFlight,
}

impl CodeSigningCertificateExtension {
    pub fn as_oid(&self) -> ConstOid {
        match self {
            Self::AppleSigning => OID_EXTENSION_APPLE_SIGNING,
            Self::IPhoneDeveloper => OID_EXTENSION_IPHONE_DEVELOPER,
            Self::IPhoneOsApplicationSigning => OID_EXTENSION_IPHONE_OS_APPLICATION_SIGNING,
            Self::AppleDeveloperCertificateSubmission => {
                OID_EXTENSION_APPLE_DEVELOPER_CERTIFICATE_SUBMISSION
            }
            Self::SafariDeveloper => OID_EXTENSION_SAFARI_DEVELOPER,
            Self::IPhoneOsVpnSigning => OID_EXTENSION_IPHONE_OS_VPN_SIGNING,
            Self::AppleMacAppSigningDevelopment => OID_EXTENSION_APPLE_MAC_APP_SIGNING_DEVELOPMENT,
            Self::AppleMacAppSigningSubmission => OID_EXTENSION_APPLE_MAC_APP_SIGNING_SUBMISSION,
            Self::AppleMacAppStoreCodeSigning => OID_EXTENSION_APPLE_MAC_APP_STORE_CODE_SIGNING,
            Self::AppleMacAppStoreInstallerSigning => {
                OID_EXTENSION_APPLE_MAC_APP_STORE_INSTALLER_SIGNING
            }
            Self::MacDeveloper => OID_EXTENSION_MAC_DEVELOPER,
            Self::DeveloperIdApplication => OID_EXTENSION_DEVELOPER_ID_APPLICATION,
            Self::DeveloperIdDate => OID_EXTENSION_DEVELOPER_ID_DATE,
            Self::DeveloperIdInstaller => OID_EXTENSION_DEVELOPER_ID_INSTALLER,
            Self::ApplePayPassbookSigning => OID_EXTENSION_PASSBOOK_SIGNING,
            Self::WebsitePushNotificationSigning => OID_EXTENSION_WEBSITE_PUSH_NOTIFICATION_SIGNING,
            Self::DeveloperIdKernel => OID_EXTENSION_DEVELOPER_ID_KERNEL,
            Self::TestFlight => OID_EXTENSION_TEST_FLIGHT,
        }
    }
}

/// Describes combinations of key extensions for Apple code signing certificates.
///
/// Code signing certificates contain various X.509 extensions denoting them for
/// code signing.
///
/// This type represents various common extensions as used on Apple platforms.
///
/// Typically, you'll want to apply at most one of these extensions to a
/// new certificate in order to mark it as compatible for code signing.
pub enum KeyExtensions {
    /// Mac Installer Distribution.
    ///
    /// In `Keychain Access.app`, this might render as `3rd Party Mac Developer Installer`.
    ///
    /// Certificates are marked for EKU with `3rd Party Developer Installer Package
    /// Signing`.
    ///
    /// They also have the `Apple Mac App Signing (Submission)` extension.
    ///
    /// Typically issued by `Apple Worldwide Developer Relations Certificate
    /// Authority`.
    MacInstallerDistribution,

    /// Apple Distribution.
    ///
    /// Certificates are marked for EKU with `Code Signing`. They also have
    /// extensions `Apple Mac App Signing (Development)` and
    /// `Apple Developer Certificate (Submission)`.
    ///
    /// Typically issued by `Apple Worldwide Developer Relations Certificate Authority`.
    AppleDistribution,

    /// Apple Development.
    ///
    /// Certificates are marked for EKU with `Code Signing`. They also have
    /// extensions `Apple Developer Certificate (Development)` and
    /// `Mac Developer`.
    ///
    /// Typically issued by `Apple Worldwide Developer Relations Certificate
    /// Authority`.
    AppleDevelopment,

    /// Developer ID Application.
    ///
    /// Certificates are marked for EKU with `Code Signing`. They also have
    /// extensions for `Developer ID Application` and `Developer ID Date`.
    DeveloperIdApplication,

    /// Developer ID Installer.
    ///
    /// Certificates are marked for EKU with `Developer ID Application`. They also
    /// have extensions `Developer ID Installer` and `Developer ID Date`.
    DeveloperIdInstaller,
}

/// Denotes specific certificate extensions on Apple certificate authority certificates.
///
/// Apple's CA certificates have extensions that appear to identify the role of
/// that CA. This enumeration defines those.
pub enum CertificateAuthorityExtension {
    /// Apple Worldwide Developer Relations.
    ///
    /// An intermediate CA.
    AppleWorldwideDeveloperRelations,

    /// Apple Application Integration.
    AppleApplicationIntegration,

    /// Developer ID Certification Authority.
    DeveloperId,

    /// Apple Timestamp.
    AppleTimestamp,

    /// Developer Authentication Certification Authority.
    DeveloperAuthentication,

    /// Apple Worldwide Developer Relations CA - G2.
    AppleWorldwideDeveloperRelationsG2,

    /// Apple Software Update Certification.
    AppleSoftwareUpdateCertification,
}

impl CertificateAuthorityExtension {
    /// All the known OIDs constituting Apple CA extensions.
    pub fn all_oids(&self) -> &[&ConstOid] {
        ALL_OID_CA_EXTENSIONS
    }

    pub fn as_oid(&self) -> ConstOid {
        match self {
            Self::AppleWorldwideDeveloperRelations => {
                OID_CA_EXTENSION_APPLE_WORLDWIDE_DEVELOPER_RELATIONS
            }
            Self::AppleApplicationIntegration => OID_CA_EXTENSION_APPLE_APPLICATION_INTEGRATION,
            Self::DeveloperId => OID_CA_EXTENSION_DEVELOPER_ID,
            Self::AppleTimestamp => OID_CA_EXTENSION_APPLE_TIMESTAMP,
            Self::DeveloperAuthentication => OID_CA_EXTENSION_DEVELOPER_AUTHENTICATION,
            Self::AppleWorldwideDeveloperRelationsG2 => {
                OID_CA_EXTENSION_APPLE_WORLDWIDE_DEVELOPER_RELATIONS_G2
            }
            Self::AppleSoftwareUpdateCertification => {
                OID_CA_EXTENSION_APPLE_SOFTWARE_UPDATE_CERTIFICATION
            }
        }
    }
}

/// Extends functionality of [CapturedX509Certificate] with Apple specific certificate knowledge.
pub trait AppleCertificate: Sized {}

impl AppleCertificate for CapturedX509Certificate {}

fn bmp_string(s: &str) -> Vec<u8> {
    let utf16: Vec<u16> = s.encode_utf16().collect();

    let mut bytes = Vec::with_capacity(utf16.len() * 2 + 2);
    for c in utf16 {
        bytes.push((c / 256) as u8);
        bytes.push((c % 256) as u8);
    }
    bytes.push(0x00);
    bytes.push(0x00);

    bytes
}

/// Parse PFX data into a key pair.
///
/// PFX data is commonly encountered in `.p12` files, such as those created
/// when exporting certificates from Apple's `Keychain Access` application.
///
/// The contents of the PFX file require a password to decrypt. However, if
/// no password was provided to create the PFX data, this password may be the
/// empty string.
pub fn parse_pfx_data(
    data: &[u8],
    password: &str,
) -> Result<(CapturedX509Certificate, InMemorySigningKeyPair), AppleCodesignError> {
    let pfx = p12::PFX::parse(data).map_err(|e| {
        AppleCodesignError::PfxParseError(format!("data does not appear to be PFX: {:?}", e))
    })?;

    if !pfx.verify_mac(password) {
        return Err(AppleCodesignError::PfxBadPassword);
    }

    // Apple's certificate export format consists of regular data content info
    // with inner ContentInfo components holding the key and certificate.
    let data = match pfx.auth_safe {
        p12::ContentInfo::Data(data) => data,
        _ => {
            return Err(AppleCodesignError::PfxParseError(
                "unexpected PFX content info".to_string(),
            ));
        }
    };

    let content_infos = yasna::parse_der(&data, |reader| {
        reader.collect_sequence_of(p12::ContentInfo::parse)
    })
    .map_err(|e| {
        AppleCodesignError::PfxParseError(format!("failed parsing inner ContentInfo: {:?}", e))
    })?;

    let bmp_password = bmp_string(password);

    let mut certificate = None;
    let mut signing_key = None;

    for content in content_infos {
        let bags_data = match content {
            p12::ContentInfo::Data(inner) => inner,
            p12::ContentInfo::EncryptedData(encrypted) => {
                encrypted.data(&bmp_password).ok_or_else(|| {
                    AppleCodesignError::PfxParseError(
                        "failed decrypting inner EncryptedData".to_string(),
                    )
                })?
            }
            p12::ContentInfo::OtherContext(_) => {
                return Err(AppleCodesignError::PfxParseError(
                    "unexpected OtherContent content in inner PFX data".to_string(),
                ));
            }
        };

        let bags = yasna::parse_ber(&bags_data, |reader| {
            reader.collect_sequence_of(p12::SafeBag::parse)
        })
        .map_err(|e| {
            AppleCodesignError::PfxParseError(format!(
                "failed parsing SafeBag within inner Data: {:?}",
                e
            ))
        })?;

        for bag in bags {
            match bag.bag {
                p12::SafeBagKind::CertBag(cert_bag) => match cert_bag {
                    p12::CertBag::X509(cert_data) => {
                        certificate = Some(CapturedX509Certificate::from_der(cert_data)?);
                    }
                    p12::CertBag::SDSI(_) => {
                        return Err(AppleCodesignError::PfxParseError(
                            "unexpected SDSI certificate data".to_string(),
                        ));
                    }
                },
                p12::SafeBagKind::Pkcs8ShroudedKeyBag(key_bag) => {
                    let decrypted = key_bag.decrypt(&bmp_password).ok_or_else(|| {
                        AppleCodesignError::PfxParseError(
                            "error decrypting PKCS8 shrouded key bag; is the password correct?"
                                .to_string(),
                        )
                    })?;

                    signing_key = Some(InMemorySigningKeyPair::from_pkcs8_der(&decrypted)?);
                }
                p12::SafeBagKind::OtherBagKind(_) => {
                    return Err(AppleCodesignError::PfxParseError(
                        "unexpected bag type in inner PFX content".to_string(),
                    ));
                }
            }
        }
    }

    match (certificate, signing_key) {
        (Some(certificate), Some(signing_key)) => Ok((certificate, signing_key)),
        (None, Some(_)) => Err(AppleCodesignError::PfxParseError(
            "failed to find x509 certificate in PFX data".to_string(),
        )),
        (_, None) => Err(AppleCodesignError::PfxParseError(
            "failed to find signing key in PFX data".to_string(),
        )),
    }
}

/// Create a new self-signed X.509 certificate suitable for signing code.
///
/// The created certificate contains all the extensions needed to convey
/// that it is used for code signing and should resemble certificates.
///
/// However, because the certificate isn't signed by Apple or another
/// trusted certificate authority, binaries signed with the certificate
/// may not pass Apple's verification requirements and the OS may refuse
/// to proceed. Needless to say, only use certificates generated with this
/// function for testing purposes only.
pub fn create_self_signed_code_signing_certificate(
    algorithm: KeyAlgorithm,
    common_name: &str,
    country_name: &str,
    email_address: &str,
    validity_duration: chrono::Duration,
) -> Result<
    (
        CapturedX509Certificate,
        InMemorySigningKeyPair,
        ring::pkcs8::Document,
    ),
    AppleCodesignError,
> {
    let mut builder = X509CertificateBuilder::new(algorithm);

    builder
        .subject()
        .append_common_name_utf8_string(common_name)
        .map_err(AppleCodesignError::CertificateCharset)?;
    builder
        .subject()
        .append_country_utf8_string(country_name)
        .map_err(AppleCodesignError::CertificateCharset)?;
    builder
        .subject()
        .append_utf8_string(Oid(OID_EMAIL_ADDRESS.as_ref().into()), email_address)
        .map_err(AppleCodesignError::CertificateCharset)?;

    builder.validity_duration(validity_duration);

    // Digital Signature key usage extension.
    builder.add_extension_der_data(
        Oid(OID_EXTENSION_KEY_USAGE.as_ref().into()),
        true,
        &[3, 2, 7, 128],
    );

    let captured =
        bcder::encode::sequence(Oid(Bytes::from(OID_EKU_PURPOSE_CODE_SIGNING.as_ref())).encode())
            .to_captured(Mode::Der);

    builder.add_extension_der_data(
        Oid(OID_EXTENSION_EXTENDED_KEY_USAGE.as_ref().into()),
        true,
        captured.as_slice(),
    );

    Ok(builder.create_with_random_keypair()?)
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        cryptographic_message_syntax::{SignedData, SignedDataBuilder, SignerBuilder},
        x509_certificate::EcdsaCurve,
    };

    #[test]
    fn parse_keychain_p12_export() {
        let data = include_bytes!("apple-codesign-testuser.p12");

        let err = parse_pfx_data(data, "bad-password").unwrap_err();
        assert!(matches!(err, AppleCodesignError::PfxBadPassword));

        parse_pfx_data(data, "password123").unwrap();
    }

    #[test]
    fn generate_self_signed_certificate_ecdsa() {
        for curve in EcdsaCurve::all() {
            create_self_signed_code_signing_certificate(
                KeyAlgorithm::Ecdsa(*curve),
                "test",
                "US",
                "nobody@example.com",
                chrono::Duration::hours(1),
            )
            .unwrap();
        }
    }

    #[test]
    fn generate_self_signed_certificate_ed25519() {
        create_self_signed_code_signing_certificate(
            KeyAlgorithm::Ed25519,
            "test",
            "US",
            "nobody@example.com",
            chrono::Duration::hours(1),
        )
        .unwrap();
    }

    #[test]
    fn cms_self_signed_certificate_signing_ecdsa() {
        for curve in EcdsaCurve::all() {
            let (cert, signing_key, _) = create_self_signed_code_signing_certificate(
                KeyAlgorithm::Ecdsa(*curve),
                "test",
                "US",
                "nobody@example.com",
                chrono::Duration::hours(1),
            )
            .unwrap();

            let plaintext = "hello, world";

            let cms = SignedDataBuilder::default()
                .certificate(cert.clone())
                .signed_content(plaintext.as_bytes().to_vec())
                .signer(SignerBuilder::new(&signing_key, cert))
                .build_ber()
                .unwrap();

            let signed_data = SignedData::parse_ber(&cms).unwrap();

            for signer in signed_data.signers() {
                signer
                    .verify_signature_with_signed_data(&signed_data)
                    .unwrap();
            }
        }
    }

    #[test]
    fn cms_self_signed_certificate_signing_ed25519() {
        let (cert, signing_key, _) = create_self_signed_code_signing_certificate(
            KeyAlgorithm::Ed25519,
            "test",
            "US",
            "nobody@example.com",
            chrono::Duration::hours(1),
        )
        .unwrap();

        let plaintext = "hello, world";

        let cms = SignedDataBuilder::default()
            .certificate(cert.clone())
            .signed_content(plaintext.as_bytes().to_vec())
            .signer(SignerBuilder::new(&signing_key, cert))
            .build_ber()
            .unwrap();

        let signed_data = SignedData::parse_ber(&cms).unwrap();

        for signer in signed_data.signers() {
            signer
                .verify_signature_with_signed_data(&signed_data)
                .unwrap();
        }
    }
}
