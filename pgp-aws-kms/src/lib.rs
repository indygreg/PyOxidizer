// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! PGP with AWS KMS

This crate provides a mechanism to use AWS KMS hosted keys with PGP.

PGP keys can typically be used for both encrypt/decrypt and sign/verify operations. However,
AWS KMS keys must choose at creation time whether they are *symmetrical* - used for
encrypt/decrypt - or *asymmetrical* - used for sign/verify. Therefore, KMS hosted keys
are not as versatile as locally generated PGP keys since they can only be used for a
single logical purpose.

We only implement support for version 5 PGP keys.
*/

use {
    bcder::{decode::Constructed, Mode},
    byteorder::{BigEndian, WriteBytesExt},
    pgp::{
        composed::{KeyDetails, PublicKey, SecretKey, SecretKeyParams},
        crypto::{ecc_curve_from_oid, HashAlgorithm, PublicKeyAlgorithm, SymmetricKeyAlgorithm},
        packet::PacketTrait,
        ser::Serialize,
        types::{
            KeyId, KeyTrait, KeyVersion, Mpi, PublicKeyTrait, PublicParams, SecretKeyRepr,
            SecretKeyTrait, Tag, Version,
        },
    },
    rand::{CryptoRng, Rng},
    rusoto_core::RusotoError,
    rusoto_kms::{
        DescribeKeyError, DescribeKeyRequest, DescribeKeyResponse, GetPublicKeyError,
        GetPublicKeyRequest, GetPublicKeyResponse, KeyMetadata, Kms, KmsClient, SignError,
        SignRequest,
    },
    sha1::Sha1,
    std::io::Write,
    thiserror::Error,
    x509_certificate::{
        rfc5280::SubjectPublicKeyInfo, rfc8017::RsaPublicKey, KeyAlgorithm, X509CertificateError,
    },
};

#[derive(Debug, Error)]
pub enum PgpKmsError {
    #[error("x509 error: {0:?}")]
    X509(#[from] X509CertificateError),

    #[error("asn.1 decode error: {0}")]
    Asn1Decode(bcder::decode::Error),

    #[error("PGP error: {0:?}")]
    Pgp(#[from] pgp::errors::Error),

    #[error("KMS DescribeKeyError: {0:?}")]
    DescribeError(#[from] RusotoError<DescribeKeyError>),

    #[error("KMS GetPublicKeyError: {0:?}")]
    GetPublicKeyError(#[from] RusotoError<GetPublicKeyError>),

    #[error("KMS SignError: {0:?}")]
    SignError(#[from] RusotoError<SignError>),

    #[error("no key metadata found")]
    MissingKeyMetadata,

    #[error("no public key data found")]
    MissingPublicKey,

    #[error("failed parsing elliptic curve public key: {0}")]
    EllipticCurvePublicKeyParse(&'static str),

    #[error("unrecognized key algorithm {0:?}")]
    UnrecognizedKeyAlgorithm(KeyAlgorithm),
}

fn tokio_runtime() -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
}

/// Public part of a PGP key hosted by KMS.
///
/// This type implements [KeyTrait] and [PublicKeyTrait]. [PublicKeyTrait] enables
/// signature verification and encryption, as these operations only require access
/// to the public key.
///
/// This type is an _offline_ type and will not incur KMS API calls once constructed.
/// However, constructing instances may require KMS API calls in order to obtain key
/// metadata and the public key.
#[derive(Clone, Debug)]
pub struct KmsPgpPublicKey {
    metadata: KeyMetadata,
    public_key_algorithm: PublicKeyAlgorithm,
    public_params: PublicParams,
}

impl KmsPgpPublicKey {
    /// Construct an instance using a [KmsClient] and a KMS key ID.
    ///
    /// This function will issue KMS `DescribeKey` and `GetPublicKey` API calls to obtain
    /// data for the given KMS key ID.
    ///
    /// The Key ID can be specified as its UUID id (e.g. `1234abcd-12ab-34cd-56ef-1234567890ab`),
    /// its ARN (e.g. `arn:aws:kms:us-east-2:111122223333:key/1234abcd-12ab-34cd-56ef-1234567890ab`)
    /// an alias (e.g. `alias/ExampleAlias`) or an alias ARN (e.g.
    /// `arn:aws:kms:us-east-2:111122223333:alias/ExampleAlias`).
    ///
    /// The [KmsClient] is discarded once it is used in to produce the returned value
    /// and will not be used further by the returned value.
    pub async fn from_kms_client(
        client: &KmsClient,
        key_id: String,
        grant_tokens: Option<Vec<String>>,
    ) -> Result<Self, PgpKmsError> {
        let key2 = key_id.clone();
        let grant_tokens_2 = grant_tokens.clone();

        let (describe, public_key) = tokio::join!(
            async {
                client
                    .describe_key(DescribeKeyRequest {
                        grant_tokens: grant_tokens_2,
                        key_id: key2,
                    })
                    .await
            },
            async {
                client
                    .get_public_key(GetPublicKeyRequest {
                        grant_tokens,
                        key_id,
                    })
                    .await
            }
        );

        Self::from_kms_responses(describe?, public_key?)
    }

    /// Construct an instance from KMS API response messages.
    ///
    /// The arguments are the responses from `DescribeKey` and `GetPublicKey` APIs.
    pub fn from_kms_responses(
        describe: DescribeKeyResponse,
        public_key: GetPublicKeyResponse,
    ) -> Result<Self, PgpKmsError> {
        let metadata = describe
            .key_metadata
            .ok_or(PgpKmsError::MissingKeyMetadata)?;

        let public_key_data = public_key.public_key.ok_or(PgpKmsError::MissingPublicKey)?;

        let spki = Constructed::decode(public_key_data.as_ref(), Mode::Der, |cons| {
            SubjectPublicKeyInfo::take_from(cons)
        })
        .map_err(PgpKmsError::Asn1Decode)?;

        let key_algorithm = KeyAlgorithm::try_from(&spki.algorithm.algorithm)?;

        let (public_key_algorithm, public_params) = match key_algorithm {
            KeyAlgorithm::Rsa => {
                let rsa_public_key = Constructed::decode(
                    spki.subject_public_key.octet_bytes().as_ref(),
                    Mode::Der,
                    |cons| RsaPublicKey::take_from(cons),
                )
                .map_err(PgpKmsError::Asn1Decode)?;

                (
                    PublicKeyAlgorithm::RSA,
                    PublicParams::RSA {
                        n: Mpi::from_raw(rsa_public_key.modulus.into_bytes().to_vec()),
                        e: Mpi::from_raw(rsa_public_key.public_exponent.into_bytes().to_vec()),
                    },
                )
            }
            KeyAlgorithm::Ecdsa(_) => {
                // The algorithm parameters are a curve OID.
                let oid = spki
                    .algorithm
                    .parameters
                    .ok_or({
                        PgpKmsError::EllipticCurvePublicKeyParse("algorithm parameters not present")
                    })?
                    .decode_oid()
                    .map_err(PgpKmsError::Asn1Decode)?;
                let curve = ecc_curve_from_oid(oid.as_ref())
                    .ok_or_else(|| pgp::errors::Error::Unsupported("unknown curve OID".into()))?;

                let p = Mpi::from_raw(spki.subject_public_key.octet_bytes().to_vec());

                // EdDSA only for signing / ECDH only for encryption.
                if metadata.signing_algorithms.is_some() {
                    (
                        PublicKeyAlgorithm::EdDSA,
                        PublicParams::EdDSA { curve, q: p },
                    )
                } else {
                    (
                        PublicKeyAlgorithm::ECDH,
                        PublicParams::ECDH {
                            curve,
                            p,
                            hash: HashAlgorithm::default(),
                            alg_sym: SymmetricKeyAlgorithm::AES256,
                        },
                    )
                }
            }
            _ => return Err(PgpKmsError::UnrecognizedKeyAlgorithm(key_algorithm)),
        };

        Ok(Self {
            metadata,
            public_key_algorithm,
            public_params,
        })
    }

    /// Convert this instance to a [KmsPgpPrivateKey].
    pub fn as_private_key(&self, client: KmsClient) -> KmsPgpPrivateKey {
        KmsPgpPrivateKey {
            client,
            public_key: self.clone(),
        }
    }

    /// Convert to a _composed_ [PublicKey].
    ///
    /// Whereas this type represents a handle on the raw public key material, the
    /// returned type represents a PGP flavored handle on the public key. It is
    /// essentially a wrapper with additional metadata such as the primary user ID
    /// and what the operations the key is allowed to perform. This metadata is
    /// provided via a [KeyDetails] instance.
    pub fn as_public_key(&self, details: KeyDetails) -> Result<PublicKey, PgpKmsError> {
        let data = self.to_bytes()?;

        let primary_key = pgp::packet::PublicKey::from_slice(Version::New, &data)?;

        Ok(PublicKey::new(primary_key, details, vec![]))
    }

    fn created_at(&self) -> u32 {
        self.metadata.creation_date.unwrap_or_default().round() as u32
    }
}

impl KeyTrait for KmsPgpPublicKey {
    fn fingerprint(&self) -> Vec<u8> {
        // The fingerprint is defined by https://datatracker.ietf.org/doc/html/rfc4880#section-12.2:
        //
        // Essentially the SHA-1 digest of:
        // * 0x99
        // * u16be length of following data.
        // * Version number
        // * u32be timestamp of key creation
        // * Algorithm specific fields.

        let mut packet = vec![4u8];

        let creation_time = self.metadata.creation_date.unwrap_or_default().round() as u64;
        packet.extend_from_slice(&creation_time.to_be_bytes());
        packet.push(self.algorithm() as u8);
        self.public_params
            .to_writer(&mut packet)
            .expect("writing public key parameters should never fail");

        let mut h = Sha1::new();
        h.update(&[0x99]);
        h.update((packet.len() as u16).to_be_bytes().as_slice());
        h.update(&packet);

        h.digest().bytes().to_vec()
    }

    fn key_id(&self) -> KeyId {
        // The key ID is the lower 64 bits of the fingerprint.
        // Lower 64 bits
        let fingerprint = self.fingerprint();

        KeyId::from_slice(&fingerprint[fingerprint.len() - 8..])
            .expect("KeyId should always be derivable from fingerprint")
    }

    fn algorithm(&self) -> PublicKeyAlgorithm {
        self.public_key_algorithm
    }

    fn is_signing_key(&self) -> bool {
        self.metadata.signing_algorithms.is_some()
    }

    fn is_encryption_key(&self) -> bool {
        self.metadata.encryption_algorithms.is_some()
    }
}

impl PublicKeyTrait for KmsPgpPublicKey {
    fn verify_signature(
        &self,
        hash: HashAlgorithm,
        data: &[u8],
        sig: &[Mpi],
    ) -> pgp::errors::Result<()> {
        match self.public_params {
            PublicParams::RSA { ref n, ref e } => {
                assert_eq!(sig.len(), 1);

                pgp::crypto::rsa::verify(n.as_bytes(), e.as_bytes(), hash, data, sig[0].as_bytes())
            }
            PublicParams::EdDSA { ref curve, ref q } => {
                pgp::crypto::eddsa::verify(curve, q, hash, data, sig)
            }
            _ => Err(pgp::errors::Error::Unsupported(format!(
                "verification of {:?} signatures not supported",
                self.public_params
            ))),
        }
    }

    fn encrypt<R: CryptoRng + Rng>(
        &self,
        rng: &mut R,
        plain: &[u8],
    ) -> pgp::errors::Result<Vec<Mpi>> {
        let res = match self.public_params {
            PublicParams::RSA { ref n, ref e } => {
                pgp::crypto::rsa::encrypt(rng, n.as_bytes(), e.as_bytes(), plain)
            }
            PublicParams::ECDH {
                ref curve,
                ref p,
                hash,
                alg_sym,
            } => {
                pgp::crypto::ecdh::encrypt(rng, curve, alg_sym, hash, &self.fingerprint(), p, plain)
            }
            _ => Err(pgp::errors::Error::Unsupported(
                "encryption not supported for key type".into(),
            )),
        }?;

        Ok(res
            .iter()
            .map(|v| Mpi::from_raw_slice(&v[..]))
            .collect::<Vec<_>>())
    }

    fn to_writer_old(&self, _: &mut impl Write) -> pgp::errors::Result<()> {
        unimplemented!()
    }
}

impl Serialize for KmsPgpPublicKey {
    fn to_writer<W: Write>(&self, writer: &mut W) -> pgp::errors::Result<()> {
        writer.write_all(&[KeyVersion::V4 as u8])?;

        writer.write_u32::<BigEndian>(self.created_at())?;
        writer.write_all(&[self.algorithm() as u8])?;
        self.public_params.to_writer(writer)?;

        Ok(())
    }
}

impl PacketTrait for KmsPgpPublicKey {
    fn packet_version(&self) -> Version {
        Version::New
    }

    fn tag(&self) -> Tag {
        Tag::PublicKey
    }
}

#[derive(Clone)]
pub struct KmsPgpPrivateKey {
    client: KmsClient,
    public_key: KmsPgpPublicKey,
}

impl KmsPgpPrivateKey {
    /// Construct an instance using a [KmsClient] and a KMS key ID.
    pub async fn from_kms_client(
        client: KmsClient,
        key_id: String,
        grant_tokens: Option<Vec<String>>,
    ) -> Result<Self, PgpKmsError> {
        let public_key = KmsPgpPublicKey::from_kms_client(&client, key_id, grant_tokens).await?;

        Ok(Self { client, public_key })
    }

    /// Construct an instance from KMS response messages.
    ///
    /// The instance will be bound to the specified [KmsClient], which will be used
    /// for signing operations.
    pub fn from_kms_responses(
        client: KmsClient,
        describe: DescribeKeyResponse,
        public_key: GetPublicKeyResponse,
    ) -> Result<Self, PgpKmsError> {
        let public_key = KmsPgpPublicKey::from_kms_responses(describe, public_key)?;

        Ok(Self { client, public_key })
    }
}

impl std::fmt::Debug for KmsPgpPrivateKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KmsPgpPrivateKey")
            .field("client", &"<KmsClient>")
            .field("public_key", &self.public_key)
            .finish()
    }
}

impl PublicKeyTrait for KmsPgpPrivateKey {
    fn verify_signature(
        &self,
        hash: HashAlgorithm,
        data: &[u8],
        sig: &[Mpi],
    ) -> pgp::errors::Result<()> {
        self.public_key.verify_signature(hash, data, sig)
    }

    fn encrypt<R: CryptoRng + Rng>(
        &self,
        rng: &mut R,
        plain: &[u8],
    ) -> pgp::errors::Result<Vec<Mpi>> {
        self.public_key.encrypt(rng, plain)
    }

    fn to_writer_old(&self, writer: &mut impl Write) -> pgp::errors::Result<()> {
        self.public_key.to_writer_old(writer)
    }
}

impl KeyTrait for KmsPgpPrivateKey {
    fn fingerprint(&self) -> Vec<u8> {
        self.public_key.fingerprint()
    }

    fn key_id(&self) -> KeyId {
        self.public_key.key_id()
    }

    fn algorithm(&self) -> PublicKeyAlgorithm {
        self.public_key.algorithm()
    }

    fn is_signing_key(&self) -> bool {
        self.public_key.is_signing_key()
    }

    fn is_encryption_key(&self) -> bool {
        self.public_key.is_encryption_key()
    }
}

impl SecretKeyTrait for KmsPgpPrivateKey {
    type PublicKey = KmsPgpPublicKey;

    fn unlock<F, G>(&self, _pw: F, _work: G) -> pgp::errors::Result<()>
    where
        F: FnOnce() -> String,
        G: FnOnce(&SecretKeyRepr) -> pgp::errors::Result<()>,
    {
        // AWS hosted keys have no concept of locking.
        Ok(())
    }

    fn create_signature<F>(
        &self,
        _key_pw: F,
        hash: HashAlgorithm,
        data: &[u8],
    ) -> pgp::errors::Result<Vec<Mpi>>
    where
        F: FnOnce() -> String,
    {
        let signing_algorithm = match self.public_key.public_key_algorithm {
            PublicKeyAlgorithm::RSA => match hash {
                HashAlgorithm::SHA2_256 => "RSASSA_PKCS1_V1_5_SHA_256".to_string(),
                HashAlgorithm::SHA2_384 => "RSASSA_PKCS1_V1_5_SHA_384".to_string(),
                HashAlgorithm::SHA2_512 => "RSASSA_PKCS1_V1_5_SHA_512".to_string(),
                alg => {
                    return Err(pgp::errors::Error::Unsupported(format!(
                        "KMS signatures cannot use hash {:?}",
                        alg
                    )))
                }
            },
            PublicKeyAlgorithm::ECDH => match hash {
                HashAlgorithm::SHA2_256 => "ECDSA_SHA_256".to_string(),
                HashAlgorithm::SHA2_384 => "ECDSA_SHA_384".to_string(),
                HashAlgorithm::SHA2_512 => "ECDSA_SHA_512".to_string(),
                alg => {
                    return Err(pgp::errors::Error::Unsupported(format!(
                        "KMS signatures cannot use hash {:?}",
                        alg
                    )))
                }
            },
            alg => {
                return Err(pgp::errors::Error::Unsupported(format!(
                    "unsupported signing algorithm: {:?}",
                    alg
                )))
            }
        };

        let req = SignRequest {
            grant_tokens: None,
            key_id: self.public_key.metadata.key_id.clone(),
            message: bytes::Bytes::copy_from_slice(data),
            message_type: Some("DIGEST".to_string()),
            signing_algorithm,
        };

        let call = async { self.client.sign(req).await };

        let response = tokio_runtime()?
            .block_on(call)
            .map_err(|e| pgp::errors::Error::Message(format!("KMS sign error: {0:?}", e)))?;

        let signature = response
            .signature
            .ok_or_else(|| pgp::errors::Error::Message("signature data not present".to_string()))?;

        let signature = Mpi::from_raw(signature.to_vec());

        Ok(vec![signature])
    }

    fn public_key(&self) -> Self::PublicKey {
        self.public_key.clone()
    }
}

#[cfg(test)]
mod test {
    use {
        super::*,
        pgp::packet::{KeyFlags, UserId},
    };

    const ECC_NIST_P256_DESCRIBE_JSON: &str = include_str!("testdata/ecc_nist_p256-describe.json");
    const ECC_NIST_P256_PUBLIC_KEY_JSON: &str =
        include_str!("testdata/ecc_nist_p256-public-key.json");
    const ECC_NIST_P384_DESCRIBE_JSON: &str = include_str!("testdata/ecc_nist_p384-describe.json");
    const ECC_NIST_P384_PUBLIC_KEY_JSON: &str =
        include_str!("testdata/ecc_nist_p384-public-key.json");
    const ECC_NIST_P521_DESCRIBE_JSON: &str = include_str!("testdata/ecc_nist_p521-describe.json");
    const ECC_NIST_P521_PUBLIC_KEY_JSON: &str =
        include_str!("testdata/ecc_nist_p521-public-key.json");
    const ECC_SECG_P256K1_DESCRIBE_JSON: &str =
        include_str!("testdata/ecc_secg_p256k1-describe.json");
    const ECC_SECG_P256K1_PUBLIC_KEY_JSON: &str =
        include_str!("testdata/ecc_secg_p256k1-public-key.json");
    const RSA_2048_DESCRIBE_JSON: &str = include_str!("testdata/rsa_2048-describe.json");
    const RSA_2048_PUBLIC_KEY_JSON: &str = include_str!("testdata/rsa_2048-public-key.json");
    const RSA_3072_DESCRIBE_JSON: &str = include_str!("testdata/rsa_3072-describe.json");
    const RSA_3072_PUBLIC_KEY_JSON: &str = include_str!("testdata/rsa_3072-public-key.json");
    const RSA_4096_DESCRIBE_JSON: &str = include_str!("testdata/rsa_4096-describe.json");
    const RSA_4096_PUBLIC_KEY_JSON: &str = include_str!("testdata/rsa_4096-public-key.json");

    fn private_key(describe: &str, public_key: &str) -> Result<KmsPgpPrivateKey, PgpKmsError> {
        let describe = serde_json::from_str::<DescribeKeyResponse>(describe).unwrap();
        let public_key = serde_json::from_str::<GetPublicKeyResponse>(public_key).unwrap();

        let client = KmsClient::new(rusoto_core::Region::UsWest1);

        KmsPgpPrivateKey::from_kms_responses(client, describe, public_key)
    }

    fn ecc_nist_p256() -> Result<KmsPgpPrivateKey, PgpKmsError> {
        private_key(ECC_NIST_P256_DESCRIBE_JSON, ECC_NIST_P256_PUBLIC_KEY_JSON)
    }

    fn ecc_nist_p384() -> Result<KmsPgpPrivateKey, PgpKmsError> {
        private_key(ECC_NIST_P384_DESCRIBE_JSON, ECC_NIST_P384_PUBLIC_KEY_JSON)
    }

    fn ecc_nist_p521() -> Result<KmsPgpPrivateKey, PgpKmsError> {
        private_key(ECC_NIST_P521_DESCRIBE_JSON, ECC_NIST_P521_PUBLIC_KEY_JSON)
    }

    fn ecc_secg_p256k1() -> Result<KmsPgpPrivateKey, PgpKmsError> {
        private_key(
            ECC_SECG_P256K1_DESCRIBE_JSON,
            ECC_SECG_P256K1_PUBLIC_KEY_JSON,
        )
    }

    fn rsa_2048() -> Result<KmsPgpPrivateKey, PgpKmsError> {
        private_key(RSA_2048_DESCRIBE_JSON, RSA_2048_PUBLIC_KEY_JSON)
    }

    fn rsa_3072() -> Result<KmsPgpPrivateKey, PgpKmsError> {
        private_key(RSA_3072_DESCRIBE_JSON, RSA_3072_PUBLIC_KEY_JSON)
    }

    fn rsa_4096() -> Result<KmsPgpPrivateKey, PgpKmsError> {
        private_key(RSA_4096_DESCRIBE_JSON, RSA_4096_PUBLIC_KEY_JSON)
    }

    #[test]
    fn rsa_construct() -> Result<(), PgpKmsError> {
        rsa_2048()?;
        rsa_3072()?;
        rsa_4096()?;

        Ok(())
    }

    #[test]
    fn ecc_construct() -> Result<(), PgpKmsError> {
        ecc_nist_p256()?;
        ecc_nist_p384()?;
        ecc_nist_p521()?;
        ecc_secg_p256k1()?;

        //let digest = b"\x00".repeat(32);
        //let sig = key.create_signature(|| "".into(), HashAlgorithm::SHA2_256, &digest)?;
        //key.verify_signature(HashAlgorithm::SHA2_256, &digest, &sig)?;

        Ok(())
    }

    #[test]
    fn rsa_composed() -> Result<(), PgpKmsError> {
        let key = rsa_4096()?;

        let mut key_flags = KeyFlags::default();
        key_flags.set_sign(true);

        let details = KeyDetails::new(
            UserId::from_str(Version::New, "Me <me@example.com>"),
            vec![],
            vec![],
            key_flags,
            Default::default(),
            Default::default(),
            Default::default(),
            None,
        );

        let public_key = key.public_key.as_public_key(details)?;

        assert!(public_key.is_signing_key());
        assert!(public_key.is_encryption_key());

        Ok(())
    }
}
