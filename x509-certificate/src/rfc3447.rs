// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! ASN.1 types defined in RFC 3447.

use {
    crate::rfc5280::AlgorithmIdentifier,
    bcder::{
        decode::{Constructed, Source},
        encode::{self, Values},
        Mode, OctetString,
    },
    std::io::Write,
};

/// Digest information.
///
/// ```asn.1
/// DigestInfo ::= SEQUENCE {
///     digestAlgorithm DigestAlgorithm,
///     digest OCTET STRING
/// }
///
/// DigestAlgorithm ::=
///     AlgorithmIdentifier { {PKCS1-v1-5DigestAlgorithms} }
/// ```
#[derive(Clone)]
pub struct DigestInfo {
    pub algorithm: AlgorithmIdentifier,
    pub digest: OctetString,
}

impl DigestInfo {
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, S::Err> {
        cons.take_sequence(|cons| {
            let algorithm = AlgorithmIdentifier::take_from(cons)?;
            let digest = OctetString::take_from(cons)?;

            Ok(Self { algorithm, digest })
        })
    }

    pub fn encode_ref(&self) -> impl Values + '_ {
        encode::sequence((&self.algorithm, self.digest.encode_ref()))
    }
}

impl Values for DigestInfo {
    fn encoded_len(&self, mode: Mode) -> usize {
        self.encode_ref().encoded_len(mode)
    }

    fn write_encoded<W: Write>(&self, mode: Mode, target: &mut W) -> Result<(), std::io::Error> {
        self.encode_ref().write_encoded(mode, target)
    }
}
