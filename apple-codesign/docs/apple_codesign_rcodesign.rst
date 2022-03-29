.. _apple_codesign_rcodesign:

===================
Using ``rcodesign``
===================

The ``rcodesign`` executable provided by this project provides a command
mechanism to interact with Apple code signing.

Signing with ``sign``
=====================

The ``rcodesign sign`` command can be used to sign a filesystem
path.

Unless you want to create an ad-hoc signature on a Mach-O binary, you'll
need to tell this command what code signing certificate to use.

To sign a Mach-O executable::

    rcodesign sign \
      --p12-file developer-id.p12 --p12-password-file ~/.certificate-password \
      --code-signature-flags runtime \
      path/to/executable

To sign an ``.app`` bundle (and all Mach-O binaries inside)::

   rcodesign sign \
     --p12-file developer-id.p12 --p12-password-file ~/.certificate-password \
     path/to/My.app

To sign a DMG image:

   rcodesign sign \
     --p12-file developer-id.p12 --p12-password-file ~/.certificate-password \
     path/to/app.dmg

To sign a ``.pkg`` installer::

   rcodesign sign \
    --p12-file developer-id-installer.p12 --p12-password-file ~/.certificate-password \
    path/to/installer.pkg

Notarizing and Stapling
=======================

You can notarize a signed asset via ``rcodesign notarize``.

Notarization requires an Apple Connect API Key. See
:ref:`apple_codesign_apple_connect_api_key` for instructions on how
to obtain one.

Notarization also requires Apple's Transporter tool. See
:ref:`apple_codesign_transporter` for more about Transporter. The
``rcodesign find-transporter`` command can be used to see if ``rcodesign``
can find Transporter.

You will need an API Key ``AuthKey_<ID>.p8`` file on disk in one of the
default locations used by Apple Transporter. These are
``$(pwd)/private_keys/``, ``~/private_keys/``, ``~/.private_keys/``, and
``~/.appstoreconnect/private_keys/``.

You need to provide both the Key ID and IssuerID when invoking this command.
Both can be found at https://appstoreconnect.apple.com/access/api.

To notarize an already signed asset::

    rcodesign notarize \
      --api-issuer 68911d4c-110c-4172-b9f7-b7efa30f9680 \
      --api-key DEADBEEF \
      path/to/file/to/notarize

By default ``notarize`` just uploads the asset to Apple. To wait
on its notarization result, add ``--wait``::

    rcodesign notarize \
      --api-issuer 68911d4c-110c-4172-b9f7-b7efa30f9680 \
      --api-key DEADBEEF \
      --wait \
      path/to/file/to/notarize

Or to wait and automatically staple the file if notarization was successful::

    rcodesign notarize \
      --api-issuer 68911d4c-110c-4172-b9f7-b7efa30f9680 \
      --api-key DEADBEEF \
      --staple \
      path/to/file/to/notarize

If notarization is interrupted or was initiated on another machine and you
just want to attempt to staple an asset that was already notarized, you
can run ``rcodesign staple``. e.g.::

    rcodesign staple \
      --api-issuer 68911d4c-110c-4172-b9f7-b7efa30f9680 \
      --api-key DEADBEEF \
      path/to/file/to/staple

Comparing Behavior with Apple Tooling
=====================================

``rcodesign`` strives to behave similarly to Apple's official ``codesign``, ``notarytool``,
``stapler``, and other similar tools. However, the operations these tools perform is subtly
complex and there will be bugs in this tool's implementation.

The ``rcodesign print-signature-info`` command can be used to dump YAML
describing any signable file entity. Just point it at a Mach-O, bundle, DMG,
or ``.pkg`` installer and it will tell you what it knows about the entity.

The ``rcodesign diff-signatures`` command will internally execute
``print-signature-info`` against 2 paths and print the difference between them.
This command is exceptionally useful at understanding how ``rcodesign`` varies in
behavior from the canonical Apple tools.

.. important::

   Including the output of ``print-signature-info`` or ``diff-signatures`` in bug
   reports is exceptionally useful in bug reports against this project.

Using Hardware Devices for Signing
==================================

Version 0.11 of this project introduced initial support for leveraging
smart cards for signing.

Only support for YubiKeys is tested and only YubiKeys may work because the
hardware integration is currently implemented using the
`yubikey.rs <https://github.com/iqlusioninc/yubikey.rs>` project.

To see if your smartcard device is recognized and certificates can be found::

    rcodesign scan-smartcards
    Device 0: Yubico YubiKey OTP+FIDO+CCID 0
    Device 0: Serial: 12345678
    Device 0: Version: 5.2.7
    Device 0: Certificate in slot Signature / 9c
    Subject CN:                  gps
    Issuer CN:                   gps
    Subject is Issuer?:          true
    Team ID:                     <missing>
    SHA-1 fingerprint:           c847e830c01845517d7e3775805ab56313aa11c8
    SHA-256 fingerprint:         7c0bc8fe1a2d7831ca0b0787dc6d5c28c6f562c2723a7eaaab42d39e7a3b7924
    Signed by Apple?:            false
    Guessed Certificate Profile: none
    Is Apple Root CA?:           false
    Is Apple Intermediate CA?:   false
    Apple CA Extension:          none
    Apple Extended Key Usage Purpose Extensions:
    Apple Code Signing Extensions:

If a certificate is found, you can pass ``--smartcard-slot`` to ``rcodesign sign``
to use the hardware device for signing::

    rcodesign sign \
        --smartcard-slot 9c \
        path/to/entity/to/sign

Smartcards often require a PIN on signing operations. You should be prompted
for your PIN value if the signing operation is initially unauthenticated.
