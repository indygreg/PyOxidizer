// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! ASN.1 types defined in RFC 3447.

use {
    crate::rfc5280::AlgorithmIdentifier,
    bcder::{
        decode::{Constructed, DecodeError, Source},
        encode::{self, PrimitiveContent, Values},
        Mode, OctetString, Unsigned,
    },
    std::{
        io::Write,
        ops::{Deref, DerefMut},
    },
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
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, DecodeError<S::Error>> {
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

/// Other prime info
///
/// ```asn.1
/// OtherPrimeInfo ::= SEQUENCE {
///     prime             INTEGER,  -- ri
///     exponent          INTEGER,  -- di
///     coefficient       INTEGER   -- ti
/// }
/// ```
#[derive(Clone, Debug)]
pub struct OtherPrimeInfo {
    pub ri: Unsigned,
    pub di: Unsigned,
    pub ti: Unsigned,
}

impl OtherPrimeInfo {
    pub fn take_opt_from<S: Source>(
        cons: &mut Constructed<S>,
    ) -> Result<Option<Self>, DecodeError<S::Error>> {
        cons.take_opt_sequence(|cons| Self::from_sequence(cons))
    }

    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, DecodeError<S::Error>> {
        cons.take_sequence(|cons| Self::from_sequence(cons))
    }

    fn from_sequence<S: Source>(cons: &mut Constructed<S>) -> Result<Self, DecodeError<S::Error>> {
        let ri = Unsigned::take_from(cons)?;
        let di = Unsigned::take_from(cons)?;
        let ti = Unsigned::take_from(cons)?;

        Ok(Self { ri, di, ti })
    }

    pub fn encode_ref(&self) -> impl Values + '_ {
        encode::sequence((self.ri.encode(), self.di.encode(), self.ti.encode()))
    }
}

impl Values for OtherPrimeInfo {
    fn encoded_len(&self, mode: Mode) -> usize {
        self.encode_ref().encoded_len(mode)
    }

    fn write_encoded<W: Write>(&self, mode: Mode, target: &mut W) -> Result<(), std::io::Error> {
        self.encode_ref().write_encoded(mode, target)
    }
}

/// ```asn.1
/// OtherPrimeInfos ::= SEQUENCE SIZE(1..MAX) OF OtherPrimeInfo
/// ```
#[derive(Clone, Debug)]
pub struct OtherPrimeInfos(Vec<OtherPrimeInfo>);

impl Deref for OtherPrimeInfos {
    type Target = Vec<OtherPrimeInfo>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for OtherPrimeInfos {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl OtherPrimeInfos {
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
        let mut vals = Vec::new();

        while let Some(info) = OtherPrimeInfo::take_opt_from(cons)? {
            vals.push(info);
        }

        Ok(Self(vals))
    }

    pub fn encode_ref(&self) -> impl Values + '_ {
        encode::sequence(&self.0)
    }
}

impl Values for OtherPrimeInfos {
    fn encoded_len(&self, mode: Mode) -> usize {
        self.encode_ref().encoded_len(mode)
    }

    fn write_encoded<W: Write>(&self, mode: Mode, target: &mut W) -> Result<(), std::io::Error> {
        self.encode_ref().write_encoded(mode, target)
    }
}

/// An RSA private key.
///
/// ```ASN.1
/// RSAPrivateKey ::= SEQUENCE {
///     version           Version,
///     modulus           INTEGER,  -- n
///     publicExponent    INTEGER,  -- e
///     privateExponent   INTEGER,  -- d
///     prime1            INTEGER,  -- p
///     prime2            INTEGER,  -- q
///     exponent1         INTEGER,  -- d mod (p-1)
///     exponent2         INTEGER,  -- d mod (q-1)
///     coefficient       INTEGER,  -- (inverse of q) mod p
///     otherPrimeInfos   OtherPrimeInfos OPTIONAL
/// }
/// ```
#[derive(Clone, Debug)]
pub struct RsaPrivateKey {
    pub version: Unsigned,
    pub n: Unsigned,
    pub e: Unsigned,
    pub d: Unsigned,
    pub p: Unsigned,
    pub q: Unsigned,
    pub dp: Unsigned,
    pub dq: Unsigned,
    pub q_inv: Unsigned,
    pub other_prime_infos: Option<OtherPrimeInfos>,
}

impl RsaPrivateKey {
    pub fn take_from<S: Source>(cons: &mut Constructed<S>) -> Result<Self, DecodeError<S::Error>> {
        cons.take_sequence(|cons| {
            let version = Unsigned::take_from(cons)?;
            let n = Unsigned::take_from(cons)?;
            let e = Unsigned::take_from(cons)?;
            let d = Unsigned::take_from(cons)?;
            let p = Unsigned::take_from(cons)?;
            let q = Unsigned::take_from(cons)?;
            let dp = Unsigned::take_from(cons)?;
            let dq = Unsigned::take_from(cons)?;
            let q_inv = Unsigned::take_from(cons)?;
            let other_prime_infos = OtherPrimeInfos::take_opt_from(cons)?;

            Ok(Self {
                version,
                n,
                e,
                d,
                p,
                q,
                dp,
                dq,
                q_inv,
                other_prime_infos,
            })
        })
    }

    pub fn encode_ref(&self) -> impl Values + '_ {
        encode::sequence((
            self.version.encode(),
            self.n.encode(),
            self.e.encode(),
            self.d.encode(),
            self.p.encode(),
            self.q.encode(),
            self.dp.encode(),
            self.dq.encode(),
            self.q_inv.encode(),
            self.other_prime_infos.as_ref().map(|x| x.encode_ref()),
        ))
    }
}

impl Values for RsaPrivateKey {
    fn encoded_len(&self, mode: Mode) -> usize {
        self.encode_ref().encoded_len(mode)
    }

    fn write_encoded<W: Write>(&self, mode: Mode, target: &mut W) -> Result<(), std::io::Error> {
        self.encode_ref().write_encoded(mode, target)
    }
}
