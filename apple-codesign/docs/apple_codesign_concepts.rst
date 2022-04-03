.. _apple_codesign_concepts:

========
Concepts
========

Code signing on Apple platforms is complex and has many parts. This
document aims to shed some light on things.

Cryptographic Signatures
========================

At the heart of code signing is the use of cryptographic signatures.

The Wikipedia article on
`digital signatures <https://en.wikipedia.org/wiki/Digital_signature>`_ explains
the concept in far more detail than we care to go into.

Essentially, mathematics is used to prove that an entity in possession of a
secret *key* digitally attested to the existence of some *signed* entity.

More concretely, an X.509 code signing certificate can be proved to have
signed some piece of software by inspecting the cryptographic signature it
produced.

Apple's cryptographic signatures use RFC 5652 / Cryptographic Message Syntax
(CMS) for representing signatures. This standardized format is used outside
the Apple ecosystem and libraries and tools like OpenSSL are capable of
interfacing with it.

Code Signing
============

*Code signing* (or just *signing*) is the mechanism of producing (and then
attaching) a signature to some entity.

Typically signing entails producing a cryptographic signature using a code
signing certificate. However, Mach-O files (the binary file format for
Apple platforms) has a concept of *ad-hoc* signing where the binary has
data structures describing the content of the binary but without the
cryptographic signature present.

Notarization
============

*Notarization* is the term Apple gives to the process of uploading an asset
to Apple for inspection.

In order to help safeguard and control their software ecosystems, Apple
imposes requirements that applications and installers be inspected by Apple
before they are allowed to run on Apple operating systems - either at all
or without scary warning signs.

When you notarize software, you are essentially asking for Apple's blessing
to distribute that software. If Apple's systems are appeased, they will
issue a *notarization ticket*.

Notarization Ticket
===================

A *notarization ticket* is a blob of data that essentially proves that Apple
notarized a piece of software.

The exact format and content of *notarization tickets* is not well known. But
they do contain some DER-encoded ASN.1 with data structures that common appear
in X.509 certificates. All that matters is that Apple's operating systems know
how to read and validate a notarization ticket.

Stapling
========

*Stapling* is the term Apple gives to the process of attaching a *notarization
ticket* to some entity. It is literally just fetching a *notarization ticket*
from Apple's servers and then making that ticket available on the entity that
was notarized.

You can think of notarization and stapling as Apple-issued cryptographic
signatures. It establishes a chain of trust between some entity to you
that also had to be inspected by Apple first.

Mach-O Binaries
===============

`Mach-O <https://en.wikipedia.org/wiki/Mach-O>`_ is the binary executable
file format used on Apple operating systems.

When you run an executable like ``/usr/bin/zsh`` on macOS, you are running
a Mach-O file.

Mach-O binaries are either *thin* or *fat*. A *thin* Mach-O contains code
for a single architecture, like x86-64 or aarch64 / arm64. A *fat* or
*universal* binary contains code for multiple architectures. At run-time,
the operating system will decide which one to execute.

Bundles
=======

`Bundles <https://developer.apple.com/library/archive/documentation/CoreFoundation/Conceptual/CFBundles/Introduction/Introduction.html#//apple_ref/doc/uid/10000123i-CH1-SW1>`_
are a filesystem based mechanism for encapsulating code and resources.

On macOS, you commonly encounter bundles as ``.app`` and ``.framework``
directories in ``/Applications`` and ``/System/Library/Frameworks``.

Bundles are essentially a well-defined set of files that the operating
system knows how to interact with. For example, macOS knows that to
execute an ``.app`` bundle it should look for a ``Contents/Info.plist``
to resolve basic application metadata, such as the name of the main
binary for the bundle, which resides in ``Contents/MacOS/`` within the
bundle.

DMGs / Disk Images
==================

`Apple Disk Images <https://en.wikipedia.org/wiki/Apple_Disk_Image>`_ are a
self-contained file format for holding filesystems. Think of DMGs
as standalone hard drives that Apple operating systems can recognize.

DMGs are often used to distribute macOS applications.

XARs / Flat Packages / ``.pkg`` Installers
==========================================

*Flat packages* is a mechanism for installing software.

They take the form of ``.pkg`` files, which are actually XAR archives
(a tar-like format for storing content for multiple files within a single
file).

.. _apple_codesign_code_signing_certificate:

Code Signing Certificate
========================

A code signing certificate is used to produce cryptographic signatures over
some signed entity.

A code signing certificate consists of a private/secret key (essentially a bunch
of large numbers or parameters) and a public certificate which describes it.

Code signing certificates are X.509 certificates. X.509 certificates are the
same technology used to secure communication with https:// websites. However,
the certificates are used for signing content instead of encrypting it.

The X.509 public certificate contains a bunch of metadata describing the
certificate. This includes the name of the person or entity it belongs to,
a date range for when it is valid, and a cryptographic signature attesting
to its origination.

Apple's operating systems look for special metadata on code signing
certificates to authenticate and trust them. There are special properties
on certificates indicating what Apple software distribution they are allowed
to perform. For example, a ``Developer ID Application`` certificate is required
for signed Mach-O binaries, bundles, and DMG files to be trusted and a
``Developer ID Installer`` certificate is required to sign ``.pkg`` installers
in order for them to be trusted.

In addition, different Apple code signing certificates are cryptographically
signed by different Apple Certificate Authorities (CAs).
