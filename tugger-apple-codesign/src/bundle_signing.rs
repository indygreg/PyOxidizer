// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Functionality for signing Apple bundles.

use {
    crate::{
        code_resources::{CodeResourcesBuilder, CodeResourcesRule},
        error::AppleCodesignError,
    },
    slog::{info, warn, Logger},
    std::{collections::BTreeMap, path::Path},
    tugger_apple_bundle::DirectoryBundle,
};

/// A primitive for signing an Apple bundle.
///
/// This type handles the high-level logic of signing an Apple bundle (e.g.
/// a `.app` or `.framework` directory with a well-defined structure).
///
/// This type handles the signing of nested bundles (if present) such that
/// they chain to the main bundle's signature.
///
/// Various functions accept an `Option<impl ToString>` to determine which
/// bundle configuration to operate on. `None` is the main bundle and `Some(T)`
/// defines the relative path of a nested bundle.
pub struct BundleSigner {
    /// All the bundles being signed, indexed by relative path.
    bundles: BTreeMap<Option<String>, SingleBundleSigner>,
}

impl BundleSigner {
    /// Construct a new instance given the path to an on-disk bundle.
    ///
    /// The path should be the root directory of the bundle. e.g. `MyApp.app`.
    pub fn new_from_path(path: impl AsRef<Path>) -> Result<Self, AppleCodesignError> {
        let main_bundle = DirectoryBundle::new_from_path(path.as_ref())
            .map_err(AppleCodesignError::DirectoryBundle)?;

        let mut bundles = main_bundle
            .nested_bundles()
            .map_err(AppleCodesignError::DirectoryBundle)?
            .into_iter()
            .map(|(k, bundle)| (Some(k), SingleBundleSigner::new(bundle, SignMode::Nested)))
            .collect::<BTreeMap<Option<String>, SingleBundleSigner>>();

        bundles.insert(None, SingleBundleSigner::new(main_bundle, SignMode::Main));

        Ok(Self { bundles })
    }

    /// Set the entitlements string to use for a bundle.
    pub fn set_bundle_entitlements_string(
        &mut self,
        bundle_path: Option<impl ToString>,
        v: impl ToString,
    ) -> Result<(), AppleCodesignError> {
        let bundle_path = bundle_path.map(|x| x.to_string());

        let bundle = self
            .bundles
            .get_mut(&bundle_path)
            .ok_or_else(|| AppleCodesignError::BundleUnknown(bundle_path.unwrap()))?;

        bundle.entitlements_string(v);

        Ok(())
    }

    /// Write a signed bundle to the given destination directory.
    ///
    /// The destination directory can be the same as the source directory. However,
    /// if this is done and an error occurs in the middle of signing, the bundle
    /// may be left in an inconsistent or corrupted state and may not be usable.
    pub fn write_signed_bundle(
        &self,
        log: &Logger,
        dest_dir: impl AsRef<Path>,
    ) -> Result<(), AppleCodesignError> {
        let dest_dir = dest_dir.as_ref();

        for (rel, nested) in &self.bundles {
            match rel {
                Some(rel) => {
                    let nested_dest_dir = dest_dir.join(rel);
                    info!(
                        log,
                        "entering nested bundle {}",
                        nested.bundle.root_dir().display(),
                    );
                    nested.write_signed_bundle(log, nested_dest_dir)?;
                    info!(
                        log,
                        "leaving nested bundle {}",
                        nested.bundle.root_dir().display()
                    );
                }
                None => {}
            }
        }

        let main = self
            .bundles
            .get(&None)
            .expect("main bundle should have a key");

        main.write_signed_bundle(log, dest_dir)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SignMode {
    Main,
    Nested,
}

/// A primitive for signing a single Apple bundle.
///
/// Unlike [BundleSigner], this type only signs a single bundle and is ignorant
/// about nested bundles. You probably want to use [BundleSigner] as the interface
/// for signing bundles, as failure to account for nested bundles can result in
/// signature verification errors.
pub struct SingleBundleSigner {
    /// The bundle being signed.
    bundle: DirectoryBundle,

    /// How we are configured for signing.
    sign_mode: SignMode,

    /// Entitlements string to use.
    entitlements: Option<String>,
}

impl SingleBundleSigner {
    /// Construct a new instance.
    pub fn new(bundle: DirectoryBundle, sign_mode: SignMode) -> Self {
        Self {
            bundle,
            sign_mode,
            entitlements: None,
        }
    }

    /// Set the entitlements string for the bundle and all its nested binaries.
    pub fn entitlements_string(&mut self, v: impl ToString) {
        self.entitlements = Some(v.to_string());
    }

    /// Write a signed bundle to the given directory.
    pub fn write_signed_bundle(
        &self,
        log: &Logger,
        dest_dir: impl AsRef<Path>,
    ) -> Result<(), AppleCodesignError> {
        let dest_dir = dest_dir.as_ref();

        warn!(
            log,
            "signing bundle at {} into {}",
            self.bundle.root_dir().display(),
            dest_dir.display()
        );

        let dest_dir = if self.bundle.shallow() {
            dest_dir.to_path_buf()
        } else {
            dest_dir.join("Contents")
        };

        let identifier = self
            .bundle
            .identifier()
            .map_err(AppleCodesignError::DirectoryBundle)?
            .ok_or_else(|| AppleCodesignError::BundleNoIdentifier(self.bundle.info_plist_path()))?;

        let main_executable = self
            .bundle
            .main_executable()
            .map_err(AppleCodesignError::DirectoryBundle)?
            .ok_or_else(|| {
                AppleCodesignError::BundleNoMainExecutable(self.bundle.info_plist_path())
            })?;

        warn!(&log, "collecting code resources files");
        let mut resources_builder = CodeResourcesBuilder::default_resources_rules()?;
        // Exclude main executable from signing, as it is special.
        resources_builder.add_exclusion_rule(
            CodeResourcesRule::new(format!("^{}$", main_executable))?.exclude(),
        );
        resources_builder.add_exclusion_rule(
            CodeResourcesRule::new(format!("^MacOS/{}$", main_executable))?.exclude(),
        );
        resources_builder.add_exclusion_rule(CodeResourcesRule::new("^_CodeSignature/")?.exclude());

        // Iterate files in this bundle and register as code resources.
        for file in self
            .bundle
            .files(false)
            .map_err(AppleCodesignError::DirectoryBundle)?
        {
            resources_builder.process_file(log, &file)?;
        }

        // The resources are now sealed. Write out that XML file.
        let code_resources_path = dest_dir.join("_CodeSignature").join("CodeResources");
        warn!(&log, "writing {}", code_resources_path.display());
        std::fs::create_dir_all(code_resources_path.parent().unwrap())?;
        {
            let mut fh = std::fs::File::create(&code_resources_path)?;
            resources_builder.write_code_resources(&mut fh)?;
        }

        Ok(())
    }
}
