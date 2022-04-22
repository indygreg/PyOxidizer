.. _apple_codesign_certificate_management:

==================================
Managing Code Signing Certificates
==================================

In order to add cryptographic signatures using this tool, you'll need to use
a :ref:`apple_codesign_code_signing_certificate`. (Follow the link for what
that means.)

In order to perform code signing in a way that is recognized and trusted by Apple
operating systems, you will need to obtain a code signing certificate that is
signed/issued by Apple. This requires joining the
`Apple Developer Program <https://developer.apple.com/programs/>`_, which has an
annual membership fee.

Once you are a member, there are various ways to generate and manage your
certificates. But first, a primer about flavors of Apple code signing
certificates.

Apple Code Signing Certificate Flavors
======================================

Apple issues different types/flavors of code signing certificates. Each one is
used to sign a different class of software.

If you are logged into your Apple Developer account, you can see Apple's
description for these at https://developer.apple.com/account/resources/certificates/add.
Here's our concise definitions:

*Apple Development*
  Sign applications for Apple operating systems that aren't distributed publicly.

*Apple Distribution*
   Sign applications for submission to the App Store or for Ad Hoc distribution.

*iOS App Development*
   Legacy version of *Apple Development* just for iOS apps. (We think.)

*iOS Distribution*
   Legacy version of *Apple Distribution* just for iOS apps. (We think.)

*Mac Development*
   Legacy version of *Apple Development* just for macOS apps. (We think.)

*Mac App Distribution*
   Sign macOS applications and configure a Distribution Provisioning Profile
   for distribution through Mac App Store.

*Mac Installer Distribution*
   Sign package installers (e.g. ``.pkg`` files) which will be distributed via the
   Mac App Store.

*Developer ID Installer*
   Sign package installers (e.g. ``.pkg`` files) which will be distributed outside
   the Mac App Store. i.e. if users fetch your installer via your website, you sign
   with this.

*Developer ID Application*
   Sign applications which will be distributed outside the Mac App Store. Used for
   signing Mach-O binaries, ``.app`` bundles, and ``.dmg`` files.

Essentially, if you are distributing macOS software to end-users via non-Apple
channels like your website, you need *Developer ID Application* and/or *Developer ID
Installer*.

If you are distributing via Apple's App stores, you need *Apple Distribution* or one
of the other types having *Distribution* in the name.

.. tip::

   The ``rcodesign analyze-certificate`` command can be used to print information
   about Apple code signing certificates. Look for a line with ``Certificate Profile``
   in its output to see which flavor of certificate this software thinks it is.

Generating Certificates with Xcode
==================================

Using Xcode from macOS is probably the easiest way to create and manage
your certificates as Xcode has built-in UI to facilitate this.

Apple keeps thorough
`documentation about how to do this <https://help.apple.com/xcode/mac/current/#/dev154b28f09>`_.
Please follow Apple's documentation to generate a certificate.

Obtaining a Certificate via a Certificate Signing Request
=========================================================

You can obtain a code signing certificate by uploading a *Certificate Signing
Request (CSR)* to Apple. Essentially, you generate a CSR, send it to Apple,
and Apple will issue a new code signing certificate which you can download.

A CSR is produced by creating a cryptographic signature (using a *private
key*) over a small set of metadata describing the *private key* for which
a certificate shall be issued.

In order to generate a CSR, you need a *private key*. As of April 2022, Apple
appears to require the use of RSA 2048 private keys.

If you have access to macOS, the easiest way to generate a private key and
CSR is to use ``Keychain Access`` using the
`procedure outlined here <https://help.apple.com/developer-account/#/devbfa00fef7>`_.

If you want to generate your own CSR using ``rcodesign``, you can! First,
you'll need a private key.

To generate an RSA 2048 private key using OpenSSL::

   openssl genrsa -out private.pem 2048

.. warning::

   The RSA private key will be in plain text on your filesystem. This is not
   very secure!

Then once you have a private key, we can generate a CSR using ``rcodesign``::

    rcodesign generate-certificate-signing-request --pem-source private.pem
    rcodesign generate-certificate-signing-request --p12-file key.p12

    # Smart cards require generating a new key then creating a CSR from that key.
    rcodesign smartcard-generate-key --smartcard-slot 9c
    rcodesign generate-certificate-signing-request --smartcard-slot 9c

This command will print the CSR to stdout. e.g.::

    -----BEGIN CERTIFICATE REQUEST-----
    MIHeMIGDAgEAMCExHzAdBgNVBAMMFkFwcGxlIENvZGUgU2lnbmluZyBDU1IwWTAT
    BgcqhkjOPQIBBggqhkjOPQMBBwNCAAQxluBlPIv/HgBDz0O3GLPhhna/NJU7menq
    GzUc9sZFOgZ7XmpR9vQTxHPEyg5D6huBapVQZsDG9IgAXjvSOmimoAAwDAYIKoZI
    zj0EAwIFAANIADBFAiEAoZpbfrlm7HgQXByfwuoPt7/V+QM7DCIILcTKCBrkIZUC
    IEIp8yA9bSg7bM9XJl8bgFesTjermlSYQI/2JY834/z7
    -----END CERTIFICATE REQUEST-----

You probably want to use ``--csr-pem-path`` to write that to a file automatically::

   rcodesign generate-certificate-signing-request --smartcard-slot 9c --csr-pem-path csr.pem

.. _apple_codesign_exchange_csr:

Exchanging a CSR for a Code Signing Certificate
-----------------------------------------------

Once you have a CSR file, you can attempt to exchange it for a code signing
certificate.

1. Go to https://developer.apple.com/account/resources/certificates/add (you must be
   logged into Apple's website)
2. Select the certificate *flavor* you want to issue.
3. Click ``Continue`` to advance to the next form.
4. Select the ``G2 Sub-CA (Xcode 11.4.1 or later)`` *Profile Type* (we support it).
5. Choose the file containing your CSR.
6. Click ``Continue``.
7. If all goes according to plan, you should see a page saying ``Download Your
   Certificate``.
8. Click the ``Download`` button.
9. Save the certificate somewhere. (The file content is likely not sensitive and
   doesn't need to be kept secret because this content will be copied to everything
   you sign with it!)

At this point, you have both a *private key* and a *public certificate*: you can
sign Apple software!

Exporting a Code Signing Certificate to a File
==============================================

``rcodesign`` supports consuming code signing certificates from multiple
sources, including hardware devices. But sometimes it is desirable to have
your code signing certificate exist as a file.

Use the instructions in one of the following sections to export a code signing
certificate.

Using Keychain Access
---------------------

(macOS)

1. Open the ``Keychain Access`` application.
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
-----------

(macOS)

See `Apple's Xcode documentation <https://help.apple.com/xcode/mac/current/#/dev154b28f09>`_.

Using ``security``
------------------

(macOS)

1. Run ``security find-identity`` to locate certificates available for export.
2. Run ``security export -t identities -f pkcs12 -o keys.p12``

If you have multiple identifies (which is common), ``security export`` will export
all of them. ``security`` doesn't seem to have a command to export just a single
certificate pair. You will need to invoke some ``openssl`` command to extract
just the certificate you care about. Please contribute back a fix for this
documentation once you figure it out!

Using a Self-Signed Certificate
===============================

If you want to cut some corners and play around with certificates not
signed by Apple, you can run ``rcodesign generate-self-signed-certificate``
to generate a self-signed code signing certificate.

This command will include special attributes in the certificate that indicate
compatibility with Apple code signing. However, since the certificate isn't
signed by Apple, its signatures won't confer the same trust that Apple signed
certificates would.

These certificates can be useful for debugging and testing.
