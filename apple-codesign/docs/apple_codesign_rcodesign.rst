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
      --pfx-file developer-id.p12 --pfx-password-file ~/.certificate-password \
      --code-signature-flags runtime \
      path/to/executable

To sign an ``.app`` bundle (and all Mach-O binaries inside)::

   rcodesign sign \
     --pfx-file developer-id.p12 --pfx-password-file ~/.certificate-password \
     path/to/My.app

To sign a DMG image:

   rcodesign sign \
     --pfx-file developer-id.p12 --pfx-password-file ~/.certificate-password \
     path/to/app.dmg

To sign a ``.pkg`` installer::

   rcodesign sign \
    --pfx-file developer-id-installer.p12 --pfx-password-file ~/.certificate-password \
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
