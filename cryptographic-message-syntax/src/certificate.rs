// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! High-level X.509 certificate interfaces. */

use {
    crate::{
        asn1::{
            rfc3280::{self, AttributeTypeAndValue, DirectoryString, Name},
            rfc4519::{
                OID_COMMON_NAME, OID_COUNTRY_NAME, OID_ORGANIZATIONAL_UNIT_NAME,
                OID_ORGANIZATION_NAME,
            },
            rfc5280,
            rfc5652::{CertificateChoices, IssuerAndSerialNumber},
        },
        CertificateKeyAlgorithm, CmsError,
    },
    bcder::{
        decode::{self, Constructed},
        encode::Values,
        Integer, Mode, Oid, Utf8String,
    },
    bytes::Bytes,
    std::{
        convert::{TryFrom, TryInto},
        str::FromStr,
    },
};

/// Represents a Relative Distinguished Name (RDN).
///
/// These are what certificate subject and issuer fields are. Manipulating
/// these yourself is hard. This type makes it easier.
#[derive(Clone, Debug, Default)]
pub struct RelativeDistinguishedName(rfc3280::RelativeDistinguishedName);

impl RelativeDistinguishedName {
    /// Obtain the Common Name (CN) field.
    pub fn common_name(&self) -> Result<Option<String>, decode::Error> {
        self.find_attribute_string(Oid(Bytes::from(OID_COMMON_NAME.as_ref())))
    }

    /// Set the value of the Common Name (CN) field.
    pub fn set_common_name(&mut self, value: &str) -> Result<(), bcder::string::CharSetError> {
        self.set_attribute_string(Oid(Bytes::from(OID_COMMON_NAME.as_ref())), value)
    }

    /// Obtain the Country Name (C) field.
    pub fn country_name(&self) -> Result<Option<String>, decode::Error> {
        self.find_attribute_string(Oid(Bytes::from(OID_COUNTRY_NAME.as_ref())))
    }

    /// Set the value of the Country Name (C) field.
    pub fn set_country_name(&mut self, value: &str) -> Result<(), bcder::string::CharSetError> {
        self.set_attribute_string(Oid(Bytes::from(OID_COUNTRY_NAME.as_ref())), value)
    }

    /// Obtain the Organization Name (O) field.
    pub fn organization_name(&self) -> Result<Option<String>, decode::Error> {
        self.find_attribute_string(Oid(Bytes::from(OID_ORGANIZATION_NAME.as_ref())))
    }

    /// Set the value of the Organization Name (O) field.
    pub fn set_organization_name(
        &mut self,
        value: &str,
    ) -> Result<(), bcder::string::CharSetError> {
        self.set_attribute_string(Oid(Bytes::from(OID_ORGANIZATION_NAME.as_ref())), value)
    }

    /// Obtain the Organizational Unit Name (OU) field.
    pub fn organizational_unit_name(&self) -> Result<Option<String>, decode::Error> {
        self.find_attribute_string(Oid(Bytes::from(OID_ORGANIZATIONAL_UNIT_NAME.as_ref())))
    }

    /// Set the value of the Organizational Unit Name (OU) field.
    pub fn set_organizational_unit_name(
        &mut self,
        value: &str,
    ) -> Result<(), bcder::string::CharSetError> {
        self.set_attribute_string(
            Oid(Bytes::from(OID_ORGANIZATIONAL_UNIT_NAME.as_ref())),
            value,
        )
    }

    fn find_attribute(&self, attribute: Oid) -> Option<&AttributeTypeAndValue> {
        self.0.iter().find(|attr| attr.typ == attribute)
    }

    fn find_attribute_mut(&mut self, attribute: Oid) -> Option<&mut AttributeTypeAndValue> {
        self.0.iter_mut().find(|attr| attr.typ == attribute)
    }

    fn find_attribute_string(&self, attribute: Oid) -> Result<Option<String>, decode::Error> {
        if let Some(attr) = self.find_attribute(attribute) {
            attr.value.clone().decode(|cons| {
                let value = DirectoryString::take_from(cons)?;

                Ok(Some(value.to_string()))
            })
        } else {
            Ok(None)
        }
    }

    pub fn set_attribute_string(
        &mut self,
        attribute: Oid,
        value: &str,
    ) -> Result<(), bcder::string::CharSetError> {
        let ds = DirectoryString::Utf8String(Utf8String::from_str(value)?);
        let captured = bcder::Captured::from_values(Mode::Der, ds);

        if let Some(mut attr) = self.find_attribute_mut(attribute.clone()) {
            attr.value = captured;

            Ok(())
        } else {
            self.0.push(AttributeTypeAndValue {
                typ: attribute,
                value: captured,
            });

            Ok(())
        }
    }
}

impl From<RelativeDistinguishedName> for Name {
    fn from(rdn: RelativeDistinguishedName) -> Self {
        let mut seq = rfc3280::RdnSequence::default();
        seq.push(rdn.0);

        Self::RdnSequence(seq)
    }
}

impl TryFrom<&Name> for RelativeDistinguishedName {
    type Error = CmsError;

    fn try_from(name: &Name) -> Result<Self, Self::Error> {
        match name {
            Name::RdnSequence(seq) => match seq.iter().next() {
                Some(rdn) => Ok(RelativeDistinguishedName(rdn.clone())),
                None => Err(CmsError::DistinguishedNameParseError),
            },
        }
    }
}

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

    /// Obtain the subject as a parsed object.
    pub fn subject_dn(&self) -> Result<RelativeDistinguishedName, CmsError> {
        RelativeDistinguishedName::try_from(&self.subject)
    }

    /// The issuer of this certificate.
    ///
    /// (Used for identification purposes.)
    pub fn issuer(&self) -> &Name {
        &self.issuer
    }

    /// Obtain the issuer as a parsed object.
    pub fn issuer_dn(&self) -> Result<RelativeDistinguishedName, CmsError> {
        RelativeDistinguishedName::try_from(&self.issuer)
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
