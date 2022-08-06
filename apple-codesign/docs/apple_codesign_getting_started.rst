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

To enable smart card integration:

.. code-block:: bash

    cargo install --features smartcard apple-codesign

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

.. _apple_codesign_app_store_connect_api_key:

Obtaining an App Store Connect API Key
======================================

To notarize and staple, you'll need an Apple Connect API Key to
authenticate connections to Apple's servers.

You can generate one at https://appstoreconnect.apple.com/access/api.

This requires an Apple Developer account, which requires paying money. You may
need to click around in the App Store Connect website to enable the API keys
feature.

See
https://developer.apple.com/documentation/appstoreconnectapi/creating_api_keys_for_app_store_connect_api
for Apple's official documentation on creating these API Keys.

.. important::

   For the *Access Role*, ``Developer`` should be sufficient.

   Other roles may or may not work for notarization.

App Store Connect API Keys have 3 components:

* An *Issuer ID* (likely a UUID).
* A *Key ID* (an alphanumeric string like ``DEADBEEF42``).
* A PEM encoded ECDSA private key (a file beginning with
  ``-----BEGIN PRIVATE KEY-----`` that you can download at most
  once when you create an API Key).

All 3 of these components are required to talk to the App Store Connect
API server. To make management of these keys simpler, we provide the
``encode-app-store-connect-api-key`` command to write out a JSON document
holding all the key info.

.. important::

   We highly recommend using our JSON keys created with
   ``encode-app-store-connect-api-key`` as it is simpler to manage a single
   entity instead of 3.

You can perform an encode of your key as follows:

.. code-block:: bash

   rcodesign encode-app-store-connect-api-key -o ~/.appstoreconnect/key.json \
     <issuer-id> <key-id> /path/to/downloaded/private_key

e.g.

.. code-block:: bash

   rcodesign encode-app-store-connect-api-key -o ~/.appstoreconnect/key.json \
     11dda589-8632-49a8-a432-03b5e17fe1d2 DEADBEEF42 ~/Downloads/AuthKey_DEADBEAF42.p8
