// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Session establishment and crypto code for remote signing protocol.
//!
//! The intent of this module / file is to isolate the code with the highest
//! sensitivity for security matters.

use {
    crate::remote_signing::RemoteSignError,
    der::{Decode, Encode},
    minicbor::{encode::Write, Decode as CborDecode, Decoder, Encode as CborEncode, Encoder},
    oid_registry::OID_PKCS1_RSAENCRYPTION,
    pkcs1::RsaPublicKey as RsaPublicKeyAsn1,
    ring::{
        aead::{
            Aad, BoundKey, Nonce, NonceSequence, OpeningKey, SealingKey, UnboundKey, AES_128_GCM,
            CHACHA20_POLY1305, NONCE_LEN,
        },
        agreement::{agree_ephemeral, EphemeralPrivateKey, UnparsedPublicKey, X25519},
        hkdf::{Salt, HKDF_SHA256},
        rand::{SecureRandom, SystemRandom},
    },
    rsa::{BigUint, PaddingScheme, PublicKey, RsaPublicKey},
    scroll::{Pwrite, LE},
    spake2::{Ed25519Group, Identity, Password, Spake2},
    spki::SubjectPublicKeyInfo,
    std::fmt::{Display, Formatter},
};

type Result<T> = std::result::Result<T, RemoteSignError>;

/// A generator of nonces that is a simple incrementing counter.
///
/// Assumed use with ChaCha20+Poly1305.
#[derive(Default)]
struct RemoteSigningNonceSequence {
    id: u32,
}

impl NonceSequence for RemoteSigningNonceSequence {
    fn advance(&mut self) -> ::std::result::Result<Nonce, ring::error::Unspecified> {
        let mut data = [0u8; NONCE_LEN];
        data.pwrite_with(self.id, 0, LE)
            .map_err(|_| ring::error::Unspecified)?;

        self.id += 1;

        Ok(Nonce::assume_unique_for_key(data))
    }
}

/// A nonce sequence that emits a constant value exactly once.
#[derive(Default)]
struct ConstantNonceSequence {
    used: bool,
}

impl NonceSequence for ConstantNonceSequence {
    fn advance(&mut self) -> ::std::result::Result<Nonce, ring::error::Unspecified> {
        if self.used {
            return Err(ring::error::Unspecified);
        }

        self.used = true;

        Ok(Nonce::assume_unique_for_key([0x42; NONCE_LEN]))
    }
}

/// The role being assumed by a peer.
#[derive(Clone, Copy, Debug)]
pub enum Role {
    /// Peer who initiated the session.
    A,
    /// Peer who joined the session.
    B,
}

impl Display for Role {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::A => "A",
            Self::B => "B",
        })
    }
}

/// Derives the identifier / info value used for HKDF expansion.
fn derive_hkdf_info(role: Role, session_id: &str, extra_identifier: &[u8]) -> Vec<u8> {
    role.to_string()
        .as_bytes()
        .iter()
        .chain(std::iter::once(&b':'))
        .chain(session_id.as_bytes().iter())
        .chain(std::iter::once(&b':'))
        .chain(extra_identifier.iter())
        .copied()
        .collect::<Vec<_>>()
}

pub struct PeerKeys {
    sealing: SealingKey<RemoteSigningNonceSequence>,
    opening: OpeningKey<RemoteSigningNonceSequence>,
}

impl PeerKeys {
    /// Encrypt / seal a plaintext message using AEAD.
    ///
    /// Receives the plaintext message to encrypt.
    ///
    /// Returns the encrypted ciphertext.
    pub fn seal(&mut self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let mut output = plaintext.to_vec();
        self.sealing
            .seal_in_place_append_tag(Aad::empty(), &mut output)
            .map_err(|_| RemoteSignError::Crypto("AEAD sealing error".into()))?;

        Ok(output)
    }

    /// Decrypt / open a ciphertext using AEAD.
    ///
    /// Receives the ciphertext message to decrypt.
    ///
    /// Returns the decrypted and verified plaintext.
    pub fn open(&mut self, mut ciphertext: Vec<u8>) -> Result<Vec<u8>> {
        let plaintext = self
            .opening
            .open_in_place(Aad::empty(), &mut ciphertext)
            .map_err(|_| RemoteSignError::Crypto("failed to decrypt message".into()))?;

        Ok(plaintext.to_vec())
    }
}

/// Derives a pair of AEAD keys from a shared encryption key.
///
/// Returns a pair of keys. One key is used for sealing / encrypting and the
/// other for opening / decrypting.
///
/// `role` is the role that the current peer is playing. The session initiator
/// generally uses `A` and the joiner / signer uses `B`.
///
/// `shared_key` is a private key that is mutually derived and identical on both
/// peers. The mechanism for obtaining it varies.
///
/// `session_id` is the server-registered session identifier.
///
/// `extra_identifier` is an extra value to use when constructing identities for
/// HKDF extraction.
fn derive_aead_keys(
    role: Role,
    shared_key: Vec<u8>,
    session_id: &str,
    extra_identifier: &[u8],
) -> Result<(
    SealingKey<RemoteSigningNonceSequence>,
    OpeningKey<RemoteSigningNonceSequence>,
)> {
    let salt = Salt::new(HKDF_SHA256, &[]);
    let prk = salt.extract(&shared_key);

    let a_identifier = derive_hkdf_info(Role::A, session_id, extra_identifier);
    let b_identifier = derive_hkdf_info(Role::B, session_id, extra_identifier);

    let a_info = [a_identifier.as_ref()];
    let b_info = [b_identifier.as_ref()];

    let a_key = prk
        .expand(&a_info, &CHACHA20_POLY1305)
        .map_err(|_| RemoteSignError::Crypto("error performing HKDF key derivation".into()))?;

    let b_key = prk
        .expand(&b_info, &CHACHA20_POLY1305)
        .map_err(|_| RemoteSignError::Crypto("error performing HKDF key derivation".into()))?;

    let (sealing_key, opening_key) = match role {
        Role::A => (a_key, b_key),
        Role::B => (b_key, a_key),
    };

    let sealing_key = SealingKey::new(sealing_key.into(), RemoteSigningNonceSequence::default());
    let opening_key = OpeningKey::new(opening_key.into(), RemoteSigningNonceSequence::default());

    Ok((sealing_key, opening_key))
}

fn encode_sjs(
    scheme: &str,
    payload: impl CborEncode<()>,
) -> ::std::result::Result<Vec<u8>, minicbor::encode::Error<std::convert::Infallible>> {
    let mut encoder = Encoder::new(Vec::<u8>::new());

    {
        let encoder = encoder.array(2)?;
        encoder.str(scheme)?;
        payload.encode(encoder, &mut ())?;
        encoder.end()?;
    }

    Ok(encoder.into_writer())
}

/// Common behaviors for a session join string.
///
/// Implementations must also implement [Encode], which will emit the CBOR
/// encoding of the instance to an encoder.
pub trait SessionJoinString<'de>: CborDecode<'de, ()> + CborEncode<()> {
    /// The scheme / name for this SJS implementation.
    ///
    /// This is advertised as the first component in the encoded SJS.
    fn scheme() -> &'static str;

    /// Obtain the raw bytes constituting the session join string.
    fn to_bytes(&self) -> Result<Vec<u8>> {
        encode_sjs(Self::scheme(), &self)
            .map_err(|e| RemoteSignError::SessionJoinString(format!("CBOR encoding error: {}", e)))
    }
}

struct PublicKeySessionJoinString {
    aes_ciphertext: Vec<u8>,
    public_key: Vec<u8>,
    message_ciphertext: Vec<u8>,
}

impl<'de, C> CborDecode<'de, C> for PublicKeySessionJoinString {
    fn decode(
        d: &mut Decoder<'de>,
        _ctx: &mut C,
    ) -> std::result::Result<Self, minicbor::decode::Error> {
        if !matches!(d.array()?, Some(3)) {
            return Err(minicbor::decode::Error::message(
                "not an array of 3 elements",
            ));
        }

        let aes_ciphertext = d.bytes()?.to_vec();
        let public_key = d.bytes()?.to_vec();
        let message_ciphertext = d.bytes()?.to_vec();

        Ok(Self {
            aes_ciphertext,
            public_key,
            message_ciphertext,
        })
    }
}

impl<C> CborEncode<C> for PublicKeySessionJoinString {
    fn encode<W: Write>(
        &self,
        e: &mut Encoder<W>,
        _ctx: &mut C,
    ) -> ::std::result::Result<(), minicbor::encode::Error<W::Error>> {
        e.array(3)?;
        e.bytes(&self.aes_ciphertext)?;
        e.bytes(&self.public_key)?;
        e.bytes(&self.message_ciphertext)?;
        e.end()?;

        Ok(())
    }
}

impl SessionJoinString<'static> for PublicKeySessionJoinString {
    fn scheme() -> &'static str {
        "publickey0"
    }
}

struct SharedSecretSessionJoinString {
    session_id: String,
    extra_identifier: Vec<u8>,
    role_a_init_message: Vec<u8>,
}

impl<'de, C> CborDecode<'de, C> for SharedSecretSessionJoinString {
    fn decode(
        d: &mut Decoder<'de>,
        _ctx: &mut C,
    ) -> std::result::Result<Self, minicbor::decode::Error> {
        if !matches!(d.array()?, Some(3)) {
            return Err(minicbor::decode::Error::message(
                "not an array of 3 elements",
            ));
        }

        let session_id = d.str()?.to_string();
        let extra_identifier = d.bytes()?.to_vec();
        let role_a_init_message = d.bytes()?.to_vec();

        Ok(Self {
            session_id,
            extra_identifier,
            role_a_init_message,
        })
    }
}

impl<C> CborEncode<C> for SharedSecretSessionJoinString {
    fn encode<W: Write>(
        &self,
        e: &mut Encoder<W>,
        _ctx: &mut C,
    ) -> ::std::result::Result<(), minicbor::encode::Error<W::Error>> {
        e.array(3)?;
        e.str(&self.session_id)?;
        e.bytes(&self.extra_identifier)?;
        e.bytes(&self.role_a_init_message)?;
        e.end()?;

        Ok(())
    }
}

impl SessionJoinString<'static> for SharedSecretSessionJoinString {
    fn scheme() -> &'static str {
        "sharedsecret0"
    }
}

/// A peer that initiates a remote signing session.
pub trait SessionInitiatePeer {
    /// Obtain the session ID to create / use.
    fn session_id(&self) -> &str;

    /// Obtain additional session context to store with the server.
    ///
    /// This context will be sent to the peer when it joins.
    fn session_create_context(&self) -> Option<Vec<u8>>;

    /// Obtain the raw bytes constituting the session join string.
    fn session_join_string_bytes(&self) -> Result<Vec<u8>>;

    /// Obtain the base 64 encoded session join string.
    fn session_join_string_base64(&self) -> Result<String> {
        Ok(base64::encode_config(
            self.session_join_string_bytes()?,
            base64::URL_SAFE_NO_PAD,
        ))
    }

    /// Obtain the PEM encoded session join string.
    fn session_join_string_pem(&self) -> Result<String> {
        Ok(pem::encode(&pem::Pem {
            tag: "SESSION JOIN STRING".to_string(),
            contents: self.session_join_string_bytes()?,
        }))
    }

    /// Finalize a peer joined session using optional context provided by the peer.
    ///
    /// Yields encryption keys for this peer.
    fn negotiate_session(self: Box<Self>, peer_context: Option<Vec<u8>>) -> Result<PeerKeys>;
}

pub enum SessionJoinState {
    /// A generic shared secret value.
    SharedSecret(Vec<u8>),

    /// An entity capable of decrypting messages encrypted by the peer.
    PublicKeyDecrypt(Box<dyn PublicKeyPeerDecrypt>),
}

/// A peer that joins sessions in a state before it has spoken to the server.
pub trait SessionJoinPeerPreJoin {
    /// Register additional state with the peer.
    ///
    /// This is used as a generic way to import implementation-specific state that
    /// enables the peer join to complete.
    fn register_state(&mut self, state: SessionJoinState) -> Result<()>;

    /// Obtain information needed to join to a session.
    ///
    /// Consumes self because joining should be a one-time operation.
    fn join_context(self: Box<Self>) -> Result<SessionJoinContext>;
}

pub trait SessionJoinPeerHandshake {
    /// Finalize a peer joining session.
    ///
    /// Yields encryption keys for this peer.
    fn negotiate_session(self: Box<Self>) -> Result<PeerKeys>;
}

/// Holds data needs to enable a joining peer to join a session.
pub struct SessionJoinContext {
    /// URL of server to join.
    ///
    /// If not set, the client default URL is used.
    pub server_url: Option<String>,

    /// The session ID to join.
    pub session_id: String,

    /// Additional data to relay to the peer to enable it to finalize the session.
    pub peer_context: Option<Vec<u8>>,

    /// Object that will finalize the peer handshake and derive encryption keys.
    pub peer_handshake: Box<dyn SessionJoinPeerHandshake>,
}

#[derive(CborDecode, CborEncode)]
#[cbor(array)]
struct PublicKeySecretMessage {
    #[n(0)]
    server_url: Option<String>,

    #[n(1)]
    session_id: String,

    #[n(2)]
    challenge: Vec<u8>,

    #[n(3)]
    agreement_public: Vec<u8>,
}

pub struct PublicKeyInitiator {
    session_id: String,
    extra_identifier: Vec<u8>,
    sjs: PublicKeySessionJoinString,
    agreement_private: EphemeralPrivateKey,
}

impl SessionInitiatePeer for PublicKeyInitiator {
    fn session_id(&self) -> &str {
        &self.session_id
    }

    fn session_create_context(&self) -> Option<Vec<u8>> {
        None
    }

    fn session_join_string_bytes(&self) -> Result<Vec<u8>> {
        self.sjs.to_bytes()
    }

    fn negotiate_session(self: Box<Self>, peer_context: Option<Vec<u8>>) -> Result<PeerKeys> {
        let public_key = peer_context.ok_or_else(|| {
            RemoteSignError::Crypto(
                "missing peer public key context in session join message".into(),
            )
        })?;

        let public_key = UnparsedPublicKey::new(&X25519, public_key);

        let (sealing, opening) = agree_ephemeral(
            self.agreement_private,
            &public_key,
            RemoteSignError::Crypto("error deriving agreement key".into()),
            |agreement_key| {
                derive_aead_keys(
                    Role::A,
                    agreement_key.to_vec(),
                    &self.session_id,
                    &self.extra_identifier,
                )
            },
        )
        .map_err(|_| {
            RemoteSignError::Crypto("error deriving AEAD keys from agreement key".into())
        })?;

        Ok(PeerKeys { sealing, opening })
    }
}

impl PublicKeyInitiator {
    /// Create a new initiator using public key agreement.
    pub fn new(peer_public_key: impl AsRef<[u8]>, server_url: Option<String>) -> Result<Self> {
        let spki = SubjectPublicKeyInfo::from_der(peer_public_key.as_ref())
            .map_err(|e| RemoteSignError::Crypto(format!("when parsing SPKI data: {}", e)))?;

        let session_id = uuid::Uuid::new_v4().to_string();

        let rng = SystemRandom::new();

        let mut challenge = [0u8; 32];
        rng.fill(&mut challenge)
            .map_err(|_| RemoteSignError::Crypto("failed to generate random data".into()))?;

        let mut aes_key_data = [0u8; 16];
        rng.fill(&mut aes_key_data)
            .map_err(|_| RemoteSignError::Crypto("failed to generate random data".into()))?;

        let agreement_private = EphemeralPrivateKey::generate(&X25519, &rng).map_err(|_| {
            RemoteSignError::Crypto("failed to generate ephemeral agreement key".into())
        })?;

        let agreement_public = agreement_private.compute_public_key().map_err(|_| {
            RemoteSignError::Crypto(
                "failed to derive public key from ephemeral agreement key".into(),
            )
        })?;

        let peer_message = PublicKeySecretMessage {
            server_url,
            session_id: session_id.clone(),
            challenge: challenge.as_ref().to_vec(),
            agreement_public: agreement_public.as_ref().to_vec(),
        };

        // The unique AES key is used to encrypt the main CBOR message.
        let mut message_ciphertext = minicbor::to_vec(peer_message)
            .map_err(|e| RemoteSignError::Crypto(format!("CBOR encode error: {}", e)))?;
        let aes_key = UnboundKey::new(&AES_128_GCM, &aes_key_data).map_err(|_| {
            RemoteSignError::Crypto("failed to load AES encryption key into ring".into())
        })?;
        let mut sealing_key = SealingKey::new(aes_key, ConstantNonceSequence::default());
        sealing_key
            .seal_in_place_append_tag(Aad::empty(), &mut message_ciphertext)
            .map_err(|_| RemoteSignError::Crypto("failed to AES encrypt message to peer".into()))?;

        // The AES encrypting key is encrypted using asymmetric encryption.

        let aes_ciphertext = match spki.algorithm.oid.as_ref() {
            x if x == OID_PKCS1_RSAENCRYPTION.as_bytes() => {
                let public_key =
                    RsaPublicKeyAsn1::from_der(spki.subject_public_key).map_err(|e| {
                        RemoteSignError::Crypto(format!("when parsing RSA public key: {}", e))
                    })?;

                let n = BigUint::from_bytes_be(public_key.modulus.as_bytes());
                let e = BigUint::from_bytes_be(public_key.public_exponent.as_bytes());

                let rsa_public = RsaPublicKey::new(n, e).map_err(|e| {
                    RemoteSignError::Crypto(format!("when constructing RSA public key: {}", e))
                })?;

                let padding = PaddingScheme::new_oaep::<sha2::Sha256>();

                rsa_public
                    .encrypt(&mut rand::thread_rng(), padding, &aes_key_data)
                    .map_err(|e| {
                        RemoteSignError::Crypto(format!("RSA public key encryption error: {}", e))
                    })?
            }
            _ => {
                return Err(RemoteSignError::Crypto(format!(
                    "do not know how to encrypt for algorithm {}",
                    spki.algorithm.oid
                )));
            }
        };

        let public_key = spki
            .to_vec()
            .map_err(|e| RemoteSignError::Crypto(format!("when encoding SPKI to DER: {}", e)))?;

        let sjs = PublicKeySessionJoinString {
            aes_ciphertext,
            public_key,
            message_ciphertext,
        };

        Ok(Self {
            session_id,
            extra_identifier: challenge.as_ref().to_vec(),
            sjs,
            agreement_private,
        })
    }
}

/// Describes a type that is capable of decrypting messages used during public key negotiation.
pub trait PublicKeyPeerDecrypt {
    /// Decrypt an encrypted message.
    fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>>;
}

/// A joining peer using public key encryption.
struct PublicKeyPeerPreJoined {
    sjs: PublicKeySessionJoinString,

    decrypter: Option<Box<dyn PublicKeyPeerDecrypt>>,
}

impl SessionJoinPeerPreJoin for PublicKeyPeerPreJoined {
    fn register_state(&mut self, state: SessionJoinState) -> Result<()> {
        match state {
            SessionJoinState::PublicKeyDecrypt(decrypt) => {
                self.decrypter = Some(decrypt);
                Ok(())
            }
            SessionJoinState::SharedSecret(_) => Ok(()),
        }
    }

    fn join_context(self: Box<Self>) -> Result<SessionJoinContext> {
        let decrypter = self
            .decrypter
            .ok_or_else(|| RemoteSignError::Crypto("decryption key not registered".into()))?;

        let aes_key = decrypter.decrypt(&self.sjs.aes_ciphertext)?;
        let aes_key = UnboundKey::new(&AES_128_GCM, &aes_key).map_err(|_| {
            RemoteSignError::Crypto("failed to construct AES key from key data".into())
        })?;
        let mut opening_key = OpeningKey::new(aes_key, ConstantNonceSequence::default());

        let mut cbor_message = self.sjs.message_ciphertext.clone();
        let cbor_plaintext = opening_key
            .open_in_place(Aad::empty(), &mut cbor_message)
            .map_err(|_| {
                RemoteSignError::Crypto("failed to decrypt using shared AES key".into())
            })?;

        // The plaintext is a CBOR encoded message.
        let message = minicbor::decode::<PublicKeySecretMessage>(cbor_plaintext)
            .map_err(|e| RemoteSignError::Crypto(format!("CBOR decode error: {}", e)))?;

        let agreement_private = EphemeralPrivateKey::generate(&X25519, &SystemRandom::new())
            .map_err(|_| {
                RemoteSignError::Crypto("failed to generate ephemeral agreement key".into())
            })?;
        let agreement_public = agreement_private.compute_public_key().map_err(|_| {
            RemoteSignError::Crypto(
                "failed to derive public key from ephemeral agreement key".into(),
            )
        })?;

        let peer_handshake = Box::new(PublicKeyHandshakePeer {
            session_id: message.session_id.clone(),
            extra_identifier: message.challenge,
            agreement_private,
            agreement_public: message.agreement_public,
        });

        Ok(SessionJoinContext {
            server_url: message.server_url,
            session_id: message.session_id,
            peer_context: Some(agreement_public.as_ref().to_vec()),
            peer_handshake,
        })
    }
}

impl PublicKeyPeerPreJoined {
    fn new(sjs: PublicKeySessionJoinString) -> Result<Self> {
        Ok(Self {
            sjs,
            decrypter: None,
        })
    }
}

pub struct PublicKeyHandshakePeer {
    session_id: String,
    extra_identifier: Vec<u8>,
    agreement_private: EphemeralPrivateKey,
    agreement_public: Vec<u8>,
}

impl SessionJoinPeerHandshake for PublicKeyHandshakePeer {
    fn negotiate_session(self: Box<Self>) -> Result<PeerKeys> {
        let peer_public_key = UnparsedPublicKey::new(&X25519, &self.agreement_public);

        let (sealing, opening) = agree_ephemeral(
            self.agreement_private,
            &peer_public_key,
            RemoteSignError::Crypto("error deriving agreement key".into()),
            |agreement_key| {
                derive_aead_keys(
                    Role::B,
                    agreement_key.to_vec(),
                    &self.session_id,
                    &self.extra_identifier,
                )
            },
        )
        .map_err(|_| {
            RemoteSignError::Crypto("error deriving AEAD keys from agreement key".into())
        })?;

        Ok(PeerKeys { sealing, opening })
    }
}

fn spake_identity(role: Role, session_id: &str, extra_identifier: &[u8]) -> Identity {
    Identity::new(&derive_hkdf_info(role, session_id, extra_identifier))
}

pub struct SharedSecretInitiator {
    sjs: SharedSecretSessionJoinString,
    spake: Spake2<Ed25519Group>,
}

impl SessionInitiatePeer for SharedSecretInitiator {
    fn session_id(&self) -> &str {
        &self.sjs.session_id
    }

    fn session_create_context(&self) -> Option<Vec<u8>> {
        None
    }

    fn session_join_string_bytes(&self) -> Result<Vec<u8>> {
        self.sjs.to_bytes()
    }

    fn negotiate_session(self: Box<Self>, peer_context: Option<Vec<u8>>) -> Result<PeerKeys> {
        let spake_b = peer_context.ok_or_else(|| {
            RemoteSignError::Crypto(
                "missing SPAKE2 initialization context in session join message".into(),
            )
        })?;

        let shared_key = self.spake.finish(&spake_b).map_err(|e| {
            RemoteSignError::Crypto(format!("error finishing SPAKE2 key negotiation: {}", e))
        })?;

        let (sealing, opening) = derive_aead_keys(
            Role::A,
            shared_key,
            &self.sjs.session_id,
            &self.sjs.extra_identifier,
        )?;

        Ok(PeerKeys { sealing, opening })
    }
}

impl SharedSecretInitiator {
    pub fn new(shared_secret: Vec<u8>) -> Result<Self> {
        let session_id = uuid::Uuid::new_v4().to_string();

        let rng = SystemRandom::new();
        let mut extra_identifier = [0u8; 16];
        rng.fill(&mut extra_identifier)
            .map_err(|_| RemoteSignError::Crypto("unable to generate random value".into()))?;

        let (spake, role_a_init_message) = Spake2::<Ed25519Group>::start_a(
            &Password::new(shared_secret),
            &spake_identity(Role::A, &session_id, &extra_identifier),
            &spake_identity(Role::B, &session_id, &extra_identifier),
        );

        Ok(Self {
            sjs: SharedSecretSessionJoinString {
                session_id,
                extra_identifier: extra_identifier.as_ref().to_vec(),
                role_a_init_message,
            },
            spake,
        })
    }
}

/// A joining peer using shared secrets.
struct SharedSecretPeerPreJoined {
    sjs: SharedSecretSessionJoinString,
    shared_secret: Option<Vec<u8>>,
}

impl SessionJoinPeerPreJoin for SharedSecretPeerPreJoined {
    fn register_state(&mut self, state: SessionJoinState) -> Result<()> {
        match state {
            SessionJoinState::SharedSecret(secret) => {
                self.shared_secret = Some(secret);
                Ok(())
            }
            SessionJoinState::PublicKeyDecrypt(_) => Ok(()),
        }
    }

    fn join_context(self: Box<Self>) -> Result<SessionJoinContext> {
        let shared_secret = self
            .shared_secret
            .as_ref()
            .ok_or_else(|| RemoteSignError::Crypto("shared secret not defined".into()))?;

        let (spake, init_message) = Spake2::<Ed25519Group>::start_b(
            &Password::new(shared_secret),
            &spake_identity(Role::A, &self.sjs.session_id, &self.sjs.extra_identifier),
            &spake_identity(Role::B, &self.sjs.session_id, &self.sjs.extra_identifier),
        );

        let peer_handshake = Box::new(SharedSecretHandshakePeer {
            session_id: self.sjs.session_id.clone(),
            extra_identifier: self.sjs.extra_identifier,
            role_a_init_message: self.sjs.role_a_init_message,
            spake,
        });

        Ok(SessionJoinContext {
            // TODO set this field if not the default.
            server_url: None,
            session_id: self.sjs.session_id,
            peer_context: Some(init_message),
            peer_handshake,
        })
    }
}

impl SharedSecretPeerPreJoined {
    fn new(sjs: SharedSecretSessionJoinString) -> Result<Self> {
        Ok(Self {
            sjs,
            shared_secret: None,
        })
    }
}

pub struct SharedSecretHandshakePeer {
    session_id: String,
    extra_identifier: Vec<u8>,
    role_a_init_message: Vec<u8>,
    spake: Spake2<Ed25519Group>,
}

impl SessionJoinPeerHandshake for SharedSecretHandshakePeer {
    fn negotiate_session(self: Box<Self>) -> Result<PeerKeys> {
        let shared_key = self.spake.finish(&self.role_a_init_message).map_err(|e| {
            RemoteSignError::Crypto(format!("error finishing SPAKE2 key negotiation: {}", e))
        })?;

        let (sealing, opening) = derive_aead_keys(
            Role::B,
            shared_key,
            &self.session_id,
            &self.extra_identifier,
        )?;

        Ok(PeerKeys { sealing, opening })
    }
}

pub fn create_session_joiner(
    session_join_string: impl ToString,
) -> Result<Box<dyn SessionJoinPeerPreJoin>> {
    let input = session_join_string.to_string();

    let trimmed = input.trim();

    // Multiline is assumed to be PEM.
    let sjs = if trimmed.contains('\n') {
        let no_comments = trimmed
            .lines()
            .filter(|line| !line.starts_with('#'))
            .collect::<Vec<_>>()
            .join("\n");

        let doc = pem::parse(no_comments.as_bytes())?;

        if doc.tag == "SESSION JOIN STRING" {
            doc.contents
        } else {
            return Err(RemoteSignError::SessionJoinString(
                "PEM does not define a SESSION JOIN STRING".into(),
            ));
        }
    } else {
        base64::decode_config(trimmed.as_bytes(), base64::URL_SAFE_NO_PAD)?
    };

    let mut decoder = Decoder::new(&sjs);
    if !matches!(
        decoder.array().map_err(|_| {
            RemoteSignError::SessionJoinString("decode error: not a CBOR array".into())
        })?,
        Some(2)
    ) {
        return Err(RemoteSignError::SessionJoinString(
            "decode error: not a CBOR array with 2 elements".into(),
        ));
    }

    let scheme = decoder
        .str()
        .map_err(|_| RemoteSignError::SessionJoinString("failed to decode scheme name".into()))?;

    match scheme {
        _ if scheme == PublicKeySessionJoinString::scheme() => {
            let sjs = PublicKeySessionJoinString::decode(&mut decoder, &mut ()).map_err(|e| {
                RemoteSignError::SessionJoinString(format!("error decoding payload: {}", e))
            })?;

            Ok(Box::new(PublicKeyPeerPreJoined::new(sjs)?) as Box<dyn SessionJoinPeerPreJoin>)
        }
        _ if scheme == SharedSecretSessionJoinString::scheme() => {
            let sjs =
                SharedSecretSessionJoinString::decode(&mut decoder, &mut ()).map_err(|e| {
                    RemoteSignError::SessionJoinString(format!("error decoding payload: {}", e))
                })?;

            Ok(Box::new(SharedSecretPeerPreJoined::new(sjs)?) as Box<dyn SessionJoinPeerPreJoin>)
        }
        _ => Err(RemoteSignError::SessionJoinString(format!(
            "unknown scheme: {}",
            scheme
        ))),
    }
}
