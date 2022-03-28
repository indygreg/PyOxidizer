.. _apple_codesign_custom_assessment_policies:

================================================================
Selectively Bypassing Gatekeeper with Custom Assessment Policies
================================================================

By default, Apple locks down their operating systems such that the default
assessment policies enforced by Gatekeeper restrict what can be run. The
restrictions vary by operating system (iOS is more locked down than macOS for
example).

On macOS, it is possible to change the system assessment policies via
the ``spctl`` tool. By injecting your own rules, you can allow binaries
through meeting criteria expressible via *code requirements language
expressions*. This allows you to allow binaries having:

* A specific *code directory hash* (uniquely identifies the binary).
* A specific code signing certificate identified by its certificate hash.
* Any code signing certificate whose trust/signing chain leads to a trusted
  certificate.
* Any code signing certificate signed by a certificate containing a
  certain X.509 extension OID.
* A code signing certificate with specific values in its subject field.
* And many more possibilities. See
  `Apple's docs <https://developer.apple.com/library/archive/documentation/Security/Conceptual/CodeSigningGuide/RequirementLang/RequirementLang.html>`_
  on the requirements language for more possibilities.

Defining custom rules is possible via the under-documented
``spctl --add --requirement`` mode. In this mode, you can register a code
requirements expression into the system database for Gatekeeper to
utilize. The following sections give some examples of this.

Verifying Assessment Policies
=============================

The sections below document how to define custom assessment policies
to allow execution of binaries/installers/etc signed by certificates
that aren't normally supported.

When doing this, you probably want a way to verify things work as
expected.

The ``spctl --assess`` mode puts ``spctl`` in *assessment mode* and tells you
what verdict Gatekeeper would render. e.g.::

    $ spctl --assess --type execute -vv /Applications/Firefox.app
    /Applications/Firefox.app: accepted
    source=Notarized Developer ID

Do note that this only works on app bundles (not standalone executable
binaries)! If you run ``spctl --assess`` on a standalone executable, you
get an error::

    $ spctl --assess -vv /usr/bin/ssh
    /usr/bin/ssh: rejected (the code is valid but does not seem to be an app)
    origin=Software Signing

In addition, macOS uses the ``com.apple.quarantine`` extended file attribute
to *quarantine* files and prevent them from running via the graphical UI.
It can sometimes be handy to add this attribute back to a file to simulate
a fresh quarantine. You can do this by running a command like the following::

    xattr -w com.apple.quarantine "0001;$(printf %x $(date +%s));manual;$(/usr/bin/uuidgen)" /path/to/file

(This extended attribute isn't added to files downloaded by tools like ``curl``
or ``wget`` which is why you can execute binaries obtained via these tools but
can't run the same binary downloaded via a web browser.)

Allowing Execution of Binaries Signed by a Specific Certificate
===============================================================

Say you have a single code signing certificate and want to be able to
run all binaries signed by that certificate. We can construct a
*code requirement expression* that refers to this specific certificate.

The most reliable way to specify a single certificate is via a
digest of its content. Assuming no two certificates have the same
digest, this uniquely identifies a certificate.

You can use ``rcodesign analyze-certificate`` to locate a certificate's
content digest.::

    rcodesign analyze-certificate --pem-source path/to/cert | grep fingerprint
    SHA-1 fingerprint:           0b724bcd713c9f3691b0a8b0926ae0ecf9e7edd8
    SHA-256 fingerprint:         ac5c4b5936677942e017bca1570aaa9e763674c4b66709231b15118e5842aeca

The *code requirement* language only supports SHA-1 hashes. So we
construct our expression referring to this certificate as
``certificate leaf H"0b724bcd713c9f3691b0a8b0926ae0ecf9e7edd8"``.

Now, we define an assessment rule to allow execution of binaries
signed with this certificate::

    sudo spctl --add --type execute --label 'My Cert' --requirement \
      'certificate leaf H"0b724bcd713c9f3691b0a8b0926ae0ecf9e7edd8"'

Now Gatekeeper should allow execution of all binaries signed with this
exact code signing certificate!

If the signing certificate hash is registered in the system assessment
policy database, there is no need to register the certificate in a
*keychain* or mark that certificate as *trusted* in a keychain. The signing
certificate also does not need to chain back to an Apple certificate.
And since the requirement expression doesn't say ``and notarized``, binaries
don't need to be notarized by Apple either. **This effectively allows you
to sidestep the default requirement that binaries be signed and notarized
by certificates that Apple is aware of.** Congratulations, you've just
escaped Apple's walled garden (at your own risk of course).

Do note that for files with the ``com.apple.quarantine`` extended attribute,
you may see a dialog the first time you run this file. You can prevent that
by removing the extended attribute via
``xattr -d com.apple.quarantine /path/to/file``.

Allowing Execution of Binaries Signed by a Trusted CA
=====================================================

Say you are an enterprise or distributed organization and want to have
multiple code signing certificates. Using the approach in the section
above you could individually register each code signing certificate you
want to allow. However, the number of certificates can quickly grow and
become unmanageable.

To solve this problem, you can employ the strategy that Apple itself uses
for code signing certificates associated with Developer ID accounts: trust
code signing certificates themselves issued/signed by a trusted certificate
authority (CA).

To do this, we'll again craft a *code requirement expression* referring to
our trusted CA certificate.

This looks very similar to above except we change the position of the
trusted certificate::

    sudo spctl --add --type execute --label 'My Trusted CA' --requirement \
      'certificate 1 H"0b724bcd713c9f3691b0a8b0926ae0ecf9e7edd8"'

That ``certificate 1`` says to apply to the certificate that signed the
certificate that produced the code signature. By trusting the CA certificate,
you implicitly trust all certificates signed by that CA certificate.

Note that if you use a custom CA for signing code signing certificates,
you'll probably want to follow some best practices for running your own
Public Key Infrastructure (PKI) like publishing a Certificate Revocation List
(CRL). This is a complex topic outside the scope of this documentation. Ask
someone with *Security* in their job title for assistance.

For CA certificates issuing/signing code signing certificates, you'll
want to enable a few X.509 certificate extensions:

* Key Usage (``2.5.29.15``): *Digital Signature* and *Key Cert Sign*
* Basic Constraints (``2.5.29.19``): CA=yes
* Extended Key Usage (``2.5.29.37``): Code Signing (``1.3.6.1.5.5.7.3.3``); critical=true

You can create CA certificates in the ``Keychain Access`` macOS application.
If you create CA certificates another way, you may want to compare certificate
extensions and other fields against those produced via ``Keychain Access`` to
make sure they align. It is unknown how much Apple's operating systems
enforce requirements on the X.509 certificates. But it is a good idea to
keep things as similar as possible.
