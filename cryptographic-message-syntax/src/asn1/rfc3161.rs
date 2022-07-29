// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! ASN.1 types defined by RFC 3161.

use {
    crate::asn1::{rfc4210::PkiFreeText, rfc5652::ContentInfo},
    bcder::{
        decode::{Constructed, DecodeError, Primitive, Source},
        encode::{self, PrimitiveContent, Values},
        ConstOid, Integer, OctetString, Oid, Tag,
    },
    x509_certificate::{
        asn1time::GeneralizedTime,
        rfc3280::GeneralName,
        rfc5280::{AlgorithmIdentifier, Extensions},
    },
};

/// Content-Type for Time-Stamp Token Info.
///
/// 1.2.840.113549.1.9.16.1.4
pub const OID_CONTENT_TYPE_TST_INFO: ConstOid = Oid(&[42, 134, 72, 134, 247, 13, 1, 9, 16, 1, 4]);

/// id-aa-timeStampToken
///
/// 1.2.840.113549.1.9.16.2.14
pub const OID_TIME_STAMP_TOKEN: ConstOid = Oid(&[42, 134, 72, 134, 247, 13, 1, 9, 16, 2, 14]);

/// A time-stamp request.
///
/// ```ASN.1
/// TimeStampReq ::= SEQUENCE  {
///    version                  INTEGER  { v1(1) },
///    messageImprint           MessageImprint,
///      --a hash algorithm OID and the hash value of the data to be
///      --time-stamped
///    reqPolicy                TSAPolicyId                OPTIONAL,
///    nonce                    INTEGER                    OPTIONAL,
///    certReq                  BOOLEAN                    DEFAULT FALSE,
///    extensions               [0] IMPLICIT Extensions    OPTIONAL  }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimeStampReq {
    pub version: Integer,
    pub message_imprint: MessageImprint,
    pub req_policy: Option<TsaPolicyId>,
    pub nonce: Option<Integer>,
    pub cert_req: Option<bool>,
    pub extensions: Option<Extensions>,
}

impl TimeStampReq {
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, DecodeError<S::Error>> {
        cons.take_sequence(|cons| {
            let version = Integer::take_from(cons)?;
            let message_imprint = MessageImprint::take_from(cons)?;
            let req_policy = TsaPolicyId::take_opt_from(cons)?;
            let nonce =
                cons.take_opt_primitive_if(Tag::INTEGER, |prim| Integer::from_primitive(prim))?;
            let cert_req = cons.take_opt_bool()?;
            let extensions =
                cons.take_opt_constructed_if(Tag::CTX_0, |cons| Extensions::take_from(cons))?;

            Ok(Self {
                version,
                message_imprint,
                req_policy,
                nonce,
                cert_req,
                extensions,
            })
        })
    }

    pub fn encode_ref(&self) -> impl Values + '_ {
        encode::sequence((
            (&self.version).encode(),
            self.message_imprint.encode_ref(),
            self.req_policy
                .as_ref()
                .map(|req_policy| req_policy.encode_ref()),
            self.nonce.as_ref().map(|nonce| nonce.encode()),
            self.cert_req.as_ref().map(|cert_req| cert_req.encode_ref()),
            self.extensions
                .as_ref()
                .map(|extensions| extensions.encode_ref_as(Tag::CTX_0)),
        ))
    }
}

/// Message imprint.
///
/// ```ASN.1
/// MessageImprint ::= SEQUENCE  {
///      hashAlgorithm                AlgorithmIdentifier,
///      hashedMessage                OCTET STRING  }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MessageImprint {
    pub hash_algorithm: AlgorithmIdentifier,
    pub hashed_message: OctetString,
}

impl MessageImprint {
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, DecodeError<S::Error>> {
        cons.take_sequence(|cons| {
            let hash_algorithm = AlgorithmIdentifier::take_from(cons)?;
            let hashed_message = OctetString::take_from(cons)?;

            Ok(Self {
                hash_algorithm,
                hashed_message,
            })
        })
    }

    pub fn encode_ref(&self) -> impl Values + '_ {
        encode::sequence((&self.hash_algorithm, self.hashed_message.encode_ref()))
    }
}

pub type TsaPolicyId = Oid;

/// Time stamp response.
///
/// ```ASN.1
/// TimeStampResp ::= SEQUENCE  {
///      status                  PKIStatusInfo,
///      timeStampToken          TimeStampToken     OPTIONAL  }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimeStampResp {
    pub status: PkiStatusInfo,
    pub time_stamp_token: Option<TimeStampToken>,
}

impl TimeStampResp {
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, DecodeError<S::Error>> {
        cons.take_sequence(|cons| {
            let status = PkiStatusInfo::take_from(cons)?;
            let time_stamp_token = TimeStampToken::take_opt_from(cons)?;

            Ok(Self {
                status,
                time_stamp_token,
            })
        })
    }

    pub fn encode_ref(&self) -> impl Values + '_ {
        encode::sequence((
            self.status.encode_ref(),
            if let Some(time_stamp_token) = &self.time_stamp_token {
                Some(time_stamp_token)
            } else {
                None
            },
        ))
    }
}

/// PKI status info
///
/// ```ASN.1
/// PKIStatusInfo ::= SEQUENCE {
///     status        PKIStatus,
///     statusString  PKIFreeText     OPTIONAL,
///     failInfo      PKIFailureInfo  OPTIONAL  }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PkiStatusInfo {
    pub status: PkiStatus,
    pub status_string: Option<PkiFreeText>,
    pub fail_info: Option<PkiFailureInfo>,
}

impl PkiStatusInfo {
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, DecodeError<S::Error>> {
        cons.take_sequence(|cons| {
            let status = PkiStatus::take_from(cons)?;
            let status_string = PkiFreeText::take_opt_from(cons)?;
            let fail_info = PkiFailureInfo::take_opt_from(cons)?;

            Ok(Self {
                status,
                status_string,
                fail_info,
            })
        })
    }

    pub fn encode_ref(&self) -> impl Values + '_ {
        encode::sequence((
            (&self.status).encode(),
            self.status_string
                .as_ref()
                .map(|status_string| status_string.encode_ref()),
            self.fail_info.as_ref().map(|fail_info| fail_info.encode()),
        ))
    }
}

/// PKI status.
///
/// ```ASN.1
/// PKIStatus ::= INTEGER {
///     granted                (0),
///     -- when the PKIStatus contains the value zero a TimeStampToken, as
///        requested, is present.
///     grantedWithMods        (1),
///      -- when the PKIStatus contains the value one a TimeStampToken,
///        with modifications, is present.
///     rejection              (2),
///     waiting                (3),
///     revocationWarning      (4),
///      -- this message contains a warning that a revocation is
///      -- imminent
///     revocationNotification (5)
///      -- notification that a revocation has occurred   }
///
///     -- When the TimeStampToken is not present
///     -- failInfo indicates the reason why the
///     -- time-stamp request was rejected and
///     -- may be one of the following values.
/// ```
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PkiStatus {
    Granted = 0,
    GrantedWithMods = 1,
    Rejection = 2,
    Waiting = 3,
    RevocationWarning = 4,
    RevocationNotification = 5,
}

impl PkiStatus {
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, DecodeError<S::Error>> {
        match cons.take_primitive_if(Tag::INTEGER, Integer::i8_from_primitive)? {
            0 => Ok(Self::Granted),
            1 => Ok(Self::GrantedWithMods),
            2 => Ok(Self::Rejection),
            3 => Ok(Self::Waiting),
            4 => Ok(Self::RevocationWarning),
            5 => Ok(Self::RevocationNotification),
            _ => Err(cons.content_err("unknown PKIStatus value")),
        }
    }

    pub fn encode(self) -> impl Values {
        u8::from(self).encode()
    }
}

impl From<PkiStatus> for u8 {
    fn from(v: PkiStatus) -> u8 {
        match v {
            PkiStatus::Granted => 0,
            PkiStatus::GrantedWithMods => 1,
            PkiStatus::Rejection => 2,
            PkiStatus::Waiting => 3,
            PkiStatus::RevocationWarning => 4,
            PkiStatus::RevocationNotification => 5,
        }
    }
}

/// PKI failure info.
///
/// ```ASN.1
/// PKIFailureInfo ::= BIT STRING {
///     badAlg               (0),
///       -- unrecognized or unsupported Algorithm Identifier
///     badRequest           (2),
///       -- transaction not permitted or supported
///     badDataFormat        (5),
///       -- the data submitted has the wrong format
///     timeNotAvailable    (14),
///       -- the TSA's time source is not available
///     unacceptedPolicy    (15),
///       -- the requested TSA policy is not supported by the TSA.
///     unacceptedExtension (16),
///       -- the requested extension is not supported by the TSA.
///     addInfoNotAvailable (17)
///       -- the additional information requested could not be understood
///       -- or is not available
///     systemFailure       (25)
///       -- the request cannot be handled due to system failure  }
/// ```
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PkiFailureInfo {
    BadAlg = 0,
    BadRequest = 1,
    BadDataFormat = 5,
    TimeNotAvailable = 14,
    UnacceptedPolicy = 15,
    UnacceptedExtension = 16,
    AddInfoNotAvailable = 17,
    SystemFailure = 25,
}

impl PkiFailureInfo {
    pub fn take_opt_from<S: Source>(
        cons: &mut Constructed<S>,
    ) -> Result<Option<Self>, DecodeError<S::Error>> {
        cons.take_opt_primitive_if(Tag::INTEGER, Self::from_primitive)
    }

    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, DecodeError<S::Error>> {
        cons.take_primitive_if(Tag::INTEGER, Self::from_primitive)
    }

    pub fn from_primitive<S: Source>(
        prim: &mut Primitive<S>,
    ) -> Result<Self, DecodeError<S::Error>> {
        match Integer::i8_from_primitive(prim)? {
            0 => Ok(Self::BadAlg),
            1 => Ok(Self::BadRequest),
            5 => Ok(Self::BadDataFormat),
            14 => Ok(Self::TimeNotAvailable),
            15 => Ok(Self::UnacceptedPolicy),
            16 => Ok(Self::UnacceptedExtension),
            17 => Ok(Self::AddInfoNotAvailable),
            25 => Ok(Self::SystemFailure),
            _ => Err(prim.content_err("Unknown PKIFailureInfo value")),
        }
    }

    pub fn encode(self) -> impl Values {
        u8::from(self).encode()
    }
}

impl From<PkiFailureInfo> for u8 {
    fn from(v: PkiFailureInfo) -> u8 {
        match v {
            PkiFailureInfo::BadAlg => 0,
            PkiFailureInfo::BadRequest => 1,
            PkiFailureInfo::BadDataFormat => 5,
            PkiFailureInfo::TimeNotAvailable => 14,
            PkiFailureInfo::UnacceptedPolicy => 15,
            PkiFailureInfo::UnacceptedExtension => 16,
            PkiFailureInfo::AddInfoNotAvailable => 17,
            PkiFailureInfo::SystemFailure => 25,
        }
    }
}

/// Time stamp token.
///
/// ```ASN.1
/// TimeStampToken ::= ContentInfo
/// ```
pub type TimeStampToken = ContentInfo;

/// Time stamp token info.
///
/// ```ASN.1
/// TSTInfo ::= SEQUENCE  {
///     version                      INTEGER  { v1(1) },
///     policy                       TSAPolicyId,
///     messageImprint               MessageImprint,
///       -- MUST have the same value as the similar field in
///       -- TimeStampReq
///     serialNumber                 INTEGER,
///      -- Time-Stamping users MUST be ready to accommodate integers
///      -- up to 160 bits.
///     genTime                      GeneralizedTime,
///     accuracy                     Accuracy                 OPTIONAL,
///     ordering                     BOOLEAN             DEFAULT FALSE,
///     nonce                        INTEGER                  OPTIONAL,
///       -- MUST be present if the similar field was present
///       -- in TimeStampReq.  In that case it MUST have the same value.
///     tsa                          [0] GeneralName          OPTIONAL,
///     extensions                   [1] IMPLICIT Extensions  OPTIONAL   }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TstInfo {
    pub version: Integer,
    pub policy: TsaPolicyId,
    pub message_imprint: MessageImprint,
    pub serial_number: Integer,
    pub gen_time: GeneralizedTime,
    pub accuracy: Option<Accuracy>,
    pub ordering: Option<bool>,
    pub nonce: Option<Integer>,
    pub tsa: Option<GeneralName>,
    pub extensions: Option<Extensions>,
}

impl TstInfo {
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, DecodeError<S::Error>> {
        cons.take_sequence(|cons| {
            let version = Integer::take_from(cons)?;
            let policy = TsaPolicyId::take_from(cons)?;
            let message_imprint = MessageImprint::take_from(cons)?;
            let serial_number = Integer::take_from(cons)?;
            let gen_time = GeneralizedTime::take_from_allow_fractional_z(cons)?;
            let accuracy = Accuracy::take_opt_from(cons)?;
            let ordering = cons.take_opt_bool()?;
            let nonce =
                cons.take_opt_primitive_if(Tag::INTEGER, |prim| Integer::from_primitive(prim))?;
            let tsa =
                cons.take_opt_constructed_if(Tag::CTX_0, |cons| GeneralName::take_from(cons))?;
            let extensions =
                cons.take_opt_constructed_if(Tag::CTX_1, |cons| Extensions::take_from(cons))?;

            Ok(Self {
                version,
                policy,
                message_imprint,
                serial_number,
                gen_time,
                accuracy,
                ordering,
                nonce,
                tsa,
                extensions,
            })
        })
    }

    pub fn encode_ref(&self) -> impl Values + '_ {
        encode::sequence((
            (&self.version).encode(),
            self.policy.encode_ref(),
            self.message_imprint.encode_ref(),
            (&self.serial_number).encode(),
            self.gen_time.encode_ref(),
            self.accuracy.as_ref().map(|accuracy| accuracy.encode_ref()),
            self.ordering.as_ref().map(|ordering| ordering.encode_ref()),
            self.nonce.as_ref().map(|nonce| nonce.encode()),
            self.tsa
                .as_ref()
                .map(|tsa| tsa.encode_ref().explicit(Tag::CTX_0)),
            self.extensions
                .as_ref()
                .map(|extensions| extensions.encode_ref_as(Tag::CTX_1)),
        ))
    }
}

/// Accuracy
///
/// ```ASN.1
/// Accuracy ::= SEQUENCE {
///                 seconds        INTEGER           OPTIONAL,
///                 millis     [0] INTEGER  (1..999) OPTIONAL,
///                 micros     [1] INTEGER  (1..999) OPTIONAL  }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Accuracy {
    pub seconds: Option<Integer>,
    pub millis: Option<Integer>,
    pub micros: Option<Integer>,
}

impl Accuracy {
    pub fn take_opt_from<S: Source>(
        cons: &mut Constructed<S>,
    ) -> Result<Option<Self>, DecodeError<S::Error>> {
        cons.take_opt_sequence(|cons| Self::from_sequence(cons))
    }

    pub fn from_sequence<S: Source>(
        cons: &mut Constructed<S>,
    ) -> Result<Self, DecodeError<S::Error>> {
        let seconds =
            cons.take_opt_primitive_if(Tag::INTEGER, |prim| Integer::from_primitive(prim))?;
        let millis = cons.take_opt_constructed_if(Tag::CTX_0, |cons| Integer::take_from(cons))?;
        let micros = cons.take_opt_constructed_if(Tag::CTX_1, |cons| Integer::take_from(cons))?;

        Ok(Self {
            seconds,
            millis,
            micros,
        })
    }

    pub fn encode_ref(&self) -> impl Values + '_ {
        encode::sequence((
            self.seconds.as_ref().map(|seconds| seconds.encode()),
            self.millis.as_ref().map(|millis| millis.encode()),
            self.micros.as_ref().map(|micros| micros.encode()),
        ))
    }
}
