// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! High level signing primitives.

use {
    crate::{
        bundle_signing::BundleSigner,
        dmg::DmgSigner,
        error::AppleCodesignError,
        macho_signing::{write_macho_file, MachOSigner},
        reader::PathType,
        signing_settings::{SettingsScope, SigningSettings},
    },
    apple_xar::{reader::XarReader, signing::XarSigner},
    log::{info, warn},
    std::{fs::File, path::Path},
};

/// An entity for performing signing that is able to handle all supported target types.
pub struct UnifiedSigner<'key> {
    settings: SigningSettings<'key>,
}

impl<'key> UnifiedSigner<'key> {
    /// Construct a new instance bound to a [SigningSettings].
    pub fn new(settings: SigningSettings<'key>) -> Self {
        Self { settings }
    }

    /// Signs `input_path` and writes the signed output to `output_path`.
    pub fn sign_path(
        &self,
        input_path: impl AsRef<Path>,
        output_path: impl AsRef<Path>,
    ) -> Result<(), AppleCodesignError> {
        let input_path = input_path.as_ref();

        match PathType::from_path(input_path)? {
            PathType::Bundle => self.sign_bundle(input_path, output_path),
            PathType::Dmg => self.sign_dmg(input_path, output_path),
            PathType::MachO => self.sign_macho(input_path, output_path),
            PathType::Xar => self.sign_xar(input_path, output_path),
            PathType::Zip | PathType::Other => Err(AppleCodesignError::UnrecognizedPathType),
        }
    }

    /// Sign a filesystem path in place.
    ///
    /// This is just a convenience wrapper for [Self::sign_path()] with the same path passed
    /// to both the input and output path.
    pub fn sign_path_in_place(&self, path: impl AsRef<Path>) -> Result<(), AppleCodesignError> {
        let path = path.as_ref();

        self.sign_path(path, path)
    }

    /// Sign a Mach-O binary.
    pub fn sign_macho(
        &self,
        input_path: impl AsRef<Path>,
        output_path: impl AsRef<Path>,
    ) -> Result<(), AppleCodesignError> {
        let input_path = input_path.as_ref();
        let output_path = output_path.as_ref();

        warn!("signing {} as a Mach-O binary", input_path.display());
        let macho_data = std::fs::read(input_path)?;

        let mut settings = self.settings.clone();

        settings.import_settings_from_macho(&macho_data)?;

        if settings.binary_identifier(SettingsScope::Main).is_none() {
            let identifier = input_path
                .file_name()
                .ok_or_else(|| {
                    AppleCodesignError::CliGeneralError(
                        "unable to resolve file name of binary".into(),
                    )
                })?
                .to_string_lossy();

            warn!("setting binary identifier to {}", identifier);
            settings.set_binary_identifier(SettingsScope::Main, identifier);
        }

        warn!("parsing Mach-O");
        let signer = MachOSigner::new(&macho_data)?;

        let mut macho_data = vec![];
        signer.write_signed_binary(&settings, &mut macho_data)?;
        warn!("writing Mach-O to {}", output_path.display());
        write_macho_file(input_path, output_path, &macho_data)?;

        Ok(())
    }

    /// Sign a `.dmg` file.
    pub fn sign_dmg(
        &self,
        input_path: impl AsRef<Path>,
        output_path: impl AsRef<Path>,
    ) -> Result<(), AppleCodesignError> {
        let input_path = input_path.as_ref();
        let output_path = output_path.as_ref();

        warn!("signing {} as a DMG", input_path.display());

        // There must be a binary identifier on the DMG. So try to derive one
        // from the filename if one isn't present in the settings.
        let mut settings = self.settings.clone();

        if settings.binary_identifier(SettingsScope::Main).is_none() {
            let file_name = input_path
                .file_stem()
                .ok_or_else(|| {
                    AppleCodesignError::CliGeneralError("unable to resolve file name of DMG".into())
                })?
                .to_string_lossy();

            warn!(
                "setting binary identifier to {} (derived from file name)",
                file_name
            );
            settings.set_binary_identifier(SettingsScope::Main, file_name);
        }

        // The DMG signer signs in place because it needs a `File` handle. So if
        // the output path is different, copy the DMG first.

        // This is not robust same file detection.
        if input_path != output_path {
            info!(
                "copying {} to {} in preparation for signing",
                input_path.display(),
                output_path.display()
            );
            if let Some(parent) = output_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            std::fs::copy(input_path, output_path)?;
        }

        let signer = DmgSigner::default();
        let mut fh = std::fs::File::options()
            .read(true)
            .write(true)
            .open(output_path)?;
        signer.sign_file(&settings, &mut fh)?;

        Ok(())
    }

    /// Sign a bundle.
    pub fn sign_bundle(
        &self,
        input_path: impl AsRef<Path>,
        output_path: impl AsRef<Path>,
    ) -> Result<(), AppleCodesignError> {
        let input_path = input_path.as_ref();
        warn!("signing bundle at {}", input_path.display());

        let signer = BundleSigner::new_from_path(input_path)?;
        signer.write_signed_bundle(output_path, &self.settings)?;

        Ok(())
    }

    pub fn sign_xar(
        &self,
        input_path: impl AsRef<Path>,
        output_path: impl AsRef<Path>,
    ) -> Result<(), AppleCodesignError> {
        let input_path = input_path.as_ref();
        let output_path = output_path.as_ref();

        // The XAR can get corrupted if we sign into place. So we always go through a temporary
        // file. We could potentially avoid the overhead if we're not signing in place...

        let output_path_temp =
            output_path.with_file_name(if let Some(file_name) = output_path.file_name() {
                file_name.to_string_lossy().to_string() + ".tmp"
            } else {
                "xar.tmp".to_string()
            });

        warn!(
            "signing XAR pkg installer at {} to {}",
            input_path.display(),
            output_path_temp.display()
        );

        let (signing_key, signing_cert) = self
            .settings
            .signing_key()
            .ok_or(AppleCodesignError::XarNoAdhoc)?;

        {
            let reader = XarReader::new(File::open(input_path)?)?;
            let mut signer = XarSigner::new(reader);

            let mut fh = File::create(&output_path_temp)?;
            signer.sign(
                &mut fh,
                signing_key,
                signing_cert,
                self.settings.time_stamp_url(),
                self.settings.certificate_chain().iter().cloned(),
            )?;
        }

        if output_path.exists() {
            warn!("removing existing {}", output_path.display());
            std::fs::remove_file(&output_path)?;
        }

        warn!(
            "renaming {} -> {}",
            output_path_temp.display(),
            output_path.display()
        );
        std::fs::rename(&output_path_temp, &output_path)?;

        Ok(())
    }
}
