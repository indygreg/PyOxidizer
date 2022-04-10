// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Yubikey interaction.

use {
    crate::{
        cryptography::{rsa_oaep_post_decrypt_decode, PrivateKey},
        remote_signing::{session_negotiation::PublicKeyPeerDecrypt, RemoteSignError},
        AppleCodesignError,
    },
    bcder::encode::Values,
    bytes::Bytes,
    log::{error, warn},
    signature::Signer,
    std::ops::DerefMut,
    std::sync::{Arc, Mutex, MutexGuard},
    x509::SubjectPublicKeyInfo,
    x509_certificate::{
        asn1time, rfc3280, rfc5280, CapturedX509Certificate, EcdsaCurve, KeyAlgorithm,
        KeyInfoSigner, Sign, Signature, SignatureAlgorithm, X509CertificateError,
    },
    yubikey::{
        certificate::{CertInfo, Certificate as YkCertificate},
        piv::{import_ecc_key, import_rsa_key, AlgorithmId, SlotId},
        Error as YkError, MgmKey, YubiKey as RawYubiKey, {PinPolicy, TouchPolicy},
    },
    zeroize::Zeroizing,
};

/// A function that will attempt to resolve the PIN to unlock a YubiKey.
pub type PinCallback = fn() -> Result<Vec<u8>, AppleCodesignError>;

fn algorithm_from_certificate(
    cert: &CapturedX509Certificate,
) -> Result<AlgorithmId, X509CertificateError> {
    let key_algorithm = cert
        .key_algorithm()
        .ok_or(X509CertificateError::UnknownKeyAlgorithm(format!(
            "{:?}",
            cert.key_algorithm_oid()
        )))?;

    match key_algorithm {
        KeyAlgorithm::Rsa => match cert.rsa_public_key_data()?.modulus.as_slice().len() {
            129 => Ok(AlgorithmId::Rsa1024),
            257 => Ok(AlgorithmId::Rsa2048),
            _ => Err(X509CertificateError::Other(
                "unable to determine RSA key algorithm".into(),
            )),
        },
        KeyAlgorithm::Ed25519 => Err(X509CertificateError::UnknownKeyAlgorithm(
            "unable to use ed25519 keys with smartcards".into(),
        )),
        KeyAlgorithm::Ecdsa(curve) => match curve {
            EcdsaCurve::Secp256r1 => Ok(AlgorithmId::EccP256),
            EcdsaCurve::Secp384r1 => Ok(AlgorithmId::EccP384),
        },
    }
}

/// Describes the needed authentication for an operation.
pub enum RequiredAuthentication {
    Pin,
    ManagementKey,
    ManagementKeyAndPin,
}

impl RequiredAuthentication {
    pub fn requires_pin(&self) -> bool {
        match self {
            Self::Pin | Self::ManagementKeyAndPin => true,
            Self::ManagementKey => false,
        }
    }

    pub fn requires_management_key(&self) -> bool {
        match self {
            Self::ManagementKey | Self::ManagementKeyAndPin => true,
            Self::Pin => false,
        }
    }
}

fn attempt_authenticated_operation<T>(
    yk: &mut RawYubiKey,
    op: impl Fn(&mut RawYubiKey) -> Result<T, AppleCodesignError>,
    required_authentication: RequiredAuthentication,
    get_device_pin: Option<&PinCallback>,
) -> Result<T, AppleCodesignError> {
    const MAX_ATTEMPTS: u8 = 3;

    for attempt in 1..MAX_ATTEMPTS + 1 {
        warn!("attempt {}/{}", attempt, MAX_ATTEMPTS);

        match op(yk) {
            Ok(x) => {
                return Ok(x);
            }
            Err(AppleCodesignError::YubiKey(YkError::AuthenticationError)) => {
                // This was our last attempt. Give up now.
                if attempt == MAX_ATTEMPTS {
                    return Err(AppleCodesignError::SmartcardFailedAuthentication);
                }

                warn!("device refused operation due to authentication error");

                if required_authentication.requires_management_key() {
                    match yk.authenticate(MgmKey::default()) {
                        Ok(()) => {
                            warn!("management key authentication successful");
                        }
                        Err(e) => {
                            error!("management key authentication failure: {}", e);
                            continue;
                        }
                    }
                }

                if required_authentication.requires_pin() {
                    if let Some(pin_cb) = get_device_pin {
                        let pin = Zeroizing::new(pin_cb().map_err(|e| {
                            X509CertificateError::Other(format!(
                                "error retrieving device pin: {}",
                                e
                            ))
                        })?);

                        match yk.verify_pin(&pin) {
                            Ok(()) => {
                                warn!("pin verification successful");
                            }
                            Err(e) => {
                                error!("pin verification failure: {}", e);
                                continue;
                            }
                        }
                    } else {
                        warn!(
                            "unable to retrieve device pin; future attempts will fail; giving up"
                        );
                        return Err(AppleCodesignError::SmartcardFailedAuthentication);
                    }
                }
            }
            Err(e) => {
                return Err(e);
            }
        }
    }

    Err(AppleCodesignError::SmartcardFailedAuthentication)
}

/// Represents a connection to a yubikey device.
pub struct YubiKey {
    yk: Arc<Mutex<RawYubiKey>>,
    pin_callback: Option<PinCallback>,
}

impl From<RawYubiKey> for YubiKey {
    fn from(yk: RawYubiKey) -> Self {
        Self {
            yk: Arc::new(Mutex::new(yk)),
            pin_callback: None,
        }
    }
}

impl YubiKey {
    /// Construct a new instance.
    pub fn new() -> Result<Self, AppleCodesignError> {
        let yk = Arc::new(Mutex::new(RawYubiKey::open()?));

        Ok(Self {
            yk,
            pin_callback: None,
        })
    }

    /// Set a callback function to be used for retrieving the PIN.
    pub fn set_pin_callback(&mut self, cb: PinCallback) {
        self.pin_callback = Some(cb);
    }

    pub fn inner(&self) -> Result<MutexGuard<RawYubiKey>, AppleCodesignError> {
        self.yk.lock().map_err(|_| AppleCodesignError::PoisonedLock)
    }

    /// Find certificates in this device.
    pub fn find_certificates(
        &mut self,
    ) -> Result<Vec<(SlotId, CapturedX509Certificate)>, AppleCodesignError> {
        let mut guard = self.inner()?;
        let yk = guard.deref_mut();

        let slots = yk
            .piv_keys()?
            .into_iter()
            .map(|key| key.slot())
            .collect::<Vec<_>>();

        let mut res = vec![];

        for slot in slots {
            let cert = YkCertificate::read(yk, slot)?;

            let cert = CapturedX509Certificate::from_der(cert.into_buffer().to_vec())?;

            res.push((slot, cert));
        }

        Ok(res)
    }

    /// Obtain an entity for creating signatures using a certificate at a slot.
    pub fn get_certificate_signer(
        &mut self,
        slot_id: SlotId,
    ) -> Result<Option<CertificateSigner>, AppleCodesignError> {
        Ok(self
            .find_certificates()?
            .into_iter()
            .find_map(|(slot, cert)| {
                if slot == slot_id {
                    Some(CertificateSigner {
                        yk: self.yk.clone(),
                        slot: slot_id,
                        cert,
                        pin_callback: self.pin_callback.clone(),
                    })
                } else {
                    None
                }
            }))
    }

    fn import_rsa_key(
        &mut self,
        p: &[u8],
        q: &[u8],
        cert: &CapturedX509Certificate,
        slot: SlotId,
        touch_policy: TouchPolicy,
        pin_policy: PinPolicy,
    ) -> Result<(), AppleCodesignError> {
        let slot_pretty = hex::encode([u8::from(slot)]);

        let public_key_data = cert.rsa_public_key_data()?;

        let algorithm = match public_key_data.modulus.as_slice().len() {
            129 => AlgorithmId::Rsa1024,
            257 => AlgorithmId::Rsa2048,
            _ => {
                return Err(X509CertificateError::Other(
                    "unable to determine RSA key algorithm".into(),
                )
                .into());
            }
        };

        warn!(
            "attempting import of {:?} private key to slot {}",
            algorithm, slot_pretty
        );

        let mut yk = self.inner()?;

        attempt_authenticated_operation(
            yk.deref_mut(),
            |yk| {
                let rsa_key = ::yubikey::piv::RsaKeyData::new(&p, &q);

                import_rsa_key(yk, slot, algorithm, rsa_key, touch_policy, pin_policy)?;

                Ok(())
            },
            RequiredAuthentication::ManagementKeyAndPin,
            self.pin_callback.as_ref(),
        )?;

        Ok(())
    }

    fn import_ecdsa_key(
        &mut self,
        private_key: &[u8],
        cert: &CapturedX509Certificate,
        slot: SlotId,
        touch_policy: TouchPolicy,
        pin_policy: PinPolicy,
    ) -> Result<(), AppleCodesignError> {
        let slot_pretty = hex::encode([u8::from(slot)]);

        let algorithm = algorithm_from_certificate(cert)?;

        warn!(
            "attempting import of ECDSA private key to slot {}",
            slot_pretty
        );

        let mut yk = self.inner()?;

        attempt_authenticated_operation(
            yk.deref_mut(),
            |yk| {
                import_ecc_key(yk, slot, algorithm, private_key, touch_policy, pin_policy)?;

                Ok(())
            },
            RequiredAuthentication::ManagementKeyAndPin,
            self.pin_callback.as_ref(),
        )?;

        Ok(())
    }

    /// Attempt to import a private key and certificate into the YubiKey.
    pub fn import_key(
        &mut self,
        slot: SlotId,
        key: &dyn KeyInfoSigner,
        cert: &CapturedX509Certificate,
        touch_policy: TouchPolicy,
        pin_policy: PinPolicy,
    ) -> Result<(), AppleCodesignError> {
        let slot_pretty = hex::encode([u8::from(slot)]);

        match cert.key_algorithm() {
            Some(KeyAlgorithm::Rsa) => {
                let (p, q) = key.rsa_primes()?.ok_or_else(|| {
                    X509CertificateError::Other(
                        "could not locate RSA private key parameters".into(),
                    )
                })?;

                self.import_rsa_key(&p, &q, cert, slot, touch_policy, pin_policy)?;
            }
            Some(KeyAlgorithm::Ecdsa(_)) => {
                let private_key = key.private_key_data().ok_or_else(|| {
                    X509CertificateError::Other("could not retrieve private key data".into())
                })?;

                self.import_ecdsa_key(&private_key, cert, slot, touch_policy, pin_policy)?;
            }
            Some(algorithm) => {
                return Err(AppleCodesignError::CertificateUnsupportedKeyAlgorithm(
                    algorithm,
                ));
            }
            None => {
                return Err(X509CertificateError::UnknownKeyAlgorithm("unknown".into()).into());
            }
        }

        warn!(
            "successfully wrote private key to slot {}; proceeding to write certificate",
            slot_pretty
        );

        // The key is imported! Now try to write the public certificate next to it.
        self.import_certificate(slot, cert)?;

        warn!("successfully wrote certificate to slot {}", slot_pretty);

        Ok(())
    }

    /// Generate a new private key in the specified slot.
    pub fn generate_key(
        &mut self,
        slot: SlotId,
        touch_policy: TouchPolicy,
        pin_policy: PinPolicy,
    ) -> Result<(), AppleCodesignError> {
        let slot_pretty = hex::encode([u8::from(slot)]);

        let mut yk = self.inner()?;

        // Apple seems to require RSA 2048 in their CSR requests. So hardcode until we
        // have a reason to support others.
        let algorithm = AlgorithmId::Rsa2048;
        let key_algorithm = KeyAlgorithm::Rsa;
        let signature_algorithm = SignatureAlgorithm::RsaSha256;

        // There's unfortunately some hackiness here.
        //
        // We don't have an API to access the public key info for a slot containing a private key
        // and no certificate. In order to get around this limitation and allow our signer
        // implementation to work (which is needed in order to issue a CSR with the new key),
        // we import a fake certificate into the slot. The certificate has the signature and
        // public key info of a "real" certificate. This enables us to sign using the private key.
        // We don't even bother with self signing the certificate because we don't even want to
        // give the illusion that the certificate is proper.

        warn!(
            "attempting to generate {:?} key in slot {}",
            algorithm, slot_pretty,
        );

        // Any existing certificate would stop working once its private key changes.
        // So delete the certificate first to avoid false promises of a working certificate
        // in the slot.
        attempt_authenticated_operation(
            yk.deref_mut(),
            |yk| {
                warn!("ensuring slot doesn't contain a certificate");
                Ok(YkCertificate::delete(yk, slot)?)
            },
            RequiredAuthentication::ManagementKeyAndPin,
            self.pin_callback.as_ref(),
        )?;

        let key_info = attempt_authenticated_operation(
            yk.deref_mut(),
            |yk| {
                warn!("generating new key on device...");
                Ok(yubikey::piv::generate(
                    yk,
                    slot,
                    algorithm,
                    pin_policy,
                    touch_policy,
                )?)
            },
            RequiredAuthentication::ManagementKeyAndPin,
            self.pin_callback.as_ref(),
        )?;

        warn!("private key successfully generated");

        let mut subject = rfc3280::Name::default();
        subject
            .append_common_name_utf8_string("unusable placeholder certificate")
            .map_err(|e| AppleCodesignError::CertificateBuildError(format!("{:?}", e)))?;

        // We don't have an API to access the public key info for a slot containing a private key
        // and no certificate. So we write a placeholder self-signed certificate to allow future
        // operations to have access to public key metadata.
        let tbs_certificate = rfc5280::TbsCertificate {
            version: Some(rfc5280::Version::V3),
            serial_number: 1.into(),
            signature: signature_algorithm.into(),
            issuer: subject.clone(),
            validity: rfc5280::Validity {
                not_before: asn1time::Time::UtcTime(asn1time::UtcTime::now()),
                not_after: asn1time::Time::UtcTime(asn1time::UtcTime::now()),
            },
            subject,
            subject_public_key_info: rfc5280::SubjectPublicKeyInfo {
                algorithm: key_algorithm.into(),
                subject_public_key: bcder::BitString::new(0, key_info.public_key().into()),
            },
            issuer_unique_id: None,
            subject_unique_id: None,
            extensions: None,
            raw_data: None,
        };

        // It appears the hardware doesn't validate the signature. That makes things
        // easier!

        let temp_cert = rfc5280::Certificate {
            tbs_certificate,
            signature_algorithm: signature_algorithm.into(),
            signature: bcder::BitString::new(0, Bytes::new()),
        };

        let mut temp_cert_der = vec![];
        temp_cert
            .encode_ref()
            .write_encoded(bcder::Mode::Der, &mut temp_cert_der)?;

        let fake_cert = YkCertificate::from_bytes(temp_cert_der)?;

        attempt_authenticated_operation(
            yk.deref_mut(),
            |yk| {
                warn!("writing temp cert");
                Ok(fake_cert.write(yk, slot, CertInfo::Uncompressed)?)
            },
            RequiredAuthentication::ManagementKeyAndPin,
            self.pin_callback.as_ref(),
        )?;

        Ok(())
    }

    /// Import a certificate into a PIV slot.
    ///
    /// This imports the public certificate only: the existing private key is untouched.
    ///
    /// No validation that the certificate matches the existing key is performed.
    pub fn import_certificate(
        &mut self,
        slot: SlotId,
        cert: &CapturedX509Certificate,
    ) -> Result<(), AppleCodesignError> {
        let slot_pretty = hex::encode([u8::from(slot)]);

        let cert = YkCertificate::from_bytes(cert.encode_der()?)?;

        let mut yk = self.inner()?;

        attempt_authenticated_operation(
            yk.deref_mut(),
            |yk| {
                warn!("writing certificate to slot {}", slot_pretty);
                Ok(cert.write(yk, slot, CertInfo::Uncompressed)?)
            },
            RequiredAuthentication::ManagementKeyAndPin,
            self.pin_callback.as_ref(),
        )?;

        warn!("certificate import successful");

        Ok(())
    }
}

/// Entity for creating signatures using a certificate in a given PIV slot.
///
/// This needs to be its own type so we can implement [Sign].
#[derive(Clone)]
pub struct CertificateSigner {
    yk: Arc<Mutex<RawYubiKey>>,
    slot: SlotId,
    cert: CapturedX509Certificate,
    pin_callback: Option<PinCallback>,
}

impl Signer<Signature> for CertificateSigner {
    fn try_sign(&self, message: &[u8]) -> Result<Signature, signature::Error> {
        let algorithm_id =
            algorithm_from_certificate(&self.cert).map_err(signature::Error::from_source)?;

        let signature_algorithm = self
            .cert
            .signature_algorithm()
            .ok_or(X509CertificateError::UnknownDigestAlgorithm(
                "failed to resolve digest algorithm for certificate".into(),
            ))
            .map_err(signature::Error::from_source)?;

        // We need to feed the digest into the signing api, not the data to be
        // digested.
        let digest_algorithm = signature_algorithm
            .digest_algorithm()
            .ok_or(X509CertificateError::UnknownDigestAlgorithm(
                "unable to resolve digest algorithm from signature algorithm".into(),
            ))
            .map_err(signature::Error::from_source)?;

        // Need to apply PKCS#1 padding for RSA.
        let digest = match algorithm_id {
            AlgorithmId::Rsa1024 => digest_algorithm
                .rsa_pkcs1_encode(&message, 1024 / 8)
                .map_err(signature::Error::from_source)?,
            AlgorithmId::Rsa2048 => digest_algorithm
                .rsa_pkcs1_encode(&message, 2048 / 8)
                .map_err(signature::Error::from_source)?,
            AlgorithmId::EccP256 => digest_algorithm.digest_data(&message),
            AlgorithmId::EccP384 => digest_algorithm.digest_data(&message),
        };

        let mut guard = self
            .yk
            .lock()
            .map_err(|_| signature::Error::from_source("unable to acquire lock on YubiKey"))?;

        let yk = guard.deref_mut();

        warn!("initial signing attempt may fail if the certificate requires a pin to unlock");

        attempt_authenticated_operation(
            yk,
            |yk| {
                let signature = ::yubikey::piv::sign_data(yk, &digest, algorithm_id, self.slot)
                    .map_err(AppleCodesignError::YubiKey)?;

                Ok(Signature::from(signature.to_vec()))
            },
            RequiredAuthentication::Pin,
            self.pin_callback.as_ref(),
        )
        .map_err(signature::Error::from_source)
    }
}

impl Sign for CertificateSigner {
    fn sign(&self, message: &[u8]) -> Result<(Vec<u8>, SignatureAlgorithm), X509CertificateError> {
        let algorithm = self.signature_algorithm()?;

        Ok((self.try_sign(message)?.into(), algorithm))
    }

    fn key_algorithm(&self) -> Option<KeyAlgorithm> {
        self.cert.key_algorithm()
    }

    fn public_key_data(&self) -> Bytes {
        self.cert.public_key_data()
    }

    fn signature_algorithm(&self) -> Result<SignatureAlgorithm, X509CertificateError> {
        Ok(self.cert.signature_algorithm().ok_or(
            X509CertificateError::UnknownSignatureAlgorithm(format!(
                "{:?}",
                self.cert.signature_algorithm_oid()
            )),
        )?)
    }

    fn private_key_data(&self) -> Option<Vec<u8>> {
        // We never have access to private keys stored on hardware devices.
        None
    }

    fn rsa_primes(&self) -> Result<Option<(Vec<u8>, Vec<u8>)>, X509CertificateError> {
        Ok(None)
    }
}

impl KeyInfoSigner for CertificateSigner {}

impl PublicKeyPeerDecrypt for CertificateSigner {
    fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, RemoteSignError> {
        let mut guard = self
            .yk
            .lock()
            .map_err(|_| RemoteSignError::Crypto("unable to acquire lock on YubiKey".into()))?;

        let yk = guard.deref_mut();

        let algorithm_id = algorithm_from_certificate(&self.cert)?;

        // The YubiKey's decrypt primitive is super low level. So we need to undo OAEP
        // padding on RSA keys first.

        attempt_authenticated_operation(
            yk,
            |yk| {
                let plaintext =
                    ::yubikey::piv::decrypt_data(yk, ciphertext, algorithm_id, self.slot)?;

                let rsa_modulus_length = match algorithm_id {
                    AlgorithmId::Rsa1024 => Some(1024 / 8),
                    AlgorithmId::Rsa2048 => Some(2048 / 8),
                    AlgorithmId::EccP256 | AlgorithmId::EccP384 => None,
                };

                let plaintext = match algorithm_id {
                    // The YubiKey only does RSA decrypt without padding awareness. So we need to decode
                    // padding ourselves.
                    AlgorithmId::Rsa1024 | AlgorithmId::Rsa2048 => {
                        let mut digest = sha2::Sha256::default();
                        let mut mgf_digest = sha2::Sha256::default();

                        rsa_oaep_post_decrypt_decode(
                            rsa_modulus_length.unwrap(),
                            plaintext.to_vec(),
                            &mut digest,
                            &mut mgf_digest,
                            None,
                        )
                        .map_err(|e| {
                            RemoteSignError::Crypto(format!("error during OAEP decoding: {}", e))
                        })?
                    }

                    AlgorithmId::EccP256 | AlgorithmId::EccP384 => plaintext.to_vec(),
                };

                Ok(plaintext)
            },
            RequiredAuthentication::Pin,
            self.pin_callback.as_ref(),
        )
        .map_err(|e| RemoteSignError::Crypto(format!("failed to decrypt using YubiKey: {}", e)))
    }
}

impl PrivateKey for CertificateSigner {
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

impl CertificateSigner {
    pub fn slot(&self) -> SlotId {
        self.slot
    }

    pub fn certificate(&self) -> &CapturedX509Certificate {
        &self.cert
    }
}
