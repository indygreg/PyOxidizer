// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{
        rfc5958::OneAsymmetricKey, KeyAlgorithm, SignatureAlgorithm, X509CertificateError as Error,
    },
    bcder::decode::Constructed,
    ring::signature::{self, KeyPair},
    std::convert::TryFrom,
};

/// Represents a key pair that exists in memory and can be used to create cryptographic signatures.
///
/// This is a wrapper around ring's various key pair types. It provides
/// abstractions tailored for X.509 certificates.
#[derive(Debug)]
pub enum InMemorySigningKeyPair {
    /// ECDSA key pair.
    Ecdsa(signature::EcdsaKeyPair),

    /// ED25519 key pair.
    Ed25519(signature::Ed25519KeyPair),

    /// RSA key pair.
    Rsa(signature::RsaKeyPair),
}

impl InMemorySigningKeyPair {
    /// Attempt to instantiate an instance from PKCS#8 DER data.
    ///
    /// The DER data should be a [OneAsymmetricKey] ASN.1 structure.
    pub fn from_pkcs8_der(
        data: impl AsRef<[u8]>,
        ecdsa_signing_algorithm: Option<&'static signature::EcdsaSigningAlgorithm>,
    ) -> Result<Self, Error> {
        // We need to parse the PKCS#8 to know what kind of key we're dealing with.
        let key = Constructed::decode(data.as_ref(), bcder::Mode::Der, |cons| {
            OneAsymmetricKey::take_from(cons)
        })?;

        let algorithm = KeyAlgorithm::try_from(&key.private_key_algorithm.algorithm)?;
        let ecdsa_signing_algorithm =
            ecdsa_signing_algorithm.unwrap_or(&ring::signature::ECDSA_P256_SHA256_ASN1_SIGNING);

        match algorithm {
            KeyAlgorithm::Rsa => Ok(Self::Rsa(signature::RsaKeyPair::from_pkcs8(data.as_ref())?)),
            KeyAlgorithm::Ecdsa => Ok(Self::Ecdsa(signature::EcdsaKeyPair::from_pkcs8(
                ecdsa_signing_algorithm,
                data.as_ref(),
            )?)),
            KeyAlgorithm::Ed25519 => Ok(Self::Ed25519(signature::Ed25519KeyPair::from_pkcs8(
                data.as_ref(),
            )?)),
        }
    }

    /// Attempt to instantiate an instance from PEM encoded PKCS#8.
    ///
    /// This is just a wrapper for [Self::from_pkcs8_der] that does the PEM
    /// decoding for you.
    pub fn from_pkcs8_pem(
        data: impl AsRef<[u8]>,
        ecdsa_signing_algorithm: Option<&'static signature::EcdsaSigningAlgorithm>,
    ) -> Result<Self, Error> {
        let der = pem::parse(data.as_ref()).map_err(Error::PemDecode)?;

        Self::from_pkcs8_der(&der.contents, ecdsa_signing_algorithm)
    }

    /// Obtain the raw bytes constituting the key pair's public key.
    pub fn public_key_data(&self) -> &[u8] {
        match self {
            Self::Rsa(key) => key.public_key().as_ref(),
            Self::Ecdsa(key) => key.public_key().as_ref(),
            Self::Ed25519(key) => key.public_key().as_ref(),
        }
    }

    /// Obtain the default [SignatureAlgorithm] to use with this key pair.
    ///
    /// This is just a convenience wrapper for coercing self into a
    /// [KeyAlgorithm] and calling [KeyAlgorithm::default_signature_algorithm].
    /// The same caveats about security apply.
    pub fn default_signature_algorithm(&self) -> SignatureAlgorithm {
        KeyAlgorithm::from(self).default_signature_algorithm()
    }

    /// Sign a message using this signing key.
    ///
    /// Returns the raw bytes constituting the signature.
    ///
    /// This will use a new instance of ring's SystemRandom. The RSA
    /// padding algorithm is hard-coded to RSA_PCS1_SHA256.
    ///
    /// If you want total control over signing parameters, obtain the
    /// underlying ring keypair and call its `.sign()`.
    pub fn sign(&self, message: impl AsRef<[u8]>) -> Result<Vec<u8>, Error> {
        match self {
            Self::Rsa(key) => {
                let mut signature = vec![0; key.public_modulus_len()];

                key.sign(
                    &ring::signature::RSA_PKCS1_SHA256,
                    &ring::rand::SystemRandom::new(),
                    message.as_ref(),
                    &mut signature,
                )
                .map_err(|_| Error::SignatureCreationInMemoryKey)?;

                Ok(signature)
            }
            Self::Ecdsa(key) => {
                let signature = key
                    .sign(&ring::rand::SystemRandom::new(), message.as_ref())
                    .map_err(|_| Error::SignatureCreationInMemoryKey)?;

                Ok(signature.as_ref().to_vec())
            }
            Self::Ed25519(key) => {
                let signature = key.sign(message.as_ref());

                Ok(signature.as_ref().to_vec())
            }
        }
    }
}

impl From<signature::EcdsaKeyPair> for InMemorySigningKeyPair {
    fn from(key: signature::EcdsaKeyPair) -> Self {
        Self::Ecdsa(key)
    }
}

impl From<signature::Ed25519KeyPair> for InMemorySigningKeyPair {
    fn from(key: signature::Ed25519KeyPair) -> Self {
        Self::Ed25519(key)
    }
}

impl From<signature::RsaKeyPair> for InMemorySigningKeyPair {
    fn from(key: signature::RsaKeyPair) -> Self {
        Self::Rsa(key)
    }
}

impl From<&InMemorySigningKeyPair> for KeyAlgorithm {
    fn from(key: &InMemorySigningKeyPair) -> Self {
        match key {
            InMemorySigningKeyPair::Rsa(_) => KeyAlgorithm::Rsa,
            InMemorySigningKeyPair::Ecdsa(_) => KeyAlgorithm::Ecdsa,
            InMemorySigningKeyPair::Ed25519(_) => KeyAlgorithm::Ed25519,
        }
    }
}

#[cfg(test)]
mod test {
    use {super::*, crate::testutil::*, ring::signature::UnparsedPublicKey};

    #[test]
    fn signing_key_from_ecdsa_pkcs8() {
        let rng = ring::rand::SystemRandom::new();

        let doc = ring::signature::EcdsaKeyPair::generate_pkcs8(
            &ring::signature::ECDSA_P256_SHA256_ASN1_SIGNING,
            &rng,
        )
        .unwrap();

        let signing_key = InMemorySigningKeyPair::from_pkcs8_der(doc.as_ref(), None).unwrap();
        assert!(matches!(signing_key, InMemorySigningKeyPair::Ecdsa(_)));

        let pem_data = pem::encode(&pem::Pem {
            tag: "PRIVATE KEY".to_string(),
            contents: doc.as_ref().to_vec(),
        });

        let signing_key =
            InMemorySigningKeyPair::from_pkcs8_pem(pem_data.as_bytes(), None).unwrap();
        assert!(matches!(signing_key, InMemorySigningKeyPair::Ecdsa(_)));

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

        let signing_key = InMemorySigningKeyPair::from_pkcs8_der(doc.as_ref(), None).unwrap();
        assert!(matches!(signing_key, InMemorySigningKeyPair::Ed25519(_)));

        let pem_data = pem::encode(&pem::Pem {
            tag: "PRIVATE KEY".to_string(),
            contents: doc.as_ref().to_vec(),
        });

        let signing_key =
            InMemorySigningKeyPair::from_pkcs8_pem(pem_data.as_bytes(), None).unwrap();
        assert!(matches!(signing_key, InMemorySigningKeyPair::Ed25519(_)));

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

    #[test]
    fn ecdsa_self_signed_certificate_verification() {
        let (cert, _) = self_signed_ecdsa_key_pair();
        cert.verify_signed_by_certificate(&cert).unwrap();
    }

    #[test]
    fn ed25519_self_signed_certificate_verification() {
        let (cert, _) = self_signed_ed25519_key_pair();
        cert.verify_signed_by_certificate(&cert).unwrap();
    }

    #[test]
    fn rsa_signing_roundtrip() {
        let key = rsa_private_key();
        let cert = rsa_cert();
        let message = b"hello, world";

        let signature = key.sign(message).unwrap();

        let public_key =
            UnparsedPublicKey::new(SignatureAlgorithm::Sha256Rsa.into(), cert.public_key_data());

        public_key.verify(message, &signature).unwrap();
    }
}
