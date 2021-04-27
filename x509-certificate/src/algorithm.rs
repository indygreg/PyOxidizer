// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Cryptographic algorithms commonly encountered in X.509 certificates.

use {
    crate::{rfc5280::AlgorithmIdentifier, X509CertificateError as Error},
    bcder::{ConstOid, Oid},
    ring::{digest, signature},
    std::convert::TryFrom,
};

/// SHA-256 digest algorithm.
///
/// 2.16.840.1.101.3.4.2.1
const OID_SHA256: ConstOid = Oid(&[96, 134, 72, 1, 101, 3, 4, 2, 1]);

/// SHA-512 digest algorithm.
///
/// 2.16.840.1.101.3.4.2.3
const OID_SHA512: ConstOid = Oid(&[96, 134, 72, 1, 101, 3, 4, 2, 3]);

/// RSA+SHA-1 encryption.
///
/// 1.2.840.113549.1.1.5
const OID_SHA1_RSA: ConstOid = Oid(&[42, 134, 72, 134, 247, 13, 1, 1, 5]);

/// RSA+SHA-256 encryption.
///
/// 1.2.840.113549.1.1.11
const OID_SHA256_RSA: ConstOid = Oid(&[42, 134, 72, 134, 247, 13, 1, 1, 11]);

/// RSA+SHA-512 encryption.
///
/// 1.2.840.113549.1.1.13
const OID_SHA512_RSA: ConstOid = Oid(&[42, 134, 72, 134, 247, 13, 1, 1, 13]);

/// RSAES-PKCS1-v1_5
///
/// 1.2.840.113549.1.1.1
const OID_RSAES_PKCS_V15: ConstOid = Oid(&[42, 134, 72, 134, 247, 13, 1, 1, 1]);

/// RSA encryption.
///
/// 1.2.840.113549.1.1.1
const OID_RSA: ConstOid = Oid(&[42, 134, 72, 134, 247, 13, 1, 1, 1]);

/// ECDSA with SHA-256.
///
/// 1.2.840.10045.4.3.2
const OID_ECDSA_SHA256: ConstOid = Oid(&[42, 134, 72, 206, 61, 4, 3, 2]);

/// Elliptic curve public key cryptography.
///
/// 1.2.840.10045.2.1
const OID_EC_PUBLIC_KEY: ConstOid = Oid(&[42, 134, 72, 206, 61, 2, 1]);

/// ED25519 key agreement.
///
/// 1.3.101.110
const OID_ED25519_KEY_AGREEMENT: ConstOid = Oid(&[43, 101, 110]);

/// Edwards curve digital signature algorithm.
///
/// 1.3.101.112
const OID_ED25519_SIGNATURE_ALGORITHM: ConstOid = Oid(&[43, 101, 112]);

/// A hasing algorithm used for digesting data.
///
/// Instances can be converted to and from [Oid] via `From`/`Into`
/// implementations.
///
/// They can also be converted to and from The ASN.1 [AlgorithmIdentifier],
/// which is commonly used to represent them in X.509 certificates.
///
/// Instances can be converted into a [digest::Context] capable of computing
/// digests via `From`/`Into`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DigestAlgorithm {
    /// SHA-256.
    ///
    /// Corresponds to OID 2.16.840.1.101.3.4.2.1.
    Sha256,
    /// SHA-512.
    ///
    /// Corresponds to OID 2.16.840.1.101.3.4.2.3.
    Sha512,
}

impl From<DigestAlgorithm> for Oid {
    fn from(alg: DigestAlgorithm) -> Self {
        Oid(match alg {
            DigestAlgorithm::Sha256 => OID_SHA256.as_ref(),
            DigestAlgorithm::Sha512 => OID_SHA512.as_ref(),
        }
        .into())
    }
}

impl TryFrom<&Oid> for DigestAlgorithm {
    type Error = Error;

    fn try_from(v: &Oid) -> Result<Self, Self::Error> {
        if v == &OID_SHA256 {
            Ok(Self::Sha256)
        } else if v == &OID_SHA512 {
            Ok(Self::Sha512)
        } else {
            Err(Error::UnknownDigestAlgorithm(format!("{}", v)))
        }
    }
}

impl TryFrom<&AlgorithmIdentifier> for DigestAlgorithm {
    type Error = Error;

    fn try_from(v: &AlgorithmIdentifier) -> Result<Self, Self::Error> {
        Self::try_from(&v.algorithm)
    }
}

impl From<DigestAlgorithm> for AlgorithmIdentifier {
    fn from(alg: DigestAlgorithm) -> Self {
        Self {
            algorithm: alg.into(),
            parameters: None,
        }
    }
}

impl From<DigestAlgorithm> for digest::Context {
    fn from(alg: DigestAlgorithm) -> Self {
        digest::Context::new(match alg {
            DigestAlgorithm::Sha256 => &digest::SHA256,
            DigestAlgorithm::Sha512 => &digest::SHA512,
        })
    }
}

impl DigestAlgorithm {
    /// Obtain an object that can be used to digest content using this algorithm.
    pub fn digester(&self) -> digest::Context {
        digest::Context::from(*self)
    }
}

/// An algorithm used to digitally sign content.
///
/// Instances can be converted to/from [Oid] via `From`/`Into`.
///
/// Similarly, instances can be converted to/from an ASN.1
/// [AlgorithmIdentifier].
///
/// It is also possible to obtain a [signature::VerificationAlgorithm] from
/// an instance. This type can perform actual cryptographic verification
/// that was signed with this algorithm.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum SignatureAlgorithm {
    /// SHA-1 with RSA encryption.
    ///
    /// Corresponds to OID 1.2.840.113549.1.1.5.
    Sha1Rsa,

    /// SHA-256 with RSA encryption.
    ///
    /// Corresponds to OID 1.2.840.113549.1.1.11.
    Sha256Rsa,

    /// SHA-512 with RSA encryption.
    ///
    /// Corresponds to OID 1.2.840.113549.1.1.13.
    Sha512Rsa,

    /// RSAES-PKCS1-v1_5 encryption scheme.
    ///
    /// Corresponds to OID 1.2.840.113549.1.1.1.
    RsaesPkcsV15,

    /// ECDSA with SHA-256.
    ///
    /// Corresponds to OID 1.2.840.10045.4.3.2.
    EcdsaSha256,

    /// ED25519
    ///
    /// Corresponds to OID 1.3.101.112.
    Ed25519,
}

impl From<SignatureAlgorithm> for Oid {
    fn from(alg: SignatureAlgorithm) -> Self {
        Oid(match alg {
            SignatureAlgorithm::Sha1Rsa => OID_SHA1_RSA.as_ref(),
            SignatureAlgorithm::Sha256Rsa => OID_SHA256_RSA.as_ref(),
            SignatureAlgorithm::Sha512Rsa => OID_SHA512_RSA.as_ref(),
            SignatureAlgorithm::RsaesPkcsV15 => OID_RSAES_PKCS_V15.as_ref(),
            SignatureAlgorithm::EcdsaSha256 => OID_ECDSA_SHA256.as_ref(),
            SignatureAlgorithm::Ed25519 => OID_ED25519_SIGNATURE_ALGORITHM.as_ref(),
        }
        .into())
    }
}

impl TryFrom<&Oid> for SignatureAlgorithm {
    type Error = Error;

    fn try_from(v: &Oid) -> Result<Self, Self::Error> {
        if v == &OID_SHA1_RSA {
            Ok(Self::Sha1Rsa)
        } else if v == &OID_SHA256_RSA {
            Ok(Self::Sha256Rsa)
        } else if v == &OID_SHA512_RSA {
            Ok(Self::Sha512Rsa)
        } else if v == &OID_RSAES_PKCS_V15 {
            Ok(Self::RsaesPkcsV15)
        } else if v == &OID_ECDSA_SHA256 {
            Ok(Self::EcdsaSha256)
        } else if v == &OID_ED25519_SIGNATURE_ALGORITHM {
            Ok(Self::Ed25519)
        } else {
            Err(Error::UnknownSignatureAlgorithm(format!("{}", v)))
        }
    }
}

impl TryFrom<&AlgorithmIdentifier> for SignatureAlgorithm {
    type Error = Error;

    fn try_from(v: &AlgorithmIdentifier) -> Result<Self, Self::Error> {
        Self::try_from(&v.algorithm)
    }
}

impl From<SignatureAlgorithm> for AlgorithmIdentifier {
    fn from(alg: SignatureAlgorithm) -> Self {
        Self {
            algorithm: alg.into(),
            parameters: None,
        }
    }
}

impl From<SignatureAlgorithm> for &'static dyn signature::VerificationAlgorithm {
    fn from(alg: SignatureAlgorithm) -> Self {
        match alg {
            SignatureAlgorithm::Sha1Rsa => {
                &ring::signature::RSA_PKCS1_2048_8192_SHA1_FOR_LEGACY_USE_ONLY
            }
            SignatureAlgorithm::Sha256Rsa => &ring::signature::RSA_PKCS1_2048_8192_SHA256,
            SignatureAlgorithm::Sha512Rsa => &ring::signature::RSA_PKCS1_2048_8192_SHA512,
            SignatureAlgorithm::RsaesPkcsV15 => {
                &ring::signature::RSA_PKCS1_1024_8192_SHA256_FOR_LEGACY_USE_ONLY
            }
            SignatureAlgorithm::EcdsaSha256 => &ring::signature::ECDSA_P256_SHA256_ASN1,
            SignatureAlgorithm::Ed25519 => &ring::signature::ED25519,
        }
    }
}

/// Cryptographic algorithm used by a private key.
///
/// Instances can be converted to/from the underlying ASN.1 type and
/// OIDs.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum KeyAlgorithm {
    /// RSA
    ///
    /// Corresponds to OID 1.2.840.113549.1.1.1.
    Rsa,

    /// Corresponds to OID 1.2.840.10045.2.1
    Ecdsa,

    /// Corresponds to OID 1.3.101.110
    Ed25519,
}

impl TryFrom<&Oid> for KeyAlgorithm {
    type Error = Error;

    fn try_from(v: &Oid) -> Result<Self, Self::Error> {
        if v == &OID_RSA {
            Ok(Self::Rsa)
        } else if v == &OID_EC_PUBLIC_KEY {
            Ok(Self::Ecdsa)
        // ED25519 appears to use the signature algorithm OID for private key
        // identification, so we need to accept both.
        } else if v == &OID_ED25519_KEY_AGREEMENT || v == &OID_ED25519_SIGNATURE_ALGORITHM {
            Ok(Self::Ed25519)
        } else {
            Err(Error::UnknownKeyAlgorithm(format!("{}", v)))
        }
    }
}

impl From<KeyAlgorithm> for Oid {
    fn from(alg: KeyAlgorithm) -> Self {
        Oid(match alg {
            KeyAlgorithm::Rsa => OID_RSA.as_ref(),
            KeyAlgorithm::Ecdsa => OID_EC_PUBLIC_KEY.as_ref(),
            KeyAlgorithm::Ed25519 => OID_ED25519_KEY_AGREEMENT.as_ref(),
        }
        .into())
    }
}

impl TryFrom<&AlgorithmIdentifier> for KeyAlgorithm {
    type Error = Error;

    fn try_from(v: &AlgorithmIdentifier) -> Result<Self, Self::Error> {
        Self::try_from(&v.algorithm)
    }
}

impl From<KeyAlgorithm> for AlgorithmIdentifier {
    fn from(alg: KeyAlgorithm) -> Self {
        Self {
            algorithm: alg.into(),
            parameters: None,
        }
    }
}

impl KeyAlgorithm {
    /// Obtain a default signature algorithm used by this key algorithm.
    ///
    /// We attempt to make the default choice secure. Use at your own risk.
    pub fn default_signature_algorithm(&self) -> SignatureAlgorithm {
        match self {
            Self::Rsa => SignatureAlgorithm::Sha256Rsa,
            Self::Ecdsa => SignatureAlgorithm::EcdsaSha256,
            Self::Ed25519 => SignatureAlgorithm::Ed25519,
        }
    }
}
