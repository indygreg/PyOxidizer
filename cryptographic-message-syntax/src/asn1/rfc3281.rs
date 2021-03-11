// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::asn1::{common::*, rfc3280::*, rfc5280::*},
    bcder::{
        decode::{Constructed, Source, Unimplemented},
        BitString, Oid,
    },
};

/// Attribute certificate.
///
/// ```ASN.1
/// AttributeCertificate ::= SEQUENCE {
///   acinfo               AttributeCertificateInfo,
///   signatureAlgorithm   AlgorithmIdentifier,
///   signatureValue       BIT STRING
/// }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttributeCertificate {
    pub ac_info: AttributeCertificateInfo,
    pub signature_algorithm: AlgorithmIdentifier,
    pub signature_value: BitString,
}

impl AttributeCertificate {
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, S::Err> {
        cons.take_sequence(|cons| {
            let ac_info = AttributeCertificateInfo::take_from(cons)?;
            let signature_algorithm = AlgorithmIdentifier::take_from(cons)?;
            let signature_value = BitString::take_from(cons)?;

            Ok(Self {
                ac_info,
                signature_algorithm,
                signature_value,
            })
        })
    }
}

/// Attribute certificate info.
///
/// ```ASN.1
/// AttributeCertificateInfo ::= SEQUENCE {
///   version              AttCertVersion -- version is v2,
///   holder               Holder,
///   issuer               AttCertIssuer,
///   signature            AlgorithmIdentifier,
///   serialNumber         CertificateSerialNumber,
///   attrCertValidityPeriod   AttCertValidityPeriod,
///   attributes           SEQUENCE OF Attribute,
///   issuerUniqueID       UniqueIdentifier OPTIONAL,
///   extensions           Extensions OPTIONAL
/// }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttributeCertificateInfo {
    pub version: AttCertVersion,
    pub holder: Holder,
    pub issuer: AttCertIssuer,
    pub signature: AlgorithmIdentifier,
    pub serial_number: CertificateSerialNumber,
    pub attr_cert_validity_period: AttCertValidityPeriod,
    pub attributes: Vec<Attribute>,
    pub issuer_unique_ud: Option<UniqueIdentifier>,
    pub extensions: Option<Extensions>,
}

impl AttributeCertificateInfo {
    pub fn take_from<S: Source>(_cons: &mut Constructed<S>) -> Result<Self, S::Err> {
        Err(Unimplemented.into())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttCertVersion {
    V2 = 1,
}

/// Holder
///
/// ```ASN.1
/// Holder ::= SEQUENCE {
///   baseCertificateID   [0] IssuerSerial OPTIONAL,
///     -- the issuer and serial number of
///     -- the holder's Public Key Certificate
///   entityName          [1] GeneralNames OPTIONAL,
///     -- the name of the claimant or role
///   objectDigestInfo    [2] ObjectDigestInfo OPTIONAL
///     -- used to directly authenticate the holder,
///     -- for example, an executable
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Holder {
    pub base_certificate_id: Option<IssuerSerial>,
    pub entity_name: Option<GeneralNames>,
    pub object_digest_info: Option<ObjectDigestInfo>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DigestedObjectType {
    PublicKey = 0,
    PublicKeyCert = 1,
    OtherObjectTypes = 2,
}

/// Object digest info.
///
/// ```ASN.1
/// ObjectDigestInfo ::= SEQUENCE {
///   digestedObjectType  ENUMERATED {
///     publicKey            (0),
///     publicKeyCert        (1),
///     otherObjectTypes     (2) },
///       -- otherObjectTypes MUST NOT
///       -- be used in this profile
///   otherObjectTypeID   OBJECT IDENTIFIER OPTIONAL,
///   digestAlgorithm     AlgorithmIdentifier,
///   objectDigest        BIT STRING
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObjectDigestInfo {
    pub digested_object_type: DigestedObjectType,
    pub other_object_type_id: Oid,
    pub digest_algorithm: AlgorithmIdentifier,
    pub object_digest: BitString,
}

/// Att cert issuer
///
/// ```ASN.1
/// AttCertIssuer ::= CHOICE {
///   v1Form   GeneralNames,  -- MUST NOT be used in this
///                           -- profile
///   v2Form   [0] V2Form     -- v2 only
/// }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AttCertIssuer {
    V1Form(GeneralNames),
    V2Form(Box<V2Form>),
}

/// V2 Form
///
/// ```ASN.1
/// V2Form ::= SEQUENCE {
///   issuerName            GeneralNames  OPTIONAL,
///   baseCertificateID     [0] IssuerSerial  OPTIONAL,
///   objectDigestInfo      [1] ObjectDigestInfo  OPTIONAL
///     -- issuerName MUST be present in this profile
///     -- baseCertificateID and objectDigestInfo MUST NOT
///     -- be present in this profile
/// }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct V2Form {
    pub issuer_name: Option<GeneralNames>,
    pub base_certificate_id: Option<IssuerSerial>,
    pub object_digest_info: Option<ObjectDigestInfo>,
}

/// Issuer serial.
///
/// IssuerSerial  ::=  SEQUENCE {
///   issuer         GeneralNames,
///   serial         CertificateSerialNumber,
///   issuerUID      UniqueIdentifier OPTIONAL
/// }
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IssuerSerial {
    pub issuer: GeneralNames,
    pub serial: CertificateSerialNumber,
    pub issuer_uid: Option<UniqueIdentifier>,
}

/// Att cert validity period
///
/// ```ASN.1
/// AttCertValidityPeriod  ::= SEQUENCE {
///   notBeforeTime  GeneralizedTime,
///   notAfterTime   GeneralizedTime
/// }
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttCertValidityPeriod {
    pub not_before_time: GeneralizedTime,
    pub not_after_time: GeneralizedTime,
}

/// Attribute
///
/// ```ASN.1
/// Attribute ::= SEQUENCE {
///   type      AttributeType,
///   values    SET OF AttributeValue
///     -- at least one value is required
/// }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Attribute {
    pub typ: AttributeType,
    pub values: Vec<AttributeValue>,
}

pub type AttributeType = Oid;

// TODO Any.
pub type AttributeValue = Option<()>;
