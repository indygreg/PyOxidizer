// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{asn1::rfc5958::OneAsymmetricKey, CmsError},
    bcder::decode::Constructed,
    ring::signature::{EcdsaKeyPair, Ed25519KeyPair, KeyPair, RsaKeyPair},
    std::convert::TryFrom,
    x509_certificate::{KeyAlgorithm, SignatureAlgorithm},
};

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

        let algorithm = KeyAlgorithm::try_from(&key.private_key_algorithm.algorithm)?;
        let ecdsa_signing_algorithm =
            ecdsa_signing_algorithm.unwrap_or(&ring::signature::ECDSA_P256_SHA256_ASN1_SIGNING);

        match algorithm {
            KeyAlgorithm::Rsa => Ok(Self::Rsa(RsaKeyPair::from_pkcs8(data)?)),
            KeyAlgorithm::Ecdsa => Ok(Self::Ecdsa(EcdsaKeyPair::from_pkcs8(
                ecdsa_signing_algorithm,
                data,
            )?)),
            KeyAlgorithm::Ed25519 => Ok(Self::Ed25519(Ed25519KeyPair::from_pkcs8(data)?)),
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
    fn signing_key_from_ecdsa_pkcs8() {
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
            KeyAlgorithm::Ecdsa.into()
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
            SignatureAlgorithm::Ed25519.into()
        );
        assert!(key_pair_asn1.private_key_algorithm.parameters.is_none());
    }
}
