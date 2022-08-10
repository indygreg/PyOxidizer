// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Common cryptography primitives.

use {
    crate::{
        remote_signing::{session_negotiation::PublicKeyPeerDecrypt, RemoteSignError},
        AppleCodesignError,
    },
    bytes::Bytes,
    der::{asn1, Document, Encodable},
    elliptic_curve::{
        sec1::{FromEncodedPoint, ModulusSize, ToEncodedPoint},
        AffinePoint, Curve, FieldSize, ProjectiveArithmetic, SecretKey as ECSecretKey,
    },
    oid_registry::{
        OID_EC_P256, OID_KEY_TYPE_EC_PUBLIC_KEY, OID_PKCS1_RSAENCRYPTION, OID_SIG_ED25519,
    },
    p256::NistP256,
    pkcs1::RsaPrivateKeyDocument,
    pkcs8::{
        AlgorithmIdentifier, EncodePrivateKey, ObjectIdentifier, PrivateKeyDocument, PrivateKeyInfo,
    },
    ring::signature::{EcdsaKeyPair, Ed25519KeyPair, KeyPair, RsaKeyPair},
    rsa::{
        algorithms::mgf1_xor, pkcs1::DecodeRsaPrivateKey, BigUint, PaddingScheme,
        RsaPrivateKey as RsaConstructedKey,
    },
    signature::Signer,
    subtle::{Choice, ConditionallySelectable, ConstantTimeEq, CtOption},
    x509_certificate::{
        CapturedX509Certificate, EcdsaCurve, InMemorySigningKeyPair, KeyAlgorithm, KeyInfoSigner,
        Sign, Signature, SignatureAlgorithm, X509CertificateError,
    },
    zeroize::Zeroizing,
};

/// A supertrait generically describing a private key capable of signing and possibly decryption.
pub trait PrivateKey: KeyInfoSigner {
    fn as_key_info_signer(&self) -> &dyn KeyInfoSigner;

    fn to_public_key_peer_decrypt(
        &self,
    ) -> Result<Box<dyn PublicKeyPeerDecrypt>, AppleCodesignError>;

    /// Signals the end of operations on the private key.
    ///
    /// Implementations can use this to do things like destroy private key matter, disconnect
    /// from a hardware device, etc.
    fn finish(&self) -> Result<(), AppleCodesignError>;
}

#[derive(Clone, Debug)]
pub struct InMemoryRsaKey {
    private_key: RsaPrivateKeyDocument,
}

impl From<&InMemoryRsaKey> for RsaConstructedKey {
    fn from(key: &InMemoryRsaKey) -> Self {
        let key = key.private_key.decode();

        let n = BigUint::from_bytes_be(key.modulus.as_bytes());
        let e = BigUint::from_bytes_be(key.public_exponent.as_bytes());
        let d = BigUint::from_bytes_be(key.private_exponent.as_bytes());
        let prime1 = BigUint::from_bytes_be(key.prime1.as_bytes());
        let prime2 = BigUint::from_bytes_be(key.prime2.as_bytes());
        let primes = vec![prime1, prime2];

        Self::from_components(n, e, d, primes)
    }
}

impl TryFrom<InMemoryRsaKey> for InMemorySigningKeyPair {
    type Error = AppleCodesignError;

    fn try_from(value: InMemoryRsaKey) -> Result<Self, Self::Error> {
        let key_pair = RsaKeyPair::from_der(value.private_key.as_der()).map_err(|e| {
            AppleCodesignError::CertificateGeneric(format!(
                "error importing RSA key to ring: {}",
                e
            ))
        })?;

        Ok(InMemorySigningKeyPair::Rsa(
            key_pair,
            value.private_key.as_ref().to_vec(),
        ))
    }
}

impl EncodePrivateKey for InMemoryRsaKey {
    fn to_pkcs8_der(&self) -> pkcs8::Result<PrivateKeyDocument> {
        PrivateKeyInfo::new(pkcs1::ALGORITHM_ID, self.private_key.as_der()).to_der()
    }
}

impl PublicKeyPeerDecrypt for InMemoryRsaKey {
    fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, RemoteSignError> {
        let key = RsaConstructedKey::from_pkcs1_der(self.private_key.as_der())
            .map_err(|e| RemoteSignError::Crypto(format!("failed to parse RSA key: {}", e)))?;

        let padding = PaddingScheme::new_oaep::<sha2::Sha256>();

        let plaintext = key
            .decrypt(padding, ciphertext)
            .map_err(|e| RemoteSignError::Crypto(format!("RSA decryption failure: {}", e)))?;

        Ok(plaintext)
    }
}

#[derive(Clone, Debug)]
pub struct InMemoryEcdsaKey<C>
where
    C: Curve + ProjectiveArithmetic,
    AffinePoint<C>: FromEncodedPoint<C> + ToEncodedPoint<C>,
    FieldSize<C>: ModulusSize,
{
    curve: ObjectIdentifier,
    secret_key: ECSecretKey<C>,
}

impl<'a, C> InMemoryEcdsaKey<C>
where
    C: Curve + ProjectiveArithmetic,
    AffinePoint<C>: FromEncodedPoint<C> + ToEncodedPoint<C>,
    FieldSize<C>: ModulusSize,
{
    pub fn curve(&self) -> Result<EcdsaCurve, AppleCodesignError> {
        match self.curve.as_bytes() {
            x if x == OID_EC_P256.as_bytes() => Ok(EcdsaCurve::Secp256r1),
            _ => Err(AppleCodesignError::CertificateGeneric(format!(
                "unknown ECDSA curve: {}",
                self.curve
            ))),
        }
    }
}

impl<C> TryFrom<InMemoryEcdsaKey<C>> for InMemorySigningKeyPair
where
    C: Curve + ProjectiveArithmetic,
    AffinePoint<C>: FromEncodedPoint<C> + ToEncodedPoint<C>,
    FieldSize<C>: ModulusSize,
{
    type Error = AppleCodesignError;

    fn try_from(key: InMemoryEcdsaKey<C>) -> Result<Self, Self::Error> {
        let curve = key.curve()?;

        let private_key = key.secret_key.to_be_bytes();
        let public_key = key.secret_key.public_key().to_encoded_point(false);

        let key_pair = EcdsaKeyPair::from_private_key_and_public_key(
            curve.into(),
            private_key.as_ref(),
            public_key.as_bytes(),
        )
        .map_err(|e| {
            AppleCodesignError::CertificateGeneric(format!(
                "unable to convert ECDSA private key: {}",
                e
            ))
        })?;

        Ok(Self::Ecdsa(key_pair, curve, vec![]))
    }
}

impl<C> EncodePrivateKey for InMemoryEcdsaKey<C>
where
    C: Curve + ProjectiveArithmetic,
    AffinePoint<C>: FromEncodedPoint<C> + ToEncodedPoint<C>,
    FieldSize<C>: ModulusSize,
{
    fn to_pkcs8_der(&self) -> pkcs8::Result<PrivateKeyDocument> {
        let private_key = self.secret_key.to_sec1_der()?;

        PrivateKeyInfo {
            algorithm: AlgorithmIdentifier {
                oid: ObjectIdentifier::from_bytes(OID_KEY_TYPE_EC_PUBLIC_KEY.as_bytes())
                    .expect("OID construction should work"),
                parameters: Some(asn1::Any::from(&self.curve)),
            },
            private_key: private_key.as_ref(),
            public_key: None,
        }
        .try_into()
    }
}

impl<C> PublicKeyPeerDecrypt for InMemoryEcdsaKey<C>
where
    C: Curve + ProjectiveArithmetic,
    AffinePoint<C>: FromEncodedPoint<C> + ToEncodedPoint<C>,
    FieldSize<C>: ModulusSize,
{
    fn decrypt(&self, _ciphertext: &[u8]) -> Result<Vec<u8>, RemoteSignError> {
        Err(RemoteSignError::Crypto(
            "decryption using ECDSA keys is not yet implemented".into(),
        ))
    }
}

#[derive(Clone, Debug)]
pub struct InMemoryEd25519Key {
    private_key: Zeroizing<Vec<u8>>,
}

impl TryFrom<InMemoryEd25519Key> for InMemorySigningKeyPair {
    type Error = AppleCodesignError;

    fn try_from(key: InMemoryEd25519Key) -> Result<Self, Self::Error> {
        let key_pair =
            Ed25519KeyPair::from_seed_unchecked(key.private_key.as_ref()).map_err(|e| {
                AppleCodesignError::CertificateGeneric(format!(
                    "unable to convert ED25519 private key: {}",
                    e
                ))
            })?;

        Ok(Self::Ed25519(key_pair))
    }
}

impl EncodePrivateKey for InMemoryEd25519Key {
    fn to_pkcs8_der(&self) -> pkcs8::Result<PrivateKeyDocument> {
        let algorithm = AlgorithmIdentifier {
            oid: ObjectIdentifier::from_bytes(OID_SIG_ED25519.as_bytes()).expect("OID is valid"),
            parameters: None,
        };

        let value = Zeroizing::new(asn1::OctetString::new(self.private_key.as_ref())?.to_vec()?);

        PrivateKeyInfo::new(algorithm, value.as_ref()).try_into()
    }
}

impl PublicKeyPeerDecrypt for InMemoryEd25519Key {
    fn decrypt(&self, _ciphertext: &[u8]) -> Result<Vec<u8>, RemoteSignError> {
        Err(RemoteSignError::Crypto(
            "decryption using ED25519 keys is not yet implemented".into(),
        ))
    }
}

/// Holds a private key in memory.
#[derive(Clone, Debug)]
pub enum InMemoryPrivateKey {
    /// ECDSA private key using Nist P256 curve.
    EcdsaP256(InMemoryEcdsaKey<NistP256>),
    /// ED25519 private key.
    Ed25519(InMemoryEd25519Key),
    /// RSA private key.
    Rsa(InMemoryRsaKey),
}

impl<'a> TryFrom<PrivateKeyInfo<'a>> for InMemoryPrivateKey {
    type Error = pkcs8::Error;

    fn try_from(value: PrivateKeyInfo<'a>) -> Result<Self, Self::Error> {
        match value.algorithm.oid {
            x if x.as_bytes() == OID_PKCS1_RSAENCRYPTION.as_bytes() => {
                let private_key = RsaPrivateKeyDocument::from_der(value.private_key)?;

                Ok(Self::Rsa(InMemoryRsaKey { private_key }))
            }
            x if x.as_bytes() == OID_KEY_TYPE_EC_PUBLIC_KEY.as_bytes() => {
                let curve_oid = value.algorithm.parameters_oid()?;

                match curve_oid.as_bytes() {
                    x if x == OID_EC_P256.as_bytes() => {
                        let secret_key = ECSecretKey::<NistP256>::try_from(value)?;

                        Ok(Self::EcdsaP256(InMemoryEcdsaKey {
                            curve: curve_oid,
                            secret_key,
                        }))
                    }
                    _ => Err(pkcs8::Error::ParametersMalformed),
                }
            }
            x if x.as_bytes() == OID_SIG_ED25519.as_bytes() => {
                // The private key seed should start at byte offset 2.
                Ok(Self::Ed25519(InMemoryEd25519Key {
                    private_key: Zeroizing::new((&value.private_key[2..]).to_vec()),
                }))
            }
            _ => Err(pkcs8::Error::KeyMalformed),
        }
    }
}

impl TryFrom<InMemoryPrivateKey> for InMemorySigningKeyPair {
    type Error = AppleCodesignError;

    fn try_from(key: InMemoryPrivateKey) -> Result<Self, Self::Error> {
        match key {
            InMemoryPrivateKey::Rsa(key) => key.try_into(),
            InMemoryPrivateKey::EcdsaP256(key) => key.try_into(),
            InMemoryPrivateKey::Ed25519(key) => key.try_into(),
        }
    }
}

impl EncodePrivateKey for InMemoryPrivateKey {
    fn to_pkcs8_der(&self) -> pkcs8::Result<PrivateKeyDocument> {
        match self {
            Self::EcdsaP256(key) => key.to_pkcs8_der(),
            Self::Ed25519(key) => key.to_pkcs8_der(),
            Self::Rsa(key) => key.to_pkcs8_der(),
        }
    }
}

impl Signer<Signature> for InMemoryPrivateKey {
    fn try_sign(&self, msg: &[u8]) -> Result<Signature, signature::Error> {
        let key_pair = InMemorySigningKeyPair::try_from(self.clone())
            .map_err(signature::Error::from_source)?;

        key_pair.try_sign(msg)
    }
}

impl Sign for InMemoryPrivateKey {
    fn sign(&self, message: &[u8]) -> Result<(Vec<u8>, SignatureAlgorithm), X509CertificateError> {
        let algorithm = self.signature_algorithm()?;

        Ok((self.try_sign(message)?.into(), algorithm))
    }

    fn key_algorithm(&self) -> Option<KeyAlgorithm> {
        Some(match self {
            Self::EcdsaP256(_) => KeyAlgorithm::Ecdsa(EcdsaCurve::Secp256r1),
            Self::Ed25519(_) => KeyAlgorithm::Ed25519,
            Self::Rsa(_) => KeyAlgorithm::Rsa,
        })
    }

    fn public_key_data(&self) -> Bytes {
        match self {
            Self::EcdsaP256(key) => Bytes::copy_from_slice(
                key.secret_key
                    .public_key()
                    .to_encoded_point(false)
                    .as_bytes(),
            ),
            Self::Ed25519(key) => {
                if let Ok(key) = Ed25519KeyPair::from_seed_unchecked(key.private_key.as_ref()) {
                    Bytes::copy_from_slice(key.public_key().as_ref())
                } else {
                    Bytes::new()
                }
            }
            Self::Rsa(key) => Bytes::copy_from_slice(
                key.private_key
                    .decode()
                    .public_key()
                    .to_der()
                    .expect("RSA public key DER encoding should not fail")
                    .as_ref(),
            ),
        }
    }

    fn signature_algorithm(&self) -> Result<SignatureAlgorithm, X509CertificateError> {
        Ok(match self {
            Self::EcdsaP256(_) => SignatureAlgorithm::EcdsaSha256,
            Self::Ed25519(_) => SignatureAlgorithm::Ed25519,
            Self::Rsa(_) => SignatureAlgorithm::RsaSha256,
        })
    }

    fn private_key_data(&self) -> Option<Vec<u8>> {
        match self {
            Self::EcdsaP256(key) => Some(key.secret_key.to_be_bytes().to_vec()),
            Self::Ed25519(key) => Some((*key.private_key).clone()),
            Self::Rsa(key) => Some(key.private_key.as_ref().to_vec()),
        }
    }

    fn rsa_primes(&self) -> Result<Option<(Vec<u8>, Vec<u8>)>, X509CertificateError> {
        if let Self::Rsa(key) = self {
            Ok(Some((
                key.private_key.decode().prime1.as_bytes().to_vec(),
                key.private_key.decode().prime2.as_bytes().to_vec(),
            )))
        } else {
            Ok(None)
        }
    }
}

impl KeyInfoSigner for InMemoryPrivateKey {}

impl PublicKeyPeerDecrypt for InMemoryPrivateKey {
    fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, RemoteSignError> {
        match self {
            Self::Rsa(key) => key.decrypt(ciphertext),
            Self::EcdsaP256(key) => key.decrypt(ciphertext),
            Self::Ed25519(key) => key.decrypt(ciphertext),
        }
    }
}

impl PrivateKey for InMemoryPrivateKey {
    fn as_key_info_signer(&self) -> &dyn KeyInfoSigner {
        self
    }

    fn to_public_key_peer_decrypt(
        &self,
    ) -> Result<Box<dyn PublicKeyPeerDecrypt>, AppleCodesignError> {
        Ok(Box::new(self.clone()))
    }

    fn finish(&self) -> Result<(), AppleCodesignError> {
        Ok(())
    }
}

impl InMemoryPrivateKey {
    /// Construct an instance by parsing PKCS#8 DER data.
    pub fn from_pkcs8_der(data: impl AsRef<[u8]>) -> Result<Self, AppleCodesignError> {
        let pki = PrivateKeyInfo::try_from(data.as_ref()).map_err(|e| {
            AppleCodesignError::CertificateGeneric(format!("when parsing PKCS#8 data: {}", e))
        })?;

        pki.try_into().map_err(|e| {
            AppleCodesignError::CertificateGeneric(format!(
                "when converting parsed PKCS#8 to a private key: {}",
                e
            ))
        })
    }
}

fn bmp_string(s: &str) -> Vec<u8> {
    let utf16: Vec<u16> = s.encode_utf16().collect();

    let mut bytes = Vec::with_capacity(utf16.len() * 2 + 2);
    for c in utf16 {
        bytes.push((c / 256) as u8);
        bytes.push((c % 256) as u8);
    }
    bytes.push(0x00);
    bytes.push(0x00);

    bytes
}

/// Parse PFX data into a key pair.
///
/// PFX data is commonly encountered in `.p12` files, such as those created
/// when exporting certificates from Apple's `Keychain Access` application.
///
/// The contents of the PFX file require a password to decrypt. However, if
/// no password was provided to create the PFX data, this password may be the
/// empty string.
pub fn parse_pfx_data(
    data: &[u8],
    password: &str,
) -> Result<(CapturedX509Certificate, InMemoryPrivateKey), AppleCodesignError> {
    let pfx = p12::PFX::parse(data).map_err(|e| {
        AppleCodesignError::PfxParseError(format!("data does not appear to be PFX: {:?}", e))
    })?;

    if !pfx.verify_mac(password) {
        return Err(AppleCodesignError::PfxBadPassword);
    }

    // Apple's certificate export format consists of regular data content info
    // with inner ContentInfo components holding the key and certificate.
    let data = match pfx.auth_safe {
        p12::ContentInfo::Data(data) => data,
        _ => {
            return Err(AppleCodesignError::PfxParseError(
                "unexpected PFX content info".to_string(),
            ));
        }
    };

    let content_infos = yasna::parse_der(&data, |reader| {
        reader.collect_sequence_of(p12::ContentInfo::parse)
    })
    .map_err(|e| {
        AppleCodesignError::PfxParseError(format!("failed parsing inner ContentInfo: {:?}", e))
    })?;

    let bmp_password = bmp_string(password);

    let mut certificate = None;
    let mut signing_key = None;

    for content in content_infos {
        let bags_data = match content {
            p12::ContentInfo::Data(inner) => inner,
            p12::ContentInfo::EncryptedData(encrypted) => {
                encrypted.data(&bmp_password).ok_or_else(|| {
                    AppleCodesignError::PfxParseError(
                        "failed decrypting inner EncryptedData".to_string(),
                    )
                })?
            }
            p12::ContentInfo::OtherContext(_) => {
                return Err(AppleCodesignError::PfxParseError(
                    "unexpected OtherContent content in inner PFX data".to_string(),
                ));
            }
        };

        let bags = yasna::parse_ber(&bags_data, |reader| {
            reader.collect_sequence_of(p12::SafeBag::parse)
        })
        .map_err(|e| {
            AppleCodesignError::PfxParseError(format!(
                "failed parsing SafeBag within inner Data: {:?}",
                e
            ))
        })?;

        for bag in bags {
            match bag.bag {
                p12::SafeBagKind::CertBag(cert_bag) => match cert_bag {
                    p12::CertBag::X509(cert_data) => {
                        certificate = Some(CapturedX509Certificate::from_der(cert_data)?);
                    }
                    p12::CertBag::SDSI(_) => {
                        return Err(AppleCodesignError::PfxParseError(
                            "unexpected SDSI certificate data".to_string(),
                        ));
                    }
                },
                p12::SafeBagKind::Pkcs8ShroudedKeyBag(key_bag) => {
                    let decrypted = key_bag.decrypt(&bmp_password).ok_or_else(|| {
                        AppleCodesignError::PfxParseError(
                            "error decrypting PKCS8 shrouded key bag; is the password correct?"
                                .to_string(),
                        )
                    })?;

                    signing_key = Some(InMemoryPrivateKey::from_pkcs8_der(&decrypted)?);
                }
                p12::SafeBagKind::OtherBagKind(_) => {
                    return Err(AppleCodesignError::PfxParseError(
                        "unexpected bag type in inner PFX content".to_string(),
                    ));
                }
            }
        }
    }

    match (certificate, signing_key) {
        (Some(certificate), Some(signing_key)) => Ok((certificate, signing_key)),
        (None, Some(_)) => Err(AppleCodesignError::PfxParseError(
            "failed to find x509 certificate in PFX data".to_string(),
        )),
        (_, None) => Err(AppleCodesignError::PfxParseError(
            "failed to find signing key in PFX data".to_string(),
        )),
    }
}

/// RSA OAEP post decrypt depadding.
///
/// This implements the procedure described by RFC 3447 Section 7.1.2
/// starting at Step 3 (after the ciphertext has been fed into the low-level
/// RSA decryption.
///
/// This implementation has NOT been audited and shouldn't be used. It only
/// exists here because we need it to support RSA decryption using YubiKeys.
/// https://github.com/RustCrypto/RSA/issues/159 is fixed to hopefully get this
/// exposed as an API on the rsa crate.
#[allow(unused)]
pub(crate) fn rsa_oaep_post_decrypt_decode(
    modulus_length_bytes: usize,
    mut em: Vec<u8>,
    digest: &mut dyn digest::DynDigest,
    mgf_digest: &mut dyn digest::DynDigest,
    label: Option<String>,
) -> Result<Vec<u8>, rsa::errors::Error> {
    let k = modulus_length_bytes;
    let digest_len = digest.output_size();

    // 3. EME_OAEP decoding.

    // 3a.
    let label = label.unwrap_or_default();
    digest.update(label.as_bytes());
    let label_digest = digest.finalize_reset();

    // 3b.
    let (y, remaining) = em.split_at_mut(1);
    let (masked_seed, masked_db) = remaining.split_at_mut(digest_len);

    if masked_seed.len() != digest_len || masked_db.len() != k - digest_len - 1 {
        return Err(rsa::errors::Error::Decryption);
    }

    // 3c - 3f.
    mgf1_xor(masked_seed, mgf_digest, masked_db);
    mgf1_xor(masked_db, mgf_digest, masked_seed);

    // 3g.
    //
    // We need to split into padding string (all zeroes) and message M with a
    // 0x01 between them. The padding string should be all zeroes. And this should
    // execute in constant time, which makes it tricky.

    let digests_equivalent = masked_db[0..digest_len].ct_eq(label_digest.as_ref());

    let mut looking_for_index = Choice::from(1u8);
    let mut index = 0u32;
    let mut padding_invalid = Choice::from(0u8);

    for (i, value) in masked_db.iter().skip(digest_len).enumerate() {
        let is_zero = value.ct_eq(&0u8);
        let is_one = value.ct_eq(&1u8);

        index.conditional_assign(&(i as u32), looking_for_index & is_one);
        looking_for_index &= !is_one;
        padding_invalid |= looking_for_index & !is_zero;
    }

    let y_is_zero = y[0].ct_eq(&0u8);

    let valid = y_is_zero & digests_equivalent & !padding_invalid & !looking_for_index;

    let res = CtOption::new((em, index + 2 + (digest_len * 2) as u32), valid);

    if res.is_none().into() {
        return Err(rsa::errors::Error::Decryption);
    }

    let (out, index) = res.unwrap();

    Ok(out[index as usize..].to_vec())
}

#[cfg(test)]
mod test {
    use {super::*, ring::signature::KeyPair, x509_certificate::Sign};

    const RSA_2048_PKCS8_DER: &[u8] = include_bytes!("testdata/rsa-2048.pk8");
    const ED25519_PKCS8_DER: &[u8] = include_bytes!("testdata/ed25519.pk8");
    const SECP256_PKCS8_DER: &[u8] = include_bytes!("testdata/secp256r1.pk8");

    #[test]
    fn parse_keychain_p12_export() {
        let data = include_bytes!("apple-codesign-testuser.p12");

        let err = parse_pfx_data(data, "bad-password").unwrap_err();
        assert!(matches!(err, AppleCodesignError::PfxBadPassword));

        parse_pfx_data(data, "password123").unwrap();
    }

    #[test]
    fn rsa_key_operations() -> Result<(), AppleCodesignError> {
        let ring_key = RsaKeyPair::from_pkcs8(RSA_2048_PKCS8_DER).unwrap();
        let ring_public_key_data = ring_key.public_key().as_ref();

        let pki = PrivateKeyInfo::from_der(RSA_2048_PKCS8_DER).unwrap();
        let key = InMemoryPrivateKey::try_from(pki).unwrap();

        assert_eq!(key.to_pkcs8_der().unwrap().as_ref(), RSA_2048_PKCS8_DER);

        let our_key = InMemorySigningKeyPair::try_from(key)?;
        let our_public_key = our_key.public_key_data();

        assert_eq!(our_public_key.as_ref(), ring_public_key_data);

        InMemoryPrivateKey::from_pkcs8_der(RSA_2048_PKCS8_DER)?;

        Ok(())
    }

    #[test]
    fn ed25519_key_operations() -> Result<(), AppleCodesignError> {
        let pki = PrivateKeyInfo::from_der(ED25519_PKCS8_DER).unwrap();
        let seed = &pki.private_key[2..];
        let key = InMemoryPrivateKey::try_from(pki).unwrap();

        assert_eq!(key.to_pkcs8_der().unwrap().as_ref(), ED25519_PKCS8_DER);

        let our_key = InMemorySigningKeyPair::try_from(key)?;
        let our_public_key = our_key.public_key_data();

        let ring_key = Ed25519KeyPair::from_seed_unchecked(seed).unwrap();
        let ring_public_key_data = ring_key.public_key().as_ref();

        assert_eq!(our_public_key.as_ref(), ring_public_key_data);

        InMemoryPrivateKey::from_pkcs8_der(ED25519_PKCS8_DER)?;

        Ok(())
    }

    #[test]
    fn ecdsa_key_operations_secp256() -> Result<(), AppleCodesignError> {
        let ring_key = EcdsaKeyPair::from_pkcs8(
            &ring::signature::ECDSA_P256_SHA256_ASN1_SIGNING,
            SECP256_PKCS8_DER,
        )
        .unwrap();
        let ring_public_key_data = ring_key.public_key().as_ref();

        let pki = PrivateKeyInfo::from_der(SECP256_PKCS8_DER).unwrap();
        let key = InMemoryPrivateKey::try_from(pki).unwrap();

        assert_eq!(key.to_pkcs8_der().unwrap().as_ref(), SECP256_PKCS8_DER);

        let our_key = InMemorySigningKeyPair::try_from(key)?;
        let our_public_key = our_key.public_key_data();

        assert_eq!(our_public_key.as_ref(), ring_public_key_data);

        InMemoryPrivateKey::from_pkcs8_der(SECP256_PKCS8_DER)?;

        Ok(())
    }
}
