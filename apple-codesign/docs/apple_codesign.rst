.. _apple_codesign:

==================
Apple Code Signing
==================

The ``apple-codesign`` Rust crate and its corresponding ``rcodesign`` CLI
tool implement code signing for Apple platforms.

We believe this crate provides the most comprehensive implementation of Apple
code signing outside the canonical Apple tools. We have support for the following
features:

* Signing Mach-O binaries (the executable file format on Apple operating systems).
* Signing, notarizing, and stapling directory bundles (e.g. `.app` directories).
* Signing, notarizing, and stapling XAR archives / `.pkg` installers.
* Signing, notarizing, and stapling DMG disk images.

What this all means is that you can sign, notarize, and release Apple software
from Linux and Windows without needing access to proprietary Apple software.

.. toctree::
   :maxdepth: 2

   apple_codesign_getting_started
   apple_codesign_rcodesign
   apple_codesign_certificate_management
   apple_codesign_smartcard
   apple_codesign_concepts
   apple_codesign_quirks
   apple_codesign_gatekeeper
   apple_codesign_custom_assessment_policies
