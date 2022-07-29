// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! ASN.1 types defined in RFC 5652.
//!
//! Only the types referenced by X.509 certificates are defined here.
//! For the higher-level CMS types, see the `cryptographic-message-syntax`
//! crate.

use {
    bcder::{
        decode::{Constructed, DecodeError, Source},
        encode::{self, PrimitiveContent, Values},
        Captured, Mode, Oid,
    },
    std::{
        fmt::{Debug, Formatter},
        io::Write,
        ops::{Deref, DerefMut},
    },
};

/// A single attribute.
///
/// ```ASN.1
/// Attribute ::= SEQUENCE {
///   attrType OBJECT IDENTIFIER,
///   attrValues SET OF AttributeValue }
/// ```
#[derive(Clone, Eq, PartialEq)]
pub struct Attribute {
    pub typ: Oid,
    pub values: Vec<AttributeValue>,
}

impl Debug for Attribute {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("Attribute");
        s.field("type", &format_args!("{}", self.typ));
        s.field("values", &self.values);
        s.finish()
    }
}

impl Attribute {
    pub fn take_opt_from<S: Source>(
        cons: &mut Constructed<S>,
    ) -> Result<Option<Self>, DecodeError<S::Error>> {
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

#[derive(Clone)]
pub struct AttributeValue(Captured);

impl Debug for AttributeValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "{}",
            hex::encode(self.0.clone().into_bytes().as_ref())
        ))
    }
}

impl AttributeValue {
    /// Construct a new instance from captured data.
    pub fn new(captured: Captured) -> Self {
        Self(captured)
    }

    pub fn take_opt_from<S: Source>(
        cons: &mut Constructed<S>,
    ) -> Result<Option<Self>, DecodeError<S::Error>> {
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
