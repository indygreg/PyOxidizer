// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! High-level X.509 certificate interfaces. */

use {
    crate::{
        algorithm::SignatureAlgorithm,
        asn1::{
            rfc3280::Name,
            rfc5280,
            rfc5652::{CertificateChoices, IssuerAndSerialNumber},
        },
        CertificateKeyAlgorithm, CmsError,
    },
    bcder::{decode::Constructed, encode::Values, Integer, Mode},
    std::{
        cmp::Ordering,
        convert::{TryFrom, TryInto},
    },
};

/// Defines an X.509 certificate used for signing data.
#[derive(Clone, Debug, Eq, PartialEq)]
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

    /// The parsed ASN.1 certificate backing this instance.
    raw_cert: rfc5280::Certificate,
}

impl Certificate {
    /// Obtain an instance from an already parsed ASN.1 data structure.
    pub fn from_parsed_asn1(cert: rfc5280::Certificate) -> Result<Self, CmsError> {
        Ok(Self {
            serial_number: cert.tbs_certificate.serial_number.clone(),
            subject: cert.tbs_certificate.subject.clone(),
            issuer: cert.tbs_certificate.issuer.clone(),
            public_key: (&cert.tbs_certificate.subject_public_key_info).try_into()?,
            raw_cert: cert,
        })
    }

    pub fn from_der(data: &[u8]) -> Result<Self, CmsError> {
        let cert = Constructed::decode(data, Mode::Der, |cons| {
            crate::asn1::rfc5280::Certificate::take_from(cons)
        })?;

        Ok(Self {
            serial_number: cert.tbs_certificate.serial_number.clone(),
            subject: cert.tbs_certificate.subject.clone(),
            issuer: cert.tbs_certificate.issuer.clone(),
            public_key: (&cert.tbs_certificate.subject_public_key_info).try_into()?,
            raw_cert: cert,
        })
    }

    pub fn from_pem(data: &[u8]) -> Result<Self, CmsError> {
        let pem = pem::parse(data)?;

        Self::from_der(&pem.contents)
    }

    /// Parse PEM data potentially containing multiple certificate records.
    pub fn from_pem_multiple(data: impl AsRef<[u8]>) -> Result<Vec<Self>, CmsError> {
        pem::parse_many(data)
            .into_iter()
            .map(|pem| Self::from_der(&pem.contents))
            .collect::<Result<Vec<_>, CmsError>>()
    }

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

    /// Obtain the parsed certificate data structure backing this instance.
    pub fn raw_certificate(&self) -> &crate::asn1::rfc5280::Certificate {
        &self.raw_cert
    }

    /// Whether the certificate is self-signed.
    pub fn is_self_signed(&self) -> bool {
        self.subject == self.issuer
    }

    /// Serialize this certificate to BER.
    pub fn as_ber(&self) -> Result<Vec<u8>, CmsError> {
        let mut res = Vec::<u8>::new();

        self.raw_cert
            .encode_ref()
            .write_encoded(Mode::Ber, &mut res)?;

        Ok(res)
    }

    /// Serialize this certificate to DER.
    pub fn as_der(&self) -> Result<Vec<u8>, CmsError> {
        let mut res = Vec::<u8>::new();

        self.raw_cert
            .encode_ref()
            .write_encoded(Mode::Der, &mut res)?;

        Ok(res)
    }

    /// Serialize this certificate to PEM.
    pub fn as_pem(&self) -> Result<String, CmsError> {
        Ok(pem::encode(&pem::Pem {
            tag: "CERTIFICATE".to_string(),
            contents: self.as_der()?,
        }))
    }

    /// Compare 2 instances, sorting so an issuer comes before the issued.
    pub fn issuer_compare(&self, other: &Self) -> Ordering {
        // Self signed certificate has no ordering.
        if self.subject == self.issuer {
            Ordering::Equal
            // We were issued by the other certificate. The issuer comes first.
        } else if self.issuer == other.subject {
            Ordering::Greater
        } else if self.subject == other.issuer {
            // We issued the other certificate. We come first.
            Ordering::Less
        } else {
            Ordering::Equal
        }
    }

    /// Verifies the signature of this certificate using a [rfc5280::SubjectPublicKeyInfo] instance.
    ///
    /// The [rfc5280::SubjectPublicKey] should come from another certificate, presumably
    /// the signer of this one. If this is a self-signed certificate, it comes from self.
    /// And because we implement `AsRef<SubjectPublicKey>`, you can pass `self` into
    /// this function to verify a self-signed certificate.
    pub fn verify_signature(
        &self,
        other: impl Into<rfc5280::SubjectPublicKeyInfo>,
    ) -> Result<(), CmsError> {
        let tbs_data = self
            .raw_cert
            .tbs_certificate
            .raw_data
            .as_ref()
            .ok_or(CmsError::CertificateMissingData)?;

        let spki = other.into();

        let signature_algorithm = SignatureAlgorithm::try_from(&self.raw_cert.signature_algorithm)?;
        let verify_algorithm = signature_algorithm.as_verification_algorithm();

        let key = ring::signature::UnparsedPublicKey::new(
            verify_algorithm,
            spki.subject_public_key.octet_bytes(),
        );
        let signature = self.raw_cert.signature.octet_bytes();

        key.verify(tbs_data, signature.as_ref())
            .map_err(|_| CmsError::SignatureVerificationError)
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
            raw_cert: cert.clone(),
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

impl AsRef<rfc5280::SubjectPublicKeyInfo> for Certificate {
    fn as_ref(&self) -> &rfc5280::SubjectPublicKeyInfo {
        &self.raw_cert.tbs_certificate.subject_public_key_info
    }
}

impl From<&Certificate> for rfc5280::SubjectPublicKeyInfo {
    fn from(cert: &Certificate) -> rfc5280::SubjectPublicKeyInfo {
        cert.raw_cert
            .tbs_certificate
            .subject_public_key_info
            .clone()
    }
}

/// Describes a public key in a X.509 certificate key pair.
#[derive(Clone, Debug, Eq, PartialEq)]
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
