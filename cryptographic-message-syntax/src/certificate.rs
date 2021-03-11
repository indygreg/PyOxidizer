// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! High-level X.509 certificate interfaces. */

use {
    crate::{
        asn1::{
            rfc3280::Name,
            rfc5652::{CertificateChoices, IssuerAndSerialNumber},
        },
        CertificateKeyAlgorithm, CmsError,
    },
    bcder::Integer,
    std::convert::TryFrom,
};

/// Defines an X.509 certificate used for signing data.
#[derive(Clone, Debug)]
pub struct Certificate {
    /// The certificate's serial number.
    ///
    /// We need to store an ASN.1 primitive because ASN.1 integers are
    /// unbounded.
    serial_number: Integer,

    /// Name of this certificate.
    ///
    /// We store the ASN.1 type because doing this differently is hard.
    subject: Name,

    /// The issuer of this certificate.
    ///
    /// We store the ASN.1 type because this differently is hard.
    issuer: Name,

    /// The public key for this certificate.
    pub public_key: CertificatePublicKey,
}

impl Certificate {
    /// The serial number of this certificate.
    ///
    /// (Used for identification purposes.)
    pub fn serial_number(&self) -> &Integer {
        &self.serial_number
    }

    /// The subject of this certificate.
    ///
    /// (Used for identification purposes.)
    pub fn subject(&self) -> &Name {
        &self.subject
    }

    /// The issuer of this certificate.
    ///
    /// (Used for identification purposes.)
    pub fn issuer(&self) -> &Name {
        &self.issuer
    }

    /// Obtain the public key associated with this certificate.
    ///
    /// The public keys gives you access to the pieces needed to perform
    /// cryptographic signature verification.
    pub fn public_key(&self) -> &CertificatePublicKey {
        &self.public_key
    }
}

impl TryFrom<&CertificateChoices> for Certificate {
    type Error = CmsError;

    fn try_from(cert: &CertificateChoices) -> Result<Self, Self::Error> {
        match cert {
            CertificateChoices::Certificate(cert) => Self::try_from(cert.as_ref()),
            _ => Err(CmsError::UnknownCertificateFormat),
        }
    }
}

impl TryFrom<&crate::asn1::rfc5280::Certificate> for Certificate {
    type Error = CmsError;

    fn try_from(cert: &crate::asn1::rfc5280::Certificate) -> Result<Self, Self::Error> {
        let serial_number = cert.tbs_certificate.serial_number.clone();
        let subject = cert.tbs_certificate.subject.clone();
        let issuer = cert.tbs_certificate.issuer.clone();

        let public_key =
            CertificatePublicKey::try_from(&cert.tbs_certificate.subject_public_key_info)?;

        Ok(Self {
            serial_number,
            subject,
            issuer,
            public_key,
        })
    }
}

impl From<Certificate> for IssuerAndSerialNumber {
    fn from(cert: Certificate) -> Self {
        Self {
            issuer: cert.subject,
            serial_number: cert.serial_number,
        }
    }
}

/// Describes a public key in a X.509 certificate key pair.
#[derive(Clone, Debug)]
pub struct CertificatePublicKey {
    /// Key algorithm.
    pub algorithm: CertificateKeyAlgorithm,

    /// Raw public key data.
    pub key: Vec<u8>,
}

impl TryFrom<&crate::asn1::rfc5280::SubjectPublicKeyInfo> for CertificatePublicKey {
    type Error = CmsError;

    fn try_from(info: &crate::asn1::rfc5280::SubjectPublicKeyInfo) -> Result<Self, Self::Error> {
        let algorithm = CertificateKeyAlgorithm::try_from(&info.algorithm)?;
        let key = info.subject_public_key.octet_bytes().to_vec();

        Ok(Self { algorithm, key })
    }
}

/// Whether one certificate is a subset of another certificate.
///
/// This returns true iff the two certificates have the same serial number
/// and every `Name` attribute in the first certificate is present in the other.
pub fn certificate_is_subset_of(
    a_serial: &Integer,
    a_name: &Name,
    b_serial: &Integer,
    b_name: &Name,
) -> bool {
    if a_serial != b_serial {
        return false;
    }

    let Name::RdnSequence(a_sequence) = &a_name;
    let Name::RdnSequence(b_sequence) = &b_name;

    a_sequence.iter().all(|rdn| b_sequence.contains(rdn))
}
