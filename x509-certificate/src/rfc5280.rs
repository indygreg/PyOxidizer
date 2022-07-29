// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! ASN.1 types defined RFC 5280. */

use {
    crate::{asn1time::*, rfc3280::*},
    bcder::{
        decode::{BytesSource, Constructed, DecodeError, IntoSource, Source},
        encode,
        encode::{PrimitiveContent, Values},
        BitString, Captured, Integer, Mode, OctetString, Oid, Tag,
    },
    bytes::Bytes,
    std::{
        fmt::{Debug, Formatter},
        io::Write,
        ops::{Deref, DerefMut},
    },
};

/// Algorithm identifier.
///
/// ```ASN.1
/// AlgorithmIdentifier  ::=  SEQUENCE  {
///   algorithm               OBJECT IDENTIFIER,
///   parameters              ANY DEFINED BY algorithm OPTIONAL  }
/// ```
#[derive(Clone, Eq, PartialEq)]
pub struct AlgorithmIdentifier {
    pub algorithm: Oid,
    pub parameters: Option<AlgorithmParameter>,
}

impl Debug for AlgorithmIdentifier {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("AlgorithmIdentifier");
        s.field("algorithm", &format_args!("{}", self.algorithm));
        s.field("parameters", &self.parameters);
        s.finish()
    }
}

impl AlgorithmIdentifier {
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, DecodeError<S::Error>> {
        cons.take_sequence(|cons| Self::take_sequence(cons))
    }

    pub fn take_opt_from<S: Source>(
        cons: &mut Constructed<S>,
    ) -> Result<Option<Self>, DecodeError<S::Error>> {
        cons.take_opt_sequence(|cons| Self::take_sequence(cons))
    }

    fn take_sequence<S: Source>(cons: &mut Constructed<S>) -> Result<Self, DecodeError<S::Error>> {
        let algorithm = Oid::take_from(cons)?;
        let parameters = cons.capture_all()?;

        let parameters = if parameters.is_empty() {
            None
        } else {
            Some(AlgorithmParameter(parameters))
        };

        Ok(Self {
            algorithm,
            parameters,
        })
    }

    fn encoded_values(&self, mode: Mode) -> impl Values + '_ {
        // parameters is strictly OPTIONAL, which means we can omit it completely.
        // However, it is common to see this field encoded as NULL and some
        // parsers seem to insist the NULL be there or else they refuse to
        // parse the ASN.1. So we ensure this field is always set.
        let captured = if let Some(params) = self.parameters.as_ref() {
            params.clone()
        } else {
            AlgorithmParameter(Captured::from_values(mode, ().encode_as(Tag::NULL)))
        };

        encode::sequence((self.algorithm.clone().encode(), captured))
    }
}

impl Values for AlgorithmIdentifier {
    fn encoded_len(&self, mode: Mode) -> usize {
        self.encoded_values(mode).encoded_len(mode)
    }

    fn write_encoded<W: Write>(&self, mode: Mode, target: &mut W) -> Result<(), std::io::Error> {
        self.encoded_values(mode).write_encoded(mode, target)
    }
}

/// A parameter for an algorithm.
///
/// This type doesn't exist in the ASN.1. We've implemented it to
/// make (de)serialization simpler.
#[derive(Clone, Debug)]
pub struct AlgorithmParameter(Captured);

impl AlgorithmParameter {
    /// Construct a new instance consisting of a single OID.
    pub fn from_oid(oid: Oid) -> Self {
        let captured = Captured::from_values(Mode::Der, oid.encode());

        Self(captured)
    }

    /// Attempt to decode a single OID from the captured value.
    pub fn decode_oid(&self) -> Result<Oid, DecodeError<<BytesSource as Source>::Error>> {
        let source = BytesSource::new(Bytes::copy_from_slice(self.0.as_slice()));

        Constructed::decode(source, Mode::Der, |cons| Oid::take_from(cons))
    }
}

impl Deref for AlgorithmParameter {
    type Target = Captured;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for AlgorithmParameter {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl PartialEq for AlgorithmParameter {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_slice() == other.0.as_slice()
    }
}

impl Eq for AlgorithmParameter {}

impl Values for AlgorithmParameter {
    fn encoded_len(&self, mode: Mode) -> usize {
        self.0.encoded_len(mode)
    }

    fn write_encoded<W: Write>(&self, mode: Mode, target: &mut W) -> Result<(), std::io::Error> {
        self.0.write_encoded(mode, target)
    }
}

/// An X.509 certificate.
///
/// This is the main data structure representing an X.509 certificate.
///
/// ```ASN.1
/// Certificate  ::=  SEQUENCE  {
///   tbsCertificate       TBSCertificate,
///   signatureAlgorithm   AlgorithmIdentifier,
///   signature            BIT STRING  }
/// ```
#[derive(Clone, Eq, PartialEq)]
pub struct Certificate {
    pub tbs_certificate: TbsCertificate,
    pub signature_algorithm: AlgorithmIdentifier,
    pub signature: BitString,
}

impl Debug for Certificate {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("Certificate");
        s.field("tbs_certificate", &self.tbs_certificate);
        s.field("signature_algorithm", &self.signature_algorithm);
        s.field(
            "signature",
            &format_args!(
                "{} (unused {})",
                hex::encode(self.signature.octet_bytes()),
                self.signature.unused()
            ),
        );
        s.finish()
    }
}

impl Certificate {
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, DecodeError<S::Error>> {
        cons.take_sequence(|cons| Self::from_sequence(cons))
    }

    pub fn from_sequence<S: Source>(
        cons: &mut Constructed<S>,
    ) -> Result<Self, DecodeError<S::Error>> {
        let tbs_certificate = TbsCertificate::take_from(cons)?;
        let signature_algorithm = AlgorithmIdentifier::take_from(cons)?;
        let signature = BitString::take_from(cons)?;

        Ok(Self {
            tbs_certificate,
            signature_algorithm,
            signature,
        })
    }

    pub fn encode_ref(&self) -> impl Values + '_ {
        encode::sequence((
            self.tbs_certificate.encode_ref(),
            &self.signature_algorithm,
            self.signature.encode_ref(),
        ))
    }

    /// Iterate over extensions defined on this certificate.
    pub fn iter_extensions(&self) -> impl Iterator<Item = &Extension> {
        self.tbs_certificate
            .extensions
            .iter()
            .flat_map(|x| x.iter())
    }
}

/// TBS Certificate.
///
/// This holds most of the metadata within an X.509 certificate.
///
/// ```ASN.1
/// TBSCertificate  ::=  SEQUENCE  {
///      version         [0]  Version DEFAULT v1,
///      serialNumber         CertificateSerialNumber,
///      signature            AlgorithmIdentifier,
///      issuer               Name,
///      validity             Validity,
///      subject              Name,
///      subjectPublicKeyInfo SubjectPublicKeyInfo,
///      issuerUniqueID  [1]  IMPLICIT UniqueIdentifier OPTIONAL,
///                           -- If present, version MUST be v2 or v3
///      subjectUniqueID [2]  IMPLICIT UniqueIdentifier OPTIONAL,
///                           -- If present, version MUST be v2 or v3
///      extensions      [3]  Extensions OPTIONAL
///                           -- If present, version MUST be v3 --  }
/// ```
#[derive(Clone, Eq, PartialEq)]
pub struct TbsCertificate {
    pub version: Option<Version>,
    pub serial_number: CertificateSerialNumber,
    pub signature: AlgorithmIdentifier,
    pub issuer: Name,
    pub validity: Validity,
    pub subject: Name,
    pub subject_public_key_info: SubjectPublicKeyInfo,
    pub issuer_unique_id: Option<UniqueIdentifier>,
    pub subject_unique_id: Option<UniqueIdentifier>,
    pub extensions: Option<Extensions>,

    /// Raw bytes this instance was constructed from.
    ///
    /// This is what signature verification should be performed against.
    pub raw_data: Option<Vec<u8>>,
}

impl Debug for TbsCertificate {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("TbsCertificate");
        s.field("version", &self.version);
        s.field("serial_number", &self.serial_number);
        s.field("signature", &self.signature);
        s.field("issuer", &self.issuer);
        s.field("validity", &self.validity);
        s.field("subject", &self.subject);
        s.field("subject_public_key_info", &self.subject_public_key_info);
        s.field("issuer_unique_id", &self.issuer_unique_id);
        s.field("subject_unique_id", &self.subject_unique_id);
        s.field("extensions", &self.extensions);
        s.field(
            "raw_data",
            &format_args!("{:?}", self.raw_data.as_ref().map(hex::encode)),
        );
        s.finish()
    }
}

impl TbsCertificate {
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, DecodeError<S::Error>> {
        // The TbsCertificate data is what's signed by the issuing certificate. To
        // facilitate signature verification, we stash away the raw data so they
        // can be accessed later.
        let mut res = None;

        let captured = cons.capture(|cons| {
            cons.take_sequence(|cons| {
                let version = cons.take_opt_constructed_if(Tag::CTX_0, Version::take_from)?;
                let serial_number = CertificateSerialNumber::take_from(cons)?;
                let signature = AlgorithmIdentifier::take_from(cons)?;
                let issuer = Name::take_from(cons)?;
                let validity = Validity::take_from(cons)?;
                let subject = Name::take_from(cons)?;
                let subject_public_key_info = SubjectPublicKeyInfo::take_from(cons)?;
                let issuer_unique_id = cons.take_opt_constructed_if(Tag::CTX_1, |cons| {
                    UniqueIdentifier::take_from(cons)
                })?;
                let subject_unique_id = cons.take_opt_constructed_if(Tag::CTX_2, |cons| {
                    UniqueIdentifier::take_from(cons)
                })?;
                let extensions =
                    cons.take_opt_constructed_if(Tag::CTX_3, |cons| Extensions::take_from(cons))?;

                res = Some(Self {
                    version,
                    serial_number,
                    signature,
                    issuer,
                    validity,
                    subject,
                    subject_public_key_info,
                    issuer_unique_id,
                    subject_unique_id,
                    extensions,
                    raw_data: None,
                });

                Ok(())
            })
        })?;

        let mut res = res.unwrap();
        res.raw_data = Some(captured.to_vec());

        Ok(res)
    }

    pub fn encode_ref(&self) -> impl Values + '_ {
        encode::sequence((
            self.version
                .as_ref()
                .map(|v| encode::Constructed::new(Tag::CTX_0, u8::from(*v).encode())),
            (&self.serial_number).encode(),
            &self.signature,
            self.issuer.encode_ref(),
            self.validity.encode_ref(),
            self.subject.encode_ref(),
            self.subject_public_key_info.encode_ref(),
            self.issuer_unique_id
                .as_ref()
                .map(|id| id.encode_ref_as(Tag::CTX_1)),
            self.subject_unique_id
                .as_ref()
                .map(|id| id.encode_ref_as(Tag::CTX_2)),
            self.extensions
                .as_ref()
                .map(|extensions| encode::Constructed::new(Tag::CTX_3, extensions.encode_ref())),
        ))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Version {
    V1 = 0,
    V2 = 1,
    V3 = 2,
}

impl Version {
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, DecodeError<S::Error>> {
        match cons.take_primitive_if(Tag::INTEGER, Integer::i8_from_primitive)? {
            0 => Ok(Self::V1),
            1 => Ok(Self::V2),
            2 => Ok(Self::V3),
            _ => Err(cons.content_err("unexpected Version value")),
        }
    }

    pub fn encode(self) -> impl Values {
        u8::from(self).encode()
    }
}

impl From<Version> for u8 {
    fn from(v: Version) -> Self {
        match v {
            Version::V1 => 0,
            Version::V2 => 1,
            Version::V3 => 2,
        }
    }
}

pub type CertificateSerialNumber = Integer;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Validity {
    pub not_before: Time,
    pub not_after: Time,
}

impl Validity {
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, DecodeError<S::Error>> {
        cons.take_sequence(|cons| {
            let not_before = Time::take_from(cons)?;
            let not_after = Time::take_from(cons)?;

            Ok(Self {
                not_before,
                not_after,
            })
        })
    }

    pub fn encode_ref(&self) -> impl Values + '_ {
        encode::sequence((self.not_before.encode_ref(), self.not_after.encode_ref()))
    }
}

pub type UniqueIdentifier = BitString;

/// Subject public key info.
///
/// ```ASN.1
/// SubjectPublicKeyInfo  ::=  SEQUENCE  {
///   algorithm            AlgorithmIdentifier,
///   subjectPublicKey     BIT STRING  }
/// ```
#[derive(Clone, Eq, PartialEq)]
pub struct SubjectPublicKeyInfo {
    pub algorithm: AlgorithmIdentifier,
    pub subject_public_key: BitString,
}

impl Debug for SubjectPublicKeyInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("SubjectPublicKeyInfo");
        s.field("algorithm", &self.algorithm);
        s.field(
            "subject_public_key",
            &format_args!(
                "{} (unused {})",
                hex::encode(self.subject_public_key.octet_bytes().as_ref()),
                self.subject_public_key.unused()
            ),
        );
        s.finish()
    }
}

impl SubjectPublicKeyInfo {
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, DecodeError<S::Error>> {
        cons.take_sequence(|cons| {
            let algorithm = AlgorithmIdentifier::take_from(cons)?;
            let subject_public_key = BitString::take_from(cons)?;

            Ok(Self {
                algorithm,
                subject_public_key,
            })
        })
    }

    pub fn encode_ref(&self) -> impl Values + '_ {
        encode::sequence((&self.algorithm, self.subject_public_key.encode_ref()))
    }
}

/// Extensions
///
/// ```ASN.1
/// Extensions  ::=  SEQUENCE SIZE (1..MAX) OF Extension
/// ```
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Extensions(Vec<Extension>);

impl Extensions {
    pub fn take_opt_from<S: Source>(
        cons: &mut Constructed<S>,
    ) -> Result<Option<Self>, DecodeError<S::Error>> {
        cons.take_opt_sequence(|cons| Self::from_sequence(cons))
    }

    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, DecodeError<S::Error>> {
        cons.take_sequence(|cons| Self::from_sequence(cons))
    }

    pub fn from_sequence<S: Source>(
        cons: &mut Constructed<S>,
    ) -> Result<Self, DecodeError<S::Error>> {
        let mut extensions = Vec::new();

        while let Some(extension) = Extension::take_opt_from(cons)? {
            extensions.push(extension);
        }

        Ok(Self(extensions))
    }

    pub fn encode_ref(&self) -> impl Values + '_ {
        encode::sequence(&self.0)
    }

    pub fn encode_ref_as(&self, tag: Tag) -> impl Values + '_ {
        encode::sequence_as(tag, &self.0)
    }
}

impl Deref for Extensions {
    type Target = Vec<Extension>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Extensions {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Extension.
///
/// ```ASN.1
/// Extension  ::=  SEQUENCE  {
///      extnID      OBJECT IDENTIFIER,
///      critical    BOOLEAN DEFAULT FALSE,
///      extnValue   OCTET STRING
///                  -- contains the DER encoding of an ASN.1 value
///                  -- corresponding to the extension type identified
///                  -- by extnID
///      }
/// ```
#[derive(Clone, Eq, PartialEq)]
pub struct Extension {
    pub id: Oid,
    pub critical: Option<bool>,
    pub value: OctetString,
}

impl Debug for Extension {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("Extension");
        s.field("id", &format_args!("{}", self.id));
        s.field("critical", &self.critical);
        s.field(
            "value",
            &format_args!("{}", hex::encode(self.value.clone().into_bytes().as_ref())),
        );
        s.finish()
    }
}

impl Extension {
    pub fn take_opt_from<S: Source>(
        cons: &mut Constructed<S>,
    ) -> Result<Option<Self>, DecodeError<S::Error>> {
        cons.take_opt_sequence(|cons| Self::from_sequence(cons))
    }

    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, DecodeError<S::Error>> {
        cons.take_sequence(|cons| Self::from_sequence(cons))
    }

    fn from_sequence<S: Source>(cons: &mut Constructed<S>) -> Result<Self, DecodeError<S::Error>> {
        let id = Oid::take_from(cons)?;
        let critical = cons.take_opt_bool()?;
        let value = OctetString::take_from(cons)?;

        Ok(Self {
            id,
            critical,
            value,
        })
    }

    pub fn encode_ref(&self) -> impl Values + '_ {
        encode::sequence((
            self.id.encode_ref(),
            if self.critical == Some(true) {
                Some(true.encode())
            } else {
                None
            },
            self.value.encode_ref(),
        ))
    }

    /// Attempt to decode a single OID present in an outer sequence.
    ///
    /// If this works, we return Some. Else None.
    pub fn try_decode_sequence_single_oid(&self) -> Option<Oid> {
        Constructed::decode(self.value.clone().into_source(), Mode::Der, |cons| {
            cons.take_sequence(|cons| Oid::take_from(cons))
        })
        .ok()
    }
}

impl Values for Extension {
    fn encoded_len(&self, mode: Mode) -> usize {
        self.encode_ref().encoded_len(mode)
    }

    fn write_encoded<W: Write>(&self, mode: Mode, target: &mut W) -> Result<(), std::io::Error> {
        self.encode_ref().write_encoded(mode, target)
    }
}

/// Certificate list.
///
/// ```ASN.1
/// CertificateList  ::=  SEQUENCE  {
///      tbsCertList          TBSCertList,
///      signatureAlgorithm   AlgorithmIdentifier,
///      signature            BIT STRING  }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CertificateList {
    pub tbs_cert_list: TbsCertList,
    pub signature_algorithm: AlgorithmIdentifier,
    pub signature: BitString,
}

impl CertificateList {
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, DecodeError<S::Error>> {
        let tbs_cert_list = TbsCertList::take_from(cons)?;
        let signature_algorithm = AlgorithmIdentifier::take_from(cons)?;
        let signature = BitString::take_from(cons)?;

        Ok(Self {
            tbs_cert_list,
            signature_algorithm,
            signature,
        })
    }
}

/// Tbs Certificate list.
///
/// ```ASN.1
/// TBSCertList  ::=  SEQUENCE  {
///   version                 Version OPTIONAL,
///     -- if present, MUST be v2
///   signature               AlgorithmIdentifier,
///   issuer                  Name,
///   thisUpdate              Time,
///   nextUpdate              Time OPTIONAL,
///   revokedCertificates     SEQUENCE OF SEQUENCE  {
///     userCertificate         CertificateSerialNumber,
///     revocationDate          Time,
///     crlEntryExtensions      Extensions OPTIONAL
///                                 -- if present, MUST be v2
///  }  OPTIONAL,
///  crlExtensions           [0] Extensions OPTIONAL }
///                                -- if present, MUST be v2
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TbsCertList {
    pub version: Option<Version>,
    pub signature: AlgorithmIdentifier,
    pub issuer: Name,
    pub this_update: Time,
    pub next_update: Option<Time>,
    pub revoked_certificates: Vec<(CertificateSerialNumber, Time, Option<Extensions>)>,
    pub crl_extensions: Option<Extensions>,
}

impl TbsCertList {
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, DecodeError<S::Error>> {
        Err(cons.content_err("parsing of TBSCertList not implemented"))
    }
}
