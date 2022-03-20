// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Support for retrieving notarization tickets and stapling artifacts. */

use {
    crate::AppleCodesignError,
    log::warn,
    reqwest::blocking::{Client, ClientBuilder},
    serde::{Deserialize, Serialize},
    std::collections::HashMap,
};

/// URL of HTTP service where Apple publishes stapling tickets.
pub const APPLE_TICKET_LOOKUP_URL: &str = "https://api.apple-cloudkit.com/database/1/com.apple.gk.ticket-delivery/production/public/records/lookup";

/// Main JSON request object for ticket lookup requests.
#[derive(Clone, Debug, Serialize)]
pub struct TicketLookupRequest {
    pub records: Vec<TicketLookupRequestRecord>,
}

/// Represents a single record to look up in a ticket lookup request.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TicketLookupRequestRecord {
    pub record_name: String,
}

/// Main JSON response object to ticket lookup requests.
#[derive(Clone, Debug, Deserialize)]
pub struct TicketLookupResponse {
    pub records: Vec<TicketLookupResponseRecord>,
}

impl TicketLookupResponse {
    /// Obtain the signed ticket for a given record name.
    ///
    /// `record_name` is of the form `2/<digest_type>/<digest>`. e.g.
    /// `2/2/deadbeefdeadbeef....`.
    ///
    /// Returns an `Err` if a signed ticket could not be found.
    pub fn signed_ticket(&self, record_name: &str) -> Result<Vec<u8>, AppleCodesignError> {
        let record = self
            .records
            .iter()
            .find(|r| r.record_name() == record_name)
            .ok_or_else(|| {
                AppleCodesignError::NotarizationRecordNotInResponse(record_name.to_string())
            })?;

        match record {
            TicketLookupResponseRecord::Success(r) => r
                .signed_ticket_data()
                .ok_or(AppleCodesignError::NotarizationRecordNoSignedTicket)?,
            TicketLookupResponseRecord::Failure(r) => {
                Err(AppleCodesignError::NotarizationLookupFailure(
                    r.server_error_code.clone(),
                    r.reason.clone(),
                ))
            }
        }
    }
}

/// Describes the results of a ticket lookup for a specific record.
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum TicketLookupResponseRecord {
    /// Ticket was found.
    Success(TicketLookupResponseRecordSuccess),

    /// Some error occurred.
    Failure(TicketLookupResponseRecordFailure),
}

impl TicketLookupResponseRecord {
    /// Obtain the record name associated with this record.
    pub fn record_name(&self) -> &str {
        match self {
            Self::Success(r) => &r.record_name,
            Self::Failure(r) => &r.record_name,
        }
    }
}

/// Represents a successful ticket lookup response record.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TicketLookupResponseRecordSuccess {
    /// Name of record that was looked up.
    pub record_name: String,

    pub created: TicketRecordEvent,
    pub deleted: bool,
    /// Holds data.
    ///
    /// The `signedTicket` key holds the ticket.
    pub fields: HashMap<String, Field>,
    pub modified: TicketRecordEvent,
    // TODO pluginFields
    pub record_change_tag: String,

    /// A value like `DeveloperIDTicket`.
    ///
    /// We could potentially turn this into an enumeration...
    pub record_type: String,
}

impl TicketLookupResponseRecordSuccess {
    /// Obtain the raw signed ticket data in this record.
    ///
    /// Evaluates to `Some` if there appears to be a signed ticket and `None`
    /// otherwise.
    ///
    /// There can be an inner `Err` if we don't know how to decode the response data
    /// or there was an error decoding.
    pub fn signed_ticket_data(&self) -> Option<Result<Vec<u8>, AppleCodesignError>> {
        match self.fields.get("signedTicket") {
            Some(field) => {
                if field.typ == "BYTES" {
                    Some(
                        base64::decode(&field.value)
                            .map_err(AppleCodesignError::NotarizationRecordDecodeFailure),
                    )
                } else {
                    Some(Err(
                        AppleCodesignError::NotarizationRecordSignedTicketNotBytes(
                            field.typ.clone(),
                        ),
                    ))
                }
            }
            None => None,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TicketLookupResponseRecordFailure {
    pub record_name: String,
    pub reason: String,
    pub server_error_code: String,
}

/// Represents an event in a ticket record.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TicketRecordEvent {
    #[serde(rename = "deviceID")]
    pub device_id: String,
    pub timestamp: u64,
    pub user_record_name: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Field {
    #[serde(rename = "type")]
    pub typ: String,
    pub value: String,
}

/// Obtain the default [Client] to use for HTTP requests.
pub fn default_client() -> Result<Client, AppleCodesignError> {
    Ok(ClientBuilder::default()
        .user_agent("apple-codesign crate (https://crates.io/crates/apple-codesign)")
        .build()?)
}

/// Look up a notarization ticket given an HTTP client and an iterable of record names.
///
/// The record name is of the form `2/<digest_type>/<code_directory_digest>`.
pub fn lookup_notarization_tickets<'a>(
    client: &Client,
    record_names: impl Iterator<Item = &'a str>,
) -> Result<TicketLookupResponse, AppleCodesignError> {
    let body = TicketLookupRequest {
        records: record_names
            .map(|x| {
                warn!("looking up notarization ticket for {}", x);
                TicketLookupRequestRecord {
                    record_name: x.to_string(),
                }
            })
            .collect::<Vec<_>>(),
    };

    let req = client
        .post(APPLE_TICKET_LOOKUP_URL)
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .json(&body);

    let response = req.send()?;

    let body = response.bytes()?;

    let response = serde_json::from_slice::<TicketLookupResponse>(&body)?;

    Ok(response)
}

/// Look up a single notarization ticket.
///
/// This is just a convenience wrapper around [lookup_notarization_tickets()].
pub fn lookup_notarization_ticket(
    client: &Client,
    record_name: &str,
) -> Result<TicketLookupResponse, AppleCodesignError> {
    lookup_notarization_tickets(client, std::iter::once(record_name))
}

#[cfg(test)]
mod test {
    use super::*;

    const PYOXIDIZER_APP_RECORD: &str = "2/2/1b747faf223750de74febed7929f14a73af8c933";
    const DEADBEEF: &str = "2/2/deadbeefdeadbeefdeadbeefdeadbeefdeadbeef";

    #[test]
    fn lookup_ticket() -> Result<(), AppleCodesignError> {
        let client = default_client()?;

        let res = lookup_notarization_ticket(&client, PYOXIDIZER_APP_RECORD)?;

        assert!(matches!(
            &res.records[0],
            TicketLookupResponseRecord::Success(_)
        ));

        let ticket = res.signed_ticket(PYOXIDIZER_APP_RECORD)?;
        assert_eq!(&ticket[0..4], b"s8ch");

        let res = lookup_notarization_ticket(&client, DEADBEEF)?;
        assert!(matches!(
            &res.records[0],
            TicketLookupResponseRecord::Failure(_)
        ));
        assert!(matches!(
            res.signed_ticket(DEADBEEF),
            Err(AppleCodesignError::NotarizationLookupFailure(_, _))
        ));

        Ok(())
    }
}
