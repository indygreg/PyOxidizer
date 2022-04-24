.. _apple_codesign_remote_signing_protocol:

============================
Remote Code Signing Protocol
============================

Overview
========

The remote signing protocol facilitates the cryptographic signing of messages
involving 2 discrete network peers.

The peer that wants something signed is the **initiator**.

The peer with access to the signing key that produces cryptographic
signatures is the **signer**.

Peers establish persistent websocket connections to a central server to
enable them to speak with each through firewalls and NATs.

Peers register an ephemeral *session* with the server, which is essentially
a binding between 2 connected websocket clients.

Peers derive session-specific encryption keys using mutually agreed upon
ahead of time data. They then relay end-to-end encrypted messages through
the central server and perform cryptographic signing operations.

Wire Protocol
=============

The protocol entails the exchange of JSON encoded objects via websockets.

The JSON objects sent from clients to the server have the following keys:

``request_id``
   (string) (required) A unique identifier for this request.

``api``
   (string) (required) The name of the API / method to invoke on the server.

``payload``
   (object) (optional) Parameters passed to this API invocation.

The JSON objects sent from servers to clients have the following keys:

``request_id``
   (string) (optional) Echo of ``request_id`` from the message that generated
   this one. The value could be unknown to the receiver if this message was
   generated from the other peer in the session.

``type``
   (string) (required) The message type.

``ttl``
   (number) (optional) Integer number of seconds remaining before the session
   expires and will be automatically deleted by the server.

``payload``
   (object) (optional) Payload further describing this message.

All other fields in the top-level object are reserved for future use.

Messages sent from the client to server ALWAYS result in the server responding
to that API request.

It is also possible for servers to send messages to clients asynchronously
of any client-initiated message.

Initial Connection Protocol
===========================

When a client connects to the server, it SHOULD issue a ``hello`` API
message and wait for the server's response.

If the response contains a *message of the day* string, it MUST be displayed
to the end-user.

Clients SHOULD also make a best effort attempt to validate the server's
advertised capabilities and make a determination about compatibility and
error or print warnings if incompatibility is detected.

.. _apple_codesign_remote_signing_sessions:

Session Negotiation
===================

The *initiator* and *signer* pair with each other by forming a *session*.

From the server's perspective, a *session* is an opaque identifier string
with associated state, such as the unique websocket connection IDs of the
*initiator* and *signer* clients.

Sessions are ephemeral and expire automatically after a duration specified
by the initiating client. (The server can impose a maximum duration to prevent
service abuse.)

Sessions are generally created by the *initiator*.

The *initiator* creates a unique session ID, ``SessionId``. ``SessionId`` MUST
be randomly chosen. It SHOULD have sufficient entropy to prevent server-side
collisions. The use of type 4 UUIDs for session IDs is recommended.

Once a server-side session is created, the *initiator* then shares a
*session join string* with the signer via an out-of-band mechanism.
See :ref:`apple_codesign_remote_session_join_strings` for more.

At this point, mechanisms diverge based on the session joining mechanism
employed. But generally speaking, the *signer* sends a
:ref:`apple_codesign_remote_api_client_join_session` to the server
to register itself as the other peer in the session. At this point, both
peers derive encryption keys and communicate with each other by issuing
:ref:`apple_codesign_remote_api_client_send_message` messages. See
:ref:`apple_codesign_remote_signing_protocol_encrypted_protocol` for more.

.. _apple_codesign_remote_session_join_strings:

Session Join Strings
====================

The *initiator* and *signer* need to leverage an out-of-band mechanism for
communicating metadata with each other in order to join a server-established
session. There are various potential solutions for this and we've purposefully
designed the mechanism to be extensible.

Generically, the mechanism to join a session is expressed through a
**session join string**, or SJS.

The SJS is ultimately a CBOR encoded array of length 2. The array's elements
are:

* (string) The scheme being used.
* (varied) The payload for that scheme.

But to end-users it is an opaque string.

The SJS can be encoded as:

* Base64 using the RFC 3548 *URL safe* character set with optional ``=``
  padding.
* PEM using ``SESSION JOIN STRING`` as the armoring tag.

In general, the *session join string* is shared out-of-band with the other
peer, who uses it to join the session.

In general, *session join strings* are designed such that a 3rd party
becoming aware of the SJS will not jeopardize the security of the current or
future signing operations. However, denial of service could occur if the SJS
exposes the session ID and a 3rd party joins the session before the *intended*
peer.

The following sections denote the defined *session join string* schemes.
Sections names are the ``scheme`` value.

``publickey0``
--------------

The ``publickey0`` session joining mechanism relies on public key cryptography
to authenticate the 2nd peer in a session by leveraging knowledge of the
2nd peer's public encryption key.

The initiating peer, ``A``, MUST know the public key of the joining peer,
``B``.

``A`` generates a random value at least 32 bytes long, ``ChallengeSecret``.

``A`` generates a new RFC 7748 Curve 25519 private key. Its private /
public components are ``AAgreementPrivate`` and ``AAgreementPublic``,
respectively.

``A`` generates a new random 16 byte value, ``SharedAESKey``.

``A`` loads the public key of ``B``, ``BPublic``. It usually does so by
extracting the X.509 SubjectPublicKeyInfo (SPKI) (RFC 5280 Section 4.1.2.7)
from an X.509 certificate or DER/PEM fragment of just the SPKI.

``A`` prepares a plaintext message to be sent to ``B``, ``AJoinPlaintext``.
This message is a CBOR array with the following elements:

``serverUrl``
   (Index 0) (optional string) URL of the server to connect to.

``sessionId``
   (Index 1) (string) The session identifier created on the server.

``challenge``
   (Index 2) (bytes) The content of ``ChallengeSecret``.

``agreementPublic``
   (Index 3) (bytes) ``SubjectPublicKeyInfo`` for ``AAgreementPublic``.

``A`` encrypts ``AJoinPlaintext`` using AES-128 in GCM with ``SharedAESKey``,
yielding ``AJoinCiphertext``. A 12 byte nonce is used where the bytes are all
``0x42``. The 16 byte authentication tag is appended to the raw ciphertext
and constitutes the final bytes of ``AJoinCiphertext``.

``A`` encrypts ``SharedAESKey`` using asymmetric encryption targeting
``BPublic``, yielding ``SharedAESCiphertext``.

For RSA, OAEP padding with SHA-256 digests MUST be used.

The payload of the *session join string* is a CBOR array with the following
elements:

``aes_ciphertext``
   (Index 0) (bytes) The ``SharedAESCiphertext`` generated above.

``bPublic``
   (Index 1) (bytes) The SPKI describing which public key was used to
   encrypt ``SharedAESCiphertext``.

``message_ciphertext``
   (Index 2) (bytes) The ``AJoinCiphertext`` generated above.

So, the final *session join string* is
``["publickey0", [SharedAESCiphertext, BSPKI, AJoinCiphertext]]``.

The *session join string* is summarily CBOR and base64 encoded and made
available to ``B``.

``B`` receives and decodes the SJS.

``B`` locates the decryption key from the provided SPKI structure. (``B``
may want to impose restrictions here to prevent clients from fishing for
specific keys.)

``B`` decrypts ``SharedAESCiphertext`` using ``BPrivate``, yielding back
``SharedAESKey``.

Using ``SharedAESKey``, ``B`` verifies and decrypts ``AJoinCiphertext``,
yielding ``AJoinPlaintext``.

On success, ``B`` generates a new RFC 7748 Curve 25519 private key,
``BAgreementPrivate`` and ``BAgreementPublic``.

``B`` connects to the server and sends a
:ref:`apple_codesign_remote_api_client_join_session` message with ``context``
set to ``BAgreementPublic``.

At this point, ``A`` and ``B`` both perform key agreement using their
ephemeral ED25519 private key and the public key of the other peer, each
mutually deriving ``SessionSharedKey``.

At this point, the procedure described in
:ref:`apple_codesign_remote_signing_aead_keys` is used to derive new symmetric
encryption keys. ``ChallengeSecret`` is used as the additional value to
derive ``IdentifierA`` and ``IdentifierB``.

Security Considerations
^^^^^^^^^^^^^^^^^^^^^^^

The *session join string* consists of 2 discrete encrypted payloads and is
generally safe against offline attacks. Unless ciphers are broken, the
private key is required to obtain for anything beyond side-channels (like
total payload size).

``SessionId`` is encrypted, so compromise of the SJS can't easily lead to a
DoS by an unwanted peer joining the session.

The server doesn't see anything: the encrypted AES key and AES encrypted
peer metadata are both encapsulated in the SJS. We could potentially move
some of these to the server to reduce the length of the SJS.

Open Questions for Security Audit
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

* We don't sign / HMAC the asymmetrically encrypted AES key. Nor do we
  include an IV or other prepended message. This seems to go against
  best practices. Does it matter? Does the additional layer of AEAD feeding
  into the key agreement compensate for this?
* Is the use of a constant nonce for the ``SharedAES`` ->  ``AJoinCiphertext``
  acceptable? The AES key is randomly generated and is used exactly once, so
  do the nonces even matter?
* Is AES-128 in GCM mode a sufficient key/cipher for encrypting the main
  message?
* We currently generate 2 distinct private keys: 1 for key agreement and 1
  for AES encryption. They are generated independently. Does this make sense
  or should perhaps HKDF be used against a common key?
* Right now there is no explicit trust anchoring between the asymmetric
  encryption targeting ``B`` and the derived shared secret key. Should ``B``
  produce a cryptographic signature using ``BPrivate`` so ``A`` doesn't assume
  that *ability to decrypt* authenticates ``B``? Or is *ability to decrypt*
  along with the assumption that only ``B`` possesses ``agreementPublic``
  sufficient?

``sharedsecret0``
-----------------

The ``sharedsecret0`` session joining mechanism uses SPAKE2 to derive a shared
encryption key using an ahead-of-time mutually agreed upon shared secret,
``SharedSecret``.

The peer creating the session, henceforth ``A``, generates unique/random
``SessionId`` and ``Identifier`` values. These values are used to construct
the SPAKE2 identifier strings: ``A:{SessionId}:{Identifier}`` and
``B:{SessionId}:{Identifier}``.

``A`` begins SPAKE2 role A initialization using ``SharedSecret`` and role A's
identifier string. This produces ``SpakeAInit``.

``A`` calls :ref:`apple_codesign_remote_api_client_create_session` to
register the new session with the server. Its ``context`` field is empty.

The *session join string* value is a CBOR array with the following elements:

``sessionId``
   (Index 0) (string) The session identifier string.

``identifier``
   (Index 1) (bytes) The random ``Identifier`` value produced earlier.

``spakeAInit``
   (Index 2) (bytes) The SPAKE2 Role A initialization message.

The final CBOR *session join string* is
``["sharedsecret0", [SessionId, Identifier, SpakeAInit]]``.

The *session join string* is summarily CBOR and base64 encoded and made
available to ``B``.

``B`` receives and decodes the SJS.

``B`` performs SPAKE2 Role B initialization, producing ``SpakeBInit``.

``B`` sends a :ref:`apple_codesign_remote_api_client_join_session` message
to the server with ``context`` set to the base64 encoding of ``SpakeBInit``.
``SpakeBInit`` is relayed to ``A`` via the server.

At this point, both ``A`` and ``B`` are able to finalize SPAKE2 using
``SpakeBInit`` and ``SpakeAInit``, respectively. They should mutually derive
a shared encryption key, ``SessionSharedKey``.

At this point, the procedure described in
:ref:`apple_codesign_remote_signing_aead_keys` is used to derive new symmetric
encryption keys. ``Identifier`` is used as the additional value used to
derive ``IdentifierA`` and ``IdentifierB``.

Security Considerations
^^^^^^^^^^^^^^^^^^^^^^^

The *session join string* containing the plaintext ``SessionId``,
``Identifier``, and ``SpakeAInit`` generally does not need to be highly
secure or made secret.

``SharedSecret`` cannot be derived from knowledge of the *session join string*.

The server does not directly observe the value for ``Identifier``, only
``SpakeBInit``. So it would need knowledge of the *session join string*
and ``SharedSecret`` to decrypt messages.

A 3rd party in a privileged network position (including the server) with
knowledge of ``SharedSecret``, ``SessionId``, and ``Identifier`` would be
able to decrypt and forge messages, as it would be able to derive ``RoleAKey``
and ``RoleBKey``. So it is important to use transport-level encryption,
a trusted server, and keep ``SharedSecret`` a secret value.

Open Questions for Security Audit
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

* Is SPAKE2 the best mechanism for deriving session encryption keys from a
  shared secret?
* Should ``SpakeAInit`` be in the *session join string* or stored on the server
  and hidden from plaintext view? What are the tradeoffs with each approach?
* As proposed, the SPAKE2 identifier contains ``SessionId`` and yet another
  random value. That random value is not sent to the server but is possibly
  world readable in the *session join string*. Is this second source of entropy
  necessary? Does attempting to prevent the server from having access to it buy
  us any security value? Or is just the client-chosen ``SessionId`` string good
  enough?
* The SPAKE2 specification seems to insist on the use of key confirmation
  messages. Since we're using HKDF into AEAD, which has built-in authentication,
  do we need to perform the SPAKE2 key confirmation since any failures in SPAKE2
  land would lead to AEAD failures anyway?
* How sensitive is SPAKE2 to the entropy of ``SharedSecret``? While we want to
  encourage a relatively strong ``SharedSecret``, we can't guarantee this.
  Should we be doing e.g. PBKDF2 on ``SharedSecret`` before feeding it into
  SPAKE2 or will SPAKE2 do sufficient *key stretching* on its own?

.. _apple_codesign_remote_signing_aead_keys:

AEAD Key Derivation
-------------------

The schemes above commonly detail the steps to enable 2 peers to mutually
derive a session-ephemeral shared encryption key, ``SessionSharedKey``.

Rather than use ``SessionSharedKey`` directly for subsequent message exchange,
we instead derive additional keys from it for use with Authenticated Encryption
and Additional Data (AEAD) encryption / message exchange.

An identifier value is associated with peers assuming roles ``A`` (the session
initiator) and ``B`` (the session joiner). The value is a bytes concatenation
of:

* The role name. e.g. ``A`` / ``0x41`` or ``B`` / ``0x42``.
* A colon (``:`` / ``0x3a``)
* The ``SessionId`` identifier, UTF-8 encoded.
* A colon (``:`` / ``0x3a``)
* An additional value communicated in the session join string. e.g.
  ``ChallengeSecret``.

These values are known as ``IdentifierA`` and ``IdentifierB``.

HKDF is used to derive new keys.

Step 1 / HKDF-Extract uses an empty salt and ``SessionSharedKey`` to produce
a pseudorandom key, ``PRK``.

Step 2 / HKDF-Expand is performed twice to derive 2 new keys. The first
invocation uses ``IdentifierA`` for ``info`` and ``32`` for ``L``, producing
``RoleAKey``. The second invocation uses ``IdentifierB`` for ``info`` and ``32``
for ``L``, producing ``RoleBKey``.

``RoleAKey`` and ``RoleBKey`` are used to empower AEAD encryption / message
exchange. ChaCha20+Poly1305 is used. Nonces are 12 bytes where the first 4
bytes are a little-endian u32 counter whose initial used value is ``0`` and
the subsequent 8 bytes are always ``0``. Additionally authenticated data
(``AAD``) is generally not used.

``RoleAKey`` is used by ``A`` to encrypt messages and by ``B`` to
verify/decrypt messages from ``A``. ``RoleBKey`` is used by ``B`` to
encrypt messages and by ``A`` to verify/decrypt messages from ``B``.

Open Questions for Security Audit
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

* Is ChaCha20+Poly1305 a reasonable cipher choice? Or should we be using
  block ciphers (e.g. AES)?
* Using a simple, easily guessable counter for nonces seems wrong. Using a
  random value seems more appropriate. But both parties need to know what the
  nonce we be. Do we use a random value for the nonce but encode the nonce in
  plaintext next to the exchanged ciphertext messages? Or do we need something
  else entirely?
* We could potentially use additionally authenticated data (AAD) to encapsulate
  more details of the request, such as the request ID. Does that buy us
  security benefits?


.. _apple_codesign_remote_signing_protocol_encrypted_protocol:

Signing Protocol
================

Once 2 peers have established a session and derived encryption keys to
facilitate end-to-end encrypted communication, they communicate with each
other using :ref:`peer to peer messages <apple_codesign_remote_api_peer_messages>`
by invoking the :ref:`apple_codesign_remote_api_client_send_message` API.

This process generally involves a handshake:

1. Both peers simultaneously send :ref:`apple_codesign_remote_api_peer_ping`
   messages.
2. Upon receipt, each peer sends a :ref:`apple_codesign_remote_api_peer_pong`
   in response. This dance confirms peer presence and that the derived
   encryption keys work.
3. The *initiator* sends a
   :ref:`apple_codesign_remote_api_peer_request_signing_certificate` to request
   information about the signer's public certificate. This is necessary in
   order to allow the signer to do things like estimate the sizes of signatures
   and to derive additional details needed for signing.
4. The *signer* sends a
   :ref:`apple_codesign_remote_api_peer_signing_certificate` in response.

At this point, both peers are ready to commence signing.

5. The *initiator* sends a :ref:`apple_codesign_remote_api_peer_sign_request`.
6. The *signer* receives the request, assesses it, creates a cryptographic
   signature, and sends a :ref:`apple_codesign_remote_api_peer_signature`
   in reply.
7. Steps 5-6 are repeated as necessary.

Finally,

8. Either peer sends a :ref:`apple_codesign_remote_api_client_goodbye` to
   finalize the session.

Client Issued Messages
======================

The following sections denote the types of messages issued from clients to
servers.

Section names denote the value of the ``api`` key in the messages.

.. _apple_codesign_remote_api_client_hello:

``hello``
---------

Greets the server and obtains information about the server.

This message type has no payload.

Servers respond to this message with a
:ref:`apple_codesign_remote_api_server_greeting`.

.. _apple_codesign_remote_api_client_create_session:

``create-session``
------------------

Requests the creation of a new session on the server.

Sent by the *initiator* as part of session negotiation.

Fields:

``session_id``
   (string) (required) Unique identifier to use for this session.

``ttl``
   (number) (required) Requested session duration, in seconds.

``context``
   (string) (optional) Additional context to be passed to the peer when it
   joins the session.

Servers SHOULD automatically expire the server-side session state after its
TTL duration expires. Servers MAY close connections to connected clients when
their session expires. Servers MAY impose a shorter TTL if the requested TTL
is too long.

Servers respond to this message with a
:ref:`apple_codesign_remote_api_server_session_created`.

.. _apple_codesign_remote_api_client_join_session:

``join-session``
----------------

Attempts to join an existing session.

Sent by the *signer* as part of session negotiation.

Fields:

``session_id``
   (string) (required) Identifier of session to join.

``context``
   (string) (optional) Additional context to pass through to the other
   peer.

Servers respond to this message with a
:ref:`apple_codesign_remote_api_server_session_joined`.

.. _apple_codesign_remote_api_client_send_message:

``send-message``
----------------

Sends an (encrypted) message to the other peer in this session.

Fields:

``session_id``
   (string) (required) Identifier of session to use for peer lookup.

``message``
   (string) (required) Base64 encoded ciphertext of an AEAD encrypted
   message to send to the peer.

Server implementations MUST ensure that the client issuing this request
are bound to the session they are attempting to send a message to.

Servers react to this message by sending a
:ref:`apple_codesign_remote_api_server_peer_message` to the other peer
in the specified session.

Servers respond to this message with a
:ref:`apple_codesign_remote_api_server_message_sent`.

.. _apple_codesign_remote_api_client_goodbye:

``goodbye``
-----------

Indicates the client is finished and will be disconnecting.

Fields:

``session_id``
   (string) (required) Identifier of session to use for peer lookup.

``reason``
   (string) (option) Reason the client is disconnecting.

Server implementations MUST ensure that the client issuing this request
is bound to the session they are attempting to close.

Servers react to this message by sending a
:ref:`apple_codesign_remote_api_server_session_closed` to the other peer
in the specified session.

Servers respond to this message with a
:ref:`apple_codesign_remote_api_server_session_closed`.

Server Sent Messages
====================

The following sections denote the types of messages sent from the server
to clients.

Section names denote the value of the ``type`` field in the message.

.. _apple_codesign_remote_api_server_greeting:

``error``
---------

Conveys information about a server-side error.

Could be sent in reply to any API request or sent asynchronously if some
error occurred (such as the peer disconnecting unexpectedly).

Fields:

``code``
   (string) (required) Value that uniquely identifies this error type.

``message``
   (string) (required) Human readable error message.

``greeting``
------------

Conveys information about the server.

Sent in reply to a :ref:`apple_codesign_remote_api_client_hello` request.

Fields:

``apis``
   (array of strings) (required) Names of APIs that the server supports.

``motd``
   (string) (optional) *Message of the day* conveying messaging that the
   server operator wishes clients to know about.

.. _apple_codesign_remote_api_server_session_created:

``session-created``
-------------------

Conveys the successful creation of a session.

Sent in reply to a :ref:`apple_codesign_remote_api_client_create_session`
request.

.. _apple_codesign_remote_api_server_session_joined:

``session-joined``
------------------

Conveys the successful joining into a session.

Sent in reply to a :ref:`apple_codesign_remote_api_client_join_session`
request.

Sent asynchronously by servers in response to a
:ref:`apple_codesign_remote_api_client_join_session` issued by the joining
peer.

Fields:

``context``
   (string) (optional) Data from the peer required to finish initializing
   the session.

   If this message was sent in reply to a
   :ref:`apple_codesign_remote_api_client_join_session`, the value will be
   from the initiating peer.

   If this message was sent to the pre-existing peer in reaction to a
   :ref:`apple_codesign_remote_api_client_join_session`, the value will be
   from the joining peer.

.. _apple_codesign_remote_api_server_message_sent:

``message-sent``
----------------

Conveys the successful sending of a message to the session peer.

Sent in reply to a :ref:`apple_codesign_remote_api_client_send_message`
request.

.. _apple_codesign_remote_api_server_peer_message:

``peer-message``
----------------

Delivers an (encrypted) message from the peer in this session.

Sent asynchronously by servers in response to a
:ref:`apple_codesign_remote_api_client_send_message` issued by the
other peer in a session.

Fields:

``message``
   (string) (required) Base64 encoded AEAD message.

.. _apple_codesign_remote_api_server_session_closed:

``session-closed``
------------------

Conveys that the session has been finalized and can no longer be used.

Sent in reply to a :ref:`apple_codesign_remote_api_client_goodbye` request
as well as asynchronously to the peer in its session.

Fields:

``reason``
   (string) (optional) Provides further context on why the session was closed.

.. _apple_codesign_remote_api_peer_messages:

Peer to Peer Messages
=====================

Peers within a session communicate with each other by sending and receiving
:ref:`apple_codesign_remote_api_client_send_message` and
:ref:`apple_codesign_remote_api_server_peer_message`, respectively.

The ``message`` field denotes a base64 encoded AEAD encrypted message. The
message consists of the ciphertext with the authentication tag appended. The
plaintext of these messages is the JSON encoding of an object having the
following keys:

``type``
   (string) (required) The message type. This is unique message namespace from
   server-sent messages.

``payload``
   (object) (optional) Payload for this message.

The following sections denote the types of peer-to-peer messages. The section
names denote the value for the ``type`` field.

.. _apple_codesign_remote_api_peer_ping:

``ping``
--------

Check on the status of the peer.

Receivers should send a :ref:`apple_codesign_remote_api_peer_pong` in response.

.. _apple_codesign_remote_api_peer_pong:

``pong``
--------

Respond to a status check from a peer.

Sent in response to a :ref:`apple_codesign_remote_api_peer_ping` message.

.. _apple_codesign_remote_api_peer_request_signing_certificate:

``request-signing-certificate``
-------------------------------

Requests the peer to send it information about its signing certificate.

Receivers should send a
:ref:`apple_codesign_remote_api_peer_signing_certificate` in response.

Should only be sent by the *initiator*.

.. _apple_codesign_remote_api_peer_signing_certificate:

``signing-certificate``
-----------------------

Describes the signing certificate(s) that is being used by the signer.

Sent in response to a
:ref:`apple_codesign_remote_api_peer_request_signing_certificate`.

Fields:

``certificates``
   (array of object) (required) Contains a list of signing certificates that
   will potentially be used.

   Each entry is an object described below.

   Today, there is likely a single certificate in this array. We've
   left the door open for supporting the use of multiple signing
   certificates in the future.

Each entry in the ``certificatess`` array is an object with the following
fields:

``certificate``
   (string) (required) Base64 encoded DER of the public X.509 certificate.

``chain``
   (array of strings) (optional) Base64 encoded DER of additional public
   X.509 certificates in the signing chain for this certificate.

.. _apple_codesign_remote_api_peer_sign_request:

``sign-request``
----------------

Requests the cryptographic signing of a message.

Fields:

``message``
   (string) (required) Base64 encoded message to be signed.

.. _apple_codesign_remote_api_peer_signature:

``signature``
-------------

Conveys the cryptographic signature over a message.

Sent in response to a
:ref:`apple_codesign_remote_api_peer_sign_request`.

Fields:

``message``
   (string) (required) Base64 encoded message that was signed.

``signature``
   (string) (required) Base64 encoded signature data.

``algorithm_oid``
   (string) (required) Base64 encoded DER encoding of OID denoting the
   signature algorithm.
