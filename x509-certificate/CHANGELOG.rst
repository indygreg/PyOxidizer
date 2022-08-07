============================
``x509-certificate`` History
============================

0.15.0
======

(Not yet released)

0.14.0
======

(Released 2022-08-07)

* bcder crate upgraded from 0.6.1 to 0.7.0. This entailed a lot of changes,
  mainly to error handling.

0.13.0
======

* ``X509Certificate`` now implements the ``spki::EncodePublicKey`` trait.
  This change marks the beginning of a shift/intent to converge this crate
  onto the interfaces defined by crates under the
  `RustCrypto <https://github.com/RustCrypto>`_ umbrella for better interop
  with the rest of the Rust ecosystem.
* ``KeyAlgorithm`` now implements conversion from/to ``spki::ObjectIdentifier``.
* ``InMemorySigningKeyPair`` now implements ``signature::Signer``. This means
  there are now 2 implementations of ``sign()`` on this type. So if both traits
  are in scope you will need to disambiguate the call.
* The ``Sign::sign()`` trait method is now marked as deprecated. Please switch
  to the ``signature::Signer`` trait.

0.12.0
======

* Defined a new ``Sign`` trait to indicate support for cryptographic signing.
  ``InMemorySigningKeyPair`` implements this trait and callers may need to
  ``use x509_certificate::Sign`` to pull the trait into scope.
* Some functions for resolving algorithm identifiers now return ``Result``.
* Defined RFC 3447 ASN.1 types for representing RSA private keys.
* ``InMemorySigningKeyPair`` now holds the the raw private key data. This
  enables the content to be retrieved later.
* Added certificate signing request ASN.1 types to the new ``rfc2986`` module.
* ``X509CertificateBuilder`` has a new ``create_certificate_signing_request()``
  method to create a certificate signing request (CSR).

0.11.0
======

* Add some APIs on ``Name`` to retrieve additional well-known fields.
* Add ``Name::user_friendly_str()`` for obtaining a user-friendly string
  from a series of attributes.

0.10.0
======

* ``CapturedX509Certificate`` has gained a ``verify_signed_data_with_algorithm()``
  method that uses an explicit ``ring::signature::VerificationAlgorithm`` for
  verification. The new method allows verifying when using an alternative
  verification algorithm. ``verify_signed_data()`` now internally calls into the
  new function.

0.9.0
=====

* Store ``version`` field of ``TbsCertificate`` as ``Option<Version>`` instead
  of ``Version``. In 0.8.0 we interpreted a missing optional field as version 1.
  This was semantically correct. However, when we encoded the parsed data
  structure we would invent a new ``version`` field where it didn't exist before.
  This mismatch is relevant for operations like resolving the certificate
  fingerprint, as the extra field would produce a different fingerprint result.
  Serializing now omits the ``version`` field when it wasn't originally defined.
  (#525)

0.8.0
=====

* Properly parse ``TbsCertificate`` that is missing a ``version`` field.
  Before, we'd get a ``Malformed`` error if this optional field was missing.
  Now, we correctly interpret a missing field as version 1. (#521)

0.7.0
=====

* Refactor ``GeneralizedTime`` parsing to allow fractional seconds and timezones.
  Previously, only limited forms of ``GeneralizedTime`` were parsed. (#482)

0.6.0
=====

* Support parsing ``RSAPublicKey`` from RFC 8017.

0.5.0 and Earlier
=================

* No changelog kept.
