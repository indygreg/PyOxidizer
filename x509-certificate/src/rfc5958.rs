// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! ASN.1 primitives from RFC 5958.

use {
    crate::{rfc5280::AlgorithmIdentifier, rfc5652::Attribute, rfc5915::EcPrivateKey},
    bcder::{
        decode::{Constructed, DecodeError, IntoSource, Source},
        encode::{self, PrimitiveContent, Values},
        BitString, Integer, Mode, OctetString, Tag,
    },
    std::ops::{Deref, DerefMut},
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
    pub attributes: Option<Attributes>,
    pub public_key: Option<PublicKey>,
}

impl OneAsymmetricKey {
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, DecodeError<S::Error>> {
        cons.take_sequence(|cons| {
            let version = Version::take_from(cons)?;
            let private_key_algorithm = PrivateKeyAlgorithmIdentifier::take_from(cons)?;
            let private_key = PrivateKey::take_from(cons)?;
            let attributes = cons.take_opt_constructed_if(Tag::CTX_0, |cons| {
                let mut attributes = Attributes::default();

                while let Some(attribute) = Attribute::take_opt_from(cons)? {
                    attributes.push(attribute);
                }

                Ok(attributes)
            })?;
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
            &self.private_key_algorithm,
            self.private_key.encode_ref(),
            self.attributes
                .as_ref()
                .map(|attrs| attrs.encode_ref_as(Tag::CTX_0)),
            self.public_key
                .as_ref()
                .map(|public_key| public_key.encode_ref()),
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
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, DecodeError<S::Error>> {
        match cons.take_primitive_if(Tag::INTEGER, Integer::i8_from_primitive)? {
            0 => Ok(Self::V1),
            1 => Ok(Self::V2),
            _ => Err(cons.content_err("unexpected Version value")),
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
/// This is actually an [EcPrivateKey] stored as an OctetString.
pub type PrivateKey = OctetString;

impl TryFrom<&PrivateKey> for EcPrivateKey {
    type Error = DecodeError<std::convert::Infallible>;

    fn try_from(v: &PrivateKey) -> Result<Self, Self::Error> {
        let source = v.clone().into_source();

        Constructed::decode(
            v.as_slice()
                .ok_or_else(|| source.content_err("missing private key data"))?,
            Mode::Der,
            |cons| EcPrivateKey::take_from(cons),
        )
    }
}

/// Public key data.
pub type PublicKey = BitString;

/// Algorithm identifier for the private key.
pub type PrivateKeyAlgorithmIdentifier = AlgorithmIdentifier;

/// Attributes.
///
/// ```asn.1
/// Attributes ::= SET OF Attribute { { OneAsymmetricKeyAttributes } }
/// ```
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Attributes(Vec<Attribute>);

impl Deref for Attributes {
    type Target = Vec<Attribute>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Attributes {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Attributes {
    pub fn encode_ref_as(&self, tag: Tag) -> impl Values + '_ {
        encode::set_as(tag, encode::slice(&self.0, |x| x.clone().encode()))
    }
}

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
