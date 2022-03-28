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
        app_store_connect::{AppStoreConnectClient, ConnectToken, DevIdPlusInfoResponse},
        dmg::DmgReader,
        reader::PathType,
        AppleCodesignError,
    },
    apple_bundles::DirectoryBundle,
    apple_flat_package::PkgReader,
    log::{error, info, warn},
    md5::Digest,
    std::{
        fmt::Debug,
        fs::File,
        io::{BufRead, Cursor, Read, Seek, SeekFrom, Write},
        path::{Path, PathBuf},
        time::Duration,
    },
};

pub const TRANSPORTER_PATH_ENV_VARIABLE: &str = "APPLE_CODESIGN_TRANSPORTER_EXE";

/// Where Apple installs transporter by default on Linux and macOS.
const TRANSPORTER_DEFAULT_PATH_POSIX: &str = "/usr/local/itms/bin/iTMSTransporter";

/// Find the transporter executable to use for notarization.
///
/// See <https://help.apple.com/itc/transporteruserguide/#/apdAbeb95d60> for instructions
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

#[allow(unused)]
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
    #[allow(unused)]
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

fn digest_md5<R: Read>(reader: &mut R) -> Result<(u64, Vec<u8>), AppleCodesignError> {
    let mut hasher = md5::Md5::new();
    let mut size = 0;

    loop {
        let mut buffer = [0u8; 16384];
        let count = reader.read(&mut buffer)?;

        size += count as u64;
        hasher.update(&buffer[0..count]);

        if count < buffer.len() {
            break;
        }
    }

    Ok((size, hasher.finalize().to_vec()))
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
    let checksum_digest = hex::encode(digest_md5(&mut Cursor::new(&bundle_zip))?.1);

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

/// Write a DMG to an `.itmsp` directory.
pub fn write_dmg_to_app_store_package(
    source_path: &Path,
    dmg: &DmgReader,
    dest_dir: &Path,
) -> Result<(), AppleCodesignError> {
    let signature = dmg
        .embedded_signature()?
        .ok_or(AppleCodesignError::DmgNotarizeNoSignature)?;
    let cd = signature
        .code_directory()?
        .ok_or(AppleCodesignError::DmgNotarizeNoSignature)?;

    let primary_bundle_identifier = cd.ident.to_string();
    info!("primary bundle identifier: {}", primary_bundle_identifier);

    let app_platform = "osx".to_string();
    let asset_type = "developer-id-package".to_string();
    let file_name = source_path
        .file_name()
        .map(|x| x.to_string_lossy().to_string())
        .unwrap_or_else(|| "image.dmg".to_string());
    let checksum_type = "md5".to_string();
    let (size, checksum_digest) = digest_md5(&mut File::open(source_path)?)?;
    let checksum_digest = hex::encode(checksum_digest);

    let package = Package {
        software_assets: SoftwareAssets {
            app_platform,
            device_id: None,
            primary_bundle_identifier,
            asset: Asset {
                typ: asset_type,
                data_files: vec![DataFile {
                    file_name: file_name.clone(),
                    checksum_type,
                    checksum_digest,
                    size,
                }],
            },
        },
    };

    let metadata_xml = package
        .to_xml()
        .map_err(AppleCodesignError::AppMetadataXml)?;

    let dmg_path = dest_dir.join(&file_name);
    info!("writing {}", dmg_path.display());
    std::fs::copy(source_path, &dmg_path)?;

    let metadata_path = dest_dir.join("metadata.xml");
    info!("writing {}", metadata_path.display());
    std::fs::write(&metadata_path, &metadata_xml)?;

    Ok(())
}

/// Write a flat package (usually a `.pkg` file) to an `.itmsp` directory.
pub fn write_flat_package_to_app_store_package<F: Read + Seek + Debug>(
    mut pkg: PkgReader<F>,
    dest_dir: &Path,
) -> Result<(), AppleCodesignError> {
    let primary_bundle_identifier = if let Some(distribution) = pkg.distribution()? {
        warn!("notarizing a product installer");

        // We need to extract the primary bundle identifier from the `Distribution` XML.
        // We should probably honor the settings in the XML (this logic would belong in
        // the apple-flat-package crate). But for now we just pick the first bundle ID
        // we see.
        let id = if let Some(id) = distribution.pkg_ref.iter().find_map(|x| {
            if let Some(bv) = &x.bundle_version {
                bv.bundle.get(0).map(|bundle| bundle.id.clone())
            } else {
                None
            }
        }) {
            id
        } else {
            error!("unable to find bundle identifier in flat package (please report this bug)");
            return Err(AppleCodesignError::NotarizeFlatPackageParse);
        };

        id
    } else if let Some(_component) = pkg.root_component()? {
        warn!("notarizing a component installer");

        error!("support for notarizing a component installer is not yet implemented");
        return Err(AppleCodesignError::NotarizeFlatPackageParse);
    } else {
        error!("do not know how to extract bundle identifier from package installer");
        error!("please report this bug");

        return Err(AppleCodesignError::NotarizeFlatPackageParse);
    };

    warn!(
        "resolved primary bundle identifier to {}",
        primary_bundle_identifier
    );

    // Are flat packages supported on other platforms?
    let app_platform = "osx".to_string();
    let asset_type = "developer-id-package".to_string();

    // This doesn't appear to matter?
    let file_name = "installer.pkg".to_string();

    // In order to compute the digest we'll need access to the raw reader.
    let mut fh = pkg.into_inner().into_inner();
    fh.seek(SeekFrom::Start(0))?;

    let checksum_type = "md5".to_string();
    let (size, checksum_digest) = digest_md5(&mut fh)?;
    let checksum_digest = hex::encode(checksum_digest);

    let package = Package {
        software_assets: SoftwareAssets {
            app_platform,
            device_id: None,
            primary_bundle_identifier,
            asset: Asset {
                typ: asset_type,
                data_files: vec![DataFile {
                    file_name: file_name.clone(),
                    checksum_type,
                    checksum_digest,
                    size,
                }],
            },
        },
    };

    let metadata_xml = package
        .to_xml()
        .map_err(AppleCodesignError::AppMetadataXml)?;

    let pkg_path = dest_dir.join(&file_name);
    info!("writing {}", pkg_path.display());
    fh.seek(SeekFrom::Start(0))?;
    {
        let mut ofh = std::fs::File::create(&pkg_path)?;
        std::io::copy(&mut fh, &mut ofh)?;
    }

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

fn create_itmsp_temp_dir() -> Result<(tempfile::TempDir, PathBuf), AppleCodesignError> {
    let temp_dir = tempfile::Builder::new()
        .prefix("apple-codesign-")
        .tempdir()?;

    let id = uuid::Uuid::new_v4().to_string();

    let itmsp = format!("{}.itmsp", id);

    let dest_dir = temp_dir.path().join(&itmsp);
    std::fs::create_dir_all(&dest_dir)?;

    Ok((temp_dir, dest_dir))
}

/// Represents the result of a notarization upload.
pub enum NotarizationUpload {
    /// We performed the upload and only have the upload ID / UUID for it.
    ///
    /// (We probably didn't wait for the upload to finish processing.)
    UploadId(String),

    /// We performed an upload and have upload state from the server.
    DevIdResponse(DevIdPlusInfoResponse),
}

/// An entity for performing notarizations.
///
/// Notarization works by uploading content to Apple, waiting for Apple to inspect
/// and react to that upload, then downloading a notarization "ticket" from Apple
/// and incorporating it into the entity being signed.
#[derive(Clone)]
pub struct Notarizer {
    transporter_exe: PathBuf,
    auth: Option<(String, String, ConnectToken)>,

    /// How long to wait between polling the server for upload status.
    wait_poll_interval: Duration,
}

impl Notarizer {
    /// Construct a new instance.
    pub fn new() -> Result<Self, AppleCodesignError> {
        Ok(Self {
            transporter_exe: find_transporter_exe()
                .ok_or(AppleCodesignError::TransporterNotFound)?,
            auth: None,
            wait_poll_interval: Duration::from_secs(3),
        })
    }

    /// Set the API key used to upload.
    ///
    /// The API issuer is required when using an API key.
    pub fn set_api_key(
        &mut self,
        api_issuer: impl ToString,
        api_key: impl ToString,
    ) -> Result<(), AppleCodesignError> {
        let api_key = api_key.to_string();
        let api_issuer = api_issuer.to_string();

        let token = ConnectToken::from_api_key_id(api_key.clone(), api_issuer.clone())?;

        self.auth = Some((api_issuer, api_key, token));

        Ok(())
    }

    /// Attempt to notarize an asset defined by a filesystem path.
    ///
    /// The type of path is sniffed out and the appropriate notarization routine is called.
    pub fn notarize_path(
        &self,
        path: &Path,
        wait_limit: Option<Duration>,
    ) -> Result<NotarizationUpload, AppleCodesignError> {
        match PathType::from_path(path)? {
            PathType::Bundle => {
                let bundle = DirectoryBundle::new_from_path(path)
                    .map_err(AppleCodesignError::DirectoryBundle)?;
                self.notarize_bundle(&bundle, wait_limit)
            }
            PathType::Xar => {
                let fh = File::options().read(true).write(true).open(path)?;
                let pkg = PkgReader::new(fh)?;
                self.notarize_flat_package(pkg, wait_limit)
            }
            PathType::Dmg => {
                let mut fh = File::open(path)?;
                let reader = DmgReader::new(&mut fh)?;
                self.notarize_dmg(path, &reader, wait_limit)
            }
            PathType::MachO | PathType::Other => Err(AppleCodesignError::NotarizeUnsupportedPath(
                path.to_path_buf(),
            )),
        }
    }

    /// Attempt to notarize an on-disk bundle.
    ///
    /// If `wait_limit` is provided, we will wait for the upload to finish processing.
    /// Otherwise, this returns as soon as the upload is performed.
    pub fn notarize_bundle(
        &self,
        bundle: &DirectoryBundle,
        wait_limit: Option<Duration>,
    ) -> Result<NotarizationUpload, AppleCodesignError> {
        let (_temp_dir, dest_dir) = create_itmsp_temp_dir()?;
        warn!("writing App Store Package to {}", dest_dir.display());

        write_bundle_to_app_store_package(bundle, &dest_dir)?;

        self.upload_directory_and_maybe_wait(&dest_dir, wait_limit)
    }

    /// Attempt to notarize a DMG file.
    pub fn notarize_dmg(
        &self,
        dmg_path: &Path,
        dmg: &DmgReader,
        wait_limit: Option<Duration>,
    ) -> Result<NotarizationUpload, AppleCodesignError> {
        let (_temp_dir, dest_dir) = create_itmsp_temp_dir()?;
        warn!("writing App Store Package to {}", dest_dir.display());

        write_dmg_to_app_store_package(dmg_path, dmg, &dest_dir)?;

        self.upload_directory_and_maybe_wait(&dest_dir, wait_limit)
    }

    /// Attempt to notarize a flat package (`.pkg`) installer.
    pub fn notarize_flat_package<F: Read + Write + Seek + Sized + Debug>(
        &self,
        pkg: PkgReader<F>,
        wait_limit: Option<Duration>,
    ) -> Result<NotarizationUpload, AppleCodesignError> {
        let (_temp_dir, dest_dir) = create_itmsp_temp_dir()?;
        warn!("writing XAR to {}", dest_dir.display());

        write_flat_package_to_app_store_package(pkg, &dest_dir)?;

        self.upload_directory_and_maybe_wait(&dest_dir, wait_limit)
    }

    fn upload_directory_and_maybe_wait(
        &self,
        upload_dir: &Path,
        wait_limit: Option<Duration>,
    ) -> Result<NotarizationUpload, AppleCodesignError> {
        let upload_id = self.upload_app_store_package(upload_dir)?;

        let status = if let Some(wait_limit) = wait_limit {
            self.wait_on_app_store_package_upload_and_fetch_log(&upload_id, wait_limit)?
        } else {
            return Ok(NotarizationUpload::UploadId(upload_id));
        };

        // Make sure notarization was successful.
        let status = status.into_result()?;

        Ok(NotarizationUpload::DevIdResponse(status))
    }

    pub fn as_upload_command(&self) -> TransporterUploadCommand {
        let (api_issuer, api_key) = if let Some((issuer, key, _)) = &self.auth {
            (Some(issuer.clone()), Some(key.clone()))
        } else {
            (None, None)
        };

        TransporterUploadCommand {
            api_issuer,
            api_key,
            ..Default::default()
        }
    }

    /// Upload an App Store Package to Apple.
    ///
    /// This will invoke Transporter in upload mode to send the contents of a `.itmsp`
    /// directory to Apple.
    ///
    /// Returns the UUID of the upload.
    ///
    /// This does NOT wait on the server to process the upload.
    pub fn upload_app_store_package(&self, path: &Path) -> Result<String, AppleCodesignError> {
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

                if let Some(id) = upload_id_from_json_str(json_data)? {
                    upload_id = Some(id);
                }
            }
        }

        match upload_id {
            Some(id) => {
                warn!("transporter upload ID: {}", id);
                Ok(id)
            }
            None => Err(AppleCodesignError::NotarizeUploadFailure),
        }
    }

    /// Get the status of the upload.
    ///
    /// This queries Apple's servers to retrieve the state of a previously performed
    /// upload.
    pub fn get_upload_status(
        &self,
        upload_id: &str,
    ) -> Result<DevIdPlusInfoResponse, AppleCodesignError> {
        let client = match &self.auth {
            Some((_, _, token)) => Ok(AppStoreConnectClient::new(token.clone())?),
            None => Err(AppleCodesignError::NotarizeNoAuthCredentials),
        }?;

        let status = client.developer_id_plus_info_for_package_with_arguments(upload_id)?;

        Ok(status)
    }

    /// Wait on the upload of an app store package to complete.
    ///
    /// This will sit in a loop and poll Apple until the upload processing appears to complete.
    ///
    /// It will poll for up to `wait_limit` before returning `Err` if nothing
    /// happens in time.
    pub fn wait_on_app_store_package_upload(
        &self,
        upload_id: &str,
        wait_limit: Duration,
    ) -> Result<DevIdPlusInfoResponse, AppleCodesignError> {
        warn!(
            "waiting up to {}s for package upload {} to finish processing",
            wait_limit.as_secs(),
            upload_id
        );

        let start_time = std::time::Instant::now();

        loop {
            let status = self.get_upload_status(upload_id)?;

            let elapsed = start_time.elapsed();

            info!(
                "poll state after {}s: {}",
                elapsed.as_secs(),
                status.state_str()
            );

            if status.is_done() {
                warn!("upload operation complete");

                return Ok(status);
            }

            if elapsed >= wait_limit {
                warn!("reached wait limit after {}s", elapsed.as_secs());
                return Err(AppleCodesignError::NotarizeWaitLimitReached);
            }

            std::thread::sleep(self.wait_poll_interval);
        }
    }

    /// Obtain the processing log from an upload.
    pub fn fetch_upload_log(
        &self,
        response: &DevIdPlusInfoResponse,
    ) -> Result<String, AppleCodesignError> {
        if let Some(url) = &response.dev_id_plus.log_file_url {
            info!("fetching log from {}", url);
            let client = crate::ticket_lookup::default_client()?;

            let response = client.get(url).send()?;

            Ok(String::from_utf8_lossy(&response.bytes()?).to_string())
        } else {
            Err(AppleCodesignError::NotarizeNoLogUrl)
        }
    }

    /// Waits on an app store package upload and fetches and logs the upload log.
    ///
    /// This is just a convenience around [Self::wait_on_app_store_package_upload()] and
    /// [Self::fetch_upload_log()].
    pub fn wait_on_app_store_package_upload_and_fetch_log(
        &self,
        upload_id: &str,
        wait_limit: Duration,
    ) -> Result<DevIdPlusInfoResponse, AppleCodesignError> {
        let status = self.wait_on_app_store_package_upload(upload_id, wait_limit)?;

        let log = self.fetch_upload_log(&status)?;
        for line in log.lines() {
            warn!("upload log> {}", line);
        }

        Ok(status)
    }
}
