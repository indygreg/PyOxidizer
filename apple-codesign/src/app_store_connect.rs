// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use {
    crate::AppleCodesignError,
    jsonwebtoken::{Algorithm, EncodingKey, Header},
    reqwest::blocking::Client,
    serde::{Deserialize, Serialize},
    serde_json::Value,
    std::{path::Path, sync::Mutex, time::SystemTime},
};

pub const ITUNES_PRODUCER_SERVICE_URL: &str = "https://contentdelivery.itunes.apple.com/WebObjects/MZLabelService.woa/json/MZITunesProducerService";

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ConnectTokenRequest {
    iss: String,
    iat: u64,
    exp: u64,
    aud: String,
}

/// An authentication token for the App Store Connect API.
pub struct ConnectToken {
    key_id: String,
    issuer_id: String,
    encoding_key: EncodingKey,
}

impl ConnectToken {
    pub fn from_pkcs8_ec(
        data: &[u8],
        key_id: String,
        issuer_id: String,
    ) -> Result<Self, AppleCodesignError> {
        let encoding_key = EncodingKey::from_ec_pem(data)?;

        Ok(Self {
            key_id,
            issuer_id,
            encoding_key,
        })
    }

    pub fn from_path(
        path: impl AsRef<Path>,
        key_id: String,
        issuer_id: String,
    ) -> Result<Self, AppleCodesignError> {
        let data = std::fs::read(path.as_ref())?;

        Self::from_pkcs8_ec(&data, key_id, issuer_id)
    }

    pub fn new_token(&self, duration: u64) -> Result<String, AppleCodesignError> {
        let header = Header {
            kid: Some(self.key_id.clone()),
            alg: Algorithm::ES256,
            ..Default::default()
        };

        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("calculating UNIX time should never fail")
            .as_secs();

        let claims = ConnectTokenRequest {
            iss: self.issuer_id.clone(),
            iat: now,
            exp: now + duration,
            aud: "appstoreconnect-v1".to_string(),
        };

        let token = jsonwebtoken::encode(&header, &claims, &self.encoding_key)?;

        Ok(token)
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct JsonRpcRequest {
    id: String,
    #[serde(rename = "jsonrpc")]
    json_rpc: String,
    method: String,
    params: Value,
}

#[derive(Clone, Debug, Deserialize)]
pub struct JsonRpcResponse {
    pub id: String,
    pub result: Value,
}

#[derive(Clone, Debug, Serialize)]
pub struct DevIdPlusInfoRequest {
    #[serde(rename = "Application")]
    pub application: String,
    #[serde(rename = "ApplicationBundleId")]
    pub application_bundle_id: String,
    #[serde(rename = "DS_PLIST")]
    pub ds_plist: String,
    #[serde(rename = "RequestUUID")]
    pub request_uuid: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DevIdPlusInfoResponse {
    #[serde(rename = "DevIDPlus")]
    pub dev_id_plus: DevIdPlus,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DevIdPlus {
    pub date_str: String,
    #[serde(rename = "LogFileURL")]
    pub log_file_url: String,
    pub more_info: MoreInfo,
    pub request_status: u64,
    #[serde(rename = "RequestUUID")]
    pub request_uuid: String,
    pub status_code: u64,
    pub status_message: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MoreInfo {
    pub hash: String,
}

/// A client for App Store Connect API.
///
/// The client isn't generic. Don't get any ideas.
pub struct AppStoreConnectClient {
    client: Client,
    connect_token: ConnectToken,
    token: Mutex<Option<String>>,
}

impl AppStoreConnectClient {
    pub fn new(connect_token: ConnectToken) -> Result<Self, AppleCodesignError> {
        Ok(Self {
            client: crate::ticket_lookup::default_client()?,
            connect_token,
            token: Mutex::new(None),
        })
    }

    /// Perform a `developerIDPlusInfoForPackageWithArguments` RPC request.
    ///
    /// This looks up information for a package submission having a UUID.
    ///
    /// Essentially, this looks up the status of a transporter upload / notarization
    /// request.
    pub fn developer_id_plus_info_for_package_with_arguments(
        &self,
        request_uuid: &str,
    ) -> Result<DevIdPlusInfoResponse, AppleCodesignError> {
        let token = {
            let mut token = self.token.lock().unwrap();

            if token.is_none() {
                token.replace(self.connect_token.new_token(300)?);
            }

            token.as_ref().unwrap().clone()
        };

        let params = DevIdPlusInfoRequest {
            // Only the request UUID seems to matter?
            application: "apple-codesign".into(),
            application_bundle_id: "com.gregoryszorc.rcs".into(),
            ds_plist: "".to_string(),
            request_uuid: request_uuid.to_string(),
        };

        let body = JsonRpcRequest {
            id: uuid::Uuid::new_v4().to_string(),
            json_rpc: "2.0".into(),
            method: "developerIDPlusInfoForPackageWithArguments".into(),
            params: serde_json::to_value(params)?,
        };

        let req = self
            .client
            .post(ITUNES_PRODUCER_SERVICE_URL)
            .bearer_auth(token)
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .json(&body);

        let response = req.send()?;

        let rpc_response = response.json::<JsonRpcResponse>()?;

        let dev_id_response = serde_json::from_value::<DevIdPlusInfoResponse>(rpc_response.result)?;

        Ok(dev_id_response)
    }
}
