// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Yubikey interaction.

use {
    crate::AppleCodesignError,
    std::ops::{Deref, DerefMut},
    x509_certificate::CapturedX509Certificate,
    yubikey::{certificate::Certificate as YkCertificate, piv::SlotId, YubiKey as RawYubiKey},
};

/// Represents a connection to a yubikey device.
pub struct YubiKey {
    yk: RawYubiKey,
}

impl Deref for YubiKey {
    type Target = RawYubiKey;

    fn deref(&self) -> &Self::Target {
        &self.yk
    }
}

impl DerefMut for YubiKey {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.yk
    }
}

impl From<RawYubiKey> for YubiKey {
    fn from(yk: RawYubiKey) -> Self {
        Self { yk }
    }
}

impl YubiKey {
    /// Find certificates in this device.
    pub fn find_certificates(
        &mut self,
    ) -> Result<Vec<(SlotId, CapturedX509Certificate)>, AppleCodesignError> {
        let slots = self
            .yk
            .piv_keys()?
            .into_iter()
            .map(|key| key.slot())
            .collect::<Vec<_>>();

        let mut res = vec![];

        for slot in slots {
            let cert = YkCertificate::read(&mut self.yk, slot)?;

            let cert = CapturedX509Certificate::from_der(cert.into_buffer().to_vec())?;

            res.push((slot, cert));
        }

        Ok(res)
    }
}
