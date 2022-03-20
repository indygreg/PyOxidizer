// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Attach Apple notarization tickets to signed entities.

Stapling refers to the act of taking an Apple issued notarization
ticket (generated after uploading content to Apple for inspection)
and attaching that ticket to the entity that was uploaded. The
mechanism varies, but stapling is literally just fetching a payload
from Apple and attaching it to something else.
*/

use {
    crate::{
        bundle_signing::SignedMachOInfo,
        ticket_lookup::{default_client, lookup_notarization_ticket},
        AppleCodesignError,
    },
    apple_bundles::{BundlePackageType, DirectoryBundle},
    log::{info, warn},
    reqwest::blocking::Client,
    std::path::Path,
};

/// Resolve the notarization ticket record name from a bundle.
///
/// The record name is derived from the digest of the code directory of the
/// main binary within the bundle.
pub fn record_name_from_app_bundle(bundle: &DirectoryBundle) -> Result<String, AppleCodesignError> {
    if !matches!(bundle.package_type(), BundlePackageType::App) {
        return Err(AppleCodesignError::StapleUnsupportedBundleType(
            bundle.package_type(),
        ));
    }

    let main_exe = bundle
        .files(false)
        .map_err(AppleCodesignError::DirectoryBundle)?
        .into_iter()
        .find(|file| matches!(file.is_main_executable(), Ok(true)))
        .ok_or(AppleCodesignError::StapleMainExecutableNotFound)?;

    // Now extract the code signature so we can resolve the code directory.
    info!(
        "resolving bundle's record name from {}",
        main_exe.absolute_path().display()
    );
    let macho_data = std::fs::read(main_exe.absolute_path())?;

    let signed = SignedMachOInfo::parse_data(&macho_data)?;

    let record_name = signed.notarization_ticket_record_name()?;

    Ok(record_name)
}

/// Staple a ticket to a bundle as defined by the path to a directory.
///
/// Stapling a bundle (e.g. `MyApp.app`) is literally just writing a
/// `Contents/CodeResources` file containing the raw ticket data.
pub fn staple_ticket_to_bundle(
    bundle: &DirectoryBundle,
    ticket_data: &[u8],
) -> Result<(), AppleCodesignError> {
    let path = bundle.resolve_path("CodeResources");

    warn!("writing notarizsation ticket to {}", path.display());
    std::fs::write(&path, ticket_data)?;

    Ok(())
}

/// Handles stapling operations.
pub struct Stapler {
    client: Client,
}

impl Stapler {
    /// Construct a new instance with defaults.
    pub fn new() -> Result<Self, AppleCodesignError> {
        Ok(Self {
            client: default_client()?,
        })
    }

    /// Set the HTTP client to use for ticket lookups.
    pub fn set_client(&mut self, client: Client) {
        self.client = client;
    }

    /// Look up a notarization ticket for an app bundle.
    ///
    /// This will resolve the notarization ticket record name from the contents
    /// of the bundle then attempt to look up that notarization ticket against
    /// Apple's servers.
    ///
    /// This errors if there is a problem deriving the notarization ticket record name
    /// or if a failure occurs when looking up the notarization ticket. This can include
    /// a notarization ticket not existing for the requested record.
    pub fn lookup_ticket_for_app_bundle(
        &self,
        bundle: &DirectoryBundle,
    ) -> Result<Vec<u8>, AppleCodesignError> {
        let record_name = record_name_from_app_bundle(bundle)?;

        let response = lookup_notarization_ticket(&self.client, &record_name)?;

        let ticket_data = response.signed_ticket(&record_name)?;

        Ok(ticket_data)
    }

    /// Attempt to staple a bundle by obtaining a notarization ticket automatically.
    pub fn staple_bundle(&self, bundle: &DirectoryBundle) -> Result<(), AppleCodesignError> {
        warn!(
            "attempting to find notarization ticket for bundle at {}",
            bundle.root_dir().display()
        );
        let ticket_data = self.lookup_ticket_for_app_bundle(bundle)?;
        staple_ticket_to_bundle(bundle, &ticket_data)?;

        Ok(())
    }

    /// Attempt to staple an entity at a given filesystem path.
    pub fn staple_path(&self, path: impl AsRef<Path>) -> Result<(), AppleCodesignError> {
        let path = path.as_ref();
        warn!("attempting to staple {}", path.display());

        if let Ok(bundle) = DirectoryBundle::new_from_path(path) {
            warn!("activating bundle stapling mode");
            self.staple_bundle(&bundle)
        } else {
            Err(AppleCodesignError::StapleUnsupportedPath(
                path.to_path_buf(),
            ))
        }
    }
}
