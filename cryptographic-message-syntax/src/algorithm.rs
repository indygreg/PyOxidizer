// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::CmsError,
    bcder::{ConstOid, Oid},
    bytes::Bytes,
    ring::{
        digest::SHA256,
        signature::{EcdsaKeyPair, Ed25519KeyPair, RsaKeyPair, VerificationAlgorithm},
    },
    std::convert::TryFrom,
};

/// SHA-256 digest algorithm.
///
/// 2.16.840.1.101.3.4.2.1
const OID_SHA256: ConstOid = Oid(&[96, 134, 72, 1, 101, 3, 4, 2, 1]);

/// RSA+SHA-1 encryption.
///
/// 1.2.840.113549.1.1.5
const OID_SHA1_RSA: ConstOid = Oid(&[42, 134, 72, 134, 247, 13, 1, 1, 5]);

/// RSA+SHA-256 encryption.
///
/// 1.2.840.113549.1.1.11
const OID_SHA256_RSA: ConstOid = Oid(&[42, 134, 72, 134, 247, 13, 1, 1, 11]);

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

/// A hashing algorithm used for digesting data.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DigestAlgorithm {
    /// SHA-256.
    ///
    /// Corresponds to OID 2.16.840.1.101.3.4.2.1.
    Sha256,
}

impl TryFrom<&Oid> for DigestAlgorithm {
    type Error = CmsError;

    fn try_from(v: &Oid) -> Result<Self, Self::Error> {
        if v == &OID_SHA256 {
            Ok(Self::Sha256)
        } else {
            Err(CmsError::UnknownDigestAlgorithm(v.clone()))
        }
    }
}

impl TryFrom<&crate::asn1::rfc5652::DigestAlgorithmIdentifier> for DigestAlgorithm {
    type Error = CmsError;

    fn try_from(v: &crate::asn1::rfc5652::DigestAlgorithmIdentifier) -> Result<Self, Self::Error> {
        Self::try_from(&v.algorithm)
    }
}

impl From<DigestAlgorithm> for Oid {
    fn from(alg: DigestAlgorithm) -> Self {
        match alg {
            DigestAlgorithm::Sha256 => Oid(Bytes::copy_from_slice(OID_SHA256.as_ref())),
        }
    }
}

impl From<DigestAlgorithm> for crate::asn1::rfc5652::DigestAlgorithmIdentifier {
    fn from(alg: DigestAlgorithm) -> Self {
        Self {
            algorithm: alg.into(),
            parameters: None,
        }
    }
}

impl DigestAlgorithm {
    /// Create a new content hasher for this algorithm.
    pub fn as_hasher(&self) -> ring::digest::Context {
        match self {
            Self::Sha256 => ring::digest::Context::new(&SHA256),
        }
    }
}

/// An algorithm used to digitally sign content.
///
/// Instances can be converted to/from the underlying ASN.1 type and
/// OIDs.
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

    /// RSAES-PKCS1-v1_5 encryption scheme.
    ///
    /// Corresponds to OID 1.2.840.113549.1.1.1.
    RsaesPkcsV15,

    /// ECDSA with SHA-256.
    ///
    /// Corresponds to OID 1.2.840.10045.4.3.2.
    EcdsaSha256,
}

impl SignatureAlgorithm {
    /// Convert this algorithm into a verification algorithm.
    ///
    /// This enables you to easily obtain a ring signature verified based on
    /// the type of algorithm.
    pub fn as_verification_algorithm(&self) -> &'static dyn VerificationAlgorithm {
        match self {
            SignatureAlgorithm::Sha1Rsa => {
                &ring::signature::RSA_PKCS1_2048_8192_SHA1_FOR_LEGACY_USE_ONLY
            }
            SignatureAlgorithm::Sha256Rsa => &ring::signature::RSA_PKCS1_2048_8192_SHA256,
            SignatureAlgorithm::RsaesPkcsV15 => {
                &ring::signature::RSA_PKCS1_1024_8192_SHA256_FOR_LEGACY_USE_ONLY
            }
            SignatureAlgorithm::EcdsaSha256 => &ring::signature::ECDSA_P256_SHA256_ASN1,
        }
    }
}

impl TryFrom<&Oid> for SignatureAlgorithm {
    type Error = CmsError;

    fn try_from(v: &Oid) -> Result<Self, Self::Error> {
        if v == &OID_SHA1_RSA {
            Ok(Self::Sha1Rsa)
        } else if v == &OID_SHA256_RSA {
            Ok(Self::Sha256Rsa)
        } else if v == &OID_RSAES_PKCS_V15 {
            Ok(Self::RsaesPkcsV15)
        } else if v == &OID_ECDSA_SHA256 {
            Ok(Self::EcdsaSha256)
        } else {
            Err(CmsError::UnknownSignatureAlgorithm(v.clone()))
        }
    }
}

impl TryFrom<&crate::asn1::rfc5652::SignatureAlgorithmIdentifier> for SignatureAlgorithm {
    type Error = CmsError;

    fn try_from(
        v: &crate::asn1::rfc5652::SignatureAlgorithmIdentifier,
    ) -> Result<Self, Self::Error> {
        Self::try_from(&v.algorithm)
    }
}

impl From<SignatureAlgorithm> for Oid {
    fn from(v: SignatureAlgorithm) -> Self {
        match v {
            SignatureAlgorithm::Sha1Rsa => Oid(Bytes::copy_from_slice(OID_SHA1_RSA.as_ref())),
            SignatureAlgorithm::Sha256Rsa => Oid(Bytes::copy_from_slice(OID_SHA256_RSA.as_ref())),
            SignatureAlgorithm::RsaesPkcsV15 => {
                Oid(Bytes::copy_from_slice(OID_RSAES_PKCS_V15.as_ref()))
            }
            SignatureAlgorithm::EcdsaSha256 => {
                Oid(Bytes::copy_from_slice(OID_ECDSA_SHA256.as_ref()))
            }
        }
    }
}

impl From<SignatureAlgorithm> for crate::asn1::rfc5652::SignatureAlgorithmIdentifier {
    fn from(alg: SignatureAlgorithm) -> Self {
        Self {
            algorithm: alg.into(),
            parameters: None,
        }
    }
}

/// An algorithm used to digitally sign content.
///
/// Instances can be converted to/from the underlying ASN.1 type and
/// OIDs.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum CertificateKeyAlgorithm {
    /// RSA
    ///
    /// Corresponds to OID 1.2.840.113549.1.1.1.
    Rsa,

    /// Corresponds to OID 1.2.840.10045.2.1
    Ec,
}

impl TryFrom<&Oid> for CertificateKeyAlgorithm {
    type Error = CmsError;

    fn try_from(v: &Oid) -> Result<Self, Self::Error> {
        if v == &OID_RSA {
            Ok(Self::Rsa)
        } else if v == &OID_EC_PUBLIC_KEY {
            Ok(Self::Ec)
        } else {
            Err(CmsError::UnknownSignatureAlgorithm(v.clone()))
        }
    }
}

impl TryFrom<&crate::asn1::rfc5280::AlgorithmIdentifier> for CertificateKeyAlgorithm {
    type Error = CmsError;

    fn try_from(v: &crate::asn1::rfc5280::AlgorithmIdentifier) -> Result<Self, Self::Error> {
        Self::try_from(&v.algorithm)
    }
}

impl From<CertificateKeyAlgorithm> for Oid {
    fn from(v: CertificateKeyAlgorithm) -> Self {
        match v {
            CertificateKeyAlgorithm::Rsa => Oid(Bytes::copy_from_slice(OID_RSA.as_ref())),
            CertificateKeyAlgorithm::Ec => Oid(Bytes::copy_from_slice(OID_EC_PUBLIC_KEY.as_ref())),
        }
    }
}

/// Represents a key used for signing content.
///
/// This is a wrapper around ring's key types supporting signing. We only
/// care about the private key as this type should only be used for signing.
#[derive(Debug)]
pub enum SigningKey {
    /// ECDSA key pair.
    Ecdsa(EcdsaKeyPair),

    /// ED25519 key pair.
    Ed25519(Ed25519KeyPair),

    /// RSA key pair.
    Rsa(RsaKeyPair),
}

impl SigningKey {
    /// Sign a message using this signing key.
    ///
    /// Returns the raw bytes constituting the signature.
    pub fn sign(&self, message: &[u8]) -> Result<Vec<u8>, CmsError> {
        match self {
            Self::Rsa(key) => {
                let mut signature = vec![0; key.public_modulus_len()];

                key.sign(
                    &ring::signature::RSA_PKCS1_SHA256,
                    &ring::rand::SystemRandom::new(),
                    message,
                    &mut signature,
                )
                .map_err(|_| CmsError::SignatureCreation)?;

                Ok(signature)
            }
            Self::Ecdsa(key) => {
                let signature = key
                    .sign(&ring::rand::SystemRandom::new(), message)
                    .map_err(|_| CmsError::SignatureCreation)?;

                Ok(signature.as_ref().to_vec())
            }
            Self::Ed25519(key) => {
                let signature = key.sign(message);

                Ok(signature.as_ref().to_vec())
            }
        }
    }
}

impl From<EcdsaKeyPair> for SigningKey {
    fn from(key: EcdsaKeyPair) -> Self {
        Self::Ecdsa(key)
    }
}

impl From<Ed25519KeyPair> for SigningKey {
    fn from(key: Ed25519KeyPair) -> Self {
        Self::Ed25519(key)
    }
}

impl From<RsaKeyPair> for SigningKey {
    fn from(key: RsaKeyPair) -> Self {
        Self::Rsa(key)
    }
}

impl From<&SigningKey> for SignatureAlgorithm {
    fn from(key: &SigningKey) -> Self {
        match key {
            SigningKey::Rsa(_) => SignatureAlgorithm::Sha256Rsa,
            SigningKey::Ecdsa(_) => SignatureAlgorithm::EcdsaSha256,
            _ => unimplemented!(),
        }
    }
}
