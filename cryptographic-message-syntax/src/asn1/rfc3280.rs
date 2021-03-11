// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    bcder::{
        decode::{Constructed, Source},
        encode,
        encode::{PrimitiveContent, Values},
        string::{Ia5String, PrintableString, Utf8String},
        Captured, Mode, OctetString, Oid,
    },
    std::{
        io::Write,
        ops::{Deref, DerefMut},
    },
};

pub type GeneralNames = Vec<GeneralName>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GeneralName {
    OtherName(AnotherName),
    Rfc822Name(Ia5String),
    DnsName(Ia5String),
    // TODO implement.
    // X400Address(OrAddress),
    DirectoryName(Name),
    EdiPartyName(EdiPartyName),
    UniformResourceIdentifier(Ia5String),
    IpAddress(OctetString),
    RegisteredId(Oid),
}

/// A reference to another name.
///
/// ```ASN.1
/// AnotherName ::= SEQUENCE {
///   type-id    OBJECT IDENTIFIER,
///   value      [0] EXPLICIT ANY DEFINED BY type-id }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnotherName {
    pub type_id: Oid,
    // TODO Any
    pub value: Option<()>,
}

/// EDI party name.
///
/// ```ASN.1
/// EDIPartyName ::= SEQUENCE {
///   nameAssigner            [0]     DirectoryString OPTIONAL,
///   partyName               [1]     DirectoryString }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EdiPartyName {
    pub name_assigner: Option<DirectoryString>,
    pub party_name: DirectoryString,
}

/// Directory string.
///
/// ```ASN.1
/// DirectoryString ::= CHOICE {
///       teletexString           TeletexString (SIZE (1..MAX)),
///       printableString         PrintableString (SIZE (1..MAX)),
///       universalString         UniversalString (SIZE (1..MAX)),
///       utf8String              UTF8String (SIZE (1..MAX)),
///       bmpString               BMPString (SIZE (1..MAX)) }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DirectoryString {
    // TODO implement.
    TeletexString,
    PrintableString(PrintableString),
    // TODO implement.
    UniversalString,
    Utf8String(Utf8String),
    // TODO implement.
    BmpString,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Name {
    RdnSequence(RdnSequence),
}

impl Name {
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, S::Err> {
        Ok(Self::RdnSequence(RdnSequence::take_from(cons)?))
    }

    pub fn encode_ref(&self) -> impl Values + '_ {
        match self {
            Self::RdnSequence(seq) => seq.encode_ref(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RdnSequence(Vec<RelativeDistinguishedName>);

impl Deref for RdnSequence {
    type Target = Vec<RelativeDistinguishedName>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for RdnSequence {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl RdnSequence {
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, S::Err> {
        cons.take_sequence(|cons| {
            let mut values = Vec::new();

            while let Some(value) = RelativeDistinguishedName::take_opt_from(cons)? {
                values.push(value);
            }

            Ok(Self(values))
        })
    }

    pub fn encode_ref(&self) -> impl Values + '_ {
        encode::sequence(&self.0)
    }
}

pub type DistinguishedName = RdnSequence;

/// Relative distinguished name.
///
/// ```ASN.1
/// RelativeDistinguishedName ::=
///   SET OF AttributeTypeAndValue
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelativeDistinguishedName(Vec<AttributeTypeAndValue>);

impl Deref for RelativeDistinguishedName {
    type Target = Vec<AttributeTypeAndValue>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for RelativeDistinguishedName {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl RelativeDistinguishedName {
    pub fn take_opt_from<S: Source>(cons: &mut Constructed<S>) -> Result<Option<Self>, S::Err> {
        cons.take_opt_set(|cons| {
            let mut values = Vec::new();

            while let Some(value) = AttributeTypeAndValue::take_opt_from(cons)? {
                values.push(value);
            }

            Ok(Self(values))
        })
    }

    pub fn encode_ref(&self) -> impl Values + '_ {
        encode::set(&self.0)
    }
}

impl Values for RelativeDistinguishedName {
    fn encoded_len(&self, mode: Mode) -> usize {
        self.encode_ref().encoded_len(mode)
    }

    fn write_encoded<W: Write>(&self, mode: Mode, target: &mut W) -> Result<(), std::io::Error> {
        self.encode_ref().write_encoded(mode, target)
    }
}

/// Attribute type and its value.
///
/// ```ASN.1
/// AttributeTypeAndValue ::= SEQUENCE {
///   type     AttributeType,
///   value    AttributeValue }
/// ```
#[derive(Clone, Debug)]
pub struct AttributeTypeAndValue {
    pub typ: AttributeType,
    pub value: AttributeValue,
}

impl AttributeTypeAndValue {
    pub fn take_opt_from<S: Source>(cons: &mut Constructed<S>) -> Result<Option<Self>, S::Err> {
        cons.take_opt_sequence(|cons| {
            let typ = AttributeType::take_from(cons)?;
            let value = cons.capture_all()?;

            Ok(Self { typ, value })
        })
    }

    pub fn encode_ref(&self) -> impl Values + '_ {
        encode::sequence((self.typ.encode_ref(), &self.value))
    }
}

impl PartialEq for AttributeTypeAndValue {
    fn eq(&self, other: &Self) -> bool {
        self.typ == other.typ && self.value.as_slice() == other.value.as_slice()
    }
}

impl Eq for AttributeTypeAndValue {}

impl Values for AttributeTypeAndValue {
    fn encoded_len(&self, mode: Mode) -> usize {
        self.encode_ref().encoded_len(mode)
    }

    fn write_encoded<W: Write>(&self, mode: Mode, target: &mut W) -> Result<(), std::io::Error> {
        self.encode_ref().write_encoded(mode, target)
    }
}

pub type AttributeType = Oid;

pub type AttributeValue = Captured;
