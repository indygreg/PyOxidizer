.. _apple_codesign_getting_started:

===============
Getting Started
===============

Installing
==========

To install the latest release version of the ``rcodesign`` executable using Cargo
(Rust's package manager):

.. code-block:: bash

    cargo install apple-codesign


To compile and run from a Git checkout of its canonical repository (developer mode):

.. code-block:: bash

    cargo run --bin rcodesign -- --help

To install from a Git checkout of its canonical repository:

.. code-block:: bash

    cargo install --bin rcodesign

To install from the latest commit in the canonical Git repository:

.. code-block:: bash

    cargo install --git https://github.com/indygreg/PyOxidizer --branch main rcodesign

Obtaining a Code Signing Certificate
====================================

In order to perform code signing in a way that is recognized and trusted by Apple
operating systems, you will need to obtain a code signing certificate itself
signed/issued by Apple.

This requires joining the
`Apple Developer Program <https://developer.apple.com/programs/>`_, which has an
annual membership fee.

Once you are a member, from a Mac you can log in to Xcode and create and
manage your certificates using
`Xcode's documentation <https://help.apple.com/xcode/mac/current/#/dev154b28f09>`_.

For signing macOS software, you'll need a ``Developer ID Application``
certificate for signing Mach-O binaries, bundles, and ``.dmg`` images.
For ``.pkg`` installers, you'll need a ``Developer ID Installer`` certificate
(if distributing the ``.pkg`` outside the App Store) and a ``Mac Installer
Distribution`` certificate if distributing via the App Store.

If you want to cut some corners and play around with certificates not
signed by Apple, you can run ``rcodesign generate-self-signed-certificate``
to generate a self-signed code signing certificate. This command will
include special attributes in the certificate that indicate compatibility
with Apple code signing. However, since the certificate isn't signed by
Apple, its signatures won't confer the same trust that Apple signed
certificates would.

Please also note the existence of ``rcodesign analyze-certificate`` for
printing information about code signing certificates.

Exporting a Code Signing Certificate to a File
----------------------------------------------

``rcodesign`` currently requires the signing certificate to exist as a
local file. Use the instructions in one of the following sections to
export a code signing certificate.

Using Keychain Access
^^^^^^^^^^^^^^^^^^^^^

1. Open ``Keychain Access``.
2. Find the certificate you want to export and command click or right click on it.
3. Select the ``Export`` option.
4. Choose the ``Personal Information Exchange (.p12)`` format and select a
   file destination.
5. Enter a password used to protect the contents of the certificate.
6. If prompted to enter your system password to unlock your keychain, do so.

The exported certificate is in the PKCS#12 / PFX / p12 file format. Command
arguments with these labels in the same can be used to interact with the
exported certificate.

Using Xcode
^^^^^^^^^^^

See `Apple's Xcode documentation <https://help.apple.com/xcode/mac/current/#/dev154b28f09>`_.

Using ``security``
^^^^^^^^^^^^^^^^^^

1. Run ``security find-identity`` to locate certificates available for export.
2. Run ``security export -t identities -f pkcs12 -o keys.p12``

If you have multiple identifies (which is common), ``security export`` will export
all of them. ``security`` doesn't seem to have a command to export just a single
certificate pair. You will need to invoke some ``openssl`` command to extract
just the certificate you care about. Please contribute back a fix for this
documentation once you figure it out!

.. _apple_codesign_transporter:

Installing Apple Transporter for Notarization
=============================================

Notarization requires using Apple Transporter for uploading artifacts to
Apple for notarization. This (Java) tool is distributed for macOS, Windows,
and Linux.

You can install it by following
`Apple's instructions <https://help.apple.com/itc/transporteruserguide/#/apdAbeb95d60>`_.

If you do not want to perform notarization, you do not need to install
Apple Transporter.

.. _apple_codesign_apple_connect_api_key:

Obtaining an Apple Connect API Key
==================================

To notarize and staple, you'll need an Apple Connect API Key to
authenticate connections to Apple's servers.

You can generate one at https://appstoreconnect.apple.com/access/api.

This requires an Apple Developer account, which requires paying money. You may
need to click around in the App Store Connect website to enable the API keys
feature.

Apple Transporter looks in various locations for the API Key. Run ``iTMSTransporter
-help upload`` and read the docs for the ``-apiKey`` argument.

We recommend putting the keys in ``~/.appstoreconnect/private_keys/`` because that
is a descriptive directory name.
