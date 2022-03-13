========================
`apple-codesign` History
========================

0.9.0
=====

* Imported new Apple certificates. ``Developer ID - G2 (Expiring 09/17/2031 00:00:00 UTC)``,
  ``Worldwide Developer Relations - G4 (Expiring 12/10/2030 00:00:00 UTC)``,
  ``Worldwide Developer Relations - G5 (Expiring 12/10/2030 00:00:00 UTC)``,
  and ``Worldwide Developer Relations - G6 (Expiring 03/19/2036 00:00:00 UTC)``.
* Changed names of enum variants on ``apple_codesign::apple_certificates::KnownCertificate``
  to reflect latest naming from https://www.apple.com/certificateauthority/.
* Refreshed content of Apple certificates ``AppleAAICA.cer``, ``AppleISTCA8G1.cer``, and
  ``AppleTimestampCA.cer``.

0.8.0
=====

* Crate renamed from ``tugger-apple-codesign`` to ``apple-codesign``.
* Fixed bug where signing failed to update the ``vmsize`` field of the
  ``__LINKEDIT`` mach-o segment. Previously, a malformed mach-o file could
  be produced. (#514)
* Added ``x509-oids`` command for printing Apple OIDs related to code signing.
* Added ``analyze-certificate`` command for printing information about
  certificates that is relevant to code signing.
* Added the ``tutorial`` crate with some end-user documentation.
* Crate dependencies updated to newer versions.

0.7.0 and Earlier
=================

* Crate was published as `tugger-apple-codesign`. No history kept in this file.