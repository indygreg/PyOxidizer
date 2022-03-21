// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Apple notarization functionality.

Notarization works by uploading a payload to Apple servers and waiting for
Apple to scan the submitted content. If Apple is appeased by your submission,
they issue a notarization ticket, which can be downloaded and *stapled* (just
a fancy word for *attached*) to the content you upload.

This module implements functionality for uploading content to Apple
and waiting on the availability of a notarization ticket.
*/

use {
    crate::{
        app_metadata::{Asset, DataFile, Package, SoftwareAssets},
        AppleCodesignError,
    },
    apple_bundles::DirectoryBundle,
    log::{error, info, warn},
    md5::Digest,
    std::{
        io::{BufRead, Write},
        path::{Path, PathBuf},
    },
};

pub const TRANSPORTER_PATH_ENV_VARIABLE: &str = "APPLE_CODESIGN_TRANSPORTER_EXE";

/// Where Apple installs transporter by default on Linux and macOS.
const TRANSPORTER_DEFAULT_PATH_POSIX: &str = "/usr/local/itms/bin/iTMSTransporter";

/// Find the transporter executable to use for notarization.
///
/// See https://help.apple.com/itc/transporteruserguide/#/apdAbeb95d60 for instructions
/// on installing Transporter and where default installs are often located.
pub fn find_transporter_exe() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(TRANSPORTER_PATH_ENV_VARIABLE) {
        Some(PathBuf::from(path))
    } else if let Ok(path) = which::which("iTMSTransporter") {
        Some(path)
    } else {
        let candidate = PathBuf::from(TRANSPORTER_DEFAULT_PATH_POSIX);

        if candidate.exists() {
            return Some(candidate);
        }

        for env in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(path) = std::env::var_os(env) {
                let candidate = PathBuf::from(path).join("itms").join("iTMSTransporter.cmd");

                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }

        None
    }
}

#[derive(Clone, Copy, Debug)]
pub enum UploadDistribution {
    AppStore,
    DeveloperId,
}

impl ToString for UploadDistribution {
    fn to_string(&self) -> String {
        match self {
            Self::AppStore => "AppStore",
            Self::DeveloperId => "DeveloperId",
        }
        .to_string()
    }
}

#[derive(Clone, Copy, Debug)]
pub enum VerifyProgress {
    Text,
    Json,
}

impl ToString for VerifyProgress {
    fn to_string(&self) -> String {
        match self {
            Self::Text => "text",
            Self::Json => "json",
        }
        .to_string()
    }
}

/// Represents the arguments to a Transporter `upload` command invocation.
#[derive(Clone, Debug, Default)]
pub struct TransporterUploadCommand {
    /// Specifies your App Store Connect Issuer ID.
    ///
    /// Must also provide `api_key` when set.
    pub api_issuer: Option<String>,

    /// Specifies your App Store Connect API Key Id.
    ///
    /// Must also provide `api_issuer` when set.
    pub api_key: Option<String>,

    pub asc_provider: Option<String>,

    pub asset_description: Option<String>,

    /// Path to software asset to upload.
    ///
    /// `.dmg`, `.ipa`, .`pkg`, or `.zip` file.
    pub asset_file: Option<PathBuf>,

    pub source: Option<PathBuf>,

    /// The type of distribution.
    pub distribution: Option<UploadDistribution>,

    pub username: Option<String>,
    pub password: Option<String>,

    pub primary_bundle_id: Option<String>,

    pub provider_short_name: Option<String>,

    pub verify_progress: Option<VerifyProgress>,
}

impl TransporterUploadCommand {
    /// Derive the arguments for this command invocation.
    pub fn arguments(&self) -> Vec<String> {
        let mut args = vec!["-m".into(), "upload".into()];

        if let Some(issuer) = &self.api_issuer {
            args.push("-apiIssuer".into());
            args.push(issuer.clone());
        }

        if let Some(key) = &self.api_key {
            args.push("-apiKey".into());
            args.push(key.clone());
        }

        if let Some(value) = &self.asc_provider {
            args.push("-asc_provider".into());
            args.push(value.clone());
        }

        if let Some(value) = &self.asset_description {
            args.push("-assetDescription".into());
            args.push(value.clone());
        }

        if let Some(path) = &self.asset_file {
            args.push("-assetFile".into());
            args.push(format!("{}", path.display()));
        }

        if let Some(path) = &self.source {
            args.push("-f".into());
            args.push(format!("{}", path.display()));
        }

        if let Some(value) = &self.distribution {
            args.push("-distribution".into());
            args.push(value.to_string());
        }

        if let Some(value) = &self.username {
            args.push("-u".into());
            args.push(value.clone());
        }

        if let Some(value) = &self.password {
            args.push("-p".into());
            args.push(value.clone());
        }

        if let Some(value) = &self.primary_bundle_id {
            args.push("-primaryBundleId".into());
            args.push(value.clone());
        }

        if let Some(value) = &self.provider_short_name {
            args.push("-s".into());
            args.push(value.clone());
        }

        if let Some(value) = &self.verify_progress {
            args.push("-vp".into());
            args.push(value.to_string());
        }

        args
    }
}

/// Produce zip file data from a [DirectoryBundle].
///
/// The built zip file will contain all the files from the bundle under a directory
/// tree having the bundle name. e.g. if you pass `MyApp.app`, the zip will have
/// files like `MyApp.app/Contents/Info.plist`.
pub fn bundle_to_zip(bundle: &DirectoryBundle) -> Result<Vec<u8>, AppleCodesignError> {
    let mut zf = zip::ZipWriter::new(std::io::Cursor::new(vec![]));

    for file in bundle
        .files(true)
        .map_err(AppleCodesignError::DirectoryBundle)?
    {
        let entry = file
            .as_file_entry()
            .map_err(AppleCodesignError::DirectoryBundle)?;

        let options =
            zip::write::FileOptions::default().unix_permissions(if entry.is_executable() {
                0o0755
            } else {
                0o0644
            });

        zf.start_file(
            format!("{}/{}", bundle.name(), file.relative_path().display()),
            options,
        )?;
        zf.write_all(&entry.resolve_content()?)?;
    }

    let writer = zf.finish()?;

    Ok(writer.into_inner())
}

/// Write a bundle to an `.itmsp` directory.
///
/// In order to upload with Apple Transporter, we need to persist uploaded assets
/// to the local filesystem. And Transporter insists on the uploaded content having
/// a well-defined layout.
///
/// This function will write out a bundle into a destination directory, effectively
/// enabling that directory to be uploaded with transporter.
///
/// The directory passed should ideally be empty before this is called. The directory
/// should also be named `*.itmsp`.
pub fn write_bundle_to_app_store_package(
    bundle: &DirectoryBundle,
    dest_dir: &Path,
) -> Result<(), AppleCodesignError> {
    // The notarization payload requires a handful of metadata derived from the bundle.
    // Collect these first, as this can easily fail.

    let primary_bundle_identifier = bundle
        .identifier()
        .map_err(AppleCodesignError::DirectoryBundle)?
        .ok_or_else(|| AppleCodesignError::BundleNoIdentifier(bundle.root_dir().to_path_buf()))?;
    info!("primary bundle identifier: {}", primary_bundle_identifier);

    // The app platform is essentially the OS the bundle is targeting.
    // TODO need better logic here.
    let contents_macos = bundle.resolve_path("MacOS");
    if !contents_macos.is_dir() {
        return Err(AppleCodesignError::BundleUnknownAppPlatform);
    }

    let app_platform = "osx".to_string();
    info!("app platform: {}", app_platform);

    // The asset type is derived from the type of entity we're uploading.
    // Since we're uploading bundles for macOS, we can hardcode this for now.
    let asset_type = "developer-id-package";
    info!("asset type: {}", asset_type);

    // The notarization payload consists of a metadata record describing a zip file
    // containing the bundle.
    warn!(
        "producing zip file containing {}",
        bundle.root_dir().display()
    );
    let bundle_zip = bundle_to_zip(bundle)?;

    // MD5 is always used for the digest.
    let checksum_type = "md5".to_string();
    let mut h = md5::Md5::new();
    h.update(&bundle_zip);
    let checksum_digest = hex::encode(h.finalize());

    let file_name = format!("{}.zip", bundle.name());

    // Produce the metadata.xml file content.
    let package = Package {
        software_assets: SoftwareAssets {
            app_platform,
            device_id: None,
            primary_bundle_identifier,
            asset: Asset {
                typ: asset_type.to_string(),
                data_files: vec![DataFile {
                    file_name: file_name.clone(),
                    checksum_type,
                    checksum_digest,
                    size: bundle_zip.len() as _,
                }],
            },
        },
    };

    let metadata_xml = package
        .to_xml()
        .map_err(AppleCodesignError::AppMetadataXml)?;

    let zip_path = dest_dir.join(&file_name);
    info!("writing {}", zip_path.display());
    std::fs::write(&zip_path, &bundle_zip)?;

    let metadata_path = dest_dir.join("metadata.xml");
    info!("writing {}", metadata_path.display());
    std::fs::write(&metadata_path, &metadata_xml)?;

    Ok(())
}

fn upload_id_from_json_str(s: &str) -> Result<Option<String>, AppleCodesignError> {
    let value = serde_json::from_str::<serde_json::Value>(s)?;

    if let serde_json::Value::Object(map) = value {
        if let Some(serde_json::Value::Object(map)) = map.get("dev-id-results") {
            if let Some(serde_json::Value::String(id)) = map.get("upload_id") {
                Ok(Some(id.to_string()))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    } else {
        Ok(None)
    }
}

/// An entity for performing notarizations.
///
/// Notarization works by uploading content to Apple, waiting for Apple to inspect
/// and react to that upload, then downloading a notarization "ticket" from Apple
/// and incorporating it into the entity being signed.
#[derive(Clone, Debug)]
pub struct Notarizer {
    transporter_exe: PathBuf,
    api_issuer: Option<String>,
    api_key: Option<String>,
}

impl Notarizer {
    /// Construct a new instance.
    pub fn new() -> Result<Self, AppleCodesignError> {
        Ok(Self {
            transporter_exe: find_transporter_exe()
                .ok_or(AppleCodesignError::TransporterNotFound)?,
            api_issuer: None,
            api_key: None,
        })
    }

    /// Set the API key used to upload.
    ///
    /// The API issuer is required when using an API key.
    pub fn set_api_key(&mut self, api_issuer: impl ToString, api_key: impl ToString) {
        self.api_issuer = Some(api_issuer.to_string());
        self.api_key = Some(api_key.to_string());
    }

    /// Attempt to notarize an asset defined by a filesystem path.
    ///
    /// The type of path is sniffed out and the appropriate notarization routine is called.
    pub fn notarize_path(&self, path: &Path) -> Result<(), AppleCodesignError> {
        if let Ok(bundle) = DirectoryBundle::new_from_path(path) {
            self.notarize_bundle(&bundle)
        } else {
            Err(AppleCodesignError::NotarizeUnsupportedPath(
                path.to_path_buf(),
            ))
        }
    }

    /// Attempt to notarize an on-disk bundle.
    pub fn notarize_bundle(&self, bundle: &DirectoryBundle) -> Result<(), AppleCodesignError> {
        let temp_dir = tempfile::Builder::new()
            .prefix("apple-codesign-")
            .tempdir()?;

        let id = uuid::Uuid::new_v4().to_string();

        let itmsp = format!("{}.itmsp", id);

        let dest_dir = temp_dir.path().join(&itmsp);
        std::fs::create_dir_all(&dest_dir)?;
        warn!("writing App Store Package to {}", dest_dir.display());

        write_bundle_to_app_store_package(bundle, &dest_dir)?;

        self.upload_app_store_package(&dest_dir)?;

        Ok(())
    }

    pub fn as_upload_command(&self) -> TransporterUploadCommand {
        TransporterUploadCommand {
            api_issuer: self.api_issuer.clone(),
            api_key: self.api_key.clone(),
            ..Default::default()
        }
    }

    /// Upload an App Store Package to Apple.
    ///
    /// This will invoke Transporter in upload mode to send the contents of a `.itmsp`
    /// directory to Apple.
    pub fn upload_app_store_package(&self, path: &Path) -> Result<(), AppleCodesignError> {
        let mut command = self.as_upload_command();
        command.verify_progress = Some(VerifyProgress::Json);
        command.source = Some(path.to_path_buf());

        warn!(
            "invoking {} with args: {:?}",
            self.transporter_exe.display(),
            command.arguments()
        );
        let command = duct::cmd(&self.transporter_exe, command.arguments()).stderr_to_stdout();

        let reader = command.reader()?;
        let reader = std::io::BufReader::new(reader);

        let mut upload_id = None;

        let mut poisoned = false;

        for line in reader.lines() {
            let line = line?;

            if line.contains("ERROR") {
                poisoned = true;
            }

            if poisoned {
                error!("transporter error> {}", line);
            } else {
                info!("transporter output> {}", line);
            }

            if let (Some(start), Some(end)) = (line.find("JSON-START>>"), line.find("<<JSON-END")) {
                let json_data = &line[start + "JSON-START>>".len()..end];

                if let Some(id) = upload_id_from_json_str(&json_data)? {
                    upload_id = Some(id);
                }
            }
        }

        if let Some(id) = &upload_id {
            warn!("transporter upload ID: {}", id);
        }

        Ok(())
    }
}
