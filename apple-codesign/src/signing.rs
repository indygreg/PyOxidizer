// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! High level signing primitives.

use {
    crate::{
        bundle_signing::BundleSigner,
        code_directory::ExecutableSegmentFlags,
        error::AppleCodesignError,
        macho_signing::MachOSigner,
        signing_settings::{SettingsScope, SigningSettings},
    },
    log::warn,
    std::path::Path,
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

        if input_path.is_file() {
            self.sign_macho(input_path, output_path)
        } else {
            self.sign_bundle(input_path, output_path)
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

    fn sign_macho(
        &self,
        input_path: impl AsRef<Path>,
        output_path: impl AsRef<Path>,
    ) -> Result<(), AppleCodesignError> {
        let input_path = input_path.as_ref();
        let output_path = output_path.as_ref();

        let mut settings = self.settings.clone();

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

        if settings
            .executable_segment_flags(SettingsScope::Main)
            .is_none()
        {
            settings.set_executable_segment_flags(
                SettingsScope::Main,
                ExecutableSegmentFlags::MAIN_BINARY,
            );
        }

        warn!("signing {} as a Mach-O binary", input_path.display());
        let macho_data = std::fs::read(input_path)?;

        warn!("parsing Mach-O");
        let signer = MachOSigner::new(&macho_data)?;

        warn!("writing {}", output_path.display());
        let mut fh = std::fs::File::create(output_path)?;
        signer.write_signed_binary(&settings, &mut fh)?;

        Ok(())
    }

    fn sign_bundle(
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
}
