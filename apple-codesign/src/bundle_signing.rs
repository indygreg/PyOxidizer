// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Functionality for signing Apple bundles.

use {
    crate::{
        code_directory::CodeDirectoryBlob,
        code_requirement::RequirementType,
        code_resources::{CodeResourcesBuilder, CodeResourcesRule},
        embedded_signature::{Blob, BlobData, CodeSigningSlot},
        error::AppleCodesignError,
        macho::AppleSignable,
        macho_signing::MachOSigner,
        signing::{SettingsScope, SigningSettings},
    },
    apple_bundles::{DirectoryBundle, DirectoryBundleFile},
    goblin::mach::Mach,
    log::{info, warn},
    std::{
        collections::BTreeMap,
        io::Write,
        path::{Path, PathBuf},
    },
};

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
            .nested_bundles()
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

        let mut additional_files = Vec::new();

        for (rel, nested) in &self.bundles {
            match rel {
                Some(rel) => {
                    let nested_dest_dir = dest_dir.join(rel);
                    info!(
                        "entering nested bundle {}",
                        nested.bundle.root_dir().display(),
                    );
                    let signed_bundle = nested.write_signed_bundle(
                        nested_dest_dir,
                        &settings.as_nested_bundle_settings(rel),
                        &[],
                    )?;

                    // The main bundle's CodeResources file contains references to metadata about
                    // nested bundles' main executables. So we capture that here.
                    let main_exe = signed_bundle
                        .files(false)
                        .map_err(AppleCodesignError::DirectoryBundle)?
                        .into_iter()
                        .find(|file| matches!(file.is_main_executable(), Ok(true)));

                    if let Some(main_exe) = main_exe {
                        let macho_data = std::fs::read(main_exe.absolute_path())?;
                        let macho_info = SignedMachOInfo::parse_data(&macho_data)?;

                        let path = rel.replace('\\', "/");
                        let path = path.strip_prefix("Contents/").unwrap_or(&path).to_string();

                        additional_files.push((path, macho_info));
                    }

                    info!(
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

        main.write_signed_bundle(dest_dir, settings, &additional_files)
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
        let macho = match Mach::parse(data)? {
            Mach::Binary(macho) => macho,
            // Initial Mach-O's signature data is used.
            Mach::Fat(multi_arch) => multi_arch.get(0)?,
        };

        let signature = macho
            .code_signature()?
            .ok_or(AppleCodesignError::BinaryNoCodeSignature)?;

        let cd = signature
            .find_slot(CodeSigningSlot::CodeDirectory)
            .ok_or(AppleCodesignError::BinaryNoCodeSignature)?;

        let code_directory_blob = cd.data.to_vec();

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

        let digest_type: u8 = cd.hash_type.into();

        let mut digest = cd.digest_with(cd.hash_type)?;

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
            info!(
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
        file: &DirectoryBundleFile,
    ) -> Result<SignedMachOInfo, AppleCodesignError> {
        info!("signing Mach-O file {}", file.relative_path().display());

        let macho_data = std::fs::read(file.absolute_path())?;
        let signer = MachOSigner::new(&macho_data)?;

        let mut settings = self
            .settings
            .as_bundle_macho_settings(file.relative_path().to_string_lossy().as_ref());

        // The identifier string for a Mach-O that isn't the main executable is the
        // file name, without a `.dylib` extension.
        // TODO consider adding logic to SigningSettings?
        let identifier = file
            .relative_path()
            .file_name()
            .expect("failure to extract filename (this should never happen)")
            .to_string_lossy();

        let identifier = identifier
            .strip_suffix(".dylib")
            .unwrap_or_else(|| identifier.as_ref());
        settings.set_binary_identifier(SettingsScope::Main, identifier);

        let mut new_data = Vec::<u8>::with_capacity(macho_data.len() + 2_usize.pow(17));
        signer.write_signed_binary(&settings, &mut new_data)?;

        let dest_path = self.dest_dir.join(file.relative_path());

        // Read permissions first in case we overwrite the original file.
        let permissions = std::fs::metadata(file.absolute_path())?.permissions();
        std::fs::create_dir_all(
            dest_path
                .parent()
                .expect("parent directory should be available"),
        )?;
        {
            let mut fh = std::fs::File::create(&dest_path)?;
            fh.write_all(&new_data)?;
        }
        std::fs::set_permissions(&dest_path, permissions)?;

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
        additional_macho_files: &[(String, SignedMachOInfo)],
    ) -> Result<DirectoryBundle, AppleCodesignError> {
        let dest_dir = dest_dir.as_ref();

        warn!(
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

        self.bundle
            .identifier()
            .map_err(AppleCodesignError::DirectoryBundle)?
            .ok_or_else(|| AppleCodesignError::BundleNoIdentifier(self.bundle.info_plist_path()))?;

        warn!("collecting code resources files");
        let mut resources_builder = CodeResourcesBuilder::default_resources_rules()?;
        // Exclude code signature files we'll write.
        resources_builder.add_exclusion_rule(CodeResourcesRule::new("^_CodeSignature/")?.exclude());
        // Ignore notarization ticket.
        resources_builder.add_exclusion_rule(CodeResourcesRule::new("^CodeResources$")?.exclude());

        let handler = SingleBundleHandler {
            dest_dir: dest_dir_root.clone(),
            settings,
        };

        let mut main_exe = None;
        let mut info_plist_data = None;

        // Iterate files in this bundle and register as code resources.
        //
        // Encountered Mach-O binaries will need to be signed.
        for file in self
            .bundle
            .files(false)
            .map_err(AppleCodesignError::DirectoryBundle)?
        {
            // The main executable is special and handled below.
            if file
                .is_main_executable()
                .map_err(AppleCodesignError::DirectoryBundle)?
            {
                main_exe = Some(file);
            // The Info.plist is digested specially.
            } else if file.is_info_plist() {
                handler.install_file(&file)?;
                info_plist_data = Some(std::fs::read(file.absolute_path())?);
            } else {
                resources_builder.process_file(&file, &handler)?;
            }
        }

        // Add in any additional signed Mach-O files. This is likely used for nested
        // bundles.
        for (path, info) in additional_macho_files {
            resources_builder.add_signed_macho_file(path, info)?;
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
                settings.set_binary_identifier(SettingsScope::Main, ident);
            }

            settings.set_code_resources_data(SettingsScope::Main, resources_data);

            if let Some(info_plist_data) = info_plist_data {
                settings.set_info_plist_data(SettingsScope::Main, info_plist_data);
            }

            let mut new_data = Vec::<u8>::with_capacity(macho_data.len() + 2_usize.pow(17));
            signer.write_signed_binary(&settings, &mut new_data)?;

            let dest_path = dest_dir_root.join(exe.relative_path());

            let permissions = std::fs::metadata(exe.absolute_path())?.permissions();
            std::fs::create_dir_all(
                dest_path
                    .parent()
                    .expect("parent directory should be available"),
            )?;
            {
                let mut fh = std::fs::File::create(&dest_path)?;
                fh.write_all(&new_data)?;
            }
            std::fs::set_permissions(&dest_path, permissions)?;
        }

        DirectoryBundle::new_from_path(&dest_dir_root).map_err(AppleCodesignError::DirectoryBundle)
    }
}
