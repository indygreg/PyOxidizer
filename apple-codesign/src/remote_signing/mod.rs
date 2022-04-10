// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Remote signing support.

pub mod session_negotiation;

use {
    crate::{
        cryptography::PrivateKey,
        remote_signing::session_negotiation::{
            PeerKeys, PublicKeyPeerDecrypt, SessionInitiatePeer, SessionJoinContext,
            SessionJoinPeerPreJoin,
        },
        AppleCodesignError,
    },
    bcder::{
        encode::{PrimitiveContent, Values},
        Mode, Oid,
    },
    bytes::Bytes,
    log::{debug, error, info, warn},
    serde::{de::DeserializeOwned, Deserialize, Serialize},
    signature::Signer,
    std::{
        cell::{RefCell, RefMut},
        net::TcpStream,
    },
    thiserror::Error,
    tungstenite::{
        client::IntoClientRequest,
        protocol::{Message, WebSocket, WebSocketConfig},
        stream::MaybeTlsStream,
    },
    x509_certificate::{
        CapturedX509Certificate, KeyAlgorithm, KeyInfoSigner, Sign, Signature, SignatureAlgorithm,
        X509CertificateError,
    },
};

/// URL of default server to use.
pub const DEFAULT_SERVER_URL: &str = "wss://ws.codesign.gregoryszorc.com/";

/// An error specific to remote signing.
#[derive(Debug, Error)]
pub enum RemoteSignError {
    #[error("unexpected message received from relay server: {0}")]
    ServerUnexpectedMessage(String),

    #[error("error reported from relay server: {0}")]
    ServerError(String),

    #[error("not compatible with relay server; try upgrading to a new release?")]
    ServerIncompatible,

    #[error("cryptography error: {0}")]
    Crypto(String),

    #[error("bad client state: {0}")]
    ClientState(&'static str),

    #[error("joining state not wanted for this session type: {0}")]
    SessionJoinUnwantedState(String),

    #[error("session join string error: {0}")]
    SessionJoinString(String),

    #[error("base64 decode error: {0}")]
    Base64(#[from] base64::DecodeError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("PEM encoding error: {0}")]
    Pem(#[from] pem::PemError),

    #[error("JSON serialization error: {0}")]
    SerdeJson(#[from] serde_json::Error),

    #[error("SPAKE error: {0}")]
    Spake(spake2::Error),

    #[error("SPKI error: {0}")]
    Spki(#[from] spki::Error),

    #[error("websocket error: {0}")]
    Websocket(#[from] tungstenite::Error),

    #[error("X.509 certificate handler error: {0}")]
    X509(#[from] X509CertificateError),
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum ApiMethod {
    Hello,
    CreateSession,
    JoinSession,
    SendMessage,
    Goodbye,
}

/// A websocket message sent from the client to the server.
#[derive(Clone, Debug, Serialize)]
struct ClientMessage {
    /// Unique ID for this request.
    request_id: String,
    /// API method being called.
    api: ApiMethod,
    /// Payload for this method.
    payload: Option<ClientPayload>,
}

/// Payload for a [ClientMessage].
#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
enum ClientPayload {
    CreateSession {
        session_id: String,
        ttl: u64,
        context: Option<String>,
    },
    JoinSession {
        session_id: String,
        context: Option<String>,
    },
    SendMessage {
        session_id: String,
        message: String,
    },
    Goodbye {
        session_id: String,
        reason: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
enum ServerMessageType {
    Error,
    Greeting,
    SessionCreated,
    SessionJoined,
    MessageSent,
    PeerMessage,
    SessionClosed,
}

/// Websocket message sent from server to client.
#[derive(Clone, Debug, Deserialize)]
struct ServerMessage {
    /// ID of request responsible for this message.
    request_id: Option<String>,
    /// The type of message.
    #[serde(rename = "type")]
    typ: ServerMessageType,
    ttl: Option<u64>,
    payload: Option<serde_json::Value>,
}

impl ServerMessage {
    fn into_result(self) -> Result<Self, RemoteSignError> {
        if self.typ == ServerMessageType::Error {
            let error = self.as_error()?;
            Err(RemoteSignError::ServerError(format!(
                "{}: {}",
                error.code, error.message
            )))
        } else {
            Ok(self)
        }
    }

    fn as_type<T: DeserializeOwned>(
        &self,
        message_type: ServerMessageType,
    ) -> Result<T, RemoteSignError> {
        if self.typ == message_type {
            if let Some(value) = &self.payload {
                Ok(serde_json::from_value(value.clone())?)
            } else {
                Err(RemoteSignError::ClientState(
                    "no payload for requested type",
                ))
            }
        } else {
            Err(RemoteSignError::ClientState(
                "requested payload for wrong message type",
            ))
        }
    }

    fn as_error(&self) -> Result<ServerError, RemoteSignError> {
        self.as_type::<ServerError>(ServerMessageType::Error)
    }

    fn as_greeting(&self) -> Result<ServerGreeting, RemoteSignError> {
        self.as_type::<ServerGreeting>(ServerMessageType::Greeting)
    }

    fn as_session_joined(&self) -> Result<ServerJoined, RemoteSignError> {
        self.as_type::<ServerJoined>(ServerMessageType::SessionJoined)
    }

    fn as_peer_message(&self) -> Result<ServerPeerMessage, RemoteSignError> {
        self.as_type::<ServerPeerMessage>(ServerMessageType::PeerMessage)
    }

    fn as_session_closed(&self) -> Result<ServerSessionClosed, RemoteSignError> {
        self.as_type::<ServerSessionClosed>(ServerMessageType::SessionClosed)
    }
}

/// Response messages seen from server.
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
enum ServerPayload {
    Error(ServerError),
    Greeting(ServerGreeting),
    SessionJoined(ServerJoined),
    PeerMessage(ServerPeerMessage),
    SessionClosed(ServerSessionClosed),
}

#[derive(Clone, Debug, Deserialize)]
struct ServerError {
    code: String,
    message: String,
}

#[derive(Clone, Debug, Deserialize)]
struct ServerGreeting {
    apis: Vec<String>,
    motd: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct ServerJoined {
    context: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct ServerPeerMessage {
    message: String,
}

#[derive(Clone, Debug, Deserialize)]
struct ServerSessionClosed {
    reason: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum PeerMessageType {
    Ping,
    Pong,
    RequestSigningCertificate,
    SigningCertificate,
    SignRequest,
    Signature,
}

/// A peer-to-peer message.
#[derive(Clone, Debug, Deserialize, Serialize)]
struct PeerMessage {
    #[serde(rename = "type")]
    typ: PeerMessageType,
    payload: Option<serde_json::Value>,
}

impl PeerMessage {
    fn require_type(self, typ: PeerMessageType) -> Result<Self, RemoteSignError> {
        if self.typ == typ {
            Ok(self)
        } else {
            Err(RemoteSignError::ServerUnexpectedMessage(format!(
                "{:?}",
                self.typ
            )))
        }
    }

    fn as_type<T: DeserializeOwned>(
        &self,
        message_type: PeerMessageType,
    ) -> Result<T, RemoteSignError> {
        if self.typ == message_type {
            if let Some(value) = &self.payload {
                Ok(serde_json::from_value(value.clone())?)
            } else {
                Err(RemoteSignError::ClientState(
                    "no payload for requested type",
                ))
            }
        } else {
            Err(RemoteSignError::ClientState(
                "requested payload for wrong message type",
            ))
        }
    }

    fn as_signing_certificate(&self) -> Result<PeerSigningCertificate, RemoteSignError> {
        self.as_type::<PeerSigningCertificate>(PeerMessageType::SigningCertificate)
    }

    fn as_sign_request(&self) -> Result<PeerSignRequest, RemoteSignError> {
        self.as_type::<PeerSignRequest>(PeerMessageType::SignRequest)
    }

    fn as_signature(&self) -> Result<PeerSignature, RemoteSignError> {
        self.as_type::<PeerSignature>(PeerMessageType::Signature)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct PeerCertificate {
    certificate: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    chain: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
enum PeerPayload {
    SigningCertificate(PeerSigningCertificate),
    SignRequest(PeerSignRequest),
    Signature(PeerSignature),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct PeerSigningCertificate {
    certificates: Vec<PeerCertificate>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct PeerSignRequest {
    message: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct PeerSignature {
    message: String,
    signature: String,
    algorithm_oid: String,
}

const REQUIRED_ACTIONS: [&str; 4] = ["create-session", "join-session", "send-message", "goodbye"];

/// Represents the response from the server.
enum ServerResponse {
    /// Server closed the connection.
    Closed,

    /// A parsed protocol message.
    Message(ServerMessage),
}

/// A function that receives session information.
pub type SessionInfoCallback = fn(session_join_string: &str) -> Result<(), RemoteSignError>;

fn create_websocket(
    req: impl IntoClientRequest,
) -> Result<WebSocket<MaybeTlsStream<TcpStream>>, RemoteSignError> {
    let config = WebSocketConfig {
        max_send_queue: Some(1),
        ..Default::default()
    };

    let req = req.into_client_request()?;
    warn!("connecting to {}", req.uri());

    let (ws, _) = tungstenite::client::connect_with_config(req, Some(config), 5)?;

    Ok(ws)
}

fn wait_for_server_response(
    ws: &mut WebSocket<MaybeTlsStream<TcpStream>>,
) -> Result<ServerResponse, RemoteSignError> {
    loop {
        match ws.read_message()? {
            Message::Text(text) => {
                let message = serde_json::from_str::<ServerMessage>(&text)?;
                debug!(
                    "received message; request-id: {}; type: {:?}",
                    message
                        .request_id
                        .as_ref()
                        .unwrap_or(&"(not set)".to_string()),
                    message.typ
                );

                return Ok(ServerResponse::Message(message));
            }
            Message::Binary(_) => {
                return Err(RemoteSignError::ServerUnexpectedMessage(
                    "binary websocket message".into(),
                ))
            }
            // TODO return error for these?
            Message::Pong(_) => {}
            Message::Ping(_) => {}
            Message::Frame(_) => {}
            Message::Close(_) => {
                return Ok(ServerResponse::Closed);
            }
        }
    }
}

fn wait_for_server_message(
    ws: &mut WebSocket<MaybeTlsStream<TcpStream>>,
) -> Result<ServerMessage, RemoteSignError> {
    match wait_for_server_response(ws)? {
        ServerResponse::Closed => Err(RemoteSignError::ClientState("server closed connection")),
        ServerResponse::Message(m) => {
            debug!(
                "received server message {:?}; remaining session TTL: {}",
                m.typ,
                m.ttl.unwrap_or_default()
            );
            Ok(m)
        }
    }
}

fn wait_for_expected_server_message(
    ws: &mut WebSocket<MaybeTlsStream<TcpStream>>,
    message_type: ServerMessageType,
) -> Result<ServerMessage, RemoteSignError> {
    let res = wait_for_server_message(ws)?.into_result()?;

    if res.typ == message_type {
        Ok(res)
    } else {
        Err(RemoteSignError::ServerUnexpectedMessage(format!(
            "{:?}",
            res.typ
        )))
    }
}

/// A client for the remote signing protocol that has not yet joined a session.
///
/// Clients can perform both the initiator and signer roles.
pub struct UnjoinedSigningClient {
    ws: WebSocket<MaybeTlsStream<TcpStream>>,
}

impl UnjoinedSigningClient {
    fn new(req: impl IntoClientRequest) -> Result<Self, RemoteSignError> {
        let ws = create_websocket(req)?;

        let mut slf = Self { ws };

        slf.send_hello()?;

        Ok(slf)
    }

    /// Create a new client in the initiator role.
    pub fn new_initiator(
        req: impl IntoClientRequest,
        initiator: Box<dyn SessionInitiatePeer>,
        session_info_cb: Option<SessionInfoCallback>,
    ) -> Result<InitiatorClient, RemoteSignError> {
        let slf = Self::new(req)?;
        slf.create_session_and_wait_for_signer(initiator, session_info_cb)
    }

    /// Create a new client in the signer role.
    pub fn new_signer(
        joiner: Box<dyn SessionJoinPeerPreJoin>,
        signing_key: &dyn KeyInfoSigner,
        signing_cert: CapturedX509Certificate,
        certificates: Vec<CapturedX509Certificate>,
        default_server_url: String,
    ) -> Result<SigningClient, RemoteSignError> {
        // An error here could result in the peer hanging indefinitely because the session
        // is unjoined. Ideally we'd recover from this by attempting to join with an error.
        // However, we may not even be able to obtain the session ID since sometimes it is
        // encrypted and the error could be from a decryption failure! So for now, just let
        // the peer idle.
        let join_context = joiner.join_context()?;

        let server_url = join_context
            .server_url
            .as_ref()
            .unwrap_or(&default_server_url);

        let slf = Self::new(server_url)?;
        slf.join_session(join_context, signing_key, signing_cert, certificates)
    }

    /// Create a new signing session and wait for a signer to arrive.
    fn create_session_and_wait_for_signer(
        mut self,
        initiator: Box<dyn SessionInitiatePeer>,
        session_info_cb: Option<SessionInfoCallback>,
    ) -> Result<InitiatorClient, RemoteSignError> {
        let session_id = initiator.session_id().to_string();

        self.send_request(
            ApiMethod::CreateSession,
            Some(ClientPayload::CreateSession {
                session_id: session_id.clone(),
                ttl: 600,
                context: initiator.session_create_context().map(base64::encode),
            }),
        )?;

        let sjs = initiator.session_join_string()?;

        wait_for_expected_server_message(&mut self.ws, ServerMessageType::SessionCreated)?;
        warn!("session successfully created on server");

        if let Some(cb) = session_info_cb {
            cb(&sjs)?;
        }

        let res = wait_for_expected_server_message(&mut self.ws, ServerMessageType::SessionJoined)?;

        let joined = res.as_session_joined()?;
        warn!("signer joined session; deriving shared encryption key");

        let context = if let Some(context) = joined.context {
            Some(base64::decode(context)?)
        } else {
            None
        };

        let keys = initiator.negotiate_session(context)?;

        let mut client = PairedClient {
            ws: self.ws,
            session_id,
            keys,
        };

        client.send_ping()?;

        let (signing_cert, signing_chain) = client.request_signing_certificate()?;

        if let Some(name) = signing_cert.subject_common_name() {
            warn!("remote signer will sign with certificate: {}", name);
        }

        Ok(InitiatorClient {
            client: RefCell::new(client),
            signing_cert,
            signing_chain,
        })
    }

    /// Join a signing session.
    ///
    /// This should be called by signers once they have the session ID to join.
    pub fn join_session(
        mut self,
        join_context: SessionJoinContext,
        signing_key: &dyn KeyInfoSigner,
        signing_cert: CapturedX509Certificate,
        certificates: Vec<CapturedX509Certificate>,
    ) -> Result<SigningClient, RemoteSignError> {
        let session_id = join_context.session_id.clone();

        warn!("joining session...");
        self.send_request(
            ApiMethod::JoinSession,
            Some(ClientPayload::JoinSession {
                session_id: session_id.clone(),
                context: join_context.peer_context.map(base64::encode),
            }),
        )?;

        wait_for_expected_server_message(&mut self.ws, ServerMessageType::SessionJoined)?;

        warn!("successfully joined signing session {}", session_id);

        let keys = join_context.peer_handshake.negotiate_session()?;

        let mut client = PairedClient {
            ws: self.ws,
            session_id,
            keys,
        };

        warn!("verifying encrypted communications with peer");
        client.send_ping()?;

        Ok(SigningClient {
            client: RefCell::new(client),
            signing_key,
            signing_cert,
            certificates,
        })
    }

    fn send_request(
        &mut self,
        api: ApiMethod,
        payload: Option<ClientPayload>,
    ) -> Result<(), RemoteSignError> {
        let request_id = uuid::Uuid::new_v4().to_string();

        let message = ClientMessage {
            request_id,
            api,
            payload,
        };

        let body = serde_json::to_string(&message)?;
        self.ws.write_message(body.into())?;

        Ok(())
    }

    fn send_hello(&mut self) -> Result<(), RemoteSignError> {
        self.send_request(ApiMethod::Hello, None)?;

        let res = wait_for_expected_server_message(&mut self.ws, ServerMessageType::Greeting)?;
        let greeting = res.as_greeting()?;

        if let Some(motd) = &greeting.motd {
            warn!("message from remote server: {}", motd);
        }

        for required in REQUIRED_ACTIONS {
            if !greeting.apis.contains(&required.to_string()) {
                error!("server does not support required action {}", required);
                return Err(RemoteSignError::ServerIncompatible);
            }
        }

        Ok(())
    }
}

/// A remote signing client that has joined a session and is ready to exchange messages.
pub struct PairedClient {
    ws: WebSocket<MaybeTlsStream<TcpStream>>,
    session_id: String,
    keys: PeerKeys,
}

impl Drop for PairedClient {
    fn drop(&mut self) {
        warn!("disconnecting from relay server");
    }
}

impl PairedClient {
    fn send_request(
        &mut self,
        api: ApiMethod,
        payload: Option<ClientPayload>,
    ) -> Result<(), RemoteSignError> {
        let request_id = uuid::Uuid::new_v4().to_string();

        let message = ClientMessage {
            request_id,
            api,
            payload,
        };

        let body = serde_json::to_string(&message)?;
        self.ws.write_message(body.into())?;

        Ok(())
    }

    fn decrypt_peer_message(
        &mut self,
        message: &ServerPeerMessage,
    ) -> Result<PeerMessage, RemoteSignError> {
        let ciphertext = base64::decode(&message.message)?;

        let plaintext = self.keys.open(ciphertext)?;

        Ok(serde_json::from_slice(&plaintext)?)
    }

    fn send_encrypted_message(
        &mut self,
        message_type: PeerMessageType,
        payload: Option<PeerPayload>,
    ) -> Result<(), RemoteSignError> {
        let message = PeerMessage {
            typ: message_type,
            payload: if let Some(payload) = payload {
                Some(serde_json::to_value(payload)?)
            } else {
                None
            },
        };

        let ciphertext = self.keys.seal(&serde_json::to_vec(&message)?)?;

        self.send_request(
            ApiMethod::SendMessage,
            Some(ClientPayload::SendMessage {
                session_id: self.session_id.clone(),
                message: base64::encode(ciphertext),
            }),
        )?;

        Ok(())
    }

    fn wait_for_peer_message(&mut self) -> Result<Option<PeerMessage>, RemoteSignError> {
        let res = wait_for_server_message(&mut self.ws)?.into_result()?;

        if let Ok(closed) = res.as_session_closed() {
            warn!(
                "signing session closed; reason: {}",
                closed
                    .reason
                    .as_ref()
                    .unwrap_or(&"(none given)".to_string())
            );
            Ok(None)
        } else {
            let message = res.as_peer_message()?;

            Ok(Some(self.decrypt_peer_message(&message)?))
        }
    }

    fn wait_for_server_and_peer_response(&mut self) -> Result<PeerMessage, RemoteSignError> {
        let mut response = None;

        // We should get a server message acknowledging our request plus the response from
        // the peer. The order they arrive in is random.
        for _ in 0..2 {
            let res = wait_for_server_message(&mut self.ws)?.into_result()?;

            match res.typ {
                ServerMessageType::MessageSent => {}
                ServerMessageType::PeerMessage => {
                    let message = res.as_peer_message()?;

                    response = Some(self.decrypt_peer_message(&message)?);
                }
                m => return Err(RemoteSignError::ServerUnexpectedMessage(format!("{:?}", m))),
            }
        }

        if let Some(response) = response {
            Ok(response)
        } else {
            Err(RemoteSignError::ClientState(
                "failed to receive response from server or peer",
            ))
        }
    }

    fn send_goodbye(&mut self, reason: Option<String>) -> Result<(), RemoteSignError> {
        warn!("terminating signing session on relay");
        self.send_request(
            ApiMethod::Goodbye,
            Some(ClientPayload::Goodbye {
                session_id: self.session_id.clone(),
                reason,
            }),
        )?;

        wait_for_server_message(&mut self.ws)?.into_result()?;
        info!("relay server confirmed session termination");

        Ok(())
    }

    fn send_ping(&mut self) -> Result<(), RemoteSignError> {
        // We should get a server message acknowledging our request plus a
        // ping from the peer. The order may not be reliable.
        self.send_encrypted_message(PeerMessageType::Ping, None)?;
        let message = self.wait_for_server_and_peer_response()?;
        if !matches!(message.typ, PeerMessageType::Ping) {
            return Err(RemoteSignError::ServerUnexpectedMessage(
                "unexpected response to ping message".into(),
            ));
        }

        self.send_encrypted_message(PeerMessageType::Pong, None)?;
        let message = self.wait_for_server_and_peer_response()?;
        if !matches!(message.typ, PeerMessageType::Pong) {
            return Err(RemoteSignError::ServerUnexpectedMessage(
                "unexpected response to ping message".into(),
            ));
        }

        Ok(())
    }

    /// Request the signing certificate from the peer.
    pub fn request_signing_certificate(
        &mut self,
    ) -> Result<(CapturedX509Certificate, Vec<CapturedX509Certificate>), RemoteSignError> {
        warn!("requesting signing certificate info from signer");
        self.send_encrypted_message(PeerMessageType::RequestSigningCertificate, None)?;
        let res = self
            .wait_for_server_and_peer_response()?
            .require_type(PeerMessageType::SigningCertificate)?;

        let cert = res.as_signing_certificate()?;

        if let Some(cert) = cert.certificates.get(0) {
            let cert_der = base64::decode(&cert.certificate)?;
            let chain_der = cert
                .chain
                .iter()
                .map(base64::decode)
                .collect::<Result<Vec<_>, base64::DecodeError>>()?;

            let cert = CapturedX509Certificate::from_der(cert_der)?;
            let chain = chain_der
                .into_iter()
                .map(CapturedX509Certificate::from_der)
                .collect::<Result<Vec<_>, X509CertificateError>>()?;

            return Ok((cert, chain));
        }

        Err(RemoteSignError::ClientState(
            "did not receive any signing certificates from peer",
        ))
    }
}

/// A client fulfilling the role of the initiator.
pub struct InitiatorClient {
    client: RefCell<PairedClient>,
    signing_cert: CapturedX509Certificate,
    signing_chain: Vec<CapturedX509Certificate>,
}

impl InitiatorClient {
    /// The X.509 certificate that will be used to sign.
    pub fn signing_certificate(&self) -> &CapturedX509Certificate {
        &self.signing_cert
    }

    /// Additional X.509 certificates in the signing chain.
    pub fn certificate_chain(&self) -> &[CapturedX509Certificate] {
        &self.signing_chain
    }
}

impl Signer<Signature> for InitiatorClient {
    fn try_sign(&self, message: &[u8]) -> Result<Signature, signature::Error> {
        let mut client = self.client.borrow_mut();

        warn!("sending signing request to remote signer");

        client
            .send_encrypted_message(
                PeerMessageType::SignRequest,
                Some(PeerPayload::SignRequest(PeerSignRequest {
                    message: base64::encode(message),
                })),
            )
            .map_err(signature::Error::from_source)?;

        let response = client
            .wait_for_server_and_peer_response()
            .map_err(signature::Error::from_source)?
            .require_type(PeerMessageType::Signature)
            .map_err(signature::Error::from_source)?;

        let peer_signature = response
            .as_signature()
            .map_err(signature::Error::from_source)?;

        warn!("received signature from remote signer");

        let signature =
            base64::decode(&peer_signature.signature).map_err(signature::Error::from_source)?;
        let oid_der =
            base64::decode(&peer_signature.algorithm_oid).map_err(signature::Error::from_source)?;

        bcder::decode::Constructed::decode(oid_der.as_ref(), Mode::Der, |cons| {
            Oid::take_from(cons)
        })
        .map_err(|_| {
            signature::Error::from_source(RemoteSignError::Crypto(
                "error parsing signature OID".into(),
            ))
        })?;

        // The peer could be acting maliciously (or just be buggy) and sign with a
        // certificate from the initial one presented. So verify the signature we
        // received is valid for the message we sent.
        if let Err(e) = self.signing_cert.verify_signed_data(message, &signature) {
            error!("Peer issued signature did not verify against the certificate they provided");
            error!("The peer could be acting maliciously. Or it could just be buggy.");
            error!("Either way, it didn't issue a valid signature, so we're giving up.");

            return Err(signature::Error::from_source(e));
        }

        Ok(signature.into())
    }
}

impl Sign for InitiatorClient {
    fn sign(&self, message: &[u8]) -> Result<(Vec<u8>, SignatureAlgorithm), X509CertificateError> {
        let algorithm = self.signature_algorithm()?;

        Ok((self.try_sign(message)?.into(), algorithm))
    }

    fn key_algorithm(&self) -> Option<KeyAlgorithm> {
        self.signing_cert.key_algorithm()
    }

    fn public_key_data(&self) -> Bytes {
        self.signing_cert.public_key_data()
    }

    fn signature_algorithm(&self) -> Result<SignatureAlgorithm, X509CertificateError> {
        if let Some(algorithm) = self.signing_cert.signature_algorithm() {
            Ok(algorithm)
        } else {
            Err(X509CertificateError::UnknownSignatureAlgorithm(format!(
                "{}",
                self.signing_cert.signature_algorithm_oid()
            )))
        }
    }

    fn private_key_data(&self) -> Option<Vec<u8>> {
        // We never have access to private keys from the remote signer.
        None
    }

    fn rsa_primes(&self) -> Result<Option<(Vec<u8>, Vec<u8>)>, X509CertificateError> {
        // We never have access to private keys from the remote signer.
        Ok(None)
    }
}

impl KeyInfoSigner for InitiatorClient {}

impl PublicKeyPeerDecrypt for InitiatorClient {
    fn decrypt(&self, _ciphertext: &[u8]) -> Result<Vec<u8>, RemoteSignError> {
        Err(RemoteSignError::Crypto(
            "a remote signer cannot be used to perform signing".into(),
        ))
    }
}

impl PrivateKey for InitiatorClient {
    fn as_key_info_signer(&self) -> &dyn KeyInfoSigner {
        self
    }

    fn to_public_key_peer_decrypt(
        &self,
    ) -> Result<Box<dyn PublicKeyPeerDecrypt>, AppleCodesignError> {
        Err(
            RemoteSignError::ClientState("cannot use remote signing initiator for decryption")
                .into(),
        )
    }

    fn finish(&self) -> Result<(), AppleCodesignError> {
        // Tell the peer we're done so it disconnects
        Ok(self
            .client
            .borrow_mut()
            .send_goodbye(Some("signing operations completed".into()))?)
    }
}

pub struct SigningClient<'key> {
    client: RefCell<PairedClient>,
    signing_key: &'key dyn KeyInfoSigner,
    signing_cert: CapturedX509Certificate,
    certificates: Vec<CapturedX509Certificate>,
}

impl<'key> SigningClient<'key> {
    fn send_signing_certificate(
        &self,
        mut client: RefMut<PairedClient>,
    ) -> Result<(), RemoteSignError> {
        client.send_encrypted_message(
            PeerMessageType::SigningCertificate,
            Some(PeerPayload::SigningCertificate(PeerSigningCertificate {
                certificates: vec![PeerCertificate {
                    certificate: base64::encode(self.signing_cert.encode_der()?),
                    chain: self
                        .certificates
                        .iter()
                        .map(|cert| {
                            let der = cert.encode_der()?;

                            Ok(base64::encode(der))
                        })
                        .collect::<Result<Vec<_>, RemoteSignError>>()?,
                }],
            })),
        )?;

        wait_for_expected_server_message(&mut client.ws, ServerMessageType::MessageSent)?;

        Ok(())
    }

    fn handle_sign_request(
        &self,
        mut client: RefMut<PairedClient>,
        request: PeerSignRequest,
    ) -> Result<(), RemoteSignError> {
        let message = base64::decode(&request.message)?;

        warn!(
            "creating signature for remote message: {}",
            &request.message
        );
        let signature = self
            .signing_key
            .try_sign(&message)
            .map_err(|e| RemoteSignError::Crypto(format!("when creating signature: {}", e)))?;
        let algorithm = self.signing_key.signature_algorithm()?;

        let oid = Oid::from(algorithm);
        let mut oid_der = vec![];
        oid.encode().write_encoded(Mode::Der, &mut oid_der)?;

        warn!("sending signature to peer");
        client.send_encrypted_message(
            PeerMessageType::Signature,
            Some(PeerPayload::Signature(PeerSignature {
                message: base64::encode(message),
                signature: base64::encode(signature),
                algorithm_oid: base64::encode(oid_der),
            })),
        )?;

        wait_for_expected_server_message(&mut client.ws, ServerMessageType::MessageSent)?;
        info!("relay acknowledged signature message received");

        Ok(())
    }

    fn process_next_message(&self) -> Result<bool, RemoteSignError> {
        let mut client = self.client.borrow_mut();

        info!("waiting for server to send us a message...");
        let res = if let Some(res) = client.wait_for_peer_message()? {
            res
        } else {
            return Ok(false);
        };

        match res.typ {
            PeerMessageType::RequestSigningCertificate => {
                self.send_signing_certificate(client)?;
            }
            PeerMessageType::Ping => {
                client.send_encrypted_message(PeerMessageType::Pong, None)?;
                wait_for_expected_server_message(&mut client.ws, ServerMessageType::MessageSent)?;
            }
            PeerMessageType::Pong => {}
            PeerMessageType::SignRequest => {
                self.handle_sign_request(client, res.as_sign_request()?)?;
            }
            typ => {
                warn!("unprocessed message: {:?}", typ);
            }
        }

        Ok(true)
    }

    pub fn run(self) -> Result<(), RemoteSignError> {
        while self.process_next_message()? {}

        Ok(())
    }
}
