// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! ASN.1 primitives defined by RFC 8017.

use bcder::{
    decode::{Constructed, DecodeError, Source},
    encode::{self, PrimitiveContent, Values},
    Unsigned,
};

/// RSA Public Key.
///
/// ```ASN.1
/// RSAPublicKey ::= SEQUENCE {
///   modulus           INTEGER,  -- n
///   publicExponent    INTEGER   -- e
/// }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RsaPublicKey {
    pub modulus: Unsigned,
    pub public_exponent: Unsigned,
}

impl RsaPublicKey {
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, DecodeError<S::Error>> {
        cons.take_sequence(|cons| {
            let modulus = Unsigned::take_from(cons)?;
            let public_exponent = Unsigned::take_from(cons)?;

            Ok(Self {
                modulus,
                public_exponent,
            })
        })
    }

    pub fn encode_ref(&self) -> impl Values + '_ {
        encode::sequence((self.modulus.encode(), self.public_exponent.encode()))
    }
}
