// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Functionality for signing Apple bundles.

use {
    crate::{
        code_resources::{CodeResourcesBuilder, CodeResourcesRule},
        error::AppleCodesignError,
        macho::{find_signature_data, parse_signature_data, CodeSigningSlot, RequirementType},
        macho_signing::MachOSigner,
    },
    cryptographic_message_syntax::{Certificate, SigningKey},
    goblin::mach::Mach,
    reqwest::{IntoUrl, Url},
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
pub struct BundleSigner<'key> {
    /// All the bundles being signed, indexed by relative path.
    bundles: BTreeMap<Option<String>, SingleBundleSigner<'key>>,
}

impl<'key> BundleSigner<'key> {
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

    /// See [MachOSignatureBuilder::load_existing_signature_context].
    pub fn load_existing_signature_settings(&mut self) {
        for signer in self.bundles.values_mut() {
            signer.load_existing_signature_settings();
        }
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

    /// Set the signing key to use for all cryptographic signatures.
    pub fn set_signing_key(&mut self, private: &'key SigningKey, public: Certificate) {
        for signer in self.bundles.values_mut() {
            signer.signing_key(private, public.clone());
        }
    }

    /// Add a DER encoded X.509 certificate to the certificate chain.
    pub fn chain_certificate_der(
        &mut self,
        data: impl AsRef<[u8]>,
    ) -> Result<(), AppleCodesignError> {
        for signer in self.bundles.values_mut() {
            signer.chain_certificate_der(data.as_ref())?;
        }

        Ok(())
    }

    /// Add a PEM encoded X.509 certificate to the certificate chain.
    pub fn chain_certificate_pem(
        &mut self,
        data: impl AsRef<[u8]>,
    ) -> Result<(), AppleCodesignError> {
        for signer in self.bundles.values_mut() {
            signer.chain_certificate_pem(data.as_ref())?;
        }

        Ok(())
    }

    /// Set the URL of a Time-Stamp Protocol server to use.
    pub fn time_stamp_url(&mut self, url: impl IntoUrl) -> Result<(), AppleCodesignError> {
        let url = url.into_url()?;

        for signer in self.bundles.values_mut() {
            signer.time_stamp_url(url.clone())?;
        }

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
    ) -> Result<DirectoryBundle, AppleCodesignError> {
        let dest_dir = dest_dir.as_ref();

        let mut additional_files = Vec::new();

        for (rel, nested) in &self.bundles {
            match rel {
                Some(rel) => {
                    let nested_dest_dir = dest_dir.join(rel);
                    info!(
                        log,
                        "entering nested bundle {}",
                        nested.bundle.root_dir().display(),
                    );
                    let signed_bundle = nested.write_signed_bundle(log, nested_dest_dir, &[])?;

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

        main.write_signed_bundle(log, dest_dir, &additional_files)
    }
}

/// Metadata about a signed Mach-O file or bundle.
///
/// If referring to a bundle, the metadata refers to the 1st Mach-O in the
/// bundle's main executable.
///
/// This contains enough metadata to construct references to the file/bundle
/// in [CodeResources] files.
pub struct SignedMachOInfo {
    /// Raw data constituting the code directory blob.
    ///
    /// Is typically digested to construct a <cdhash>.
    pub code_directory_blob: Vec<u8>,

    /// Designated code requirements string.
    ///
    /// Typically pccupies a `<key>requirement</key>` in a [CodeResources] file.
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

        let signature_data =
            find_signature_data(&macho)?.ok_or(AppleCodesignError::BinaryNoCodeSignature)?;
        let signature = parse_signature_data(&signature_data.signature_data)?;

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
}

/// Used to process individual files within a bundle.
///
/// This abstraction lets entities like [CodeResourcesBuilder] drive the
/// installation of files into a new bundle.
pub trait BundleFileHandler {
    /// Ensures a file (regular or symlink) is installed.
    fn install_file(
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

struct SingleBundleHandler<'a, 'key> {
    signer: &'a SingleBundleSigner<'key>,

    dest_dir: PathBuf,
}

impl<'a, 'key> BundleFileHandler for SingleBundleHandler<'a, 'key> {
    fn install_file(
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

        if self.signer.load_existing_signature_settings {
            signer.load_existing_signature_context()?;
        }

        if let Some((private, public)) = &self.signer.signing_key {
            signer.signing_key(private, public.clone());
        }

        for cert in &self.signer.certificates {
            signer.chain_certificate_der(cert.as_der()?)?;
        }

        if let Some(entitlements) = &self.signer.entitlements {
            signer.set_entitlements_string(entitlements);
        }

        if let Some(time_stamp_url) = &self.signer.time_stamp_url {
            signer.time_stamp_url(time_stamp_url.clone())?;
        }

        let mut new_data = Vec::<u8>::with_capacity(macho_data.len() + 2_usize.pow(17));
        signer.write_signed_binary(&mut new_data)?;

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
pub struct SingleBundleSigner<'key> {
    /// The bundle being signed.
    bundle: DirectoryBundle,

    /// Whether to load existing signature settings.
    load_existing_signature_settings: bool,

    /// Entitlements string to use.
    entitlements: Option<String>,

    /// The key pair to cryptographically sign with.
    signing_key: Option<(&'key SigningKey, Certificate)>,

    /// Certificate information to include.
    certificates: Vec<Certificate>,

    /// Time-Stamp Protocol server URL to use.
    time_stamp_url: Option<Url>,
}

impl<'key> SingleBundleSigner<'key> {
    /// Construct a new instance.
    pub fn new(bundle: DirectoryBundle) -> Self {
        Self {
            bundle,
            load_existing_signature_settings: false,
            entitlements: None,
            signing_key: None,
            certificates: vec![],
            time_stamp_url: None,
        }
    }

    /// Enable loading of existing signature settings.
    pub fn load_existing_signature_settings(&mut self) {
        self.load_existing_signature_settings = true;
    }

    /// Set the entitlements string for the bundle and all its nested binaries.
    pub fn entitlements_string(&mut self, v: impl ToString) {
        self.entitlements = Some(v.to_string());
    }

    /// Set the signing key and its public certificate to create a cryptographic signature.
    ///
    /// If not called, no cryptographic signature will be recorded (ad-hoc signing).
    pub fn signing_key(&mut self, private: &'key SigningKey, public: Certificate) {
        self.signing_key = Some((private, public));
    }

    /// Add a DER encoded X.509 public certificate to the signing chain.
    ///
    /// Use this to add the raw binary content of an ASN.1 encoded public
    /// certificate.
    ///
    /// The DER data is decoded at function call time. Any error decoding the
    /// certificate will result in `Err`. No validation of the certificate is
    /// performed.
    pub fn chain_certificate_der(
        &mut self,
        data: impl AsRef<[u8]>,
    ) -> Result<(), AppleCodesignError> {
        self.certificates
            .push(Certificate::from_der(data.as_ref())?);

        Ok(())
    }

    /// Add a PEM encoded X.509 public certificate to the signing chain.
    ///
    /// PEM data looks like `-----BEGIN CERTIFICATE-----` and is a common method
    /// for encoding certificate data. (PEM is effectively base64 encoded DER data.)
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
    /// When set, the server will be contacted during signing and a Time-Stamp Token will
    /// be embedded in the CMS data structure.
    pub fn time_stamp_url(&mut self, url: impl IntoUrl) -> Result<(), AppleCodesignError> {
        self.time_stamp_url = Some(url.into_url()?);

        Ok(())
    }

    /// Write a signed bundle to the given directory.
    pub fn write_signed_bundle(
        &self,
        log: &Logger,
        dest_dir: impl AsRef<Path>,
        additional_macho_files: &[(String, SignedMachOInfo)],
    ) -> Result<DirectoryBundle, AppleCodesignError> {
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

        self.bundle
            .identifier()
            .map_err(AppleCodesignError::DirectoryBundle)?
            .ok_or_else(|| AppleCodesignError::BundleNoIdentifier(self.bundle.info_plist_path()))?;

        warn!(&log, "collecting code resources files");
        let mut resources_builder = CodeResourcesBuilder::default_resources_rules()?;
        // Exclude code signature files we'll write.
        resources_builder.add_exclusion_rule(CodeResourcesRule::new("^_CodeSignature/")?.exclude());
        // Ignore notarization ticket.
        resources_builder.add_exclusion_rule(CodeResourcesRule::new("^CodeResources$")?.exclude());

        let handler = SingleBundleHandler {
            dest_dir: dest_dir_root.clone(),
            signer: self,
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
                handler.install_file(log, &file)?;
                info_plist_data = Some(std::fs::read(file.absolute_path())?);
            } else {
                resources_builder.process_file(log, &file, &handler)?;
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
            &log,
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
            warn!(
                log,
                "signing main executable {}",
                exe.relative_path().display()
            );

            let macho_data = std::fs::read(exe.absolute_path())?;
            let mut signer = MachOSigner::new(&macho_data)?;

            signer.load_existing_signature_context()?;

            signer.code_resources_data(&resources_data)?;

            if let Some(info_plist_data) = info_plist_data {
                signer.info_plist_data(&info_plist_data)?;
            }

            let mut new_data = Vec::<u8>::with_capacity(macho_data.len() + 2_usize.pow(17));
            signer.write_signed_binary(&mut new_data)?;

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
