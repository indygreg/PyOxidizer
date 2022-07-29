// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! ASN.1 types defined by RFC 4210.

use bcder::{
    decode::{Constructed, DecodeError, Source},
    encode::{self, Values},
    Tag, Utf8String,
};

/// PKI free text.
///
/// ```ASN.1
/// PKIFreeText ::= SEQUENCE SIZE (1..MAX) OF UTF8String
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PkiFreeText(Vec<Utf8String>);

impl PkiFreeText {
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
        let mut res = vec![];

        while let Some(s) = cons.take_opt_value_if(Tag::UTF8_STRING, |content| {
            Utf8String::from_content(content)
        })? {
            res.push(s);
        }

        Ok(Self(res))
    }

    pub fn encode_ref(&self) -> impl Values + '_ {
        encode::sequence(encode::slice(&self.0, |x| x.clone().encode()))
    }
}
