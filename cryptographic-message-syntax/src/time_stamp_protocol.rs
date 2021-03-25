// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Time-Stamp Protocol (TSP) / RFC 3161 client.

use {
    crate::{
        algorithm::DigestAlgorithm,
        asn1::{
            rfc3161::{
                MessageImprint, PkiStatus, TimeStampReq, TimeStampResp, TstInfo,
                OID_CONTENT_TYPE_TST_INFO,
            },
            rfc5652::{SignedData, OID_ID_SIGNED_DATA},
        },
    },
    bcder::{
        decode::{Constructed, Malformed},
        encode::Values,
        Integer, OctetString,
    },
    reqwest::IntoUrl,
    ring::rand::SecureRandom,
    std::ops::Deref,
};

pub const HTTP_CONTENT_TYPE_REQUEST: &str = "application/timestamp-query";

pub const HTTP_CONTENT_TYPE_RESPONSE: &str = "application/timestamp-reply";

#[derive(Debug)]
pub enum TimeStampError {
    Io(std::io::Error),
    Reqwest(reqwest::Error),
    Asn1Decode(bcder::decode::Error),
    Http(&'static str),
    Random,
    NonceMismatch,
    Unsuccessful(TimeStampResp),
    BadResponse,
}

impl std::fmt::Display for TimeStampError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => f.write_fmt(format_args!("I/O error: {}", e)),
            Self::Reqwest(e) => f.write_fmt(format_args!("HTTP error: {}", e)),
            Self::Asn1Decode(e) => f.write_fmt(format_args!("ASN.1 decode error: {}", e)),
            Self::Http(msg) => f.write_str(msg),
            Self::Random => f.write_str("error generating random nonce"),
            Self::NonceMismatch => f.write_str("nonce mismatch"),
            Self::Unsuccessful(r) => f.write_fmt(format_args!(
                "unsuccessful Time-Stamp Protocol response: {:?}: {:?}",
                r.status.status, r.status.status_string
            )),
            Self::BadResponse => f.write_str("bad server response"),
        }
    }
}

impl std::error::Error for TimeStampError {}

impl From<std::io::Error> for TimeStampError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<reqwest::Error> for TimeStampError {
    fn from(e: reqwest::Error) -> Self {
        Self::Reqwest(e)
    }
}

impl From<bcder::decode::Error> for TimeStampError {
    fn from(e: bcder::decode::Error) -> Self {
        Self::Asn1Decode(e)
    }
}

/// High-level interface to [TimeStampResp].
///
/// This type provides a high-level interface to the low-level ASN.1 response
/// type from a Time-Stamp Protocol request.
pub struct TimeStampResponse(TimeStampResp);

impl Deref for TimeStampResponse {
    type Target = TimeStampResp;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl TimeStampResponse {
    /// Whether the time stamp request was successful.
    pub fn is_success(&self) -> bool {
        matches!(
            self.0.status.status,
            PkiStatus::Granted | PkiStatus::GrantedWithMods
        )
    }

    /// Decode the `SignedData` value in the response.
    pub fn signed_data(&self) -> Result<Option<SignedData>, bcder::decode::Error> {
        if let Some(token) = &self.0.time_stamp_token {
            if token.content_type == OID_ID_SIGNED_DATA {
                Ok(Some(
                    token
                        .content
                        .clone()
                        .decode(|cons| SignedData::take_from(cons))?,
                ))
            } else {
                Err(Malformed)
            }
        } else {
            Ok(None)
        }
    }

    pub fn tst_info(&self) -> Result<Option<TstInfo>, bcder::decode::Error> {
        if let Some(signed_data) = self.signed_data()? {
            if signed_data.content_info.content_type == OID_CONTENT_TYPE_TST_INFO {
                if let Some(content) = signed_data.content_info.content {
                    Ok(Some(Constructed::decode(
                        content.to_bytes(),
                        bcder::Mode::Der,
                        |cons| TstInfo::take_from(cons),
                    )?))
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
}

/// Send a [TimeStampReq] to a server via HTTP.
pub fn time_stamp_request_http(
    url: impl IntoUrl,
    request: &TimeStampReq,
) -> Result<TimeStampResponse, TimeStampError> {
    let client = reqwest::blocking::Client::new();

    let mut body = Vec::<u8>::new();
    request
        .encode_ref()
        .write_encoded(bcder::Mode::Der, &mut body)?;

    let response = client
        .post(url)
        .header("Content-Type", HTTP_CONTENT_TYPE_REQUEST)
        .body(body)
        .send()?;

    if response.status().is_success()
        && response.headers().get("Content-Type")
            == Some(&reqwest::header::HeaderValue::from_static(
                HTTP_CONTENT_TYPE_RESPONSE,
            ))
    {
        let res = TimeStampResponse(Constructed::decode(
            response.bytes()?,
            bcder::Mode::Der,
            |cons| TimeStampResp::take_from(cons),
        )?);

        // Verify nonce was reflected, if present.
        if res.is_success() {
            if let Some(tst_info) = res.tst_info()? {
                if tst_info.nonce != request.nonce {
                    return Err(TimeStampError::NonceMismatch);
                }
            }
        }

        Ok(res)
    } else {
        Err(TimeStampError::Http("bad HTTP response"))
    }
}

/// Send a Time-Stamp request for a given message to an HTTP URL.
///
/// This is a wrapper around [time_stamp_request_http] that constructs the low-level
/// ASN.1 request object with reasonable defaults.
pub fn time_stamp_message_http(
    url: impl IntoUrl,
    message: &[u8],
    digest_algorithm: DigestAlgorithm,
) -> Result<TimeStampResponse, TimeStampError> {
    let mut h = digest_algorithm.as_hasher();
    h.update(message);
    let digest = h.finish();

    let mut random = [0u8; 8];
    ring::rand::SystemRandom::new()
        .fill(&mut random)
        .map_err(|_| TimeStampError::Random)?;

    let request = TimeStampReq {
        version: Integer::from(1),
        message_imprint: MessageImprint {
            hash_algorithm: digest_algorithm.into(),
            hashed_message: OctetString::new(bytes::Bytes::copy_from_slice(digest.as_ref())),
        },
        req_policy: None,
        nonce: Some(Integer::from(u64::from_le_bytes(random))),
        cert_req: Some(true),
        extensions: None,
    };

    time_stamp_request_http(url, &request)
}

#[cfg(test)]
mod test {
    use super::*;

    const APPLE_TIMESTAMP_URL: &str = "http://timestamp.apple.com/ts01";

    #[test]
    fn simple_request() {
        let message = b"hello, world";

        let res =
            time_stamp_message_http(APPLE_TIMESTAMP_URL, message, DigestAlgorithm::Sha256).unwrap();

        let signed_data = res.signed_data().unwrap().unwrap();
        assert_eq!(
            signed_data.content_info.content_type,
            OID_CONTENT_TYPE_TST_INFO
        );
        let tst_info = res.tst_info().unwrap().unwrap();
        assert_eq!(tst_info.version, Integer::from(1));
    }
}
