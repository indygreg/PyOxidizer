// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Yubikey interaction.

use {
    crate::AppleCodesignError,
    bytes::Bytes,
    log::{error, warn},
    std::ops::DerefMut,
    std::sync::{Arc, Mutex, MutexGuard},
    x509_certificate::{
        CapturedX509Certificate, EcdsaCurve, KeyAlgorithm, Sign, SignatureAlgorithm,
        X509CertificateError,
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
        key: &dyn Sign,
        cert: &CapturedX509Certificate,
        touch_policy: TouchPolicy,
        pin_policy: PinPolicy,
    ) -> Result<(), AppleCodesignError> {
        let slot_pretty = hex::encode([u8::from(slot)]);

        let certificate = YkCertificate::from_bytes(cert.encode_der()?)?;

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

        let mut yk = self.inner()?;

        // The key is imported! Now try to write the public certificate next to it.
        attempt_authenticated_operation(
            yk.deref_mut(),
            |yk| {
                certificate.write(yk, slot, CertInfo::Uncompressed)?;

                Ok(())
            },
            RequiredAuthentication::ManagementKeyAndPin,
            self.pin_callback.as_ref(),
        )?;

        warn!("successfully wrote certificate to slot {}", slot_pretty);

        Ok(())
    }
}

/// Entity for creating signatures using a certificate in a given PIV slot.
///
/// This needs to be its own type so we can implement [Sign].
pub struct CertificateSigner {
    yk: Arc<Mutex<RawYubiKey>>,
    slot: SlotId,
    cert: CapturedX509Certificate,
    pin_callback: Option<PinCallback>,
}

impl Sign for CertificateSigner {
    fn sign(&self, message: &[u8]) -> Result<(Vec<u8>, SignatureAlgorithm), X509CertificateError> {
        let algorithm_id = algorithm_from_certificate(&self.cert)?;

        let signature_algorithm =
            self.cert
                .signature_algorithm()
                .ok_or(X509CertificateError::UnknownDigestAlgorithm(
                    "failed to resolve digest algorithm for certificate".into(),
                ))?;

        // We need to feed the digest into the signing api, not the data to be
        // digested.
        let digest_algorithm = signature_algorithm.digest_algorithm().ok_or(
            X509CertificateError::UnknownDigestAlgorithm(
                "unable to resolve digest algorithm from signature algorithm".into(),
            ),
        )?;

        // Need to apply PKCS#1 padding for RSA.
        let digest = match algorithm_id {
            AlgorithmId::Rsa1024 => digest_algorithm.rsa_pkcs1_encode(&message, 1024 / 8)?,
            AlgorithmId::Rsa2048 => digest_algorithm.rsa_pkcs1_encode(&message, 2048 / 8)?,
            AlgorithmId::EccP256 => digest_algorithm.digest_data(&message),
            AlgorithmId::EccP384 => digest_algorithm.digest_data(&message),
        };

        let mut guard = self
            .yk
            .lock()
            .map_err(|_| X509CertificateError::Other("poisoned lock".into()))?;

        let yk = guard.deref_mut();

        warn!("initial signing attempt may fail if the certificate requires a pin to unlock");

        attempt_authenticated_operation(
            yk,
            |yk| {
                let signature = ::yubikey::piv::sign_data(yk, &digest, algorithm_id, self.slot)
                    .map_err(AppleCodesignError::YubiKey)?;

                Ok((signature.to_vec(), signature_algorithm))
            },
            RequiredAuthentication::Pin,
            self.pin_callback.as_ref(),
        )
        .map_err(|e| X509CertificateError::Other(format!("code sign error: {:?}", e)))
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

    fn private_key_data(&self) -> Option<&[u8]> {
        // We never have access to private keys stored on hardware devices.
        None
    }

    fn rsa_primes(&self) -> Result<Option<(Vec<u8>, Vec<u8>)>, X509CertificateError> {
        Ok(None)
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
