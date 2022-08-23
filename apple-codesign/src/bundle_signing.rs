// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Functionality for signing Apple bundles.

use {
    crate::{
        code_directory::CodeDirectoryBlob,
        code_requirement::RequirementType,
        code_resources::{CodeResourcesBuilder, CodeResourcesRule},
        embedded_signature::{Blob, BlobData, DigestType},
        error::AppleCodesignError,
        macho::MachFile,
        macho_signing::{write_macho_file, MachOSigner},
        signing_settings::{SettingsScope, SigningSettings},
    },
    apple_bundles::{BundlePackageType, DirectoryBundle, DirectoryBundleFile},
    log::{info, warn},
    std::{
        collections::BTreeMap,
        io::Write,
        path::{Path, PathBuf},
    },
    tugger_file_manifest::create_symlink,
};

/// Copy a bundle's contents to a destination directory.
pub fn copy_bundle(bundle: &DirectoryBundle, dest_dir: &Path) -> Result<(), AppleCodesignError> {
    let settings = SigningSettings::default();

    let handler = SingleBundleHandler {
        dest_dir: dest_dir.to_path_buf(),
        settings: &settings,
    };

    for file in bundle
        .files(false)
        .map_err(AppleCodesignError::DirectoryBundle)?
    {
        handler.install_file(&file)?;
    }

    Ok(())
}

/// A primitive for signing an Apple bundle.
///
/// This type handles the high-level logic of signing an Apple bundle (e.g.
/// a `.app` or `.framework` directory with a well-defined structure).
///
/// This type handles the signing of nested bundles (if present) such that
/// they chain to the main bundle's signature.
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
            .nested_bundles(true)
            .map_err(AppleCodesignError::DirectoryBundle)?
            .into_iter()
            .map(|(k, bundle)| (Some(k), SingleBundleSigner::new(bundle)))
            .collect::<BTreeMap<Option<String>, SingleBundleSigner>>();

        bundles.insert(None, SingleBundleSigner::new(main_bundle));

        Ok(Self { bundles })
    }

    /// Write a signed bundle to the given destination directory.
    ///
    /// The destination directory can be the same as the source directory. However,
    /// if this is done and an error occurs in the middle of signing, the bundle
    /// may be left in an inconsistent or corrupted state and may not be usable.
    pub fn write_signed_bundle(
        &self,
        dest_dir: impl AsRef<Path>,
        settings: &SigningSettings,
    ) -> Result<DirectoryBundle, AppleCodesignError> {
        let dest_dir = dest_dir.as_ref();

        // We need to sign the leaf-most bundles first since a parent bundle may need
        // to record information about the child in its signature.
        let mut bundles = self
            .bundles
            .iter()
            .filter_map(|(rel, bundle)| rel.as_ref().map(|rel| (rel, bundle)))
            .collect::<Vec<_>>();

        // This won't preserve alphabetical order. But since the input was stable, output
        // should be deterministic.
        bundles.sort_by(|(a, _), (b, _)| b.len().cmp(&a.len()));

        warn!(
            "signing {} nested bundles in the following order:",
            bundles.len()
        );
        for bundle in &bundles {
            warn!("{}", bundle.0);
        }

        for (rel, nested) in bundles {
            let nested_dest_dir = dest_dir.join(rel);
            info!(
                "entering nested bundle {}",
                nested.bundle.root_dir().display(),
            );

            // If we excluded this bundle from signing, just copy all the files.
            if settings
                .path_exclusion_patterns()
                .iter()
                .any(|pattern| pattern.matches(rel))
            {
                warn!("bundle is in exclusion list; it will be copied instead of signed");
                copy_bundle(&nested.bundle, &nested_dest_dir)?;
            } else {
                nested.write_signed_bundle(
                    nested_dest_dir,
                    &settings.as_nested_bundle_settings(rel),
                )?;
            }

            info!(
                "leaving nested bundle {}",
                nested.bundle.root_dir().display()
            );
        }

        let main = self
            .bundles
            .get(&None)
            .expect("main bundle should have a key");

        main.write_signed_bundle(dest_dir, settings)
    }
}

/// Metadata about a signed Mach-O file or bundle.
///
/// If referring to a bundle, the metadata refers to the 1st Mach-O in the
/// bundle's main executable.
///
/// This contains enough metadata to construct references to the file/bundle
/// in [crate::code_resources::CodeResources] files.
pub struct SignedMachOInfo {
    /// Raw data constituting the code directory blob.
    ///
    /// Is typically digested to construct a <cdhash>.
    pub code_directory_blob: Vec<u8>,

    /// Designated code requirements string.
    ///
    /// Typically occupies a `<key>requirement</key>` in a
    /// [crate::code_resources::CodeResources] file.
    pub designated_code_requirement: Option<String>,
}

impl SignedMachOInfo {
    /// Parse Mach-O data to obtain an instance.
    pub fn parse_data(data: &[u8]) -> Result<Self, AppleCodesignError> {
        // Initial Mach-O's signature data is used.
        let mach = MachFile::parse(data)?;
        let macho = mach.nth_macho(0)?;

        let signature = macho
            .code_signature()?
            .ok_or(AppleCodesignError::BinaryNoCodeSignature)?;

        // Usually this type is used to chain content digests in the context of bundle signing /
        // code resources files. In that context, SHA-256 digests are preferred and might even
        // be the only supported digests. So, prefer a SHA-256 code directory over SHA-1.
        let cd = if let Some(cd) = signature.code_directory_for_digest(DigestType::Sha256)? {
            cd
        } else if let Some(cd) = signature.code_directory_for_digest(DigestType::Sha1)? {
            cd
        } else if let Some(cd) = signature.code_directory()? {
            cd
        } else {
            return Err(AppleCodesignError::BinaryNoCodeSignature);
        };

        let code_directory_blob = cd.to_blob_bytes()?;

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
            code_directory_blob,
            designated_code_requirement,
        })
    }

    /// Resolve the parsed code directory from stored data.
    pub fn code_directory(&self) -> Result<Box<CodeDirectoryBlob<'_>>, AppleCodesignError> {
        let blob = BlobData::from_blob_bytes(&self.code_directory_blob)?;

        if let BlobData::CodeDirectory(cd) = blob {
            Ok(cd)
        } else {
            Err(AppleCodesignError::BinaryNoCodeSignature)
        }
    }

    /// Resolve the notarization ticket record name for this Mach-O file.
    pub fn notarization_ticket_record_name(&self) -> Result<String, AppleCodesignError> {
        let cd = self.code_directory()?;

        let digest_type: u8 = cd.digest_type.into();

        let mut digest = cd.digest_with(cd.digest_type)?;

        // Digests appear to be truncated at 20 bytes / 40 characters.
        digest.truncate(20);

        let digest = hex::encode(digest);

        // Unsure what the leading `2/` means.
        Ok(format!("2/{}/{}", digest_type, digest))
    }
}

/// Used to process individual files within a bundle.
///
/// This abstraction lets entities like [CodeResourcesBuilder] drive the
/// installation of files into a new bundle.
pub trait BundleFileHandler {
    /// Ensures a file (regular or symlink) is installed.
    fn install_file(&self, file: &DirectoryBundleFile) -> Result<(), AppleCodesignError>;

    /// Sign a Mach-O file and ensure its new content is installed.
    ///
    /// Returns Mach-O metadata which will be recorded in
    /// [crate::code_resources::CodeResources].
    fn sign_and_install_macho(
        &self,
        file: &DirectoryBundleFile,
    ) -> Result<SignedMachOInfo, AppleCodesignError>;
}

struct SingleBundleHandler<'a, 'key> {
    settings: &'a SigningSettings<'key>,
    dest_dir: PathBuf,
}

impl<'a, 'key> BundleFileHandler for SingleBundleHandler<'a, 'key> {
    fn install_file(&self, file: &DirectoryBundleFile) -> Result<(), AppleCodesignError> {
        let source_path = file.absolute_path();
        let dest_path = self.dest_dir.join(file.relative_path());

        if source_path != dest_path {
            std::fs::create_dir_all(
                dest_path
                    .parent()
                    .expect("parent directory should be available"),
            )?;

            let metadata = source_path.symlink_metadata()?;
            let mtime = filetime::FileTime::from_last_modification_time(&metadata);

            if let Some(target) = file
                .symlink_target()
                .map_err(AppleCodesignError::DirectoryBundle)?
            {
                info!(
                    "replicating symlink {} -> {}",
                    dest_path.display(),
                    target.display()
                );
                create_symlink(&dest_path, target)?;
                filetime::set_symlink_file_times(
                    &dest_path,
                    filetime::FileTime::from_last_access_time(&metadata),
                    mtime,
                )?;
            } else {
                info!(
                    "copying file {} -> {}",
                    source_path.display(),
                    dest_path.display()
                );
                std::fs::copy(&source_path, &dest_path)?;
                filetime::set_file_mtime(&dest_path, mtime)?;
            }
        }

        Ok(())
    }

    fn sign_and_install_macho(
        &self,
        file: &DirectoryBundleFile,
    ) -> Result<SignedMachOInfo, AppleCodesignError> {
        info!("signing Mach-O file {}", file.relative_path().display());

        let macho_data = std::fs::read(file.absolute_path())?;
        let signer = MachOSigner::new(&macho_data)?;

        let mut settings = self
            .settings
            .as_bundle_macho_settings(file.relative_path().to_string_lossy().as_ref());

        settings.import_settings_from_macho(&macho_data)?;

        // If there isn't a defined binary identifier, derive one from the file name so one is set
        // and we avoid a signing error due to missing identifier.
        // TODO do we need to check the nested Mach-O settings?
        if settings.binary_identifier(SettingsScope::Main).is_none() {
            let identifier = file
                .relative_path()
                .file_name()
                .expect("failure to extract filename (this should never happen)")
                .to_string_lossy();

            let identifier = identifier
                .strip_suffix(".dylib")
                .unwrap_or_else(|| identifier.as_ref());

            info!(
                "Mach-O is missing binary identifier; setting to {} based on file name",
                identifier
            );
            settings.set_binary_identifier(SettingsScope::Main, identifier);
        }

        let mut new_data = Vec::<u8>::with_capacity(macho_data.len() + 2_usize.pow(17));
        signer.write_signed_binary(&settings, &mut new_data)?;

        let dest_path = self.dest_dir.join(file.relative_path());

        info!("writing Mach-O to {}", dest_path.display());
        write_macho_file(file.absolute_path(), &dest_path, &new_data)?;

        SignedMachOInfo::parse_data(&new_data)
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
}

impl SingleBundleSigner {
    /// Construct a new instance.
    pub fn new(bundle: DirectoryBundle) -> Self {
        Self { bundle }
    }

    /// Write a signed bundle to the given directory.
    pub fn write_signed_bundle(
        &self,
        dest_dir: impl AsRef<Path>,
        settings: &SigningSettings,
    ) -> Result<DirectoryBundle, AppleCodesignError> {
        let dest_dir = dest_dir.as_ref();

        warn!(
            "signing bundle at {} into {}",
            self.bundle.root_dir().display(),
            dest_dir.display()
        );

        // Frameworks are a bit special.
        //
        // Modern frameworks typically have a `Versions/` directory containing directories
        // with the actual frameworks. These are the actual directories that are signed - not
        // the top-most directory. In fact, the top-most `.framework` directory doesn't have any
        // code signature elements at all and can effectively be ignored as far as signing
        // is concerned.
        //
        // But even if there is a `Versions/` directory with nested bundles to sign, the top-level
        // directory may have some symlinks. And those need to be preserved. In addition, there
        // may be symlinks in `Versions/`. `Versions/Current` is common.
        //
        // Of course, if there is no `Versions/` directory, the top-level directory could be
        // a valid framework warranting signing.
        if self.bundle.package_type() == BundlePackageType::Framework {
            if self.bundle.root_dir().join("Versions").is_dir() {
                warn!("found a versioned framework; each version will be signed as its own bundle");

                // But we still need to preserve files (hopefully just symlinks) outside the
                // nested bundles under `Versions/`. Since we don't nest into child bundles
                // here, it should be safe to handle each encountered file.
                let handler = SingleBundleHandler {
                    dest_dir: dest_dir.to_path_buf(),
                    settings,
                };

                for file in self
                    .bundle
                    .files(false)
                    .map_err(AppleCodesignError::DirectoryBundle)?
                {
                    handler.install_file(&file)?;
                }

                return DirectoryBundle::new_from_path(dest_dir)
                    .map_err(AppleCodesignError::DirectoryBundle);
            } else {
                warn!("found an unversioned framework; signing like normal");
            }
        }

        let dest_dir_root = dest_dir.to_path_buf();

        let dest_dir = if self.bundle.shallow() {
            dest_dir_root.clone()
        } else {
            dest_dir.join("Contents")
        };

        self.bundle
            .identifier()
            .map_err(AppleCodesignError::DirectoryBundle)?
            .ok_or_else(|| AppleCodesignError::BundleNoIdentifier(self.bundle.info_plist_path()))?;

        let mut resources_digests = settings.all_digests(SettingsScope::Main);

        // State in the main executable can influence signing settings of the bundle. So examine
        // it first.

        let main_exe = self
            .bundle
            .files(false)
            .map_err(AppleCodesignError::DirectoryBundle)?
            .into_iter()
            .find(|f| matches!(f.is_main_executable(), Ok(true)));

        if let Some(exe) = &main_exe {
            let macho_data = std::fs::read(exe.absolute_path())?;
            let mach = MachFile::parse(&macho_data)?;

            for macho in mach.iter_macho() {
                if let Some(targeting) = macho.find_targeting()? {
                    let sha256_version = targeting.platform.sha256_digest_support()?;

                    if !sha256_version.matches(&targeting.minimum_os_version)
                        && resources_digests != vec![DigestType::Sha1, DigestType::Sha256]
                    {
                        info!("main executable targets OS requiring SHA-1 signatures; activating SHA-1 + SHA-256 signing");
                        resources_digests = vec![DigestType::Sha1, DigestType::Sha256];
                        break;
                    }
                }
            }
        }

        warn!("collecting code resources files");

        // The set of rules to use is determined by whether the bundle *can* have a
        // `Resources/`, not whether it necessarily does. The exact rules for this are not
        // known. Essentially we want to test for the result of CFBundleCopyResourcesDirectoryURL().
        // We assume that we can use the resources rules when there is a `Resources` directory
        // (this seems obvious!) or when the bundle isn't shallow, as a non-shallow bundle should
        // be an app bundle and app bundles can always have resources (we think).
        let mut resources_builder =
            if self.bundle.resolve_path("Resources").is_dir() || !self.bundle.shallow() {
                CodeResourcesBuilder::default_resources_rules()?
            } else {
                CodeResourcesBuilder::default_no_resources_rules()?
            };

        // Ensure emitted digests match what we're configured to emit.
        resources_builder.set_digests(resources_digests.into_iter());

        // Exclude code signature files we'll write.
        resources_builder.add_exclusion_rule(CodeResourcesRule::new("^_CodeSignature/")?.exclude());
        // Ignore notarization ticket.
        resources_builder.add_exclusion_rule(CodeResourcesRule::new("^CodeResources$")?.exclude());

        let handler = SingleBundleHandler {
            dest_dir: dest_dir_root.clone(),
            settings,
        };

        let mut info_plist_data = None;

        // Iterate files in this bundle and register as code resources.
        //
        // Traversing into nested bundles seems wrong but it is correct. The resources builder
        // has rules to determine whether to process a path and assuming the rules and evaluation
        // of them is correct, it is able to decide for itself how to handle a path.
        //
        // Furthermore, this behavior is needed as bundles can encapsulate signatures for nested
        // bundles. For example, you could have a framework bundle with an embedded app bundle in
        // `Resources/MyApp.app`! In this case, the framework's CodeResources encapsulates the
        // content of `Resources/My.app` per the processing rules.
        for file in self
            .bundle
            .files(true)
            .map_err(AppleCodesignError::DirectoryBundle)?
        {
            // The main executable is special and handled below.
            if file
                .is_main_executable()
                .map_err(AppleCodesignError::DirectoryBundle)?
            {
                continue;
            } else if file.is_info_plist() {
                // The Info.plist is digested specially. But it may also be handled by
                // the resources handler. So always feed it through.
                info!(
                    "{} is the Info.plist file; handling specially",
                    file.relative_path().display()
                );
                resources_builder.process_file(&file, &handler)?;
                info_plist_data = Some(std::fs::read(file.absolute_path())?);
            } else {
                resources_builder.process_file(&file, &handler)?;
            }
        }

        // Seal code directory digests of any nested bundles.
        //
        // Apple's tooling seems to only do this for some bundle type combinations. I'm
        // not yet sure what the complete heuristic is. But we observed that frameworks
        // don't appear to include digests of any nested app bundles. So we add that
        // exclusion. We should figure out what the actual rules here...
        if self.bundle.package_type() != BundlePackageType::Framework {
            let dest_bundle = DirectoryBundle::new_from_path(&dest_dir)
                .map_err(AppleCodesignError::DirectoryBundle)?;

            for (rel_path, nested_bundle) in dest_bundle
                .nested_bundles(false)
                .map_err(AppleCodesignError::DirectoryBundle)?
            {
                resources_builder.process_nested_bundle(&rel_path, &nested_bundle)?;
            }
        }

        // The resources are now sealed. Write out that XML file.
        let code_resources_path = dest_dir.join("_CodeSignature").join("CodeResources");
        warn!(
            "writing sealed resources to {}",
            code_resources_path.display()
        );
        std::fs::create_dir_all(code_resources_path.parent().unwrap())?;
        let mut resources_data = Vec::<u8>::new();
        resources_builder.write_code_resources(&mut resources_data)?;

        {
            let mut fh = std::fs::File::create(&code_resources_path)?;
            fh.write_all(&resources_data)?;
        }

        // Seal the main executable.
        if let Some(exe) = main_exe {
            warn!("signing main executable {}", exe.relative_path().display());

            let macho_data = std::fs::read(exe.absolute_path())?;
            let signer = MachOSigner::new(&macho_data)?;

            let mut settings = settings.clone();

            // The identifier for the main executable is defined in the bundle's Info.plist.
            if let Some(ident) = self
                .bundle
                .identifier()
                .map_err(AppleCodesignError::DirectoryBundle)?
            {
                info!("setting main executable binary identifier to {} (derived from CFBundleIdentifier in Info.plist)", ident);
                settings.set_binary_identifier(SettingsScope::Main, ident);
            } else {
                info!("unable to determine binary identifier from bundle's Info.plist (CFBundleIdentifier not set?)");
            }

            settings.import_settings_from_macho(&macho_data)?;

            settings.set_code_resources_data(SettingsScope::Main, resources_data);

            if let Some(info_plist_data) = info_plist_data {
                settings.set_info_plist_data(SettingsScope::Main, info_plist_data);
            }

            let mut new_data = Vec::<u8>::with_capacity(macho_data.len() + 2_usize.pow(17));
            signer.write_signed_binary(&settings, &mut new_data)?;

            let dest_path = dest_dir_root.join(exe.relative_path());
            info!("writing signed main executable to {}", dest_path.display());
            write_macho_file(exe.absolute_path(), &dest_path, &new_data)?;
        } else {
            warn!("bundle has no main executable to sign specially");
        }

        DirectoryBundle::new_from_path(&dest_dir_root).map_err(AppleCodesignError::DirectoryBundle)
    }
}
