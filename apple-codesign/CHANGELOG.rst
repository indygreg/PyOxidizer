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
* Renamed ``apple_codesign::macho::CodeSigningSlot::SecuritySettings`` to
  ``EntitlementsDer``.
* Add ``apple_codesign::macho::CodeSigningSlot::RepSpecific``.
* ``rcodesign extract`` has learned a ``macho-target`` output to display information
  about targeting settings of a Mach-O binary.
* The code signature data structure version is now automatically modernized when
  signing a Mach-O binary targeting iOS >= 15 or macOS >= 12. This fixes an issue
  where signatures of iOS 15+ binaries didn't meet Apple's requirements for this
  platform.
* Logging switched to ``log`` crate. This changes program output slightly and removed
  an ``&slog::Logger`` argument from various functions.
* ``SigningSettings`` now internally stores entitlements as a parsed plist. Its
  ``set_entitlements_xml()`` now returns ``Result<()>`` in order to reflect errors
  parsing plist XML. Its ``entitlements_xml()`` now returns ``Result<Option<String>>``
  instead of ``Option<&str>`` because XML serialization is fallible and the resulting
  XML is owned instead of a reference to a stored value. As a result of this change,
  the embedded entitlements XML specified via ``rcodesign sign --entitlement-xml-path``
  may be encoded differently than it was previously. Before, the content of the
  specified file was embedded verbatim. After, the file is parsed as plist XML and
  re-serialized to XML. This can result in encoding differences of the XML. This
  should hopefully not matter, as valid XML should be valid XML.
* Support for DER encoded entitlements in code signatures. Apple code signatures
  encode entitlements both in plist XML form and DER. Previously, we only supported
  the former. Now, if entitlements are being written, they are written in both XML
  and DER. This should match the default behavior of `codesign` as of macOS 12.
  (#513, #515)

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