.. _apple_codesign_smartcard:

==================
Smart Card Support
==================

This project has some support for integrating with Smart Cards. This
enables you to perform cryptographic signing using a certificate that
is stored in a hardware device.

Certificates stored this way are more secure, as it typically requires
that a physical device be unlocked in order to use the private key. And
access to the raw private key matter is typically not allowed.

Limitations
===========

We currently use `yubikey.rs <https://github.com/iqlusioninc/yubikey.rs>`_ for
smart card integration. This likely means that only YubiKeys currently work.

However, we would like to switch to a more generic interface (such as
`pcsc <https://crates.io/crates/pcsc/2.7.0>`_ in the future to allow more flexible
usage.

There is currently no support for setting the management key. If you have
set a custom management key, you won't be able to import certificates onto
your smart card. However, signing should still work.

Validating Smart Card Integration
=================================

To see if your smart card device is recognized and certificates can be found::

    rcodesign smartcard-scan
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

Pointing Commands at a Smart Card Certificate
=============================================

``rcodesign`` command that operate against certificates expose a
``--smartcard-slot`` argument to specify which smartcard slot to use.

Slot ``9c`` is the standard slot for holding certificates used for
signing.

To sign with your smart card certificate at slot ``9c``, do something like::

    rcodesign sign \
        --smartcard-slot 9c \
        path/to/entity/to/sign

Smartcards often require a PIN on signing operations. You should be prompted
for your PIN value if the signing operation is initially unauthenticated.

Importing Certificates Into a Smart Card
========================================

The ``rcodesign smartcard-import`` command can be used to import an existing
code signing certificate into your smart card.

Let's assume you created an Apple code signing certificate and exported it
to the file ``developer_id.p12``. You can import this certificate by doing
the following::

    $ rcodesign smartcard-import \
        --smartcard-slot 9c \
        --p12-file developer_id.p12 --p12-password password

    $ rcodesign smartcard-scan
    Device 0: Yubico YubiKey OTP+FIDO+CCID 0
    Device 0: Serial: 1234567
    Device 0: Version: 5.2.7
    Device 0: Certificate in slot Signature / 9c
    Subject CN:                  Developer ID Application: Gregory Szorc (MK22MZP987)
    Issuer CN:                   Developer ID Certification Authority
    Subject is Issuer?:          false
    Team ID:                     MK22MZP987
    SHA-1 fingerprint:           44d7155bcabf3b9a9221b01b8e198040ae04e0ad
    SHA-256 fingerprint:         8f610de4caea4bc138e85b56726ed4d330f7464d99cfa5957568904b6a6375ec
    Signed by Apple?:            true
    Apple Issuing Chain:
      - Developer ID Certification Authority
      - Apple Root CA
      - Apple Root Certificate Authority
    Guessed Certificate Profile: DeveloperIdApplication
    Is Apple Root CA?:           false
    Is Apple Intermediate CA?:   false
    Apple CA Extension:          none
    Apple Extended Key Usage Purpose Extensions:
      - 1.3.6.1.5.5.7.3.3 (CodeSigning)
    Apple Code Signing Extensions:
      - 1.2.840.113635.100.6.1.33 (DeveloperIdDate)
      - 1.2.840.113635.100.6.1.13 (DeveloperIdApplication)

Creating a Certificate with a Private Key Exclusive to the Smart Card
=====================================================================

It is possible to generate a private key directly on the smart card and create
a code signing certificate derived from this private key.

Code signing certificates created this way are theoretically much more secure
than other private key generation methods because most smart cards never allow the
private key content to be exported/viewed. Assuming operations involving the
private key are protected with the appropriate access protections (like pin or
touch policies), compromise of the machine or even the smart key itself may not
result in unwanted access to the private key.

To create a code signing certificate whose private key has never left the
smart card device itself, do something like the following.

First, generate a new private key on the smart card::

    rcodesign smartcard-generate-key --smartcard-slot 9c

Then create a certificate signing request (CSR)::

    rcodesign generate-certificate-signing-request \
        --smartcard-slot 9c \
        --csr-pem-path csr.pem

Then follow the instructions at :ref:`apple_codesign_exchange_csr` to submit the
CSR file to Apple and obtain a *public certificate*.

Finally, import the Apple-issued public certificate into the smart card::

    rcodesign smartcard-import \
        --der-source developerID_application.cer \
        --smartcard-slot 9c

At this point, the smart card is ready to sign using an Apple issued certificate
and the private key never has - and probably never will - leave the smart card
itself.
