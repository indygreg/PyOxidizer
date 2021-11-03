// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Apple X.509 certificates.
//!
//! This module defines well-known Apple X.509 certificates.
//!
//! The canonical source of this data is <https://www.apple.com/certificateauthority/>.
//!
//! Note that some certificates are commented out and not available
//! because the official DER-encoded certificates provided by Apple
//! do not conform to the encoding standards in RFC 5280.

use {once_cell::sync::Lazy, std::ops::Deref, x509_certificate::CapturedX509Certificate};

/// Apple Inc. Root Certificate
static APPLE_INC_ROOT_CERTIFICATE: Lazy<CapturedX509Certificate> = Lazy::new(|| {
    CapturedX509Certificate::from_der(
        include_bytes!("apple-certs/AppleIncRootCertificate.cer").to_vec(),
    )
    .unwrap()
});

/// Apple Computer, Inc. Root Certificate.
static APPLE_COMPUTER_INC_ROOT_CERTIFICATE: Lazy<CapturedX509Certificate> = Lazy::new(|| {
    CapturedX509Certificate::from_der(
        include_bytes!("apple-certs/AppleComputerRootCertificate.cer").to_vec(),
    )
    .unwrap()
});

/// Apple Root CA - G2 Root Certificate
static APPLE_ROOT_CA_G2_ROOT_CERTIFICATE: Lazy<CapturedX509Certificate> = Lazy::new(|| {
    CapturedX509Certificate::from_der(include_bytes!("apple-certs/AppleRootCA-G2.cer").to_vec())
        .unwrap()
});

/// Apple Root CA - G3 Root Certificate
static APPLE_ROOT_CA_G3_ROOT_CERTIFICATE: Lazy<CapturedX509Certificate> = Lazy::new(|| {
    CapturedX509Certificate::from_der(include_bytes!("apple-certs/AppleRootCA-G3.cer").to_vec())
        .unwrap()
});

/// Apple IST CA 2 - G1 Certificate
static APPLE_IST_CA_2_G1_CERTIFICATE: Lazy<CapturedX509Certificate> = Lazy::new(|| {
    CapturedX509Certificate::from_der(include_bytes!("apple-certs/AppleISTCA2G1.cer").to_vec())
        .unwrap()
});

/// Apple IST CA 8 - G1 Certificate
static APPLE_IST_CA_8_G1_CERTIFICATE: Lazy<CapturedX509Certificate> = Lazy::new(|| {
    CapturedX509Certificate::from_der(include_bytes!("apple-certs/AppleISTCA8G1.cer").to_vec())
        .unwrap()
});

/// Application Integration Certificate
static APPLICATION_INTEGRATION_CERTIFICATE: Lazy<CapturedX509Certificate> = Lazy::new(|| {
    CapturedX509Certificate::from_der(include_bytes!("apple-certs/AppleAAICA.cer").to_vec())
        .unwrap()
});

/// Application Integration 2 Certificate
static APPLICATION_INTEGRATION_2_CERTIFICATE: Lazy<CapturedX509Certificate> = Lazy::new(|| {
    CapturedX509Certificate::from_der(include_bytes!("apple-certs/AppleAAI2CA.cer").to_vec())
        .unwrap()
});

/// Application Integration - G3 Certificate
static APPLICATION_INTEGRATION_G3_CERTIFICATE: Lazy<CapturedX509Certificate> = Lazy::new(|| {
    CapturedX509Certificate::from_der(include_bytes!("apple-certs/AppleAAICAG3.cer").to_vec())
        .unwrap()
});

/// Apple Application Integration CA 5 - G1 Certificate
static APPLE_APPLICATION_INTEGRATION_CA_5_G1_CERTIFICATE: Lazy<CapturedX509Certificate> =
    Lazy::new(|| {
        CapturedX509Certificate::from_der(
            include_bytes!("apple-certs/AppleApplicationIntegrationCA5G1.cer").to_vec(),
        )
        .unwrap()
    });

/// Developer Authentication Certificate
static DEVELOPER_AUTHENTICATION_CERTIFICATE: Lazy<CapturedX509Certificate> = Lazy::new(|| {
    CapturedX509Certificate::from_der(include_bytes!("apple-certs/DevAuthCA.cer").to_vec()).unwrap()
});

/// Developer ID Certificate
static DEVELOPER_ID_CERTIFICATE: Lazy<CapturedX509Certificate> = Lazy::new(|| {
    CapturedX509Certificate::from_der(include_bytes!("apple-certs/DeveloperIDCA.cer").to_vec())
        .unwrap()
});

/// Software Update Certificate
static SOFTWARE_UPDATE_CERTIFICATE: Lazy<CapturedX509Certificate> = Lazy::new(|| {
    CapturedX509Certificate::from_der(
        include_bytes!("apple-certs/AppleSoftwareUpdateCertificationAuthority.cer").to_vec(),
    )
    .unwrap()
});

/// Timestamp Certificate
static TIMESTAMP_CERTIFICATE: Lazy<CapturedX509Certificate> = Lazy::new(|| {
    CapturedX509Certificate::from_der(include_bytes!("apple-certs/AppleTimestampCA.cer").to_vec())
        .unwrap()
});

/// WWDR Certificate (Expiring 02/07/2023 21:48:47 UTC)
static WORLD_WIDE_DEVELOPER_RELATIONS_AUTHORITY_2023: Lazy<CapturedX509Certificate> =
    Lazy::new(|| {
        CapturedX509Certificate::from_der(include_bytes!("apple-certs/AppleWWDRCA.cer").to_vec())
            .unwrap()
    });

/// WWDR Certificate (Expiring 02/20/2030 12:00:00 UTC)
static WORLD_WIDE_DEVELOPER_RELATIONS_AUTHORITY_2030: Lazy<CapturedX509Certificate> =
    Lazy::new(|| {
        CapturedX509Certificate::from_der(include_bytes!("apple-certs/AppleWWDRCAG3.cer").to_vec())
            .unwrap()
    });

/// Worldwide Developer Relations - G2 Certificate
static WORLD_WIDE_DEVELOPER_RELATIONS_G2_CERTIFICATE: Lazy<CapturedX509Certificate> =
    Lazy::new(|| {
        CapturedX509Certificate::from_der(include_bytes!("apple-certs/AppleWWDRCAG2.cer").to_vec())
            .unwrap()
    });

/// All known Apple certificates.
static KNOWN_CERTIFICATES: Lazy<Vec<&CapturedX509Certificate>> = Lazy::new(|| {
    vec![
        // We put the 4 roots first, newest to oldest.
        APPLE_ROOT_CA_G3_ROOT_CERTIFICATE.deref(),
        APPLE_ROOT_CA_G2_ROOT_CERTIFICATE.deref(),
        APPLE_INC_ROOT_CERTIFICATE.deref(),
        APPLE_COMPUTER_INC_ROOT_CERTIFICATE.deref(),
        APPLE_IST_CA_2_G1_CERTIFICATE.deref(),
        APPLE_IST_CA_8_G1_CERTIFICATE.deref(),
        APPLICATION_INTEGRATION_CERTIFICATE.deref(),
        APPLICATION_INTEGRATION_2_CERTIFICATE.deref(),
        APPLICATION_INTEGRATION_G3_CERTIFICATE.deref(),
        APPLE_APPLICATION_INTEGRATION_CA_5_G1_CERTIFICATE.deref(),
        DEVELOPER_AUTHENTICATION_CERTIFICATE.deref(),
        DEVELOPER_ID_CERTIFICATE.deref(),
        SOFTWARE_UPDATE_CERTIFICATE.deref(),
        TIMESTAMP_CERTIFICATE.deref(),
        WORLD_WIDE_DEVELOPER_RELATIONS_AUTHORITY_2023.deref(),
        WORLD_WIDE_DEVELOPER_RELATIONS_AUTHORITY_2030.deref(),
        WORLD_WIDE_DEVELOPER_RELATIONS_G2_CERTIFICATE.deref(),
    ]
});

static KNOWN_ROOTS: Lazy<Vec<&CapturedX509Certificate>> = Lazy::new(|| {
    vec![
        APPLE_ROOT_CA_G3_ROOT_CERTIFICATE.deref(),
        APPLE_ROOT_CA_G2_ROOT_CERTIFICATE.deref(),
        APPLE_INC_ROOT_CERTIFICATE.deref(),
        APPLE_COMPUTER_INC_ROOT_CERTIFICATE.deref(),
    ]
});

/// Defines all known Apple certificates.
///
/// This crate embeds the raw certificate data for the various known
/// Apple certificate authorities, as advertised at
/// <https://www.apple.com/certificateauthority/>.
///
/// This enumeration defines all the ones we know about. Instances can
/// be dereferenced into concrete [CapturedX509Certificate] to get at the underlying
/// certificate and access its metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KnownCertificate {
    /// Apple Computer, Inc. Root Certificate.
    ///
    /// C = US, O = "Apple Computer, Inc.", OU = Apple Computer Certificate Authority, CN = Apple Root Certificate Authority
    AppleComputerIncRoot,

    /// Apple Inc. Root Certificate
    ///
    /// C = US, O = Apple Inc., OU = Apple Certification Authority, CN = Apple Root CA
    AppleRootCa,

    /// Apple Root CA - G2 Root Certificate
    ///
    /// CN = Apple Root CA - G2, OU = Apple Certification Authority, O = Apple Inc., C = US
    AppleRootCaG2Root,

    /// Apple Root CA - G3 Root Certificate
    ///
    /// CN = Apple Root CA - G3, OU = Apple Certification Authority, O = Apple Inc., C = US
    AppleRootCaG3Root,

    /// Apple IST CA 2 - G1 Certificate
    ///
    /// CN = Apple IST CA 2 - G1, OU = Certification Authority, O = Apple Inc., C = US
    AppleIstCa2G1,

    /// Apple IST CA 8 - G1 Certificate
    ///
    /// CN = Apple IST CA 8 - G1, OU = Certification Authority, O = Apple Inc., C = US
    AppleIstCa8G1,

    /// Application Integration Certificate
    ///
    /// C = US, O = Apple Inc., OU = Apple Certification Authority, CN = Apple Application Integration Certification Authority
    ApplicationIntegration,

    /// Application Integration 2 Certificate
    ///
    /// CN = Apple Application Integration 2 Certification Authority, OU = Apple Certification Authority, O = Apple Inc., C = US
    ApplicationIntegration2,

    /// Application Integration - G3 Certificate
    ///
    /// CN = Apple Application Integration CA - G3, OU = Apple Certification Authority, O = Apple Inc., C = US
    ApplicationIntegrationG3,

    /// Apple Application Integration CA 5 - G1 Certificate
    ///
    /// CN = Apple Application Integration CA 5 - G1, OU = Apple Certification Authority, O = Apple Inc., C = US
    AppleApplicationIntegrationCa5G1,

    /// Developer Authentication Certificate
    ///
    /// CN = Developer Authentication Certification Authority, OU = Apple Worldwide Developer Relations, O = Apple Inc., C = US
    DeveloperAuthentication,

    /// Developer ID Certificate
    ///
    /// CN = Developer ID Certification Authority, OU = Apple Certification Authority, O = Apple Inc., C = US
    DeveloperId,

    /// Software Update Certificate
    ///
    /// CN = Apple Software Update Certification Authority, OU = Certification Authority, O = Apple Inc., C = US
    SoftwareUpdate,

    /// Timestamp Certificate
    ///
    /// CN = Apple Timestamp Certification Authority, OU = Apple Certification Authority, O = Apple Inc., C = US
    Timestamp,

    /// WWDR Certificate (Expiring 02/07/2023 21:48:47 UTC)
    ///
    /// C = US, O = Apple Inc., OU = Apple Worldwide Developer Relations, CN = Apple Worldwide Developer Relations Certification Authority
    Wwdr2023,

    /// WWDR Certificate (Expiring 02/20/2030 12:00:00 UTC)
    ///
    /// CN = Apple Worldwide Developer Relations Certification Authority, OU = G3, O = Apple Inc., C = US
    Wwdr2030,

    /// Worldwide Developer Relations - G2 Certificate
    ///
    /// CN = Apple Worldwide Developer Relations CA - G2, OU = Apple Certification Authority, O = Apple Inc., C = US
    WwdrG2,
}

impl Deref for KnownCertificate {
    type Target = CapturedX509Certificate;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::AppleComputerIncRoot => APPLE_COMPUTER_INC_ROOT_CERTIFICATE.deref(),
            Self::AppleRootCa => APPLE_INC_ROOT_CERTIFICATE.deref(),
            Self::AppleRootCaG2Root => APPLE_ROOT_CA_G2_ROOT_CERTIFICATE.deref(),
            Self::AppleRootCaG3Root => APPLE_ROOT_CA_G3_ROOT_CERTIFICATE.deref(),
            Self::AppleIstCa2G1 => APPLE_IST_CA_2_G1_CERTIFICATE.deref(),
            Self::AppleIstCa8G1 => APPLE_IST_CA_8_G1_CERTIFICATE.deref(),
            Self::ApplicationIntegration => APPLICATION_INTEGRATION_CERTIFICATE.deref(),
            Self::ApplicationIntegration2 => APPLICATION_INTEGRATION_2_CERTIFICATE.deref(),
            Self::ApplicationIntegrationG3 => APPLICATION_INTEGRATION_G3_CERTIFICATE.deref(),
            Self::AppleApplicationIntegrationCa5G1 => {
                APPLE_APPLICATION_INTEGRATION_CA_5_G1_CERTIFICATE.deref()
            }
            Self::DeveloperAuthentication => DEVELOPER_AUTHENTICATION_CERTIFICATE.deref(),
            Self::DeveloperId => DEVELOPER_ID_CERTIFICATE.deref(),
            Self::SoftwareUpdate => SOFTWARE_UPDATE_CERTIFICATE.deref(),
            Self::Timestamp => TIMESTAMP_CERTIFICATE.deref(),
            Self::Wwdr2023 => WORLD_WIDE_DEVELOPER_RELATIONS_AUTHORITY_2023.deref(),
            Self::Wwdr2030 => WORLD_WIDE_DEVELOPER_RELATIONS_AUTHORITY_2030.deref(),
            Self::WwdrG2 => WORLD_WIDE_DEVELOPER_RELATIONS_G2_CERTIFICATE.deref(),
        }
    }
}

impl AsRef<CapturedX509Certificate> for KnownCertificate {
    fn as_ref(&self) -> &CapturedX509Certificate {
        self.deref()
    }
}

impl TryFrom<&CapturedX509Certificate> for KnownCertificate {
    type Error = &'static str;

    fn try_from(cert: &CapturedX509Certificate) -> Result<Self, Self::Error> {
        let want = cert.constructed_data();

        match cert.constructed_data() {
            _ if APPLE_ROOT_CA_G3_ROOT_CERTIFICATE.constructed_data() == want => {
                Ok(Self::AppleRootCaG3Root)
            }
            _ if APPLE_ROOT_CA_G2_ROOT_CERTIFICATE.constructed_data() == want => {
                Ok(Self::AppleRootCaG2Root)
            }
            _ if APPLE_INC_ROOT_CERTIFICATE.constructed_data() == want => Ok(Self::AppleRootCa),
            _ if APPLE_COMPUTER_INC_ROOT_CERTIFICATE.constructed_data() == want => {
                Ok(Self::AppleComputerIncRoot)
            }
            _ if APPLE_IST_CA_2_G1_CERTIFICATE.constructed_data() == want => {
                Ok(Self::AppleIstCa2G1)
            }
            _ if APPLE_IST_CA_8_G1_CERTIFICATE.constructed_data() == want => {
                Ok(Self::AppleIstCa8G1)
            }
            _ if APPLICATION_INTEGRATION_CERTIFICATE.constructed_data() == want => {
                Ok(Self::ApplicationIntegration)
            }
            _ if APPLICATION_INTEGRATION_2_CERTIFICATE.constructed_data() == want => {
                Ok(Self::ApplicationIntegration2)
            }
            _ if APPLICATION_INTEGRATION_G3_CERTIFICATE.constructed_data() == want => {
                Ok(Self::ApplicationIntegrationG3)
            }
            _ if APPLE_APPLICATION_INTEGRATION_CA_5_G1_CERTIFICATE.constructed_data() == want => {
                Ok(Self::AppleApplicationIntegrationCa5G1)
            }
            _ if DEVELOPER_AUTHENTICATION_CERTIFICATE.constructed_data() == want => {
                Ok(Self::DeveloperAuthentication)
            }
            _ if DEVELOPER_ID_CERTIFICATE.constructed_data() == want => Ok(Self::DeveloperId),
            _ if SOFTWARE_UPDATE_CERTIFICATE.constructed_data() == want => Ok(Self::SoftwareUpdate),
            _ if TIMESTAMP_CERTIFICATE.constructed_data() == want => Ok(Self::Timestamp),
            _ if WORLD_WIDE_DEVELOPER_RELATIONS_AUTHORITY_2023.constructed_data() == want => {
                Ok(Self::Wwdr2023)
            }
            _ if WORLD_WIDE_DEVELOPER_RELATIONS_AUTHORITY_2030.constructed_data() == want => {
                Ok(Self::Wwdr2030)
            }
            _ if WORLD_WIDE_DEVELOPER_RELATIONS_G2_CERTIFICATE.constructed_data() == want => {
                Ok(Self::WwdrG2)
            }
            _ => Err("certificate not found"),
        }
    }
}

impl KnownCertificate {
    /// Obtain a slice of all known [KnownCertificate].
    ///
    /// If you want to iterate over all certificates and find one, you can use
    /// this.
    pub fn all() -> &'static [&'static CapturedX509Certificate] {
        KNOWN_CERTIFICATES.deref().as_ref()
    }

    /// All of Apple's known root certificate authority certificates.
    pub fn all_roots() -> &'static [&'static CapturedX509Certificate] {
        KNOWN_ROOTS.deref()
    }
}

#[cfg(test)]
mod test {
    use {
        super::*,
        crate::certificate::{AppleCertificate, CertificateAuthorityExtension},
    };

    #[test]
    fn all() {
        for cert in KnownCertificate::all() {
            assert!(cert.subject_common_name().is_some());
            assert!(KnownCertificate::try_from(*cert).is_ok());
        }
    }

    #[test]
    fn apple_root_ca() {
        assert!(APPLE_INC_ROOT_CERTIFICATE.is_apple_root_ca());
        assert!(!APPLE_INC_ROOT_CERTIFICATE.is_apple_intermediate_ca());
        assert!(APPLE_COMPUTER_INC_ROOT_CERTIFICATE.is_apple_root_ca());
        assert!(!APPLE_COMPUTER_INC_ROOT_CERTIFICATE.is_apple_intermediate_ca());
        assert!(APPLE_ROOT_CA_G2_ROOT_CERTIFICATE.is_apple_root_ca());
        assert!(!APPLE_ROOT_CA_G2_ROOT_CERTIFICATE.is_apple_intermediate_ca());
        assert!(APPLE_ROOT_CA_G3_ROOT_CERTIFICATE.is_apple_root_ca());
        assert!(!APPLE_ROOT_CA_G3_ROOT_CERTIFICATE.is_apple_intermediate_ca());

        assert!(!WORLD_WIDE_DEVELOPER_RELATIONS_AUTHORITY_2030.is_apple_root_ca());
        assert!(WORLD_WIDE_DEVELOPER_RELATIONS_AUTHORITY_2030.is_apple_intermediate_ca());

        let wanted = vec![
            APPLE_INC_ROOT_CERTIFICATE.deref(),
            APPLE_COMPUTER_INC_ROOT_CERTIFICATE.deref(),
            APPLE_ROOT_CA_G2_ROOT_CERTIFICATE.deref(),
            APPLE_ROOT_CA_G3_ROOT_CERTIFICATE.deref(),
        ];

        for cert in KnownCertificate::all() {
            if wanted.contains(cert) {
                continue;
            }

            assert!(!cert.is_apple_root_ca());
            assert!(cert.is_apple_intermediate_ca());
        }
    }

    #[test]
    fn intermediate_have_apple_ca_extension() {
        // All intermediate certs should have OIDs identifying them as such.
        for cert in KnownCertificate::all()
            .iter()
            .filter(|cert| !cert.is_apple_root_ca())
            // There are some intermediate certificates signed by GeoTrust. Filter them out
            // as well.
            .filter(|cert| {
                cert.issuer_name()
                    .iter_common_name()
                    .all(|atv| !atv.to_string().unwrap().contains("GeoTrust"))
            })
        {
            assert!(cert.apple_ca_extension().is_some());
        }

        // Let's spot check a few.
        assert_eq!(
            KnownCertificate::DeveloperId.apple_ca_extension(),
            Some(CertificateAuthorityExtension::DeveloperId)
        );
        assert_eq!(
            KnownCertificate::Wwdr2023.apple_ca_extension(),
            Some(CertificateAuthorityExtension::AppleWorldwideDeveloperRelations)
        )
    }

    #[test]
    fn chaining() {
        let relevant = KnownCertificate::all()
            .iter()
            .filter(|cert| {
                cert.issuer_name()
                    .iter_common_name()
                    .all(|atv| !atv.to_string().unwrap().contains("GeoTrust"))
            })
            .filter(|cert| {
                cert.constructed_data() != APPLICATION_INTEGRATION_G3_CERTIFICATE.constructed_data()
                    && cert.constructed_data()
                        != APPLE_APPLICATION_INTEGRATION_CA_5_G1_CERTIFICATE.constructed_data()
            });

        for cert in relevant {
            let chain = cert.resolve_signing_chain(KnownCertificate::all().iter().copied());
            let apple_chain = cert.apple_issuing_chain();
            assert_eq!(chain.len(), apple_chain.len());
        }
    }
}
