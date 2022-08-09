// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

pub mod api_token;
pub mod notary_api;

use {
    self::api_token::{AppStoreConnectToken, ConnectTokenEncoder},
    crate::AppleCodesignError,
    log::{debug, error},
    reqwest::blocking::Client,
    serde::{de::DeserializeOwned, Deserialize, Serialize},
    serde_json::Value,
    std::{fs::Permissions, io::Write, path::Path, sync::Mutex},
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[cfg(unix)]
fn set_permissions_private(p: &mut Permissions) {
    p.set_mode(0o600);
}

#[cfg(windows)]
fn set_permissions_private(_: &mut Permissions) {}

/// Represents all metadata for an App Store Connect API Key.
///
/// This is a convenience type to aid in the generic representation of all the components
/// of an App Store Connect API Key. The type supports serialization so we save as a single
/// file or payload to enhance usability (so people don't need to provide all 3 pieces of the
/// API Key for all operations).
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UnifiedApiKey {
    /// Who issued the key.
    ///
    /// Likely a UUID.
    issuer_id: String,

    /// Key identifier.
    ///
    /// An alphanumeric string like `DEADBEEF42`.
    key_id: String,

    /// Base64 encoded DER of ECDSA private key material.
    private_key: String,
}

impl UnifiedApiKey {
    /// Construct an instance from constitute parts and a PEM encoded ECDSA private key.
    ///
    /// This is what you want to use if importing a private key from the file downloaded
    /// from the App Store Connect web interface.
    pub fn from_ecdsa_pem_path(
        issuer_id: impl ToString,
        key_id: impl ToString,
        path: impl AsRef<Path>,
    ) -> Result<Self, AppleCodesignError> {
        let pem_data = std::fs::read(path.as_ref())?;

        let parsed = pem::parse(pem_data).map_err(|e| {
            AppleCodesignError::AppStoreConnectApiKey(format!("error parsing PEM: {}", e))
        })?;

        if parsed.tag != "PRIVATE KEY" {
            return Err(AppleCodesignError::AppStoreConnectApiKey(
                "does not look like a PRIVATE KEY".to_string(),
            ));
        }

        let private_key = base64::encode(parsed.contents);

        Ok(Self {
            issuer_id: issuer_id.to_string(),
            key_id: key_id.to_string(),
            private_key,
        })
    }

    /// Construct an instance from serialized JSON.
    pub fn from_json(data: impl AsRef<[u8]>) -> Result<Self, AppleCodesignError> {
        Ok(serde_json::from_slice(data.as_ref())?)
    }

    /// Construct an instance from a JSON file.
    pub fn from_json_path(path: impl AsRef<Path>) -> Result<Self, AppleCodesignError> {
        let data = std::fs::read(path.as_ref())?;

        Self::from_json(data)
    }

    /// Serialize this instance to a JSON object.
    pub fn to_json_string(&self) -> Result<String, AppleCodesignError> {
        Ok(serde_json::to_string_pretty(&self)?)
    }

    /// Write this instance to a JSON file.
    ///
    /// Since the file contains sensitive data, it will have limited read permissions
    /// on platforms where this is implemented. Parent directories will be created if missing
    /// using default permissions for created directories.
    ///
    /// Permissions on the resulting file may not be as restrictive as desired. It is up
    /// to callers to additionally harden as desired.
    pub fn write_json_file(&self, path: impl AsRef<Path>) -> Result<(), AppleCodesignError> {
        let path = path.as_ref();

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let data = self.to_json_string()?;

        let mut fh = std::fs::File::create(path)?;
        let mut permissions = fh.metadata()?.permissions();
        set_permissions_private(&mut permissions);
        fh.set_permissions(permissions)?;
        fh.write_all(data.as_bytes())?;

        Ok(())
    }
}

impl TryFrom<UnifiedApiKey> for ConnectTokenEncoder {
    type Error = AppleCodesignError;

    fn try_from(value: UnifiedApiKey) -> Result<Self, Self::Error> {
        let der = base64::decode(value.private_key).map_err(|e| {
            AppleCodesignError::AppStoreConnectApiKey(format!(
                "failed to base64 decode private key: {}",
                e
            ))
        })?;

        Self::from_ecdsa_der(value.key_id, value.issuer_id, &der)
    }
}

/// A client for App Store Connect API.
///
/// The client isn't generic. Don't get any ideas.
pub struct AppStoreConnectClient {
    client: Client,
    connect_token: ConnectTokenEncoder,
    token: Mutex<Option<AppStoreConnectToken>>,
}

impl AppStoreConnectClient {
    pub fn new(connect_token: ConnectTokenEncoder) -> Result<Self, AppleCodesignError> {
        Ok(Self {
            client: crate::ticket_lookup::default_client()?,
            connect_token,
            token: Mutex::new(None),
        })
    }

    fn get_token(&self) -> Result<String, AppleCodesignError> {
        let mut token = self.token.lock().unwrap();

        // TODO need to handle token expiration.
        if token.is_none() {
            token.replace(self.connect_token.new_token(300)?);
        }

        Ok(token.as_ref().unwrap().clone())
    }

    fn send_request<T: DeserializeOwned>(
        &self,
        request: reqwest::blocking::RequestBuilder,
    ) -> Result<T, AppleCodesignError> {
        let request = request.build()?;
        let url = request.url().to_string();

        debug!("{} {}", request.method(), url);

        let response = self.client.execute(request)?;

        if response.status().is_success() {
            Ok(response.json::<T>()?)
        } else {
            error!("HTTP error from {}", url);

            let body = response.bytes()?;

            if let Ok(value) = serde_json::from_slice::<Value>(body.as_ref()) {
                for line in serde_json::to_string_pretty(&value)?.lines() {
                    error!("{}", line);
                }
            } else {
                error!("{}", String::from_utf8_lossy(body.as_ref()));
            }

            Err(AppleCodesignError::NotarizeServerError)
        }
    }

    /// Create a submission to the Notary API.
    pub fn create_submission(
        &self,
        sha256: &str,
        submission_name: &str,
    ) -> Result<notary_api::NewSubmissionResponse, AppleCodesignError> {
        let token = self.get_token()?;

        let body = notary_api::NewSubmissionRequest {
            notifications: Vec::new(),
            sha256: sha256.to_string(),
            submission_name: submission_name.to_string(),
        };
        let req = self
            .client
            .post(notary_api::APPLE_NOTARY_SUBMIT_SOFTWARE_URL)
            .bearer_auth(token)
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .json(&body);

        self.send_request(req)
    }

    /// Fetch the status of a Notary API submission.
    pub fn get_submission(
        &self,
        submission_id: &str,
    ) -> Result<notary_api::SubmissionResponse, AppleCodesignError> {
        let token = self.get_token()?;

        let req = self
            .client
            .get(format!(
                "{}/{}",
                notary_api::APPLE_NOTARY_SUBMIT_SOFTWARE_URL,
                submission_id
            ))
            .bearer_auth(token)
            .header("Accept", "application/json");

        self.send_request(req)
    }

    /// Fetch details about a single completed notarization.
    pub fn get_submission_log(&self, submission_id: &str) -> Result<Value, AppleCodesignError> {
        let token = self.get_token()?;

        let req = self
            .client
            .get(format!(
                "{}/{}/logs",
                notary_api::APPLE_NOTARY_SUBMIT_SOFTWARE_URL,
                submission_id
            ))
            .bearer_auth(token)
            .header("Accept", "application/json");

        let res = self.send_request::<notary_api::SubmissionLogResponse>(req)?;

        let url = res.data.attributes.developer_log_url;
        let logs = self.client.get(url).send()?.json::<Value>()?;

        Ok(logs)
    }
}
