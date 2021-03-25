// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use bcder::decode::Error::Malformed;
use {
    bcder::{
        decode::{Constructed, Error::Unimplemented, Source},
        encode,
        encode::{PrimitiveContent, Values},
        string::{Ia5String, PrintableString, Utf8String},
        Captured, Mode, OctetString, Oid, Tag,
    },
    std::{
        io::Write,
        ops::{Deref, DerefMut},
    },
};

pub type GeneralNames = Vec<GeneralName>;

/// General name.
///
/// ```ASN.1
/// GeneralName ::= CHOICE {
///   otherName                       [0]     AnotherName,
///   rfc822Name                      [1]     IA5String,
///   dNSName                         [2]     IA5String,
///   x400Address                     [3]     ORAddress,
///   directoryName                   [4]     Name,
///   ediPartyName                    [5]     EDIPartyName,
///   uniformResourceIdentifier       [6]     IA5String,
///   iPAddress                       [7]     OCTET STRING,
///   registeredID                    [8]     OBJECT IDENTIFIER }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GeneralName {
    OtherName(AnotherName),
    Rfc822Name(Ia5String),
    DnsName(Ia5String),
    X400Address(OrAddress),
    DirectoryName(Name),
    EdiPartyName(EdiPartyName),
    UniformResourceIdentifier(Ia5String),
    IpAddress(OctetString),
    RegisteredId(Oid),
}

impl GeneralName {
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, S::Err> {
        if let Some(name) =
            cons.take_opt_constructed_if(Tag::CTX_0, |cons| AnotherName::take_from(cons))?
        {
            Ok(Self::OtherName(name))
        } else if let Some(name) =
            cons.take_opt_constructed_if(Tag::CTX_1, |cons| Ia5String::take_from(cons))?
        {
            Ok(Self::Rfc822Name(name))
        } else if let Some(name) =
            cons.take_opt_constructed_if(Tag::CTX_2, |cons| Ia5String::take_from(cons))?
        {
            Ok(Self::DnsName(name))
        } else if let Some(name) =
            cons.take_opt_constructed_if(Tag::CTX_3, |cons| OrAddress::take_from(cons))?
        {
            Ok(Self::X400Address(name))
        } else if let Some(name) =
            cons.take_opt_constructed_if(Tag::CTX_4, |cons| Name::take_from(cons))?
        {
            Ok(Self::DirectoryName(name))
        } else if let Some(name) =
            cons.take_opt_constructed_if(Tag::CTX_5, |cons| EdiPartyName::take_from(cons))?
        {
            Ok(Self::EdiPartyName(name))
        } else if let Some(name) =
            cons.take_opt_constructed_if(Tag::CTX_6, |cons| Ia5String::take_from(cons))?
        {
            Ok(Self::UniformResourceIdentifier(name))
        } else if let Some(name) =
            cons.take_opt_constructed_if(Tag::ctx(7), |cons| OctetString::take_from(cons))?
        {
            Ok(Self::IpAddress(name))
        } else if let Some(name) =
            cons.take_opt_constructed_if(Tag::ctx(8), |cons| Oid::take_from(cons))?
        {
            Ok(Self::RegisteredId(name))
        } else {
            Err(Malformed.into())
        }
    }

    pub fn encode_ref(&self) -> impl Values + '_ {
        match self {
            Self::OtherName(name) => (
                Some(name.explicit(Tag::CTX_0)),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            ),
            Self::Rfc822Name(name) => (
                None,
                Some(name.encode_ref_as(Tag::CTX_1)),
                None,
                None,
                None,
                None,
                None,
                None,
            ),
            Self::DnsName(name) => (
                None,
                None,
                Some(name.encode_ref_as(Tag::CTX_2)),
                None,
                None,
                None,
                None,
                None,
            ),
            Self::X400Address(_name) => {
                unimplemented!()
            }
            Self::DirectoryName(name) => (
                None,
                None,
                None,
                Some(name.encode_ref_as(Tag::CTX_4)),
                None,
                None,
                None,
                None,
            ),
            Self::EdiPartyName(name) => (
                None,
                None,
                None,
                None,
                Some(name.encode_ref_as(Tag::CTX_5)),
                None,
                None,
                None,
            ),
            Self::UniformResourceIdentifier(name) => (
                None,
                None,
                None,
                None,
                None,
                Some(name.encode_ref_as(Tag::CTX_6)),
                None,
                None,
            ),
            Self::IpAddress(name) => (
                None,
                None,
                None,
                None,
                None,
                None,
                Some(name.encode_ref_as(Tag::ctx(7))),
                None,
            ),
            Self::RegisteredId(name) => (
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                Some(name.encode_ref_as(Tag::ctx(8))),
            ),
        }
    }
}

/// A reference to another name.
///
/// ```ASN.1
/// AnotherName ::= SEQUENCE {
///   type-id    OBJECT IDENTIFIER,
///   value      [0] EXPLICIT ANY DEFINED BY type-id }
/// ```
#[derive(Clone, Debug)]
pub struct AnotherName {
    pub type_id: Oid,
    pub value: Captured,
}

impl PartialEq for AnotherName {
    fn eq(&self, other: &Self) -> bool {
        self.type_id == other.type_id && self.value.as_slice() == other.value.as_slice()
    }
}

impl Eq for AnotherName {}

impl AnotherName {
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, S::Err> {
        cons.take_sequence(|cons| {
            let type_id = Oid::take_from(cons)?;
            let value = cons.take_constructed_if(Tag::CTX_0, |cons| cons.capture_all())?;

            Ok(Self { type_id, value })
        })
    }
}

impl Values for AnotherName {
    fn encoded_len(&self, mode: Mode) -> usize {
        encode::sequence((self.type_id.encode_ref(), &self.value)).encoded_len(mode)
    }

    fn write_encoded<W: Write>(&self, mode: Mode, target: &mut W) -> Result<(), std::io::Error> {
        encode::sequence((self.type_id.encode_ref(), &self.value)).write_encoded(mode, target)
    }
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

impl EdiPartyName {
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, S::Err> {
        cons.take_sequence(|cons| {
            let name_assigner =
                cons.take_opt_constructed_if(Tag::CTX_0, |cons| DirectoryString::take_from(cons))?;
            let party_name =
                cons.take_constructed_if(Tag::CTX_1, |cons| DirectoryString::take_from(cons))?;

            Ok(Self {
                name_assigner,
                party_name,
            })
        })
    }

    pub fn encode_ref(&self) -> impl Values + '_ {
        encode::sequence((
            if let Some(name_assigner) = &self.name_assigner {
                Some(name_assigner.encode_ref())
            } else {
                None
            },
            self.party_name.encode_ref(),
        ))
    }

    pub fn encode_ref_as(&self, tag: Tag) -> impl Values + '_ {
        encode::sequence_as(
            tag,
            (
                if let Some(name_assigner) = &self.name_assigner {
                    Some(name_assigner.encode_ref())
                } else {
                    None
                },
                self.party_name.encode_ref(),
            ),
        )
    }
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

impl DirectoryString {
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, S::Err> {
        cons.take_value(|tag, content| {
            if tag == Tag::PRINTABLE_STRING {
                Ok(Self::PrintableString(PrintableString::from_content(
                    content,
                )?))
            } else if tag == Tag::UTF8_STRING {
                Ok(Self::Utf8String(Utf8String::from_content(content)?))
            } else {
                Err(Unimplemented.into())
            }
        })
    }

    pub fn encode_ref(&self) -> impl Values + '_ {
        match self {
            Self::PrintableString(ps) => (Some(ps.encode_ref()), None),
            Self::Utf8String(s) => (None, Some(s.encode_ref())),
            _ => unimplemented!(),
        }
    }
}

impl ToString for DirectoryString {
    fn to_string(&self) -> String {
        match self {
            Self::PrintableString(s) => s.to_string(),
            Self::Utf8String(s) => s.to_string(),
            _ => unimplemented!(),
        }
    }
}

impl Values for DirectoryString {
    fn encoded_len(&self, mode: Mode) -> usize {
        self.encode_ref().encoded_len(mode)
    }

    fn write_encoded<W: Write>(&self, mode: Mode, target: &mut W) -> Result<(), std::io::Error> {
        self.encode_ref().write_encoded(mode, target)
    }
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

    pub fn encode_ref_as(&self, tag: Tag) -> impl Values + '_ {
        match self {
            Self::RdnSequence(seq) => seq.encode_ref_as(tag),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
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

    pub fn encode_ref_as(&self, tag: Tag) -> impl Values + '_ {
        encode::sequence_as(tag, &self.0)
    }
}

pub type DistinguishedName = RdnSequence;

/// Relative distinguished name.
///
/// ```ASN.1
/// RelativeDistinguishedName ::=
///   SET OF AttributeTypeAndValue
/// ```
#[derive(Clone, Debug, Default, Eq, PartialEq)]
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrAddress {}

impl OrAddress {
    pub fn take_from<S: Source>(_: &mut Constructed<S>) -> Result<Self, S::Err> {
        Err(Unimplemented.into())
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
