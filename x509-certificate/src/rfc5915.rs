// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! ASN.1 primitives defined by RFC 5915.

use {
    crate::rfc5480::EcParameters,
    bcder::{
        decode::{Constructed, DecodeError, Source},
        encode::{self, PrimitiveContent, Values},
        BitString, ConstOid, Integer, OctetString, Oid, Tag,
    },
};

/// Named curve parameters for elliptic curve private key.
///
/// 1.3.6.1.5.5.7.0.56
pub const OID_NAMED_CURVE_PARAMETERS: ConstOid = Oid(&[43, 6, 1, 5, 5, 7, 0, 56]);

/// Elliptic curve private key.
///
/// ```ASN.1
/// ECPrivateKey ::= SEQUENCE {
///   version        INTEGER { ecPrivkeyVer1(1) } (ecPrivkeyVer1),
///   privateKey     OCTET STRING,
///   parameters [0] ECParameters {{ NamedCurve }} OPTIONAL,
///   publicKey  [1] BIT STRING OPTIONAL
/// }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EcPrivateKey {
    pub version: Integer,
    pub private_key: OctetString,
    pub parameters: Option<EcParameters>,
    pub public_key: Option<BitString>,
}

impl EcPrivateKey {
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, DecodeError<S::Error>> {
        cons.take_sequence(|cons| {
            let version = Integer::take_from(cons)?;
            let private_key = OctetString::take_from(cons)?;
            let parameters =
                cons.take_opt_constructed_if(Tag::CTX_0, |cons| EcParameters::take_from(cons))?;
            let public_key =
                cons.take_opt_constructed_if(Tag::CTX_1, |cons| BitString::take_from(cons))?;

            Ok(Self {
                version,
                private_key,
                parameters,
                public_key,
            })
        })
    }

    pub fn encode_ref(&self) -> impl Values + '_ {
        encode::sequence((
            self.version.encode(),
            self.private_key.encode_ref(),
            self.parameters
                .as_ref()
                .map(|parameters| parameters.encode_ref_as(Tag::CTX_0)),
            self.public_key
                .as_ref()
                .map(|public_key| public_key.encode_ref_as(Tag::CTX_1)),
        ))
    }
}
