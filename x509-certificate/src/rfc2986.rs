// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! ASN.1 primitives from RFC 2986.

use {
    crate::{
        rfc3280::Name,
        rfc5280::{AlgorithmIdentifier, SubjectPublicKeyInfo},
        rfc5652::Attribute,
        rfc5958::Attributes,
    },
    bcder::{
        decode::{Constructed, Malformed, Source},
        encode::{self, PrimitiveContent, Values},
        BitString, Integer, Mode, Tag,
    },
    std::io::Write,
};

#[derive(Clone, Copy, Debug)]
pub enum Version {
    V1 = 0,
}

impl From<Version> for u8 {
    fn from(v: Version) -> u8 {
        match v {
            Version::V1 => 0,
        }
    }
}

impl Version {
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, S::Err> {
        match cons.take_primitive_if(Tag::INTEGER, Integer::i8_from_primitive)? {
            0 => Ok(Self::V1),
            _ => Err(Malformed.into()),
        }
    }

    pub fn encode(self) -> impl Values {
        u8::from(self).encode()
    }
}

/// Certificate request info.
///
/// ```asn.1
/// CertificationRequestInfo ::= SEQUENCE {
///   version       INTEGER { v1(0) } (v1,...),
///   subject       Name,
///   subjectPKInfo SubjectPublicKeyInfo{{ PKInfoAlgorithms }},
///   attributes    [0] Attributes{{ CRIAttributes }}
/// }
/// ```
#[derive(Clone)]
pub struct CertificationRequestInfo {
    pub version: Version,
    pub subject: Name,
    pub subject_public_key_info: SubjectPublicKeyInfo,
    pub attributes: Attributes,
}

impl CertificationRequestInfo {
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, S::Err> {
        cons.take_sequence(|cons| Self::from_sequence(cons))
    }

    pub fn from_sequence<S: Source>(cons: &mut Constructed<S>) -> Result<Self, S::Err> {
        let version = Version::take_from(cons)?;
        let subject = Name::take_from(cons)?;
        let subject_public_key_info = SubjectPublicKeyInfo::take_from(cons)?;
        let attributes = cons.take_constructed_if(Tag::CTX_0, |cons| {
            let mut attributes = Attributes::default();

            while let Some(attribute) = Attribute::take_opt_from(cons)? {
                attributes.push(attribute);
            }

            Ok(attributes)
        })?;

        Ok(Self {
            version,
            subject,
            subject_public_key_info,
            attributes,
        })
    }

    pub fn encode_ref(&self) -> impl Values + '_ {
        encode::sequence((
            self.version.encode(),
            self.subject.encode_ref(),
            self.subject_public_key_info.encode_ref(),
            self.attributes.encode_ref_as(Tag::CTX_0),
        ))
    }
}

impl Values for CertificationRequestInfo {
    fn encoded_len(&self, mode: Mode) -> usize {
        self.encode_ref().encoded_len(mode)
    }

    fn write_encoded<W: Write>(&self, mode: Mode, target: &mut W) -> Result<(), std::io::Error> {
        self.encode_ref().write_encoded(mode, target)
    }
}

/// Certificate request.
///
/// ```asn.1
/// CertificationRequest ::= SEQUENCE {
///   certificationRequestInfo CertificationRequestInfo,
///   signatureAlgorithm AlgorithmIdentifier{{ SignatureAlgorithms }},
///   signature          BIT STRING
/// }
/// ```
#[derive(Clone)]
pub struct CertificationRequest {
    pub certificate_request_info: CertificationRequestInfo,
    pub signature_algorithm: AlgorithmIdentifier,
    pub signature: BitString,
}

impl CertificationRequest {
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, S::Err> {
        cons.take_sequence(|cons| Self::from_sequence(cons))
    }

    pub fn from_sequence<S: Source>(cons: &mut Constructed<S>) -> Result<Self, S::Err> {
        let certificate_request_info = CertificationRequestInfo::take_from(cons)?;
        let signature_algorithm = AlgorithmIdentifier::take_from(cons)?;
        let signature = BitString::take_from(cons)?;

        Ok(Self {
            certificate_request_info,
            signature_algorithm,
            signature,
        })
    }

    pub fn encode_ref(&self) -> impl Values + '_ {
        encode::sequence((
            self.certificate_request_info.encode_ref(),
            &self.signature_algorithm,
            self.signature.encode_ref(),
        ))
    }

    /// Encode this data structure to DER.
    pub fn encode_der(&self) -> Result<Vec<u8>, std::io::Error> {
        let mut buffer = vec![];
        self.write_encoded(Mode::Der, &mut buffer)?;

        Ok(buffer)
    }

    /// Encode the data structure to PEM.
    pub fn encode_pem(&self) -> Result<String, std::io::Error> {
        Ok(pem::encode(&pem::Pem {
            tag: "CERTIFICATE REQUEST".into(),
            contents: self.encode_der()?,
        }))
    }
}

impl Values for CertificationRequest {
    fn encoded_len(&self, mode: Mode) -> usize {
        self.encode_ref().encoded_len(mode)
    }

    fn write_encoded<W: Write>(&self, mode: Mode, target: &mut W) -> Result<(), std::io::Error> {
        self.encode_ref().write_encoded(mode, target)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn rsa_parse() {
        let der = include_bytes!("testdata/csr-rsa2048.der");

        let csr = Constructed::decode(der.as_ref(), Mode::Der, |cons| {
            CertificationRequest::take_from(cons)
        })
        .unwrap();

        let mut encoded = vec![];
        csr.write_encoded(Mode::Der, &mut encoded).unwrap();

        assert_eq!(&encoded, der);
    }
}
