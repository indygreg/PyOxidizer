// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Apple X.509 certificates.
//!
//! This module defines well-known Apple X.509 certificates.
//!
//! The canonical source of this data is https://www.apple.com/certificateauthority/.
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
        APPLE_INC_ROOT_CERTIFICATE.deref(),
        APPLE_COMPUTER_INC_ROOT_CERTIFICATE.deref(),
        APPLE_ROOT_CA_G2_ROOT_CERTIFICATE.deref(),
        APPLE_ROOT_CA_G3_ROOT_CERTIFICATE.deref(),
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

/// Defines all known Apple certificates.
///
/// This crate embeds the raw certificate data for the various known
/// Apple certificate authorities, as advertised at
/// https://www.apple.com/certificateauthority/.
///
/// This enumeration defines all the ones we know about. Instances can
/// be dereferenced into concrete [AppleCertificate] to get at the underlying
/// certificate and access its metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KnownCertificate {
    /// Apple Inc. Root Certificate
    AppleIncRoot,
    /// Apple Computer, Inc. Root Certificate.
    AppleComputerIncRoot,
    /// Apple Root CA - G2 Root Certificate
    AppleRootCaG2Root,
    /// Apple Root CA - G3 Root Certificate
    AppleRootCaG3Root,
    /// Apple IST CA 2 - G1 Certificate
    AppleIstCa2G1,
    /// Apple IST CA 8 - G1 Certificate
    AppleIstCa8G1,
    /// Application Integration Certificate
    ApplicationIntegration,
    /// Application Integration 2 Certificate
    ApplicationIntegration2,
    /// Application Integration - G3 Certificate
    ApplicationIntegrationG3,
    /// Apple Application Integration CA 5 - G1 Certificate
    AppleApplicationIntegrationCa5G1,
    /// Developer Authentication Certificate
    DeveloperAuthentication,
    /// Developer ID Certificate
    DeveloperId,
    /// Software Update Certificate
    SoftwareUpdate,
    /// Timestamp Certificate
    Timestamp,
    /// WWDR Certificate (Expiring 02/07/2023 21:48:47 UTC)
    Wwdr2023,
    /// WWDR Certificate (Expiring 02/20/2030 12:00:00 UTC)
    Wwdr2030,
    /// Worldwide Developer Relations - G2 Certificate
    WwdrG2,
}

impl Deref for KnownCertificate {
    type Target = CapturedX509Certificate;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::AppleIncRoot => APPLE_INC_ROOT_CERTIFICATE.deref(),
            Self::AppleComputerIncRoot => APPLE_COMPUTER_INC_ROOT_CERTIFICATE.deref(),
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

impl KnownCertificate {
    /// Obtain a slice of all known [AppleCertificate].
    ///
    /// If you want to iterate over all certificates and find one, you can use
    /// this.
    pub fn all() -> &'static [&'static CapturedX509Certificate] {
        KNOWN_CERTIFICATES.deref().as_ref()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn all() {
        for cert in KnownCertificate::all() {
            assert!(cert.subject_common_name().is_some());
        }
    }
}
