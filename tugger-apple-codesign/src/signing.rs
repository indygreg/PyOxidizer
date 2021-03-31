// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Generic primitives related to code signing.

use {
    crate::{bundle_signing::BundleSigner, error::AppleCodesignError, macho_signing::MachOSigner},
    cryptographic_message_syntax::{Certificate, SigningKey},
    reqwest::{IntoUrl, Url},
    std::path::Path,
};

/// Represents code signing settings.
///
/// This type holds global settings related to code signing. Instances can
/// be converted into lower-level signing primitives, such as [MachOSigner]
/// and [BundleSigner]. Generally speaking, you will want to use this type
/// to configure code signing settings then turn into a signer used to perform
/// the actual code signing.
#[derive(Clone, Debug, Default)]
pub struct SigningSettings<'key> {
    load_existing_signature_settings: bool,
    signing_key: Option<(&'key SigningKey, Certificate)>,
    certificates: Vec<Certificate>,
    time_stamp_url: Option<Url>,
}

impl<'key> SigningSettings<'key> {
    /// Set the signing key-pair for producing a cryptographic signature over code.
    ///
    /// If this is not called, signing will lack a cryptographic signature and will only
    /// contain digests of content. This is known as "ad-hoc" mode. Binaries lacking a
    /// cryptographic signature or signed without a key-pair issued/signed by Apple may
    /// not run in all environments.
    pub fn set_signing_key(&mut self, private: &'key SigningKey, public: Certificate) {
        self.signing_key = Some((private, public));
    }

    /// Add a DER encoded X.509 public certificate to the signing certificate chain.
    ///
    /// When producing a cryptographic signature (see [SigningSettings::set_signing_key]),
    /// information about the signing key-pair is included in the signature. The signing
    /// key's public certificate is always included. This function can be used to define
    /// additional X.509 public certificates to include. Typically, the signing chain
    /// of the signing key-pair up until the root Certificate Authority (CA) is added
    /// so clients have access to the full certificate chain for validation purposes.
    ///
    /// This setting has no effect if [SigningSettings::set_signing_key] is not called.
    pub fn chain_certificate_der(
        &mut self,
        data: impl AsRef<[u8]>,
    ) -> Result<(), AppleCodesignError> {
        self.certificates
            .push(Certificate::from_der(data.as_ref())?);

        Ok(())
    }

    /// Add a PEM encoded X.509 public certificate to the signing certificate chain.
    ///
    /// This is like [SigningSettings::chain_certificate_der] except the certificate is
    /// specified as PEM encoded data. This is a human readable string like
    /// `-----BEGIN CERTIFICATE-----` and is a common method for encoding certificate data.
    /// (PEM is effectively base64 encoded DER data.)
    ///
    /// Only a single certificate is read from the PEM data.
    pub fn chain_certificate_pem(
        &mut self,
        data: impl AsRef<[u8]>,
    ) -> Result<(), AppleCodesignError> {
        self.certificates
            .push(Certificate::from_pem(data.as_ref())?);

        Ok(())
    }

    /// Set the Time-Stamp Protocol server URL to use to generate a Time-Stamp Token.
    ///
    /// When set and a signing key-pair is defined, the server will be contacted during
    /// signing and a Time-Stamp Token will be embedded in the cryptographic signature.
    /// This Time-Stamp Token is a cryptographic proof that someone in possession of
    /// the signing key-pair produced the cryptographic signature at a given time. It
    /// facilitates validation of the signing time via an independent (presumably trusted)
    /// entity.
    pub fn time_stamp_url(&mut self, url: impl IntoUrl) -> Result<(), AppleCodesignError> {
        self.time_stamp_url = Some(url.into_url()?);

        Ok(())
    }

    /// Turn these settings into a [MachOSigner].
    ///
    /// The signer will be constructed and settings from this instance will be applied.
    pub fn as_macho_signer<'data>(
        &self,
        macho_data: &'data [u8],
    ) -> Result<MachOSigner<'data, 'key>, AppleCodesignError> {
        let mut signer = MachOSigner::new(macho_data)?;

        if self.load_existing_signature_settings {
            signer.load_existing_signature_context()?;
        }

        if let Some((private, public)) = &self.signing_key {
            signer.signing_key(private, public.clone());
        }

        for cert in &self.certificates {
            signer.chain_certificate_der(cert.as_der()?)?;
        }

        if let Some(time_stamp_url) = &self.time_stamp_url {
            signer.time_stamp_url(time_stamp_url.clone())?;
        }

        Ok(signer)
    }

    /// Turn these settings into a [BundleSigner].
    ///
    /// The signer will be constructed and settings from this instance will be applied.
    pub fn as_bundle_signer(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<BundleSigner<'key>, AppleCodesignError> {
        let mut signer = BundleSigner::new_from_path(path)?;

        if self.load_existing_signature_settings {
            signer.load_existing_signature_settings();
        }

        if let Some((private, public)) = &self.signing_key {
            signer.set_signing_key(private, public.clone());
        }

        for cert in &self.certificates {
            signer.chain_certificate_der(cert.as_der()?)?;
        }

        if let Some(time_stamp_url) = &self.time_stamp_url {
            signer.time_stamp_url(time_stamp_url.clone())?;
        }

        Ok(signer)
    }
}
