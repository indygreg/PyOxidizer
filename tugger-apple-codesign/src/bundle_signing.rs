// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Functionality for signing Apple bundles.

use {
    crate::{
        code_resources::{CodeResourcesBuilder, CodeResourcesRule},
        error::AppleCodesignError,
        macho::{
            find_signature_data, parse_signature_data, CodeSigningSlot, DigestType, RequirementType,
        },
        macho_signing::MachOSigner,
    },
    goblin::mach::Mach,
    slog::{info, warn, Logger},
    std::{
        collections::BTreeMap,
        io::Write,
        path::{Path, PathBuf},
    },
    tugger_apple_bundle::{DirectoryBundle, DirectoryBundleFile},
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

pub struct SignedMachOInfo {
    pub code_directory_sha1: Vec<u8>,
    pub designated_code_requirement: Option<String>,
}

/// Used to process individual files within a bundle.
///
/// This abstraction lets entities like [CodeResourcesBuilder] drive the
/// installation of files into a new bundle.
pub trait BundleFileHandler {
    /// Ensures a symlink is installed.
    fn install_symlink(
        &self,
        log: &Logger,
        file: &DirectoryBundleFile,
    ) -> Result<(), AppleCodesignError>;

    /// Ensures a file is installed.
    fn install_normal_file(
        &self,
        log: &Logger,
        file: &DirectoryBundleFile,
    ) -> Result<(), AppleCodesignError>;

    /// Sign a Mach-O file and ensure its new content is installed.
    ///
    /// Returns Mach-O metadata which will be recorded in [CodeResources].
    fn sign_and_install_macho(
        &self,
        log: &Logger,
        file: &DirectoryBundleFile,
    ) -> Result<SignedMachOInfo, AppleCodesignError>;
}

struct SingleBundleHandler {
    dest_dir: PathBuf,
}

impl BundleFileHandler for SingleBundleHandler {
    fn install_symlink(
        &self,
        log: &Logger,
        file: &DirectoryBundleFile,
    ) -> Result<(), AppleCodesignError> {
        let source_path = file.absolute_path();
        let dest_path = self.dest_dir.join(file.relative_path());

        if source_path != dest_path {
            info!(
                log,
                "copying symlink {} -> {}",
                source_path.display(),
                dest_path.display()
            );
            std::fs::create_dir_all(
                dest_path
                    .parent()
                    .expect("parent directory should be available"),
            )?;
            std::fs::copy(source_path, dest_path)?;
        }

        Ok(())
    }

    fn install_normal_file(
        &self,
        log: &Logger,
        file: &DirectoryBundleFile,
    ) -> Result<(), AppleCodesignError> {
        let source_path = file.absolute_path();
        let dest_path = self.dest_dir.join(file.relative_path());

        if source_path != dest_path {
            info!(
                log,
                "copying file {} -> {}",
                source_path.display(),
                dest_path.display()
            );
            std::fs::create_dir_all(
                dest_path
                    .parent()
                    .expect("parent directory should be available"),
            )?;
            std::fs::copy(source_path, dest_path)?;
        }

        Ok(())
    }

    fn sign_and_install_macho(
        &self,
        log: &Logger,
        file: &DirectoryBundleFile,
    ) -> Result<SignedMachOInfo, AppleCodesignError> {
        info!(
            log,
            "signing Mach-O file {}",
            file.relative_path().display()
        );

        let macho_data = std::fs::read(file.absolute_path())?;
        let mut signer = MachOSigner::new(&macho_data)?;

        signer.load_existing_signature_context()?;

        let mut new_data = Vec::<u8>::with_capacity(macho_data.len() + 2_usize.pow(17));
        signer.write_signed_binary(&mut new_data)?;

        let dest_path = self.dest_dir.join(file.relative_path());

        // Read permissions first in case we overwrite the original file.
        let permissions = std::fs::metadata(file.absolute_path())?.permissions();
        {
            let mut fh = std::fs::File::create(&dest_path)?;
            fh.write_all(&new_data)?;
        }
        std::fs::set_permissions(&dest_path, permissions)?;

        let macho = match Mach::parse(&new_data)? {
            Mach::Binary(macho) => macho,
            // Initial Mach-O's signature data is used.
            Mach::Fat(multi_arch) => multi_arch.get(0)?,
        };

        let signature_data =
            find_signature_data(&macho)?.ok_or(AppleCodesignError::BinaryNoCodeSignature)?;
        let signature = parse_signature_data(&signature_data.signature_data)?;

        let cd = signature
            .find_slot(CodeSigningSlot::CodeDirectory)
            .ok_or(AppleCodesignError::BinaryNoCodeSignature)?;

        let code_directory_sha1 = cd.digest_with(DigestType::Sha1)?;

        let designated_code_requirement = if let Some(requirements) =
            signature.code_requirements()?
        {
            if let Some(designated) = requirements.requirements.get(&RequirementType::Designated) {
                let req = designated.parse_expressions()?;

                Some(format!("{}", req[0]))
            } else {
                None
            }
        } else {
            None
        };

        Ok(SignedMachOInfo {
            code_directory_sha1,
            designated_code_requirement,
        })
    }
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

        let dest_dir_root = dest_dir.to_path_buf();

        let dest_dir = if self.bundle.shallow() {
            dest_dir_root.clone()
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
        // Exclude code signature files we'll write.
        resources_builder.add_exclusion_rule(CodeResourcesRule::new("^_CodeSignature/")?.exclude());
        // Ignore notarization ticket.
        resources_builder.add_exclusion_rule(CodeResourcesRule::new("^CodeResources$")?.exclude());

        let handler = SingleBundleHandler {
            dest_dir: dest_dir_root,
        };

        // Iterate files in this bundle and register as code resources.
        //
        // Encountered Mach-O binaries will need to be signed.
        for file in self
            .bundle
            .files(false)
            .map_err(AppleCodesignError::DirectoryBundle)?
        {
            resources_builder.process_file(log, &file, &handler)?;
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
