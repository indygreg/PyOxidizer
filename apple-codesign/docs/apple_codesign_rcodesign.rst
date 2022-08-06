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

Notarization requires an App Store Connect API Key. See
:ref:`apple_codesign_app_store_connect_api_key` for instructions on how
to obtain one.

Assuming you used ``rcodesign encode-app-store-connect-api-key`` to produce
a JSON file with all the API Key information, simply specify ``--api-key-path``
to define the path to this JSON file.

To notarize an already signed asset::

    rcodesign notarize \
      --api-key-path ~/.appstoreconnect/key.json \
      path/to/file/to/notarize

By default ``notarize`` just uploads the asset to Apple. To wait
on its notarization result, add ``--wait``::

    rcodesign notarize \
      --api-key-path ~/.appstoreconnect/key.json \
      --wait \
      path/to/file/to/notarize

Or to wait and automatically staple the file if notarization was successful::

    rcodesign notarize \
    --api-key-path ~/.appstoreconnect/key.json \
      --staple \
      path/to/file/to/notarize

If notarization is interrupted or was initiated on another machine and you
just want to attempt to staple an asset that was already notarized, you
can run ``rcodesign staple``. e.g.::

    rcodesign staple path/to/file/to/staple

.. tip::

   It is possible to staple any asset, not just those notarized by you.
