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

Follow the instructions at :ref:`apple_codesign_certificate_management` to obtain
a code signing certificate.

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
