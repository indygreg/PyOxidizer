// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! Apple code signing technical specifications

This document outlines how Apple code signing is implemented at a technical
level.

# High Level Overview

Mach-O binaries embed an optional binary blob containing code signing
metadata. This binary blob contains content digests of various aspects
of the binary (such as the executable code) as well as an optional
cryptographic signature which effectively attests to the digested
content of the binary.

At run-time, stored digests are used to help ensure file integrity.

The cryptographic signature is used to verify the digests haven't
been tampered with as well as to validate trust with the entity that
produced that signature.

See
<https://developer.apple.com/library/archive/technotes/tn2206/_index.html#//apple_ref/doc/uid/DTS40007919>
for an additional overview of how code signing works on Apple platforms.

# The Important Data Structures

Mach-O is the executable binary format used by Apple platforms. A
Mach-O binary contains (among other things), a series of named *segments*
holding arbitrary data and *load commands* instructing the loader how
to load/execute the binary.

Code signing data is embedded within the `__LINKEDIT` segment in a Mach-O
binary. An `LC_CODE_SIGNATURE` load command identifies the offsets of
code signing data within `__LINKEDIT`.

The code signing data within a `__LINKEDIT` segment is itself a collection
of sub-records. A *SuperBlob* header defines the signing data format, the
length of data to follow, and the number of sub-sections, or *Blob* within.
Each *Blob* occupies a defined *slot*. *Slots* are effectively well-known
pieces of signing data. These include a *Code Directory*, *Entitlements*,
and a *Signature*, among others. See the [crate::CodeSigningSlot]
enumeration for the known defined slots.

Each *Blob* contains its own header magic effectively identifying the
content type within and how bytes should be interpreted. The magic
values are independent of the *slot* type. However, there appears to be
a relationship between the two. For example, the code directory slot
will have header magic identifying the payload as a code directory structure.

The *Code Directory* blob/slot defines information about the binary
being signed. There are many fields to this data structure. But the most
important ones to understand are the hashes / content digests. The *Code
Directory* contains digests (e.g. SHA-256) of various content in the binary,
such as Mach-O segment data (i.e. the executable code) and other blobs/slots.

The *Entitlements* blob/slot contains a *plist*.

Additional file-based resources can also be signed. These are referred to as
*Code Resources*. *Code Resources* are captured in a
`_CodeSignature/CodeResources` XML plist file in the bundle and the digest
of this file is captured by the *Code Directory*. There is a defined
`RESOURCEDIR` slot to hold its digest. However, there is no explicit
magic constant for resources, implying that this data can only be provided
externally and not embedded within the *SuperBlob*.

The *Signature* blob/slot contains a Cryptographic Message Syntax (CMS)
RFC 5652 defined `SignedData` BER encoded ASN.1 data structure. CMS is
a specification for cryptographically signing arbitrary content. The
`SignedData` structure contains an additional set of *signed attributes*
(think of it as arbitrary extra content to sign), a cryptographic signature
of the signed data, and likely the X.509 certificate of the signer and its
chain of certificate signers.

# How Signing Works

Code signing logically consists of the following steps:

1. Collecting content that needs to be signed/attested/trusted.
2. Computing content digests.
3. Cryptographically signing a message derived from the content digests.
4. Adding signature data to Mach-O binary.

## Collecting Content

Embedded code signatures support signing a myriad of data formats.
These include but aren't limited to:

* The Mach-O data outside the signature data in the `__LINKEDIT` segment.
* Requested entitlements for the binary.
* A code requirement statement / expression.
* Resource files.

If your binary is already part of a *bundle*, content collection can
occur automatically using heuristics. e.g. the `Contents/Resources`
directory contains additional files whose content should be signed.

## Computing Content Digests

Once content has been assembled, a series of digests are computed.

For the code digests, the Mach-O segments are iterated. The raw segment
data is chunked into *pages* and each hashed separately. This is to allow
code data to be lazily hashed as a page is loaded into the kernel.
(Otherwise you would have to hash often megabytes on process start, which
would add overhead.)

Code hashes are a bit nuanced. A hash is emitted at segment boundaries. i.e.
hashes don't span across multiple segments. The `__PAGEZERO` segment is
not hashed. The `__LINKEDIT` segment is hashed, but only up to the start
offset of the embedded signature data, if present.

Other content (such as the entitlements, code requirement statement, and
resource files) are serialized to *Blob* data. The mechanism for this
varies by type. e.g. the entitlements plist is embedded as UTF-8
data and the code requirement statement is serialized into an expression
tree. The resulting *Blob* is then digested.

The content digests are then assembled into a *Code Directory* data
structure. Digests of code data are referred to to *code slots* and
digests of other entitles (namely *Blob* data) occupy *special slots*.
The *Code Directory* also contains important other information, such
as describing the hash/digest mechanism used, the page size for code
hashing, and executable limits for the binary.

The content of the *Code Directory* serialized to a *Blob* is then itself
digested. This value is known as the *code directory hash*.

## Cryptographic Signing

A cryptographic signature is produced using the Cryptographic Message
Syntax (CMS) signing mechanism.

From a high level, CMS takes as inputs:

* Optional content to sign.
* Optional set of additional attributes (effectively key-value data) to sign.
* A signing key.
* Information about the signing key (including its CA chain).

From these, CMS will produce a BER encoded ASN.1 blob containing the
cryptographic signature and sufficient metadata to verify it (such
as the signed attributes and information about the signing certificate).

In CMS speak, the *encapsulated content* being signed is not defined.
However, the `message-digest` signed attribute is the digest of the
*Code Directory* *Blob* data. (This appears to be not compliant with RFC 5652,
which says *encapsulated content* should be present in the *SignedObject*
structure. Omitting the data is likely done to avoid redundant storage
of this data in the Mach-O binary and/or to simplify parsing, as *Code
Directory* data wouldn't be embedded within an ASN.1 stream.)

In addition, there is a signed attribute for the signing time. There is
also an XML plist defining an array of base64 encoded *Code Directory*
hashes. There are multiple *slots* in a *SuperBlob* for code directories
and the array in the signed XML plist appears to allow hashes of all of
them to be recorded.

(TODO it isn't clear what the signed content is when there are multiple
*Code Directory* slots in use. Presumably `message-digest` is computed
over all of them.)

CMS will concatenate the *Code Directory* data with the DER serialized
ASN.1 structures defining the *signed attributes*. This becomes the
*plaintext* message to be signed.

This *plaintext* message is combined with a private key and cryptographically
signed (likely using RSA). This produces a *signature*.

CMS then serializes the *signature*, *signed attributes*, signer
certificate info, and other important metadata to a BER encoded ASN.1
data structure. This raw slice of bytes is referred to as the
*embedded signature*.

## Adding Signature Data to Mach-O Binary

The above steps have already materialized several *Blob* data
structures. The individual pieces like the entitlements and code requirement
*Blob* were materialized in order to compute their hashes for the *Code
Directory* data structure. And the *Code Directory* *Blob* was constructed
so it could be signed by CMS.

The *embedded signature* data produced by CMS is assembled into a *Blob*
structure. At this point, we have all the *Blob* ready.

All the *Blobs* are assembled together into a *SuperBlob*. The
*SuperBlob* is then written to the `__LINKEDIT` segment of the
Mach-O binary. An appropriate `LC_CODE_SIGNATURE` load command is
also written to the Mach-O binary to instruct where the *SuperBlob*
data resides.

The `__LINKEDIT` segment is the last segment in the Mach-O binary and
the *SuperBlob* often occupies the final bytes of the `__LINKEDIT`
segment. So in many cases adding code signature data to a Mach-O
requires an optional truncation to remove the existing signature then
file appends for the `__LINKEDIT` data.

However, insertion or removal of `LC_CODE_SIGNATURE` will require
rewriting the entire file and adjusting offsets in various Mach-O
data structures accordingly. In many cases, an existing code signature
can be replaced by truncating the `__LINKEDIT` section, writing the
replacement data, and updating sizes/offsets in-place in the segments
index and `LC_CODE_SIGNATURE` load command.

Note that there is a chicken-and-egg problem related to writing the
Mach-O binary and computing the digests of that binary for the *Code
Directory*! The *Code Directory* needs to compute a digest over the
content of the Mach-O file up until the signature data. But this needs
to be done before a CMS signature is produced, as we need to digest
the *Code Directory* for a CMS signed attribute. We also need to know
the size of the CMS signature, as it is part of the signature data
embedded in the Mach-O binary and its size needs to be recorded in
the `LC_CODE_SIGNATURE` load command and segment definitions, which
are hashed by the *Code Directory*. This is a circular dependency. A
trick to working around it is to pad the Mach-O signature data with
extra NULLs and record this extra long value in `LC_CODE_SIGNATURE`
before code digests are computed. The *SuperBlob* parser appears to
be lenient about this solution. Further note that calculating the
exact final length before CMS signature generation may be impossible
due to the CMS signature being non-deterministic (due to the use of
signing times and timestamp servers tokens, which could be variable
length).

# How Bundle Signing Works

Signing bundles (e.g. `.app`, `.framework` directories) has its own
complexities beyond signing individual binaries.

Bundles consist of multiple files, perhaps multiple binaries. These files
can be classified as:

1. The main executable.
2. The `Info.plist` file.
3. Support/resources files.
4. Code signature files.

When signing bundles, the high-level process is the following:

1. Find and sign all nested binaries and bundles (bundles can contain
   other bundles) except the main binary and bundle.
2. Identify support/resources files and calculate their hashes, capturing
   this metadata in a `CodeResources` XML file.
3. Sign the main binary with an embedded reference to the digest of the
   `CodeResources` file.

# How Verification Works

What happens when a binary is loaded? Read on to find out.

Please note that we don't know for sure what all occurs when a binary is
loaded because the code is proprietary. We do have some high-level
documentation from Apple and we can empirically observe what occurs.
We can also infer what is happening based on the signing technical
implementation, assuming Apple follows correct practices. But some content
of this section is speculation and is merely what *likely* occurs.

When a Mach-O binary is loaded, the loader looks for an
`LC_CODE_SIGNATURE` load command. If not found, there is no embedded
signature data and running the binary may be rejected.

The associated code signature data is located in the `__LINKEDIT` section
and parsed so *Blob* are discovered. How deeply it is parsed at this stage,
we don't know.

Data for the *Signature* slot/blob is obtained. This is the CMS *SignedData*
structure (BER encoded ASN.1). This structure is decoded and the cryptographic
signature, signed attributes, and X.509 certificates involved in the signing
are obtained from within.

We do not know the full extent of trust verification that occurs. But
Apple will examine details of the signing certificate and ensure its use
is allowed. For example, if the signing certificate wasn't issued/signed
by Apple or doesn't have the appropriate extensions present (such as bits
indicating the certificate is appropriate for code signing), it may refuse
to proceed. This trust validation likely occurs immediately after the
CMS data is parsed, as soon as the signing certificate information becomes
available for scrutiny.

The original *plaintext* message that was signed is assembled. This is
done by DER encoding the *signed attributes* from the CMS *SignedData*
structure.

This *plaintext* message, the signature of it, and the public key used
to produce the signature are all used to verify the cryptographic integrity
of the *signed attributes*. This effectively answers the question *did
something with possession of certificate X sign exactly the signed attributes
in this message.*

Successful signature verification ensures that the *signed attributes*
haven't been tampered with since they were signed.

The CMS data may also contain *unsigned attributes*. There may be
a *time stamp token* here containing a signature of the time when the
signed message was produced. This may be validated as well.

One of the signed attributes is `message-digest`. In this use of CMS,
`message-digest` is the digest of the *Code Directory* *Blob* data. This
digest is possibly verified: we don't know for sure. According to RFC 5652
it should be verified. However, it may not need to be because the digest
of the *Code Directory* data is stored elsewhere...

A signed attribute contains an XML plist containing an array of base64 encoded
hashes of *Code Directory* *blobs*. This plist is likely parsed and the hashes
within are compared to the hashes from the *Code Directory* blobs/slots from
the *SuperBlob* record. If the digests are identical, it means that the *Code
Directory* data structures in the Mach-O binary haven't been modified since the
signature was created.

The *Code Directory* data structures contain digests of code data and
other *Blob* data from the *SuperBlob*. Since the digest of the *Code Directory*
data was verified via CMS and a trust relationship was (presumably) established
with the signer of that CMS data, verification and trust is transitively applied
to the other *Blob* data and code data (this is effectively a Merkle Tree).
This means that we can digest other *Blob* entries and code data and compare to
the digests within the *Code Directory* structures. If the digests are identical,
content hasn't changed since the signature was made.

It is unclear in what order other *Blob* data is read. But presumably important
data like the embedded entitlements and code requirement statement are read very
early during binary loading so an appropriate trust policy can be applied to
the binary.
*/
