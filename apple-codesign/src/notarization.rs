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
        app_store_connect::{
            AppStoreConnectClient, ConnectTokenEncoder, NewSubmissionResponse, SubmissionResponse,
            SubmissionResponseStatus,
        },
        reader::PathType,
        AppleCodesignError,
    },
    apple_bundles::DirectoryBundle,
    aws_sdk_s3::{Credentials, Region},
    aws_smithy_http::byte_stream::ByteStream,
    log::{info, warn},
    sha2::Digest,
    std::{
        fs::File,
        io::{Read, Seek, SeekFrom, Write},
        path::{Path, PathBuf},
        time::Duration,
    },
};

fn digest<H: Digest, R: Read>(reader: &mut R) -> Result<(u64, Vec<u8>), AppleCodesignError> {
    let mut hasher = H::new();
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

fn digest_sha256<R: Read>(reader: &mut R) -> Result<(u64, Vec<u8>), AppleCodesignError> {
    digest::<sha2::Sha256, R>(reader)
}

/// Produce zip file data from a [DirectoryBundle].
///
/// The built zip file will contain all the files from the bundle under a directory
/// tree having the bundle name. e.g. if you pass `MyApp.app`, the zip will have
/// files like `MyApp.app/Contents/Info.plist`.
pub fn bundle_to_zip(bundle: &DirectoryBundle) -> Result<Vec<u8>, AppleCodesignError> {
    let mut zf = zip::ZipWriter::new(std::io::Cursor::new(vec![]));

    let mut symlinks = vec![];

    for file in bundle
        .files(true)
        .map_err(AppleCodesignError::DirectoryBundle)?
    {
        let entry = file
            .as_file_entry()
            .map_err(AppleCodesignError::DirectoryBundle)?;

        let name =
            format!("{}/{}", bundle.name(), file.relative_path().display()).replace('\\', "/");

        let options = zip::write::FileOptions::default();

        let options = if entry.link_target().is_some() {
            symlinks.push(name.as_bytes().to_vec());
            options.compression_method(zip::CompressionMethod::Stored)
        } else if entry.is_executable() {
            options.unix_permissions(0o755)
        } else {
            options.unix_permissions(0o644)
        };

        zf.start_file(name, options)?;

        if let Some(target) = entry.link_target() {
            zf.write_all(target.to_string_lossy().replace('\\', "/").as_bytes())?;
        } else {
            zf.write_all(&entry.resolve_content()?)?;
        }
    }

    let mut writer = zf.finish()?;

    // Current versions of the zip crate don't support writing symlinks. We
    // added that support upstream but it isn't released yet.
    // TODO remove this hackery once we upgrade the zip crate.
    let eocd = zip_structs::zip_eocd::ZipEOCD::from_reader(&mut writer)?;
    let cd_entries =
        zip_structs::zip_central_directory::ZipCDEntry::all_from_eocd(&mut writer, &eocd)?;

    for mut cd in cd_entries {
        if symlinks.contains(&cd.file_name_raw) {
            cd.external_file_attributes =
                (0o120777 << 16) | (cd.external_file_attributes & 0x0000ffff);
            writer.seek(SeekFrom::Start(cd.starting_position_with_signature))?;
            cd.write(&mut writer)?;
        }
    }

    Ok(writer.into_inner())
}

/// Represents the result of a notarization upload.
pub enum NotarizationUpload {
    /// We performed the upload and only have the upload ID / UUID for it.
    ///
    /// (We probably didn't wait for the upload to finish processing.)
    UploadId(String),

    /// We performed an upload and have upload state from the server.
    NotaryResponse(SubmissionResponse),
}

enum UploadKind {
    Data(Vec<u8>),
    Path(PathBuf),
}

/// An entity for performing notarizations.
///
/// Notarization works by uploading content to Apple, waiting for Apple to inspect
/// and react to that upload, then downloading a notarization "ticket" from Apple
/// and incorporating it into the entity being signed.
#[derive(Clone)]
pub struct Notarizer {
    token_encoder: Option<ConnectTokenEncoder>,

    /// How long to wait between polling the server for upload status.
    wait_poll_interval: Duration,
}

impl Notarizer {
    /// Construct a new instance.
    pub fn new() -> Result<Self, AppleCodesignError> {
        Ok(Self {
            token_encoder: None,
            wait_poll_interval: Duration::from_secs(3),
        })
    }

    /// Define the App Store Connect JWT token encoder to use.
    ///
    /// This is the most generic way to define the credentials for this client.
    pub fn set_token_encoder(&mut self, encoder: ConnectTokenEncoder) {
        self.token_encoder = Some(encoder);
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

        let encoder = ConnectTokenEncoder::from_api_key_id(api_key, api_issuer)?;

        self.set_token_encoder(encoder);

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
            PathType::Xar => self.notarize_flat_package(path, wait_limit),
            PathType::Dmg => self.notarize_dmg(path, wait_limit),
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
        let zipfile = bundle_to_zip(bundle)?;
        let digest = sha2::Sha256::digest(&zipfile);

        let submission = self.create_submission(&digest, bundle.name())?;

        self.upload_s3_and_maybe_wait(submission, UploadKind::Data(zipfile), wait_limit)
    }

    /// Attempt to notarize a DMG file.
    pub fn notarize_dmg(
        &self,
        dmg_path: &Path,
        wait_limit: Option<Duration>,
    ) -> Result<NotarizationUpload, AppleCodesignError> {
        let filename = dmg_path
            .file_name()
            .map(|x| x.to_string_lossy().to_string())
            .unwrap_or_else(|| "dmg".to_string());

        let (_, digest) = digest_sha256(&mut File::open(dmg_path)?)?;

        let submission = self.create_submission(&digest, &filename)?;

        self.upload_s3_and_maybe_wait(
            submission,
            UploadKind::Path(dmg_path.to_path_buf()),
            wait_limit,
        )
    }

    /// Attempt to notarize a flat package (`.pkg`) installer.
    pub fn notarize_flat_package(
        &self,
        pkg_path: &Path,
        wait_limit: Option<Duration>,
    ) -> Result<NotarizationUpload, AppleCodesignError> {
        let filename = pkg_path
            .file_name()
            .map(|x| x.to_string_lossy().to_string())
            .unwrap_or_else(|| "pkg".to_string());

        let (_, digest) = digest_sha256(&mut File::open(pkg_path)?)?;

        let submission = self.create_submission(&digest, &filename)?;

        self.upload_s3_and_maybe_wait(
            submission,
            UploadKind::Path(pkg_path.to_path_buf()),
            wait_limit,
        )
    }
}

impl Notarizer {
    /// Tell the notary service to expect an upload to S3.
    fn create_submission(
        &self,
        digest: &[u8],
        name: &str,
    ) -> Result<NewSubmissionResponse, AppleCodesignError> {
        let client = match &self.token_encoder {
            Some(token) => Ok(AppStoreConnectClient::new(token.clone())?),
            _ => Err(AppleCodesignError::NotarizeNoAuthCredentials),
        }?;

        Ok(client.create_submission(&hex::encode(digest), name)?)
    }

    fn upload_s3_package<'a>(
        &self,
        submission: &NewSubmissionResponse,
        upload: UploadKind,
    ) -> Result<(), AppleCodesignError> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let bytestream = match upload {
            UploadKind::Data(data) => ByteStream::from(data),
            UploadKind::Path(path) => rt.block_on(ByteStream::from_path(path))?,
        };

        // upload using s3 api
        let config = rt.block_on(
            aws_config::from_env()
                .credentials_provider(Credentials::new(
                    submission.data.attributes.aws_access_key_id.clone(),
                    submission.data.attributes.aws_secret_access_key.clone(),
                    Some(submission.data.attributes.aws_session_token.clone()),
                    None,
                    "apple-codesign",
                ))
                // The region is not given anywhere in the Apple documentation. From
                // manually testing all available regions, it appears to be
                // us-west-2.
                .region(Region::new("us-west-2"))
                .load(),
        );

        let s3_client = aws_sdk_s3::Client::new(&config);

        // TODO: Support multi-part upload.
        // Unfortunately, aws-sdk-s3 does not have a simple upload_file helper
        // like it does in other languages.
        // See https://github.com/awslabs/aws-sdk-rust/issues/494
        let fut = s3_client
            .put_object()
            .bucket(submission.data.attributes.bucket.clone())
            .key(submission.data.attributes.object.clone())
            .body(bytestream)
            .send();

        let _res = rt.block_on(fut).map_err(aws_sdk_s3::Error::from)?;

        Ok(())
    }

    fn upload_s3_and_maybe_wait(
        &self,
        submission: NewSubmissionResponse,
        upload_data: UploadKind,
        wait_limit: Option<Duration>,
    ) -> Result<NotarizationUpload, AppleCodesignError> {
        self.upload_s3_package(&submission, upload_data)?;

        let status = if let Some(wait_limit) = wait_limit {
            self.wait_on_notarization_and_fetch_log(&submission.data.id, wait_limit)?
        } else {
            return Ok(NotarizationUpload::UploadId(submission.data.id));
        };

        // Make sure notarization was successful.
        let status = status.into_result()?;

        Ok(NotarizationUpload::NotaryResponse(status))
    }

    pub fn wait_on_notarization(
        &self,
        upload_id: &str,
        wait_limit: Duration,
    ) -> Result<SubmissionResponse, AppleCodesignError> {
        warn!(
            "waiting up to {}s for package upload {} to finish processing",
            wait_limit.as_secs(),
            upload_id
        );

        let start_time = std::time::Instant::now();

        loop {
            let client = match &self.token_encoder {
                Some(token) => Ok(AppStoreConnectClient::new(token.clone())?),
                None => Err(AppleCodesignError::NotarizeNoAuthCredentials),
            }?;

            let status = client.get_submission(upload_id)?;

            let elapsed = start_time.elapsed();

            info!(
                "poll state after {}s: {:?}",
                elapsed.as_secs(),
                status.data.attributes.status
            );

            if status.data.attributes.status != SubmissionResponseStatus::InProgress {
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
    pub fn fetch_notarization_log(
        &self,
        submission_id: &str,
    ) -> Result<String, AppleCodesignError> {
        info!("fetching log from {}", submission_id);
        let client = match &self.token_encoder {
            Some(token) => Ok(AppStoreConnectClient::new(token.clone())?),
            None => Err(AppleCodesignError::NotarizeNoAuthCredentials),
        }?;
        let logs = client.get_submission_log(&submission_id)?;

        Ok(logs.to_string())
    }

    /// Waits on an app store package upload and fetches and logs the upload log.
    ///
    /// This is just a convenience around [Self::wait_on_app_store_package_upload()] and
    /// [Self::fetch_upload_log()].
    pub fn wait_on_notarization_and_fetch_log(
        &self,
        upload_id: &str,
        wait_limit: Duration,
    ) -> Result<SubmissionResponse, AppleCodesignError> {
        let status = self.wait_on_notarization(upload_id, wait_limit)?;

        let log = self.fetch_notarization_log(upload_id)?;
        for line in log.lines() {
            warn!("upload log> {}", line);
        }

        Ok(status)
    }
}
