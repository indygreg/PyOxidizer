============================
``x509-certificate`` History
============================

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
