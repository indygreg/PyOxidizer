// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{
        asn1::{rfc5280::AlgorithmIdentifier, rfc5958::OneAsymmetricKey},
        CmsError,
    },
    bcder::{decode::Constructed, ConstOid, Oid},
    bytes::Bytes,
    ring::{
        digest::SHA256,
        signature::{EcdsaKeyPair, Ed25519KeyPair, KeyPair, RsaKeyPair, VerificationAlgorithm},
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

/// ED25519 key agreement.
///
/// 1.3.101.110
const OID_ED25519_KEY_AGREEMENT: ConstOid = Oid(&[43, 101, 110]);

/// Edwards curve digital signature algorithm.
///
/// 1.3.101.112
const OID_ED25519_SIGNATURE_ALGORITHM: ConstOid = Oid(&[43, 101, 112]);

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

    /// ED25519
    ///
    /// Corresponds to OID 1.3.101.112.
    Ed25519,
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
            SignatureAlgorithm::Ed25519 => &ring::signature::ED25519,
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
        } else if v == &OID_ED25519_SIGNATURE_ALGORITHM {
            Ok(Self::Ed25519)
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
            SignatureAlgorithm::Ed25519 => Oid(Bytes::copy_from_slice(
                OID_ED25519_SIGNATURE_ALGORITHM.as_ref(),
            )),
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
    Ecdsa,

    /// Corresponds to OID 1.3.101.110
    Ed25519,
}

impl CertificateKeyAlgorithm {
    /// Obtain the OID of the default signature algorithm this key algorithm uses.
    pub fn default_signature_algorithm_oid(&self) -> Oid {
        match self {
            Self::Rsa => SignatureAlgorithm::Sha256Rsa.into(),
            Self::Ecdsa => SignatureAlgorithm::EcdsaSha256.into(),
            Self::Ed25519 => SignatureAlgorithm::Ed25519.into(),
        }
    }

    /// Obtain the default [AlgorithmIdentifier] that this key uses.
    pub fn default_signature_algorithm_identifier(&self) -> AlgorithmIdentifier {
        AlgorithmIdentifier {
            algorithm: self.default_signature_algorithm_oid(),
            parameters: None,
        }
    }
}

impl TryFrom<&Oid> for CertificateKeyAlgorithm {
    type Error = CmsError;

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
            CertificateKeyAlgorithm::Ecdsa => {
                Oid(Bytes::copy_from_slice(OID_EC_PUBLIC_KEY.as_ref()))
            }
            CertificateKeyAlgorithm::Ed25519 => {
                Oid(Bytes::copy_from_slice(OID_ED25519_KEY_AGREEMENT.as_ref()))
            }
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
    /// Attempt to instantiate an instance from PKCS#8 DER data.
    ///
    /// The document should be a [OneAsymmetricKey] data structure and should
    /// contain both the private and public key.
    pub fn from_pkcs8_der(
        data: &[u8],
        ecdsa_signing_algorithm: Option<&'static ring::signature::EcdsaSigningAlgorithm>,
    ) -> Result<Self, CmsError> {
        // We need to parse the PKCS#8 to know what kind of key we're dealing with.
        let key = Constructed::decode(data, bcder::Mode::Der, |cons| {
            OneAsymmetricKey::take_from(cons)
        })?;

        let algorithm = CertificateKeyAlgorithm::try_from(&key.private_key_algorithm.algorithm)?;
        let ecdsa_signing_algorithm =
            ecdsa_signing_algorithm.unwrap_or(&ring::signature::ECDSA_P256_SHA256_ASN1_SIGNING);

        match algorithm {
            CertificateKeyAlgorithm::Rsa => Ok(Self::Rsa(RsaKeyPair::from_pkcs8(data)?)),
            CertificateKeyAlgorithm::Ecdsa => Ok(Self::Ecdsa(EcdsaKeyPair::from_pkcs8(
                ecdsa_signing_algorithm,
                data,
            )?)),
            CertificateKeyAlgorithm::Ed25519 => {
                Ok(Self::Ed25519(Ed25519KeyPair::from_pkcs8(data)?))
            }
        }
    }

    /// Attempt to instantiate an instance from PEM encoded PKCS#8.
    ///
    /// This is a convenience wrapper for PEM decoding and calling
    /// [SigningKey::from_pkcs8_der].
    pub fn from_pkcs8_pem(
        data: &[u8],
        ecdsa_signing_algorithm: Option<&'static ring::signature::EcdsaSigningAlgorithm>,
    ) -> Result<Self, CmsError> {
        let der = pem::parse(data)?;

        Self::from_pkcs8_der(&der.contents, ecdsa_signing_algorithm)
    }

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

    /// Obtain the raw bytes constituting the public key for this signing key.
    pub fn public_key(&self) -> &[u8] {
        match self {
            Self::Rsa(key) => key.public_key().as_ref(),
            Self::Ecdsa(key) => key.public_key().as_ref(),
            Self::Ed25519(key) => key.public_key().as_ref(),
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
            SigningKey::Ed25519(_) => SignatureAlgorithm::Ed25519,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn signing_key_from_edsa_pkcs8() {
        let rng = ring::rand::SystemRandom::new();

        let doc = ring::signature::EcdsaKeyPair::generate_pkcs8(
            &ring::signature::ECDSA_P256_SHA256_ASN1_SIGNING,
            &rng,
        )
        .unwrap();

        let signing_key = SigningKey::from_pkcs8_der(doc.as_ref(), None).unwrap();
        assert!(matches!(signing_key, SigningKey::Ecdsa(_)));

        let pem_data = pem::encode(&pem::Pem {
            tag: "PRIVATE KEY".to_string(),
            contents: doc.as_ref().to_vec(),
        });

        let signing_key = SigningKey::from_pkcs8_pem(pem_data.as_bytes(), None).unwrap();
        assert!(matches!(signing_key, SigningKey::Ecdsa(_)));

        let key_pair_asn1 = Constructed::decode(doc.as_ref(), bcder::Mode::Der, |cons| {
            OneAsymmetricKey::take_from(cons)
        })
        .unwrap();
        assert_eq!(
            key_pair_asn1.private_key_algorithm.algorithm,
            OID_EC_PUBLIC_KEY
        );
        assert!(key_pair_asn1.private_key_algorithm.parameters.is_some());
    }

    #[test]
    fn signing_key_from_ed25519_pkcs8() {
        let rng = ring::rand::SystemRandom::new();

        let doc = ring::signature::Ed25519KeyPair::generate_pkcs8(&rng).unwrap();

        let signing_key = SigningKey::from_pkcs8_der(doc.as_ref(), None).unwrap();
        assert!(matches!(signing_key, SigningKey::Ed25519(_)));

        let pem_data = pem::encode(&pem::Pem {
            tag: "PRIVATE KEY".to_string(),
            contents: doc.as_ref().to_vec(),
        });

        let signing_key = SigningKey::from_pkcs8_pem(pem_data.as_bytes(), None).unwrap();
        assert!(matches!(signing_key, SigningKey::Ed25519(_)));

        let key_pair_asn1 = Constructed::decode(doc.as_ref(), bcder::Mode::Der, |cons| {
            OneAsymmetricKey::take_from(cons)
        })
        .unwrap();
        assert_eq!(
            key_pair_asn1.private_key_algorithm.algorithm,
            OID_ED25519_SIGNATURE_ALGORITHM
        );
        assert!(key_pair_asn1.private_key_algorithm.parameters.is_none());
    }
}
