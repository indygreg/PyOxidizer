// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! ASN.1 data structures defined by RFC 5652.

The types defined in this module are intended to be extremely low-level
and only to be used for (de)serialization. See types outside the
`asn1` module tree for higher-level functionality.
*/

use {
    crate::asn1::{common::*, rfc3280::*, rfc3281::*, rfc5280::*},
    bcder::{
        decode::{Constructed, Malformed, Source, Unimplemented},
        encode,
        encode::{PrimitiveContent, Values},
        BitString, Captured, ConstOid, Integer, Mode, OctetString, Oid, Tag,
    },
    std::{
        io::Write,
        ops::{Deref, DerefMut},
    },
};

/// The data content type.
///
/// `id-data` in the specification.
///
/// 1.2.840.113549.1.7.1
pub const OID_ID_DATA: ConstOid = Oid(&[42, 134, 72, 134, 247, 13, 1, 7, 1]);

/// The signed-data content type.
///
/// 1.2.840.113549.1.7.2
pub const OID_ID_SIGNED_DATA: ConstOid = Oid(&[42, 134, 72, 134, 247, 13, 1, 7, 2]);

/// Enveloped data content type.
///
/// 1.2.840.113549.1.7.3
pub const OID_ENVELOPE_DATA: ConstOid = Oid(&[42, 134, 72, 134, 247, 13, 1, 7, 3]);

/// Digested-data content type.
///
/// 1.2.840.113549.1.7.5
pub const OID_DIGESTED_DATA: ConstOid = Oid(&[42, 134, 72, 134, 247, 13, 1, 7, 5]);

/// Encrypted-data content type.
///
/// 1.2.840.113549.1.7.6
pub const OID_ENCRYPTED_DATA: ConstOid = Oid(&[42, 134, 72, 134, 247, 13, 1, 7, 6]);

/// Authenticated-data content type.
///
/// 1.2.840.113549.1.9.16.1.2
pub const OID_AUTHENTICATED_DATA: ConstOid = Oid(&[42, 134, 72, 134, 247, 13, 1, 9, 16, 1, 2]);

/// Identifies the content-type attribute.
///
/// 1.2.840.113549.1.9.3
pub const OID_CONTENT_TYPE: ConstOid = Oid(&[42, 134, 72, 134, 247, 13, 1, 9, 3]);

/// Identifies the message-digest attribute.
///
/// 1.2.840.113549.1.9.4
pub const OID_MESSAGE_DIGEST: ConstOid = Oid(&[42, 134, 72, 134, 247, 13, 1, 9, 4]);

/// Identifies the signing-time attribute.
///
/// 1.2.840.113549.1.9.5
pub const OID_SIGNING_TIME: ConstOid = Oid(&[42, 134, 72, 134, 247, 13, 1, 9, 5]);

/// Identifies the countersignature attribute.
///
/// 1.2.840.113549.1.9.6
pub const OID_COUNTER_SIGNATURE: ConstOid = Oid(&[42, 134, 72, 134, 247, 13, 1, 9, 6]);

/// Content info.
///
/// ```ASN.1
/// ContentInfo ::= SEQUENCE {
///   contentType ContentType,
///   content [0] EXPLICIT ANY DEFINED BY contentType }
/// ```
#[derive(Clone, Debug)]
pub struct ContentInfo {
    pub content_type: ContentType,
    pub content: Captured,
}

impl PartialEq for ContentInfo {
    fn eq(&self, other: &Self) -> bool {
        self.content_type == other.content_type
            && self.content.as_slice() == other.content.as_slice()
    }
}

impl Eq for ContentInfo {}

impl ContentInfo {
    pub fn take_opt_from<S: Source>(cons: &mut Constructed<S>) -> Result<Option<Self>, S::Err> {
        cons.take_opt_sequence(|cons| Self::from_sequence(cons))
    }

    pub fn from_sequence<S: Source>(cons: &mut Constructed<S>) -> Result<Self, S::Err> {
        let content_type = ContentType::take_from(cons)?;
        let content = cons.take_constructed_if(Tag::CTX_0, |cons| cons.capture_all())?;

        Ok(Self {
            content_type,
            content,
        })
    }
}

impl Values for ContentInfo {
    fn encoded_len(&self, mode: Mode) -> usize {
        encode::sequence((self.content_type.encode_ref(), &self.content)).encoded_len(mode)
    }

    fn write_encoded<W: Write>(&self, mode: Mode, target: &mut W) -> Result<(), std::io::Error> {
        encode::sequence((self.content_type.encode_ref(), &self.content))
            .write_encoded(mode, target)
    }
}

/// Represents signed data.
///
/// ASN.1 type specification:
///
/// ```ASN.1
/// SignedData ::= SEQUENCE {
///   version CMSVersion,
///   digestAlgorithms DigestAlgorithmIdentifiers,
///   encapContentInfo EncapsulatedContentInfo,
///   certificates [0] IMPLICIT CertificateSet OPTIONAL,
///   crls [1] IMPLICIT RevocationInfoChoices OPTIONAL,
///   signerInfos SignerInfos }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignedData {
    pub version: CmsVersion,
    pub digest_algorithms: DigestAlgorithmIdentifiers,
    pub content_info: EncapsulatedContentInfo,
    pub certificates: Option<CertificateSet>,
    pub crls: Option<RevocationInfoChoices>,
    pub signer_infos: SignerInfos,
}

impl SignedData {
    /// Attempt to decode BER encoded bytes to a parsed data structure.
    pub fn decode_ber(data: &[u8]) -> Result<Self, bcder::decode::Error> {
        Constructed::decode(data, bcder::Mode::Ber, |cons| Self::decode(cons))
    }

    pub fn decode<S: Source>(cons: &mut Constructed<S>) -> Result<Self, S::Err> {
        cons.take_sequence(|cons| {
            let oid = Oid::take_from(cons)?;

            if oid != OID_ID_SIGNED_DATA {
                return Err(Malformed.into());
            }

            cons.take_constructed_if(Tag::CTX_0, Self::take_from)
        })
    }

    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, S::Err> {
        cons.take_sequence(|cons| {
            let version = CmsVersion::take_from(cons)?;
            let digest_algorithms = DigestAlgorithmIdentifiers::take_from(cons)?;
            let content_info = EncapsulatedContentInfo::take_from(cons)?;
            let certificates =
                cons.take_opt_constructed_if(Tag::CTX_0, |cons| CertificateSet::take_from(cons))?;
            let crls = cons.take_opt_constructed_if(Tag::CTX_1, |cons| {
                RevocationInfoChoices::take_from(cons)
            })?;
            let signer_infos = SignerInfos::take_from(cons)?;

            Ok(Self {
                version,
                digest_algorithms,
                content_info,
                certificates,
                crls,
                signer_infos,
            })
        })
    }

    pub fn encode_ref(&self) -> impl Values + '_ {
        encode::sequence((
            OID_ID_SIGNED_DATA.encode_ref(),
            encode::sequence_as(
                Tag::CTX_0,
                encode::sequence((
                    self.version.encode(),
                    self.digest_algorithms.encode_ref(),
                    self.content_info.encode_ref(),
                    if let Some(certs) = self.certificates.as_ref() {
                        Some(certs.encode_ref_as(Tag::CTX_0))
                    } else {
                        None
                    },
                    // TODO crls.
                    self.signer_infos.encode_ref(),
                )),
            ),
        ))
    }
}

/// Digest algorithm identifiers.
///
/// ```ASN.1
/// DigestAlgorithmIdentifiers ::= SET OF DigestAlgorithmIdentifier
/// ```
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DigestAlgorithmIdentifiers(Vec<DigestAlgorithmIdentifier>);

impl Deref for DigestAlgorithmIdentifiers {
    type Target = Vec<DigestAlgorithmIdentifier>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for DigestAlgorithmIdentifiers {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl DigestAlgorithmIdentifiers {
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, S::Err> {
        cons.take_set(|cons| {
            let mut identifiers = Vec::new();

            while let Some(identifier) = AlgorithmIdentifier::take_opt_from(cons)? {
                identifiers.push(identifier);
            }

            Ok(Self(identifiers))
        })
    }

    pub fn encode_ref(&self) -> impl Values + '_ {
        encode::set(&self.0)
    }
}

pub type DigestAlgorithmIdentifier = AlgorithmIdentifier;

/// Signer infos.
///
/// ```ASN.1
/// SignerInfos ::= SET OF SignerInfo
/// ```
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SignerInfos(Vec<SignerInfo>);

impl Deref for SignerInfos {
    type Target = Vec<SignerInfo>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for SignerInfos {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl SignerInfos {
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, S::Err> {
        cons.take_set(|cons| {
            let mut infos = Vec::new();

            while let Some(info) = SignerInfo::take_opt_from(cons)? {
                infos.push(info);
            }

            Ok(Self(infos))
        })
    }

    pub fn encode_ref(&self) -> impl Values + '_ {
        encode::set(&self.0)
    }
}

/// Encapsulated content info.
///
/// ```ASN.1
/// EncapsulatedContentInfo ::= SEQUENCE {
///   eContentType ContentType,
///   eContent [0] EXPLICIT OCTET STRING OPTIONAL }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncapsulatedContentInfo {
    pub content_type: ContentType,
    pub content: Option<OctetString>,
}

impl EncapsulatedContentInfo {
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, S::Err> {
        cons.take_sequence(|cons| {
            let content_type = ContentType::take_from(cons)?;
            let content =
                cons.take_opt_constructed_if(Tag::CTX_0, |cons| OctetString::take_from(cons))?;

            Ok(Self {
                content_type,
                content,
            })
        })
    }

    pub fn encode_ref(&self) -> impl Values + '_ {
        encode::sequence((
            self.content_type.encode_ref(),
            if let Some(content) = self.content.as_ref() {
                Some(encode::sequence_as(Tag::CTX_0, content.encode_ref()))
            } else {
                None
            },
        ))
    }
}

/// Per-signer information.
///
/// ```ASN.1
/// SignerInfo ::= SEQUENCE {
///   version CMSVersion,
///   sid SignerIdentifier,
///   digestAlgorithm DigestAlgorithmIdentifier,
///   signedAttrs [0] IMPLICIT SignedAttributes OPTIONAL,
///   signatureAlgorithm SignatureAlgorithmIdentifier,
///   signature SignatureValue,
///   unsignedAttrs [1] IMPLICIT UnsignedAttributes OPTIONAL }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignerInfo {
    pub version: CmsVersion,
    pub sid: SignerIdentifier,
    pub digest_algorithm: DigestAlgorithmIdentifier,
    pub signed_attributes: Option<SignedAttributes>,
    pub signature_algorithm: SignatureAlgorithmIdentifier,
    pub signature: SignatureValue,
    pub unsigned_attributes: Option<UnsignedAttributes>,

    /// Raw bytes backing signed attributes data.
    ///
    /// Does not include constructed tag or length bytes.
    pub signed_attributes_data: Option<Vec<u8>>,
}

impl SignerInfo {
    pub fn take_opt_from<S: Source>(cons: &mut Constructed<S>) -> Result<Option<Self>, S::Err> {
        cons.take_opt_sequence(|cons| Self::from_sequence(cons))
    }

    pub fn from_sequence<S: Source>(cons: &mut Constructed<S>) -> Result<Self, S::Err> {
        let version = CmsVersion::take_from(cons)?;
        let sid = SignerIdentifier::take_from(cons)?;
        let digest_algorithm = DigestAlgorithmIdentifier::take_from(cons)?;
        let signed_attributes = cons.take_opt_constructed_if(Tag::CTX_0, |cons| {
            // RFC 5652 Section 5.3: SignedAttributes MUST be DER encoded, even if the
            // rest of the structure is BER encoded. So buffer all data so we can
            // feed into a new decoder.
            let der = cons.capture_all()?;

            // But wait there's more! The raw data constituting the signed
            // attributes is also digested and used for content/signature
            // verification. Because our DER serialization may not roundtrip
            // losslessly, we stash away a copy of these bytes so they may be
            // referenced as part of verification.
            let der_data = der.as_slice().to_vec();

            Ok((
                Constructed::decode(der.as_slice(), bcder::Mode::Der, |cons| {
                    SignedAttributes::take_from_set(cons)
                })?,
                der_data,
            ))
        })?;

        let (signed_attributes, signed_attributes_data) = if let Some((x, y)) = signed_attributes {
            (Some(x), Some(y))
        } else {
            (None, None)
        };

        let signature_algorithm = SignatureAlgorithmIdentifier::take_from(cons)?;
        let signature = SignatureValue::take_from(cons)?;
        let unsigned_attributes = cons
            .take_opt_constructed_if(Tag::CTX_1, |cons| UnsignedAttributes::take_from_set(cons))?;

        Ok(Self {
            version,
            sid,
            digest_algorithm,
            signed_attributes,
            signature_algorithm,
            signature,
            unsigned_attributes,
            signed_attributes_data,
        })
    }

    pub fn encode_ref(&self) -> impl Values + '_ {
        encode::sequence((
            u8::from(self.version).encode(),
            &self.sid,
            self.digest_algorithm.encode_ref(),
            if let Some(attrs) = self.signed_attributes.as_ref() {
                Some(attrs.encode_ref_as(Tag::CTX_0))
            } else {
                None
            },
            self.signature_algorithm.encode_ref(),
            self.signature.encode_ref(),
            if let Some(attrs) = self.unsigned_attributes.as_ref() {
                Some(attrs.encode_ref_as(Tag::CTX_1))
            } else {
                None
            },
        ))
    }

    /// Obtain content representing the signed attributes data to be digested.
    ///
    /// Computing the content to go into the digest calculation is nuanced.
    /// From RFC 5652:
    ///
    ///    The result of the message digest calculation process depends on
    ///    whether the signedAttrs field is present.  When the field is absent,
    ///    the result is just the message digest of the content as described
    ///    above.  When the field is present, however, the result is the message
    ///    digest of the complete DER encoding of the SignedAttrs value
    ///    contained in the signedAttrs field.  Since the SignedAttrs value,
    ///    when present, must contain the content-type and the message-digest
    ///    attributes, those values are indirectly included in the result.  The
    ///    content-type attribute MUST NOT be included in a countersignature
    ///    unsigned attribute as defined in Section 11.4.  A separate encoding
    ///    of the signedAttrs field is performed for message digest calculation.
    ///    The IMPLICIT [0] tag in the signedAttrs is not used for the DER
    ///    encoding, rather an EXPLICIT SET OF tag is used.  That is, the DER
    ///    encoding of the EXPLICIT SET OF tag, rather than of the IMPLICIT [0]
    ///    tag, MUST be included in the message digest calculation along with
    ///    the length and content octets of the SignedAttributes value.
    ///
    /// A few things to note here:
    ///
    /// * We must ensure DER (not BER) encoding of the entire SignedAttrs values.
    /// * The SignedAttr tag must use EXPLICIT SET OF instead of IMPLICIT [0],
    ///   so default encoding is not appropriate.
    /// * If this instance came into existence via a parse, we stashed away the
    ///   raw bytes constituting SignedAttributes to ensure we can do a lossless
    ///   copy.
    pub fn signed_attributes_digested_content(&self) -> Result<Option<Vec<u8>>, std::io::Error> {
        if let Some(signed_attributes) = &self.signed_attributes {
            if let Some(existing_data) = &self.signed_attributes_data {
                // +8 should be enough for tag + length.
                let mut buffer = Vec::with_capacity(existing_data.len() + 8);
                // EXPLICIT SET OF.
                buffer.write_all(&[0x31])?;

                // Length isn't exported by bcder :/ So do length encoding manually.
                if existing_data.len() < 0x80 {
                    buffer.write_all(&[existing_data.len() as u8])?;
                } else if existing_data.len() < 0x100 {
                    buffer.write_all(&[0x81, existing_data.len() as u8])?;
                } else if existing_data.len() < 0x10000 {
                    buffer.write_all(&[
                        0x82,
                        (existing_data.len() >> 8) as u8,
                        existing_data.len() as u8,
                    ])?;
                } else if existing_data.len() < 0x1000000 {
                    buffer.write_all(&[
                        0x83,
                        (existing_data.len() >> 16) as u8,
                        (existing_data.len() >> 8) as u8,
                        existing_data.len() as u8,
                    ])?;
                } else {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "signed attributes length too long",
                    ));
                }

                buffer.write_all(existing_data)?;

                Ok(Some(buffer))
            } else {
                // No existing copy present. Serialize from raw data structures.
                let mut der = Vec::new();
                signed_attributes
                    .encode_ref()
                    .write_encoded(bcder::Mode::Der, &mut der)?;

                Ok(Some(der))
            }
        } else {
            Ok(None)
        }
    }
}

impl Values for SignerInfo {
    fn encoded_len(&self, mode: Mode) -> usize {
        self.encode_ref().encoded_len(mode)
    }

    fn write_encoded<W: Write>(&self, mode: Mode, target: &mut W) -> Result<(), std::io::Error> {
        self.encode_ref().write_encoded(mode, target)
    }
}

/// Identifies the signer.
///
/// ```ASN.1
/// SignerIdentifier ::= CHOICE {
///   issuerAndSerialNumber IssuerAndSerialNumber,
///   subjectKeyIdentifier [0] SubjectKeyIdentifier }
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SignerIdentifier {
    IssuerAndSerialNumber(IssuerAndSerialNumber),
    SubjectKeyIdentifier(SubjectKeyIdentifier),
}

impl SignerIdentifier {
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, S::Err> {
        if let Some(identifier) =
            cons.take_opt_constructed_if(Tag::CTX_0, |cons| SubjectKeyIdentifier::take_from(cons))?
        {
            Ok(Self::SubjectKeyIdentifier(identifier))
        } else {
            Ok(Self::IssuerAndSerialNumber(
                IssuerAndSerialNumber::take_from(cons)?,
            ))
        }
    }
}

impl Values for SignerIdentifier {
    fn encoded_len(&self, mode: Mode) -> usize {
        match self {
            Self::IssuerAndSerialNumber(v) => v.encode_ref().encoded_len(mode),
            Self::SubjectKeyIdentifier(v) => v.encode_ref_as(Tag::CTX_0).encoded_len(mode),
        }
    }

    fn write_encoded<W: Write>(&self, mode: Mode, target: &mut W) -> Result<(), std::io::Error> {
        match self {
            Self::IssuerAndSerialNumber(v) => v.encode_ref().write_encoded(mode, target),
            Self::SubjectKeyIdentifier(v) => {
                v.encode_ref_as(Tag::CTX_0).write_encoded(mode, target)
            }
        }
    }
}

/// Signed attributes.
///
/// ```ASN.1
/// SignedAttributes ::= SET SIZE (1..MAX) OF Attribute
/// ```
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SignedAttributes(Vec<Attribute>);

impl Deref for SignedAttributes {
    type Target = Vec<Attribute>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for SignedAttributes {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl SignedAttributes {
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, S::Err> {
        cons.take_set(|cons| Self::take_from_set(cons))
    }

    pub fn take_from_set<S: Source>(cons: &mut Constructed<S>) -> Result<Self, S::Err> {
        let mut attributes = Vec::new();

        while let Some(attribute) = Attribute::take_opt_from(cons)? {
            attributes.push(attribute);
        }

        Ok(Self(attributes))
    }

    pub fn encode_ref(&self) -> impl Values + '_ {
        encode::set(encode::slice(&self.0, |x| x.clone().encode()))
    }

    pub fn encode_ref_as(&self, tag: Tag) -> impl Values + '_ {
        encode::set_as(tag, encode::slice(&self.0, |x| x.clone().encode()))
    }
}

/// Unsigned attributes.
///
/// ```ASN.1
/// UnsignedAttributes ::= SET SIZE (1..MAX) OF Attribute
/// ```
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UnsignedAttributes(Vec<Attribute>);

impl Deref for UnsignedAttributes {
    type Target = Vec<Attribute>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for UnsignedAttributes {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl UnsignedAttributes {
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, S::Err> {
        cons.take_set(|cons| Self::take_from_set(cons))
    }

    pub fn take_from_set<S: Source>(cons: &mut Constructed<S>) -> Result<Self, S::Err> {
        let mut attributes = Vec::new();

        while let Some(attribute) = Attribute::take_opt_from(cons)? {
            attributes.push(attribute);
        }

        Ok(Self(attributes))
    }

    pub fn encode_ref_as(&self, tag: Tag) -> impl Values + '_ {
        encode::set_as(tag, encode::slice(&self.0, |x| x.clone().encode()))
    }
}

/// A single attribute.
///
/// ```ASN.1
/// Attribute ::= SEQUENCE {
///   attrType OBJECT IDENTIFIER,
///   attrValues SET OF AttributeValue }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Attribute {
    pub typ: Oid,
    pub values: Vec<AttributeValue>,
}

impl Attribute {
    pub fn take_opt_from<S: Source>(cons: &mut Constructed<S>) -> Result<Option<Self>, S::Err> {
        cons.take_opt_sequence(|cons| {
            let typ = Oid::take_from(cons)?;

            let values = cons.take_set(|cons| {
                let mut values = Vec::new();

                while let Some(value) = AttributeValue::take_opt_from(cons)? {
                    values.push(value);
                }

                Ok(values)
            })?;

            Ok(Self { typ, values })
        })
    }

    pub fn encode_ref(&self) -> impl Values + '_ {
        encode::sequence((self.typ.encode_ref(), encode::set(&self.values)))
    }

    pub fn encode(self) -> impl Values {
        encode::sequence((self.typ.encode(), encode::set(self.values)))
    }
}

#[derive(Clone, Debug)]
pub struct AttributeValue(Captured);

impl AttributeValue {
    /// Construct a new instance from captured data.
    pub fn new(captured: Captured) -> Self {
        Self(captured)
    }

    pub fn take_opt_from<S: Source>(cons: &mut Constructed<S>) -> Result<Option<Self>, S::Err> {
        let captured = cons.capture_all()?;

        if captured.is_empty() {
            Ok(None)
        } else {
            Ok(Some(Self(captured)))
        }
    }
}

impl Values for AttributeValue {
    fn encoded_len(&self, mode: Mode) -> usize {
        self.0.encoded_len(mode)
    }

    fn write_encoded<W: Write>(&self, mode: Mode, target: &mut W) -> Result<(), std::io::Error> {
        self.0.write_encoded(mode, target)
    }
}

impl Deref for AttributeValue {
    type Target = Captured;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for AttributeValue {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl PartialEq for AttributeValue {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_slice() == other.0.as_slice()
    }
}

impl Eq for AttributeValue {}

pub type SignatureValue = OctetString;

/// Enveloped-data content type.
///
/// ```ASN.1
/// EnvelopedData ::= SEQUENCE {
///   version CMSVersion,
///   originatorInfo [0] IMPLICIT OriginatorInfo OPTIONAL,
///   recipientInfos RecipientInfos,
///   encryptedContentInfo EncryptedContentInfo,
///   unprotectedAttrs [1] IMPLICIT UnprotectedAttributes OPTIONAL }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnvelopedData {
    pub version: CmsVersion,
    pub originator_info: Option<OriginatorInfo>,
    pub recipient_infos: RecipientInfos,
    pub encrypted_content_info: EncryptedContentInfo,
    pub unprotected_attributes: Option<UnprotectedAttributes>,
}

/// Originator info.
///
/// ```ASN.1
/// OriginatorInfo ::= SEQUENCE {
///   certs [0] IMPLICIT CertificateSet OPTIONAL,
///   crls [1] IMPLICIT RevocationInfoChoices OPTIONAL }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OriginatorInfo {
    pub certs: Option<CertificateSet>,
    pub crls: Option<RevocationInfoChoices>,
}

pub type RecipientInfos = Vec<RecipientInfo>;

/// Encrypted content info.
///
/// ```ASN.1
/// EncryptedContentInfo ::= SEQUENCE {
///   contentType ContentType,
///   contentEncryptionAlgorithm ContentEncryptionAlgorithmIdentifier,
///   encryptedContent [0] IMPLICIT EncryptedContent OPTIONAL }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncryptedContentInfo {
    pub content_type: ContentType,
    pub content_encryption_algorithms: ContentEncryptionAlgorithmIdentifier,
    pub encrypted_content: Option<EncryptedContent>,
}

pub type EncryptedContent = OctetString;

pub type UnprotectedAttributes = Vec<Attribute>;

/// Recipient info.
///
/// ```ASN.1
/// RecipientInfo ::= CHOICE {
///   ktri KeyTransRecipientInfo,
///   kari [1] KeyAgreeRecipientInfo,
///   kekri [2] KEKRecipientInfo,
///   pwri [3] PasswordRecipientinfo,
///   ori [4] OtherRecipientInfo }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecipientInfo {
    KeyTransRecipientInfo(KeyTransRecipientInfo),
    KeyAgreeRecipientInfo(KeyAgreeRecipientInfo),
    KekRecipientInfo(KekRecipientInfo),
    PasswordRecipientInfo(PasswordRecipientInfo),
    OtherRecipientInfo(OtherRecipientInfo),
}

pub type EncryptedKey = OctetString;

/// Key trans recipient info.
///
/// ```ASN.1
/// KeyTransRecipientInfo ::= SEQUENCE {
///   version CMSVersion,  -- always set to 0 or 2
///   rid RecipientIdentifier,
///   keyEncryptionAlgorithm KeyEncryptionAlgorithmIdentifier,
///   encryptedKey EncryptedKey }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyTransRecipientInfo {
    pub version: CmsVersion,
    pub rid: RecipientIdentifier,
    pub key_encryption_algorithm: KeyEncryptionAlgorithmIdentifier,
    pub encrypted_key: EncryptedKey,
}

/// Recipient identifier.
///
/// ```ASN.1
/// RecipientIdentifier ::= CHOICE {
///   issuerAndSerialNumber IssuerAndSerialNumber,
///   subjectKeyIdentifier [0] SubjectKeyIdentifier }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecipientIdentifier {
    IssuerAndSerialNumber(IssuerAndSerialNumber),
    SubjectKeyIdentifier(SubjectKeyIdentifier),
}

/// Key agreement recipient info.
///
/// ```ASN.1
/// KeyAgreeRecipientInfo ::= SEQUENCE {
///   version CMSVersion,  -- always set to 3
///   originator [0] EXPLICIT OriginatorIdentifierOrKey,
///   ukm [1] EXPLICIT UserKeyingMaterial OPTIONAL,
///   keyEncryptionAlgorithm KeyEncryptionAlgorithmIdentifier,
///   recipientEncryptedKeys RecipientEncryptedKeys }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyAgreeRecipientInfo {
    pub version: CmsVersion,
    pub originator: OriginatorIdentifierOrKey,
    pub ukm: Option<UserKeyingMaterial>,
    pub key_encryption_algorithm: KeyEncryptionAlgorithmIdentifier,
    pub recipient_encrypted_keys: RecipientEncryptedKeys,
}

/// Originator identifier or key.
///
/// ```ASN.1
/// OriginatorIdentifierOrKey ::= CHOICE {
///   issuerAndSerialNumber IssuerAndSerialNumber,
///   subjectKeyIdentifier [0] SubjectKeyIdentifier,
///   originatorKey [1] OriginatorPublicKey }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OriginatorIdentifierOrKey {
    IssuerAndSerialNumber(IssuerAndSerialNumber),
    SubjectKeyIdentifier(SubjectKeyIdentifier),
    OriginatorKey(OriginatorPublicKey),
}

/// Originator public key.
///
/// ```ASN.1
/// OriginatorPublicKey ::= SEQUENCE {
///   algorithm AlgorithmIdentifier,
///   publicKey BIT STRING }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OriginatorPublicKey {
    pub algorithm: AlgorithmIdentifier,
    pub public_key: BitString,
}

/// SEQUENCE of RecipientEncryptedKey.
type RecipientEncryptedKeys = Vec<RecipientEncryptedKey>;

/// Recipient encrypted key.
///
/// ```ASN.1
/// RecipientEncryptedKey ::= SEQUENCE {
///   rid KeyAgreeRecipientIdentifier,
///   encryptedKey EncryptedKey }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecipientEncryptedKey {
    pub rid: KeyAgreeRecipientInfo,
    pub encrypted_key: EncryptedKey,
}

/// Key agreement recipient identifier.
///
/// ```ASN.1
/// KeyAgreeRecipientIdentifier ::= CHOICE {
///   issuerAndSerialNumber IssuerAndSerialNumber,
///   rKeyId [0] IMPLICIT RecipientKeyIdentifier }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum KeyAgreeRecipientIdentifier {
    IssuerAndSerialNumber(IssuerAndSerialNumber),
    RKeyId(RecipientKeyIdentifier),
}

/// Recipient key identifier.
///
/// ```ASN.1
/// RecipientKeyIdentifier ::= SEQUENCE {
///   subjectKeyIdentifier SubjectKeyIdentifier,
///   date GeneralizedTime OPTIONAL,
///   other OtherKeyAttribute OPTIONAL }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecipientKeyIdentifier {
    pub subject_key_identifier: SubjectKeyIdentifier,
    pub date: Option<GeneralizedTime>,
    pub other: Option<OtherKeyAttribute>,
}

type SubjectKeyIdentifier = OctetString;

/// Key encryption key recipient info.
///
/// ```ASN.1
/// KEKRecipientInfo ::= SEQUENCE {
///   version CMSVersion,  -- always set to 4
///   kekid KEKIdentifier,
///   keyEncryptionAlgorithm KeyEncryptionAlgorithmIdentifier,
///   encryptedKey EncryptedKey }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KekRecipientInfo {
    pub version: CmsVersion,
    pub kek_id: KekIdentifier,
    pub kek_encryption_algorithm: KeyEncryptionAlgorithmIdentifier,
    pub encrypted_key: EncryptedKey,
}

/// Key encryption key identifier.
///
/// ```ASN.1
/// KEKIdentifier ::= SEQUENCE {
///   keyIdentifier OCTET STRING,
///   date GeneralizedTime OPTIONAL,
///   other OtherKeyAttribute OPTIONAL }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KekIdentifier {
    pub key_identifier: OctetString,
    pub date: Option<GeneralizedTime>,
    pub other: Option<OtherKeyAttribute>,
}

/// Password recipient info.
///
/// ```ASN.1
/// PasswordRecipientInfo ::= SEQUENCE {
///   version CMSVersion,   -- Always set to 0
///   keyDerivationAlgorithm [0] KeyDerivationAlgorithmIdentifier
///                                OPTIONAL,
///   keyEncryptionAlgorithm KeyEncryptionAlgorithmIdentifier,
///   encryptedKey EncryptedKey }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PasswordRecipientInfo {
    pub version: CmsVersion,
    pub key_derivation_algorithm: Option<KeyDerivationAlgorithmIdentifier>,
    pub key_encryption_algorithm: KeyEncryptionAlgorithmIdentifier,
    pub encrypted_key: EncryptedKey,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OtherRecipientInfo {
    pub ori_type: Oid,
    // TODO Any
    pub ori_value: Option<()>,
}

/// Digested data.
///
/// ```ASN.1
/// DigestedData ::= SEQUENCE {
///   version CMSVersion,
///   digestAlgorithm DigestAlgorithmIdentifier,
///   encapContentInfo EncapsulatedContentInfo,
///   digest Digest }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DigestedData {
    pub version: CmsVersion,
    pub digest_algorithm: DigestAlgorithmIdentifier,
    pub content_type: EncapsulatedContentInfo,
    pub digest: Digest,
}

pub type Digest = OctetString;

/// Encrypted data.
///
/// ```ASN.1
/// EncryptedData ::= SEQUENCE {
///   version CMSVersion,
///   encryptedContentInfo EncryptedContentInfo,
///   unprotectedAttrs [1] IMPLICIT UnprotectedAttributes OPTIONAL }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncryptedData {
    pub version: CmsVersion,
    pub encrypted_content_info: EncryptedContentInfo,
    pub unprotected_attributes: Option<UnprotectedAttributes>,
}

/// Authenticated data.
///
/// ```ASN.1
/// AuthenticatedData ::= SEQUENCE {
///   version CMSVersion,
///   originatorInfo [0] IMPLICIT OriginatorInfo OPTIONAL,
///   recipientInfos RecipientInfos,
///   macAlgorithm MessageAuthenticationCodeAlgorithm,
///   digestAlgorithm [1] DigestAlgorithmIdentifier OPTIONAL,
///   encapContentInfo EncapsulatedContentInfo,
///   authAttrs [2] IMPLICIT AuthAttributes OPTIONAL,
///   mac MessageAuthenticationCode,
///   unauthAttrs [3] IMPLICIT UnauthAttributes OPTIONAL }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthenticatedData {
    pub version: CmsVersion,
    pub originator_info: Option<OriginatorInfo>,
    pub recipient_infos: RecipientInfos,
    pub mac_algorithm: MessageAuthenticationCodeAlgorithm,
    pub digest_algorithm: Option<DigestAlgorithmIdentifier>,
    pub content_info: EncapsulatedContentInfo,
    pub authenticated_attributes: Option<AuthAttributes>,
    pub mac: MessageAuthenticationCode,
    pub unauthenticated_attributes: Option<UnauthAttributes>,
}

pub type AuthAttributes = Vec<Attribute>;

pub type UnauthAttributes = Vec<Attribute>;

pub type MessageAuthenticationCode = OctetString;

pub type SignatureAlgorithmIdentifier = AlgorithmIdentifier;

pub type KeyEncryptionAlgorithmIdentifier = AlgorithmIdentifier;

pub type ContentEncryptionAlgorithmIdentifier = AlgorithmIdentifier;

pub type MessageAuthenticationCodeAlgorithm = AlgorithmIdentifier;

pub type KeyDerivationAlgorithmIdentifier = AlgorithmIdentifier;

/// Revocation info choices.
///
/// ```ASN.1
/// RevocationInfoChoices ::= SET OF RevocationInfoChoice
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RevocationInfoChoices(Vec<RevocationInfoChoice>);

impl RevocationInfoChoices {
    pub fn take_from<S: Source>(_cons: &mut Constructed<S>) -> Result<Self, S::Err> {
        Err(Unimplemented.into())
    }
}

/// Revocation info choice.
///
/// ```ASN.1
/// RevocationInfoChoice ::= CHOICE {
///   crl CertificateList,
///   other [1] IMPLICIT OtherRevocationInfoFormat }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RevocationInfoChoice {
    Crl(CertificateList),
    Other(OtherRevocationInfoFormat),
}

/// Other revocation info format.
///
/// ```ASN.1
/// OtherRevocationInfoFormat ::= SEQUENCE {
///   otherRevInfoFormat OBJECT IDENTIFIER,
///   otherRevInfo ANY DEFINED BY otherRevInfoFormat }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OtherRevocationInfoFormat {
    pub other_rev_info_info_format: Oid,
    // TODO Any
    pub other_rev_info: Option<()>,
}

/// Certificate choices.
///
/// ```ASN.1
/// CertificateChoices ::= CHOICE {
///   certificate Certificate,
///   extendedCertificate [0] IMPLICIT ExtendedCertificate, -- Obsolete
///   v1AttrCert [1] IMPLICIT AttributeCertificateV1,       -- Obsolete
///   v2AttrCert [2] IMPLICIT AttributeCertificateV2,
///   other [3] IMPLICIT OtherCertificateFormat }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CertificateChoices {
    Certificate(Box<Certificate>),
    // ExtendedCertificate(ExtendedCertificate),
    // AttributeCertificateV1(AttributeCertificateV1),
    AttributeCertificateV2(Box<AttributeCertificateV2>),
    Other(Box<OtherCertificateFormat>),
}

impl CertificateChoices {
    pub fn take_opt_from<S: Source>(cons: &mut Constructed<S>) -> Result<Option<Self>, S::Err> {
        cons.take_opt_constructed_if(Tag::CTX_0, |_cons| -> Result<(), S::Err> {
            Err(Unimplemented.into())
        })?;
        cons.take_opt_constructed_if(Tag::CTX_1, |_cons| -> Result<(), S::Err> {
            Err(Unimplemented.into())
        })?;

        // TODO these first 2 need methods that parse an already entered SEQUENCE.
        if let Some(certificate) = cons
            .take_opt_constructed_if(Tag::CTX_2, |cons| AttributeCertificateV2::take_from(cons))?
        {
            Ok(Some(Self::AttributeCertificateV2(Box::new(certificate))))
        } else if let Some(certificate) = cons
            .take_opt_constructed_if(Tag::CTX_3, |cons| OtherCertificateFormat::take_from(cons))?
        {
            Ok(Some(Self::Other(Box::new(certificate))))
        } else if let Some(certificate) =
            cons.take_opt_constructed(|_, cons| Certificate::from_sequence(cons))?
        {
            Ok(Some(Self::Certificate(Box::new(certificate))))
        } else {
            Ok(None)
        }
    }

    pub fn encode_ref(&self) -> impl Values + '_ {
        match self {
            Self::Certificate(cert) => cert.encode_ref(),
            Self::AttributeCertificateV2(_) => unimplemented!(),
            Self::Other(_) => unimplemented!(),
        }
    }
}

impl Values for CertificateChoices {
    fn encoded_len(&self, mode: Mode) -> usize {
        self.encode_ref().encoded_len(mode)
    }

    fn write_encoded<W: Write>(&self, mode: Mode, target: &mut W) -> Result<(), std::io::Error> {
        self.encode_ref().write_encoded(mode, target)
    }
}

/// Other certificate format.
///
/// ```ASN.1
/// OtherCertificateFormat ::= SEQUENCE {
///   otherCertFormat OBJECT IDENTIFIER,
///   otherCert ANY DEFINED BY otherCertFormat }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OtherCertificateFormat {
    pub other_cert_format: Oid,
    // TODO Any
    pub other_cert: Option<()>,
}

impl OtherCertificateFormat {
    pub fn take_from<S: Source>(_cons: &mut Constructed<S>) -> Result<Self, S::Err> {
        Err(Unimplemented.into())
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CertificateSet(Vec<CertificateChoices>);

impl Deref for CertificateSet {
    type Target = Vec<CertificateChoices>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for CertificateSet {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl CertificateSet {
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, S::Err> {
        let mut certs = Vec::new();

        while let Some(cert) = CertificateChoices::take_opt_from(cons)? {
            certs.push(cert);
        }

        Ok(Self(certs))
    }

    pub fn encode_ref_as(&self, tag: Tag) -> impl Values + '_ {
        encode::set_as(tag, &self.0)
    }
}

/// Issuer and serial number.
///
/// ```ASN.1
/// IssuerAndSerialNumber ::= SEQUENCE {
///   issuer Name,
///   serialNumber CertificateSerialNumber }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IssuerAndSerialNumber {
    pub issuer: Name,
    pub serial_number: CertificateSerialNumber,
}

impl IssuerAndSerialNumber {
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, S::Err> {
        cons.take_sequence(|cons| {
            let issuer = Name::take_from(cons)?;
            let serial_number = Integer::take_from(cons)?;

            Ok(Self {
                issuer,
                serial_number,
            })
        })
    }

    pub fn encode_ref(&self) -> impl Values + '_ {
        encode::sequence((self.issuer.encode_ref(), (&self.serial_number).encode()))
    }
}

pub type CertificateSerialNumber = Integer;

/// Version number.
///
/// ```ASN.1
/// CMSVersion ::= INTEGER
///                { v0(0), v1(1), v2(2), v3(3), v4(4), v5(5) }
/// ```
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CmsVersion {
    V0 = 0,
    V1 = 1,
    V2 = 2,
    V3 = 3,
    V4 = 4,
    V5 = 5,
}

impl CmsVersion {
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, S::Err> {
        match cons.take_primitive_if(Tag::INTEGER, Integer::i8_from_primitive)? {
            0 => Ok(Self::V0),
            1 => Ok(Self::V1),
            2 => Ok(Self::V2),
            3 => Ok(Self::V3),
            4 => Ok(Self::V4),
            5 => Ok(Self::V5),
            _ => Err(Malformed.into()),
        }
    }

    pub fn encode(self) -> impl Values {
        u8::from(self).encode()
    }
}

impl From<CmsVersion> for u8 {
    fn from(v: CmsVersion) -> u8 {
        match v {
            CmsVersion::V0 => 0,
            CmsVersion::V1 => 1,
            CmsVersion::V2 => 2,
            CmsVersion::V3 => 3,
            CmsVersion::V4 => 4,
            CmsVersion::V5 => 5,
        }
    }
}

pub type UserKeyingMaterial = OctetString;

/// Other key attribute.
///
/// ```ASN.1
/// OtherKeyAttribute ::= SEQUENCE {
///   keyAttrId OBJECT IDENTIFIER,
///   keyAttr ANY DEFINED BY keyAttrId OPTIONAL }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OtherKeyAttribute {
    pub key_attribute_id: Oid,
    // TODO Any
    pub key_attribute: Option<()>,
}

pub type ContentType = Oid;

pub type MessageDigest = OctetString;

pub type SigningTime = Time;

/// Time variant.
///
/// ```ASN.1
/// Time ::= CHOICE {
///   utcTime UTCTime,
///   generalizedTime GeneralizedTime }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Time {
    UtcTime(UtcTime),
    GeneralizedTime(GeneralizedTime),
}

impl Time {
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, S::Err> {
        if let Some(utc) =
            cons.take_opt_primitive_if(Tag::UTC_TIME, |prim| UtcTime::from_primitive(prim))?
        {
            Ok(Self::UtcTime(utc))
        } else if let Some(generalized) = cons
            .take_opt_primitive_if(Tag::GENERALIZED_TIME, |prim| {
                GeneralizedTime::from_primitive(prim)
            })?
        {
            Ok(Self::GeneralizedTime(generalized))
        } else {
            Err(Malformed.into())
        }
    }
}

impl From<Time> for chrono::DateTime<chrono::Utc> {
    fn from(t: Time) -> Self {
        match t {
            Time::UtcTime(utc) => *utc,
            Time::GeneralizedTime(gt) => *gt,
        }
    }
}

pub type CounterSignature = SignerInfo;

pub type AttributeCertificateV2 = AttributeCertificate;
