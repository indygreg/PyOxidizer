// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! ASN.1 primitives from RFC 5958.

use {
    crate::asn1::{rfc5280::AlgorithmIdentifier, rfc5915::EcPrivateKey},
    bcder::{
        decode::{Constructed, Malformed, Source, Unimplemented},
        encode::{self, PrimitiveContent, Values},
        BitString, Integer, Mode, OctetString, Tag,
    },
    std::convert::TryFrom,
};

/// A single asymmetric key.
///
/// ```ASN.1
/// OneAsymmetricKey ::= SEQUENCE {
///   version                   Version,
///   privateKeyAlgorithm       PrivateKeyAlgorithmIdentifier,
///   privateKey                PrivateKey,
///   attributes            [0] Attributes OPTIONAL,
///   ...,
///   [[2: publicKey        [1] PublicKey OPTIONAL ]],
///   ...
/// }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OneAsymmetricKey {
    pub version: Version,
    pub private_key_algorithm: PrivateKeyAlgorithmIdentifier,
    pub private_key: PrivateKey,
    // TODO support.
    pub attributes: Option<()>,
    pub public_key: Option<PublicKey>,
}

impl OneAsymmetricKey {
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, S::Err> {
        cons.take_sequence(|cons| {
            let version = Version::take_from(cons)?;
            let private_key_algorithm = PrivateKeyAlgorithmIdentifier::take_from(cons)?;
            let private_key = PrivateKey::take_from(cons)?;
            let attributes =
                cons.take_opt_constructed_if(Tag::CTX_0, |_| Err(Unimplemented.into()))?;
            let public_key =
                cons.take_opt_constructed_if(Tag::CTX_1, |cons| BitString::take_from(cons))?;

            Ok(Self {
                version,
                private_key_algorithm,
                private_key,
                attributes,
                public_key,
            })
        })
    }

    pub fn encode_ref(&self) -> impl Values + '_ {
        encode::sequence((
            self.version.encode(),
            self.private_key_algorithm.encode_ref(),
            self.private_key.encode_ref(),
            if let Some(attrs) = &self.attributes {
                Some(attrs.encode_ref_as(Tag::CTX_0))
            } else {
                None
            },
            if let Some(public_key) = &self.public_key {
                Some(public_key.encode_ref())
            } else {
                None
            },
        ))
    }
}

/// Version enumeration.
///
/// ```ASN.1
/// Version ::= INTEGER { v1(0), v2(1) } (v1, ..., v2)
/// ```
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Version {
    V1 = 0,
    V2 = 1,
}

impl Version {
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, S::Err> {
        match cons.take_primitive_if(Tag::INTEGER, Integer::i8_from_primitive)? {
            0 => Ok(Self::V1),
            1 => Ok(Self::V2),
            _ => Err(Malformed.into()),
        }
    }

    pub fn encode(self) -> impl Values {
        u8::from(self).encode()
    }
}

impl From<Version> for u8 {
    fn from(v: Version) -> u8 {
        match v {
            Version::V1 => 0,
            Version::V2 => 1,
        }
    }
}

/// Private key data.
///
/// This is actually an [crate::asn1::rfc5915::EcPrivateKey] stored as an
/// OctetString.
pub type PrivateKey = OctetString;

impl TryFrom<&PrivateKey> for EcPrivateKey {
    type Error = bcder::decode::Error;

    fn try_from(v: &PrivateKey) -> Result<Self, Self::Error> {
        Constructed::decode(v.as_slice().ok_or(Malformed)?, Mode::Der, |cons| {
            EcPrivateKey::take_from(cons)
        })
    }
}

/// Public key data.
pub type PublicKey = BitString;

/// Algorithm identifier for the private key.
pub type PrivateKeyAlgorithmIdentifier = AlgorithmIdentifier;

#[cfg(test)]
mod test {
    use {super::*, bcder::Mode};

    #[test]
    fn parse_generated_cert() {
        let rng = ring::rand::SystemRandom::new();

        let doc = ring::signature::EcdsaKeyPair::generate_pkcs8(
            &ring::signature::ECDSA_P256_SHA256_ASN1_SIGNING,
            &rng,
        )
        .unwrap();

        ring::signature::EcdsaKeyPair::from_pkcs8(
            &ring::signature::ECDSA_P256_SHA256_ASN1_SIGNING,
            doc.as_ref(),
        )
        .unwrap();

        let key = Constructed::decode(doc.as_ref(), Mode::Der, |cons| {
            OneAsymmetricKey::take_from(cons)
        })
        .unwrap();

        let private_key = EcPrivateKey::try_from(&key.private_key).unwrap();
        assert_eq!(private_key.version, Integer::from(1));
        assert!(private_key.parameters.is_none());
    }
}
