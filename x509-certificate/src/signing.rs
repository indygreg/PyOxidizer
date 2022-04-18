// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::{
        rfc3447::RsaPrivateKey, rfc5958::OneAsymmetricKey, EcdsaCurve, KeyAlgorithm,
        SignatureAlgorithm, X509CertificateError as Error,
    },
    bcder::decode::Constructed,
    bytes::Bytes,
    ring::{
        rand::SystemRandom,
        signature::{self as ringsig, KeyPair},
    },
    signature::{Signature as SignatureTrait, Signer},
};

/// Signifies that an entity is capable of producing cryptographic signatures.
pub trait Sign {
    /// Create a cyrptographic signature over a message.
    ///
    /// Takes the message to be signed, which will be digested by the implementation.
    ///
    /// Returns the raw bytes constituting the signature and which signature algorithm
    /// was used. The returned [SignatureAlgorithm] can be serialized into an
    /// ASN.1 `AlgorithmIdentifier` via `.into()`.
    #[deprecated(since = "0.13.0", note = "use the signature::Signer trait instead")]
    fn sign(&self, message: &[u8]) -> Result<(Vec<u8>, SignatureAlgorithm), Error>;

    /// Obtain the algorithm of the private key.
    ///
    /// If we can't coerce the key algorithm to [KeyAlgorithm], None is returned.
    fn key_algorithm(&self) -> Option<KeyAlgorithm>;

    /// Obtain the raw bytes constituting the public key of the signing certificate.
    ///
    /// This will be `.tbs_certificate.subject_public_key_info.subject_public_key` of a parsed
    /// X.509 public certificate.
    fn public_key_data(&self) -> Bytes;

    /// Obtain the [SignatureAlgorithm] that this signer will use.
    ///
    /// Instances can be coerced into the ASN.1 `AlgorithmIdentifier` via `.into()`
    /// for easy inclusion in ASN.1 structures.
    fn signature_algorithm(&self) -> Result<SignatureAlgorithm, Error>;

    /// Obtain the raw private key data.
    fn private_key_data(&self) -> Option<Vec<u8>>;

    /// Obtain RSA key primes p and q, if available.
    fn rsa_primes(&self) -> Result<Option<(Vec<u8>, Vec<u8>)>, Error>;
}

/// A superset of [Signer] and [Sign].
pub trait KeyInfoSigner: Signer<Signature> + Sign {}

#[derive(Clone, Debug)]
pub struct Signature(Vec<u8>);

impl From<Vec<u8>> for Signature {
    fn from(v: Vec<u8>) -> Self {
        Self(v)
    }
}

impl From<Signature> for Vec<u8> {
    fn from(v: Signature) -> Vec<u8> {
        v.0
    }
}

impl From<Signature> for Bytes {
    fn from(v: Signature) -> Self {
        Self::copy_from_slice(&v.0)
    }
}

impl AsRef<[u8]> for Signature {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl SignatureTrait for Signature {
    fn from_bytes(bytes: &[u8]) -> Result<Self, signature::Error> {
        Ok(Self(bytes.to_vec()))
    }
}

/// Represents a key pair that exists in memory and can be used to create cryptographic signatures.
///
/// This is a wrapper around ring's various key pair types. It provides
/// abstractions tailored for X.509 certificates.
#[derive(Debug)]
pub enum InMemorySigningKeyPair {
    /// ECDSA key pair.
    Ecdsa(ringsig::EcdsaKeyPair, EcdsaCurve, Vec<u8>),

    /// ED25519 key pair.
    Ed25519(ringsig::Ed25519KeyPair),

    /// RSA key pair.
    Rsa(ringsig::RsaKeyPair, Vec<u8>),
}

impl Signer<Signature> for InMemorySigningKeyPair {
    fn try_sign(&self, msg: &[u8]) -> Result<Signature, signature::Error> {
        match self {
            Self::Rsa(key, _) => {
                let mut signature = vec![0; key.public_modulus_len()];

                key.sign(
                    &ringsig::RSA_PKCS1_SHA256,
                    &ring::rand::SystemRandom::new(),
                    msg,
                    &mut signature,
                )
                .map_err(|_| signature::Error::new())?;

                Ok(signature.into())
            }
            Self::Ecdsa(key, _, _) => {
                let signature = key
                    .sign(&ring::rand::SystemRandom::new(), msg)
                    .map_err(|_| signature::Error::new())?;

                Signature::from_bytes(signature.as_ref())
            }
            Self::Ed25519(key) => {
                let signature = key.sign(msg);

                Signature::from_bytes(signature.as_ref())
            }
        }
    }
}

impl Sign for InMemorySigningKeyPair {
    /// This will use a new instance of ring's SystemRandom. The RSA
    /// padding algorithm is hard-coded to RSA_PCS1_SHA256.
    ///
    /// If you want total control over signing parameters, obtain the
    /// underlying ring keypair and call its `.sign()`.
    fn sign(&self, message: &[u8]) -> Result<(Vec<u8>, SignatureAlgorithm), Error> {
        let algorithm = self.signature_algorithm()?;

        Ok((self.try_sign(message)?.into(), algorithm))
    }

    fn key_algorithm(&self) -> Option<KeyAlgorithm> {
        Some(match self {
            Self::Rsa(_, _) => KeyAlgorithm::Rsa,
            Self::Ed25519(_) => KeyAlgorithm::Ed25519,
            Self::Ecdsa(_, curve, _) => KeyAlgorithm::Ecdsa(*curve),
        })
    }

    fn public_key_data(&self) -> Bytes {
        match self {
            Self::Rsa(key, _) => Bytes::copy_from_slice(key.public_key().as_ref()),
            Self::Ecdsa(key, _, _) => Bytes::copy_from_slice(key.public_key().as_ref()),
            Self::Ed25519(key) => Bytes::copy_from_slice(key.public_key().as_ref()),
        }
    }

    fn signature_algorithm(&self) -> Result<SignatureAlgorithm, Error> {
        Ok(match self {
            Self::Rsa(_, _) => SignatureAlgorithm::RsaSha256,
            Self::Ecdsa(_, curve, _) => {
                // ring refuses to mix and match the bitness of curves and signature
                // algorithms. e.g. it can't pair secp256r1 with SHA-384. It chooses
                // signatures on its own. We reimplement that logic here.
                match curve {
                    EcdsaCurve::Secp256r1 => SignatureAlgorithm::EcdsaSha256,
                    EcdsaCurve::Secp384r1 => SignatureAlgorithm::EcdsaSha384,
                }
            }
            Self::Ed25519(_) => SignatureAlgorithm::Ed25519,
        })
    }

    fn private_key_data(&self) -> Option<Vec<u8>> {
        match self {
            Self::Rsa(_, data) => Some(data.clone()),
            Self::Ecdsa(_, _, data) => Some(data.clone()),
            Self::Ed25519(_) => None,
        }
    }

    fn rsa_primes(&self) -> Result<Option<(Vec<u8>, Vec<u8>)>, Error> {
        match self {
            Self::Rsa(_, data) => {
                let key = Constructed::decode(data.as_ref(), bcder::Mode::Der, |cons| {
                    RsaPrivateKey::take_from(cons)
                })?;

                Ok(Some((key.p.as_slice().to_vec(), key.q.as_slice().to_vec())))
            }
            Self::Ecdsa(..) => Ok(None),
            Self::Ed25519(_) => Ok(None),
        }
    }
}

impl KeyInfoSigner for InMemorySigningKeyPair {}

impl InMemorySigningKeyPair {
    /// Attempt to instantiate an instance from PKCS#8 DER data.
    ///
    /// The DER data should be a [OneAsymmetricKey] ASN.1 structure.
    pub fn from_pkcs8_der(data: impl AsRef<[u8]>) -> Result<Self, Error> {
        // We need to parse the PKCS#8 to know what kind of key we're dealing with.
        let key = Constructed::decode(data.as_ref(), bcder::Mode::Der, |cons| {
            OneAsymmetricKey::take_from(cons)
        })?;

        let algorithm = KeyAlgorithm::try_from(&key.private_key_algorithm)?;

        // self.key_algorithm() assumes a 1:1 mapping between KeyAlgorithm and our enum
        // variants. If you change this, change that function as well.
        match algorithm {
            KeyAlgorithm::Rsa => {
                let pair = ringsig::RsaKeyPair::from_pkcs8(data.as_ref())?;

                Ok(Self::Rsa(pair, key.private_key.into_bytes().to_vec()))
            }
            KeyAlgorithm::Ecdsa(curve) => {
                let pair = ringsig::EcdsaKeyPair::from_pkcs8(curve.into(), data.as_ref())?;

                Ok(Self::Ecdsa(pair, curve, data.as_ref().to_vec()))
            }
            KeyAlgorithm::Ed25519 => Ok(Self::Ed25519(ringsig::Ed25519KeyPair::from_pkcs8(
                data.as_ref(),
            )?)),
        }
    }

    /// Attempt to instantiate an instance from PEM encoded PKCS#8.
    ///
    /// This is just a wrapper for [Self::from_pkcs8_der] that does the PEM
    /// decoding for you.
    pub fn from_pkcs8_pem(data: impl AsRef<[u8]>) -> Result<Self, Error> {
        let der = pem::parse(data.as_ref()).map_err(Error::PemDecode)?;

        Self::from_pkcs8_der(&der.contents)
    }

    /// Generate a random key pair given a key algorithm and optional ECDSA signing algorithm.
    ///
    /// The raw PKCS#8 document is returned to facilitate access to the private key.
    ///
    /// Not attempt is made to protect the private key in memory.
    pub fn generate_random(
        key_algorithm: KeyAlgorithm,
    ) -> Result<(Self, ring::pkcs8::Document), Error> {
        let rng = SystemRandom::new();

        let document = match key_algorithm {
            KeyAlgorithm::Ed25519 => ringsig::Ed25519KeyPair::generate_pkcs8(&rng)
                .map_err(|_| Error::KeyPairGenerationError),
            KeyAlgorithm::Ecdsa(curve) => ringsig::EcdsaKeyPair::generate_pkcs8(curve.into(), &rng)
                .map_err(|_| Error::KeyPairGenerationError),
            KeyAlgorithm::Rsa => Err(Error::RsaKeyGenerationNotSupported),
        }?;

        let key_pair = Self::from_pkcs8_der(document.as_ref())?;

        Ok((key_pair, document))
    }

    /// Attempt to resolve a verification algorithm for this key pair.
    ///
    /// This is a wrapper around [SignatureAlgorithm::resolve_verification_algorithm()]
    /// with our bound [KeyAlgorithm]. However, since there are no parameters
    /// that can result in wrong choices, this is guaranteed to always work
    /// and doesn't require `Result`.
    pub fn verification_algorithm(
        &self,
    ) -> Result<&'static dyn ringsig::VerificationAlgorithm, Error> {
        Ok(self.signature_algorithm()?
            .resolve_verification_algorithm(self.key_algorithm().expect("key algorithm should be known for InMemorySigningKeyPair")).expect(
            "illegal combination of key algorithm in signature algorithm: this should not occur"
        ))
    }
}

// We don't support coercing from EcdsaKeyPair because we don't know what the
// elliptic curve is since ring doesn't expose it.

impl From<ringsig::Ed25519KeyPair> for InMemorySigningKeyPair {
    fn from(key: ringsig::Ed25519KeyPair) -> Self {
        Self::Ed25519(key)
    }
}

impl From<&InMemorySigningKeyPair> for KeyAlgorithm {
    fn from(key: &InMemorySigningKeyPair) -> Self {
        match key {
            InMemorySigningKeyPair::Rsa(_, _) => KeyAlgorithm::Rsa,
            InMemorySigningKeyPair::Ecdsa(_, curve, _) => KeyAlgorithm::Ecdsa(*curve),
            InMemorySigningKeyPair::Ed25519(_) => KeyAlgorithm::Ed25519,
        }
    }
}

#[cfg(test)]
mod test {
    use {super::*, crate::rfc5280, crate::testutil::*, ringsig::UnparsedPublicKey};

    #[test]
    fn generate_random_ecdsa() {
        for curve in EcdsaCurve::all() {
            InMemorySigningKeyPair::generate_random(KeyAlgorithm::Ecdsa(*curve)).unwrap();
        }
    }

    #[test]
    fn generate_random_ed25519() {
        InMemorySigningKeyPair::generate_random(KeyAlgorithm::Ed25519).unwrap();
    }

    #[test]
    fn generate_random_rsa() {
        assert!(InMemorySigningKeyPair::generate_random(KeyAlgorithm::Rsa).is_err());
    }

    #[test]
    fn signing_key_from_ecdsa_pkcs8() {
        let rng = ring::rand::SystemRandom::new();

        for alg in &[
            &ringsig::ECDSA_P256_SHA256_ASN1_SIGNING,
            &ringsig::ECDSA_P384_SHA384_ASN1_SIGNING,
        ] {
            let doc = ringsig::EcdsaKeyPair::generate_pkcs8(alg, &rng).unwrap();

            let signing_key = InMemorySigningKeyPair::from_pkcs8_der(doc.as_ref()).unwrap();
            assert!(matches!(
                signing_key,
                InMemorySigningKeyPair::Ecdsa(_, _, _)
            ));

            let pem_data = pem::encode(&pem::Pem {
                tag: "PRIVATE KEY".to_string(),
                contents: doc.as_ref().to_vec(),
            });

            let signing_key = InMemorySigningKeyPair::from_pkcs8_pem(pem_data.as_bytes()).unwrap();
            assert!(matches!(
                signing_key,
                InMemorySigningKeyPair::Ecdsa(_, _, _)
            ));

            let key_pair_asn1 = Constructed::decode(doc.as_ref(), bcder::Mode::Der, |cons| {
                OneAsymmetricKey::take_from(cons)
            })
            .unwrap();
            assert_eq!(
                key_pair_asn1.private_key_algorithm.algorithm,
                // Inner value doesn't matter here.
                KeyAlgorithm::Ecdsa(EcdsaCurve::Secp256r1).into()
            );

            let expected = if *alg == &ringsig::ECDSA_P256_SHA256_ASN1_SIGNING {
                EcdsaCurve::Secp256r1
            } else if *alg == &ringsig::ECDSA_P384_SHA384_ASN1_SIGNING {
                EcdsaCurve::Secp384r1
            } else {
                panic!("unhandled test case");
            };

            assert!(key_pair_asn1.private_key_algorithm.parameters.is_some());
            let oid = key_pair_asn1
                .private_key_algorithm
                .parameters
                .unwrap()
                .decode_oid()
                .unwrap();

            assert_eq!(EcdsaCurve::try_from(&oid).unwrap(), expected);
        }
    }

    #[test]
    fn signing_key_from_ed25519_pkcs8() {
        let rng = ring::rand::SystemRandom::new();

        let doc = ringsig::Ed25519KeyPair::generate_pkcs8(&rng).unwrap();

        let signing_key = InMemorySigningKeyPair::from_pkcs8_der(doc.as_ref()).unwrap();
        assert!(matches!(signing_key, InMemorySigningKeyPair::Ed25519(_)));

        let pem_data = pem::encode(&pem::Pem {
            tag: "PRIVATE KEY".to_string(),
            contents: doc.as_ref().to_vec(),
        });

        let signing_key = InMemorySigningKeyPair::from_pkcs8_pem(pem_data.as_bytes()).unwrap();
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
        for curve in EcdsaCurve::all() {
            let (cert, _) = self_signed_ecdsa_key_pair(Some(*curve));
            cert.verify_signed_by_certificate(&cert).unwrap();

            let raw: &rfc5280::Certificate = cert.as_ref();

            let tbs_signature_algorithm =
                SignatureAlgorithm::try_from(&raw.tbs_certificate.signature).unwrap();
            let expected = match curve {
                EcdsaCurve::Secp256r1 => SignatureAlgorithm::EcdsaSha256,
                EcdsaCurve::Secp384r1 => SignatureAlgorithm::EcdsaSha384,
            };
            assert_eq!(tbs_signature_algorithm, expected);

            let spki = &raw.tbs_certificate.subject_public_key_info;

            // The algorithm in the SPKI should be constant.
            assert_eq!(
                spki.algorithm.algorithm,
                crate::algorithm::OID_EC_PUBLIC_KEY
            );
            // But the parameters depend on the curve in use.
            let expected = match curve {
                EcdsaCurve::Secp256r1 => crate::algorithm::OID_EC_SECP256R1,
                EcdsaCurve::Secp384r1 => crate::algorithm::OID_EC_SECP384R1,
            };
            assert!(spki.algorithm.parameters.is_some());
            assert_eq!(
                spki.algorithm
                    .parameters
                    .as_ref()
                    .unwrap()
                    .decode_oid()
                    .unwrap(),
                expected
            );

            // This should match the tbs signature algorithm.
            let cert_algorithm = SignatureAlgorithm::try_from(&raw.signature_algorithm).unwrap();
            assert_eq!(cert_algorithm, tbs_signature_algorithm);
        }
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

        let signature = Signer::try_sign(&key, message).unwrap();

        let public_key = UnparsedPublicKey::new(
            key.verification_algorithm().unwrap(),
            cert.public_key_data(),
        );

        public_key.verify(message, signature.as_ref()).unwrap();
    }
}
