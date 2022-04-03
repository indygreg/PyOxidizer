========================
`apple-codesign` History
========================

0.11.0
======

* The ``--pfx-file``, ``--pfx-password``, and ``--pfx-password-file`` arguments
  have been renamed to ``--p12-file``, ``--p12-password``, and
  ``--p12-password-file``, respectively. The old names are aliases and should
  continue to work.
* Initial support for using smartcards for signing. Smartcard integration may only
  work with YubiKeys due to how the integration is implemented.
* A new ``rcodesign smartcard-scan`` command can be used to scan attached
  smartcards and certificates they have available for code signing.
* ``rcodesign sign`` now accepts a ``--smartcard-slot`` argument to specify the
  slot number of a certificate to use when code signing.
* A new ``rcodesign smartcard-import`` command can be used to import a code signing
  certificate into a smartcard. It can import private-public key pair or just import
  a public certificate (and use an existing private key on the smartcard device).
* A new ``rcodesign generate-certificate-signing-request`` command can be used
  to generate a Certificate Signing Request (CSR) which can be uploaded to Apple
  and exchanged for a code signing certificate signed by Apple.
* A new ``rcodesign smartcard-generate-key`` command for generating a new private
  key on a smartcard.
* Fixed bug where ``--code-signature-flags``, `--executable-segment-flags``,
  ``--runtime-version``, and ``--info-plist-path`` could only be specified once.
* ``rcodesign sign`` now accepts an ``--extra-digest`` argument to provide an
  extra digest type to include in signatures. This facilitates signing with
  multiple digest types via e.g. ``--digest sha1 --extra-digest sha256``.
* Fixed an embarrassing number of bugs in bundle signing. Bundle signing was
  broken in several ways before: resource files in shallow app bundles (e.g. iOS
  app bundles) weren't handled correctly; symlinks weren't preserved correctly;
  framework signing was completely busted; nested bundles weren't signed in the
  correct order; entitlements in Mach-O binaries weren't preserved during
  signing; ``CodeResources`` files had extra entries in ``<files>`` that shouldn't
  have been there, and likely a few more.
* Add ``--exclude`` argument to ``rcodesign sign`` to allow excluding nested
  bundles from signing.

0.10.0
======

* Support for signing, notarizing, and stapling ``.dmg`` files.
* Support for signing, notarizing, and stapling flat packages (``.pkg`` installers).
* Various symbols related to common code signature data structures have been moved from the
  ``macho`` module to the new ``embedded_signature`` module.
* Signing settings types have been moved from the ``signing`` module to the new
  ``signing_settings`` module.
* ``rcodesign sign`` no longer requires an output path and will now sign an entity
  in place if only a single positional argument is given.
* The new ``rcodesign print-signature-info`` command prints out easy-to-read YAML
  describing code signatures detected in a given path. Just point it at a file with
  code signatures and it can print out details about the code signatures within.
* The new ``rcodesign diff-signatures`` command prints a diff of the signature content
  of 2 filesystem paths. It is essentially a built-in diffing mechanism for the output
  of ``rcodesign print-signature-info``. The intended use of the command is to aid
  in debugging differences between this tool and Apple's canonical tools.

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
* When signing, the entitlements plist associated with the signing operation
  is now parsed and keys like ``get-task-allow`` and
  ``com.apple.private.skip-library-validation`` are now automatically propagated
  to the code directory's executable segment flags. Previously, no such propagation
  occurred and special entitlements would not be fully reflected in the code
  signature. The new behavior matches that of ``codesign``.
* Fixed a bug in ``rcodesign verify`` where code directory verification was
  complaining about ``slot digest contains digest for slot not in signature``
  for the ``Info (1)`` and ``Resources (3)`` slots. The condition it was
  complaining about was actually valid. (#512)
* Better supported for setting the hardened runtime version. Previously, we
  only set the hardened runtime version in a code signature if it was present
  in the prior code signature. When signing unsigned binaries, this could
  result in the hardened runtime version not being set, which would cause
  Apple tools to complain about the hardened runtime not being enabled. Now,
  if the ``runtime`` code signature flag is set on the signing operation and
  no runtime version is present, we derive the runtime version from the version
  of the Apple SDK used to build the binary. This matches the behavior of
  ``codesign``. There is also a new ``--runtime-version`` argument to
  ``rcodesign sign`` that can be used to override the runtime version.
* When signing, code requirements are now printed in their human friendly
  code requirements language rather than using Rust's default serialization.
* ``rcodesign sign`` will now automatically set the team ID when the signing
  certificate contains one.
* Added the ``rcodesign find-transporter`` command for finding the path to
  Apple's *Transporter* program (which is used for notarization).
* Initial support for stapling. The ``rcodesign staple`` command can be used
  to staple a notarization ticket to an entity. It currently only supports
  stapling app bundles (``.app`` directories). The command will automatically
  contact Apple's servers to obtain a notarization ticket and then staple
  any found ticket to the requested entity.
* Initial support for notarizing. The ``rcodesign notarize`` command can
  be used to upload an entity to Apple. The command can optionally wait on
  notarization to finish and staple the notarization ticket if notarization
  is successful. The command currently only supports macOS app bundles
  (``.app`` directories).

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
