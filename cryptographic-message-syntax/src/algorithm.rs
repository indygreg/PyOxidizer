// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::CmsError,
    bcder::{ConstOid, Oid},
    bytes::Bytes,
    ring::{digest::SHA256, signature::VerificationAlgorithm},
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
}

impl SignatureAlgorithm {
    /// Convert this algorithm into a verification algorithm.
    ///
    /// This enables you to easily obtain a ring signature verified based on
    /// the type of algorithm.
    pub fn as_verification_algorithm(&self) -> &'static impl VerificationAlgorithm {
        match self {
            SignatureAlgorithm::Sha1Rsa => {
                &ring::signature::RSA_PKCS1_2048_8192_SHA1_FOR_LEGACY_USE_ONLY
            }
            SignatureAlgorithm::Sha256Rsa => &ring::signature::RSA_PKCS1_2048_8192_SHA256,
            SignatureAlgorithm::RsaesPkcsV15 => {
                &ring::signature::RSA_PKCS1_1024_8192_SHA256_FOR_LEGACY_USE_ONLY
            }
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
}

impl TryFrom<&Oid> for CertificateKeyAlgorithm {
    type Error = CmsError;

    fn try_from(v: &Oid) -> Result<Self, Self::Error> {
        if v == &OID_RSA {
            Ok(Self::Rsa)
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
        }
    }
}
