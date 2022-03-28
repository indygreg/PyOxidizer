.. _apple_codesign_gatekeeper:

======================
A Primer on Gatekeeper
======================

*Gatekeeper* is the name Apple gives to a set of technologies that enforce
application execution policies at the operating system level. Essentially,
Gatekeeper answers the question *is this software allowed to run*.

When Gatekeeper runs, it performs a *security assessment* against the
binary and the currently configured system policies from the system policy
database (see ``man syspolicyd``). If the binary fails to meet the requirements,
Gatekeeper prevents the binary from running.

The ``spctl`` Tool
==================

The ``spctl`` program distributed with macOS allows you to query and
manipulate the assessment policies.

If you run ``sudo spctl --list``, it will print a list of rules.  e.g.::

    $ sudo spctl --list
    8[Apple System] P20 allow lsopen
            anchor apple
    3[Apple System] P20 allow execute
            anchor apple
    2[Apple Installer] P20 allow install
            anchor apple generic and certificate 1[subject.CN] = "Apple Software Update Certification Authority"
    17[Testflight] P10 allow execute
            anchor apple generic and certificate 1[field.1.2.840.113635.100.6.2.1] exists and certificate leaf[field.1.2.840.113635.100.6.1.25.1] exists
    10[Mac App Store] P10 allow install
            anchor apple generic and certificate leaf[field.1.2.840.113635.100.6.1.10] exists
    5[Mac App Store] P10 allow install
            anchor apple generic and certificate leaf[field.1.2.840.113635.100.6.1.10] exists
    4[Mac App Store] P10 allow execute
            anchor apple generic and certificate leaf[field.1.2.840.113635.100.6.1.9] exists
    16[Notarized Developer ID] P5 allow lsopen
            anchor apple generic and certificate 1[field.1.2.840.113635.100.6.2.6] exists and certificate leaf[field.1.2.840.113635.100.6.1.13] exists and notarized
    12[Notarized Developer ID] P5 allow install
            anchor apple generic and certificate 1[field.1.2.840.113635.100.6.2.6] exists and (certificate leaf[field.1.2.840.113635.100.6.1.14] or certificate leaf[field.1.2.840.113635.100.6.1.13]) and notarized
    11[Notarized Developer ID] P5 allow execute
            anchor apple generic and certificate 1[field.1.2.840.113635.100.6.2.6] exists and certificate leaf[field.1.2.840.113635.100.6.1.13] exists and notarized
    9[Developer ID] P4 allow lsopen
            anchor apple generic and certificate 1[field.1.2.840.113635.100.6.2.6] exists and certificate leaf[field.1.2.840.113635.100.6.1.13] exists and legacy
    7[Developer ID] P4 allow install
            anchor apple generic and certificate 1[field.1.2.840.113635.100.6.2.6] exists and (certificate leaf[field.1.2.840.113635.100.6.1.14] or certificate leaf[field.1.2.840.113635.100.6.1.13]) and legacy
    6[Developer ID] P4 allow execute
            anchor apple generic and certificate 1[field.1.2.840.113635.100.6.2.6] exists and certificate leaf[field.1.2.840.113635.100.6.1.13] exists and (certificate leaf[timestamp.1.2.840.113635.100.6.1.33] absent or certificate leaf[timestamp.1.2.840.113635.100.6.1.33] < timestamp "20190408000000Z")
    2718[GKE] P0 allow lsopen [(gke)]
            cdhash H"975d9247503b596784dd8a9665fd3ff43eb7722f"
    2717[GKE] P0 allow execute [(gke)]
            cdhash H"cf782d6467be86b73a83d86cd6d8c9f87d9d9ce5"
    ...
    18[GKE] P0 allow lsopen [(gke)]
            cdhash H"cf5f88b3b2ff4d8612aabb915f6d1f712e16b6f2"
    15[Unnotarized Developer ID] P0 deny lsopen
            anchor apple generic and certificate 1[field.1.2.840.113635.100.6.2.6] exists and certificate leaf[field.1.2.840.113635.100.6.1.13] exists
    14[Unnotarized Developer ID] P0 deny install
            anchor apple generic and certificate 1[field.1.2.840.113635.100.6.2.6] exists and (certificate leaf[field.1.2.840.113635.100.6.1.14] or certificate leaf[field.1.2.840.113635.100.6.1.13])
    13[Unnotarized Developer ID] P0 deny execute
            anchor apple generic and certificate 1[field.1.2.840.113635.100.6.2.6] exists and certificate leaf[field.1.2.840.113635.100.6.1.13] exists and (certificate leaf[timestamp.1.2.840.113635.100.6.1.33] exists and certificate leaf[timestamp.1.2.840.113635.100.6.1.33] >= timestamp "20190408000000Z")
    ```

The first line of each item identifies the policy. The second line is a
*code requirement language expression*. This is a DSL that compiles to a
binary expression tree for representing a test to perform against a binary.
See ``man csreq`` for more.

Some of these expressions are pretty straightforward. For example,
the following entry says to allow executing a binary with a code signature
whose *code directory* hash is ``cf782d6467be86b73a83d86cd6d8c9f87d9d9ce5``::

    2717[GKE] P0 allow execute [(gke)]
            cdhash H"cf782d6467be86b73a83d86cd6d8c9f87d9d9ce5"

The *code directory* refers to a data structure within the code
signature that contains (among other things) content digests of the binary. The
hash/digest of the code directory itself is effectively a chained digest to the
actual binary content and theoretically a unique way of identifying a binary. So
``cdhash H"cf782d6467be86b73a83d86cd6d8c9f87d9d9ce5"`` is a very convoluted
way of saying *allow this specific binary (specified by its content hash)
to execute*.

Other rules are more interesting. For example::

    11[Notarized Developer ID] P5 allow execute
            anchor apple generic and certificate 1[field.1.2.840.113635.100.6.2.6] exists
            and certificate leaf[field.1.2.840.113635.100.6.1.13] exists and notarized

We see the description (``Notarized Developer ID``) but what does that
expression mean?

Well, first this expression parses into a tree. We won't attempt to format
the tree here. But essentially the following conditions must ``all`` be true:

* ``anchor apple generic``
* ``certificate 1[field.1.2.840.113635.100.6.2.6] exists``
* ``certificate leaf[field.1.2.840.113635.100.6.1.13] exists``
* ``notarized``

``anchor apple generic`` and ``notarized`` are essentially special expressions
that expand to mean *the certificate signing chain leads back to an Apple
root certificate authority (CA)* and *there is a supplemental code signature
from Apple that can only come from Apple's notarization service*.

But what about those ``certificate`` expressions? That
``certificate <position>[field.*]`` syntax essentially says *the code signature
certificate at ``<position>`` in the certificate chain has an X.509 certificate
extension with OID ``X``* (where ``X`` is a value like ``A.B.C.D.E.F``).

This is all pretty low level. But essentially X.509 certificates can have
a series of *extensions* that further describe the certificate. Apple code
signing uses these extensions to convey metadata about the certificate. And
since code signing certificates are signed, whoever signed those certificates
is effectively also approving of whatever is conveyed by the extensions
within.

But what do these extensions actually mean? Running ``rcodesign x509-oids``
may give us some help::

    $ rcodesign x509-oids`
    ...
    Code Signing Certificate Extension OIDs
    ...
    1.2.840.113635.100.6.1.13       DeveloperIdApplication
    ...
    Certificate Authority Certificate Extension OIDs
    ...
    1.2.840.113635.100.6.2.6        DeveloperId

We see ``1.2.840.113635.100.6.2.6`` is the OID of an extension on
certificate authorities indicating they act as the *Apple Developer
ID* certificate authority. We also see that ``1.2.840.113635.100.6.1.13``
is the OID of an extension saying the certificate acts as a code signing
certificate for *applications* associated with an *Apple Developer ID*.

So, what this expression translates to is essentially:

* Trust code signatures whose certificate signing chain leads back to an
  Apple CA.
* The signer of the code signing certificate must have the extension that
  identifies it as the *Apple Developer ID* certificate authority.
* The code signing certificate itself must have the extension that says
  it is an *Apple Developer ID* for use with *application* signing.
* The binary is *notarized*.

In simple terms, this is saying *allow execution of binaries that
were signed by a Developer ID code signing certificate which was signed
by Apple's Developer ID certificate authority and are also notarized*.
