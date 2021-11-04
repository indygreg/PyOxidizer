// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*! End-user documentation.

This document attempts to capture end-user documentation regarding code
signing on Apple platforms.

# Installing

This crate provides an `rcodesign` command line executable for performing
common tasks related to code signing for Apple platforms.

Using Rust's `cargo` tool, installing `rcodesign` should be straightforward.

To install the latest published version on crates.io:

```text
$ cargo install apple-codesign
```

To compile and run from a Git checkout of its canonical repository:

```text
$ cargo run --bin rcodesign -- --help
```

To install from a Git checkout of its canonical repository:

```text
$ cargo install --bin rcodesign
```

To install from the latest commit in the canonical Git repository:

```text
$ cargo install --git https://github.com/indygreg/PyOxidizer --branch main rcodesign
```

# Creating Code Signing Certificates

The `rcodesign generate-self-signed-certificate` command can be used to generate
a new self-signed code signing certificate. See its `--help` output for more.

# Analyzing Code Signing Certificates

The `rcodesign analyze-certificate` command can be used to parse X.509
certificates and print information relevant to Apple code signing. For
example:

```text
$ rcodesign analyze-certificate --der-source apple-signed-apple-development.cer
Nov 02 19:41:50.880 WARN reading DER file apple-codesign/src/testdata/apple-signed-apple-development.cer
# Certificate 0

Subject CN:                  Apple Development: Gregory Szorc (DD5YMVP48D)
Issuer CN:                   Apple Worldwide Developer Relations Certification Authority
Subject is Issuer?:          false
Team ID:                     MK22MZP987
SHA-1 fingerprint:           5eeadb4befce055e06b4239ad4c5f0d1bfd6af8f
SHA-256 fingerprint:         6b91c618851009aaf10fad9740be08a880cb1a9afc1672e4dbac9119c276db85
Signed by Apple?:            true
Apple Issuing Chain:
  - Apple Worldwide Developer Relations Certification Authority
  - Apple Root CA
  - Apple Root Certificate Authority
Guessed Certificate Profile: AppleDevelopment
Is Apple Root CA?:           false
Is Apple Intermediate CA?:   false
Apple CA Extension:          none
Apple Extended Key Usage Purpose Extensions:
  - 1.3.6.1.5.5.7.3.3 (CodeSigning)
Apple Code Signing Extensions:
  - 1.2.840.113635.100.6.1.2 (IPhoneDeveloper)
  - 1.2.840.113635.100.6.1.12 (MacDeveloper)
```

You can also use a command like `openssl x509 -text -in <file>` to print
information about X.509 certificates as well. What makes this command useful
is it knows about X.509 extensions used by Apple code signing as well as
the Apple CA certificates.

# Signing Binaries and Bundles with `rcodesign sign`

The `rcodesign sign` command can be used to sign Mach-O binaries and
app bundles. The command takes many arguments to control its behavior. Run
with `rcodesign sign --help` for the full list.

# A Primer on Gatekeeper

*Gatekeeper* is the name Apple gives to a set of technologies that enforce
application execution policies at the operating system level. Essentially,
Gatekeeper answers the question *is this software allowed to run*.

When Gatekeeper runs, it performs a *security assessment* against the
binary and the currently configured system policies from the system policy
database (see `man syspolicyd`). If the binary fails to meet the requirements,
Gatekeeper prevents the binary from running.

## The `spctl` Tool

The `spctl` program distributed with macOS allows you to query and
manipulate the assessment policies.

If you run `sudo spctl --list`, it will print a list of rules.  e.g.

```text
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
See `man csreq` for more. Also see this crate's pure Rust implementation in
[crate::CodeRequirements].

Some of these expressions are pretty straightforward. For example,
the following entry says to allow executing a binary with a code signature
whose *code directory* hash is `cf782d6467be86b73a83d86cd6d8c9f87d9d9ce5`:

```text
2717[GKE] P0 allow execute [(gke)]
        cdhash H"cf782d6467be86b73a83d86cd6d8c9f87d9d9ce5"
```

The *code directory* refers to a data structure within the code
signature that contains (among other things) content digests of the binary. The
hash/digest of the code directory itself is effectively a chained digest to the
actual binary content and theoretically a unique way of identifying a binary. So
`cdhash H"cf782d6467be86b73a83d86cd6d8c9f87d9d9ce5"` is a very convoluted
way of saying *allow this specific binary (specified by its content hash)
to execute*.

Other rules are more interesting. For example:

```text
11[Notarized Developer ID] P5 allow execute
        anchor apple generic and certificate 1[field.1.2.840.113635.100.6.2.6] exists
        and certificate leaf[field.1.2.840.113635.100.6.1.13] exists and notarized
```

We see the description (`Notarized Developer ID`) but what does that
expression mean?

Well, first this expression parses into a tree. We won't attempt to format
the tree here. But essentially the following conditions must `all` be true:

* `anchor apple generic`
* `certificate 1[field.1.2.840.113635.100.6.2.6] exists`
* `certificate leaf[field.1.2.840.113635.100.6.1.13] exists`
* `notarized`

`anchor apple generic` and `notarized` are essentially special expression
that expand to mean *the certificate signing chain leads back to an Apple
root certificate authority (CA)* and *there is a supplemental code signature
from Apple that can only come from Apple's notarization service*.

But what about those `certificate` expressions? That
`certificate <position>[field.*]` syntax essentially says *the code signature
certificate at `<position>` in the certificate chain has an X.509 certificate
extension with OID `X`* (where `X` is a value like `A.B.C.D.E.F`).

This is all pretty low level. But essentially X.509 certificates can have
a series of *extensions* that further describe the certificate. Apple code
signing uses these extensions to convey metadata about the certificate. And
since code signing certificates are signed, whoever signed those certificates
is effectively also approving of whatever is conveyed by the extensions
within.

But what do these extensions actually mean? Running `rcodesign x509-oids`
may give us some help:

```text
$ rcodesign x509-oids`
...
Code Signing Certificate Extension OIDs
...
1.2.840.113635.100.6.1.13       DeveloperIdApplication
...
Certificate Authority Certificate Extension OIDs
...
1.2.840.113635.100.6.2.6        DeveloperId
```

We see `1.2.840.113635.100.6.2.6` is the OID of an extension on
certificate authorities indicating they act as the *Apple Developer
ID* certificate authority. We also see that `1.2.840.113635.100.6.1.13`
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

# Deploying Custom Assessment Policies

By default, Apple locks down their operating systems such that the default
assessment policies enforced by Gatekeeper restrict what can be run. The
restrictions vary by operating system (iOS is more locked down than macOS for
example).

On macOS, it is possible to change the system assessment policies via
the `spctl` tool. By injecting your own rules, you can allow binaries
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
  [Apple's docs](https://developer.apple.com/library/archive/documentation/Security/Conceptual/CodeSigningGuide/RequirementLang/RequirementLang.html)
  on the requirements language for more possibilities.

Defining custom rules is possible via the under-documented
`spctl --add --requirement` mode. In this mode, you can register a code
requirements expression into the system database for Gatekeeper to
utilize. The following sections give some examples of this.

## Verifying Assessment Policies

The sections below document how to define custom assessment policies
to allow execution of binaries/installers/etc signed by certificates
that aren't normally supported.

When doing this, you probably want a way to verify things work as
expected.

The `spctl --assess` mode puts `spctl` in *assessment mode* and tells you
what verdict Gatekeeper would render. e.g.

```text
$ spctl --assess --type execute -vv /Applications/Firefox.app
/Applications/Firefox.app: accepted
source=Notarized Developer ID
```

Do note that this only works on app bundles (not standalone executable
binaries)! If you run `spctl --assess` on a standalone executable, you
get an error:

```text
$ spctl --assess -vv /usr/bin/ssh
/usr/bin/ssh: rejected (the code is valid but does not seem to be an app)
origin=Software Signing
```

In addition, macOS uses the `com.apple.quarantine` extended file attribute
to *quarantine* files and prevent them from running via the graphical UI.
It can sometimes be handy to add this attribute back to a file to simulate
a fresh quarantine. You can do this by running a command like the following:

```text
$ xattr -w com.apple.quarantine "0001;$(printf %x $(date +%s));manual;$(/usr/bin/uuidgen)" /path/to/file
```

(This extended attribute isn't added to files downloaded by tools like `curl`
or `wget` which is why you can execute binaries obtained via these tools but
can't run the same binary downloaded via a web browser.)

## Allowing Execution of Binaries Signed by a Specific Certificate

Say you have a single code signing certificate and want to be able to
run all binaries signed by that certificate. We can construct a
*code requirement expression* that refers to this specific certificate.

The most reliable way to specify a single certificate is via a
digest of its content. Assuming no two certificates have the same
digest, this uniquely identifies a certificate.

You can use `rcodesign analyze-certificate` to locate a certificate's
content digest.:

```text
rcodesign analyze-certificate --pem-source path/to/cert | grep fingerprint
SHA-1 fingerprint:           0b724bcd713c9f3691b0a8b0926ae0ecf9e7edd8
SHA-256 fingerprint:         ac5c4b5936677942e017bca1570aaa9e763674c4b66709231b15118e5842aeca
```

The *code requirement* language only supports SHA-1 hashes. So we
construct our expression referring to this certificate as
`certificate leaf H"0b724bcd713c9f3691b0a8b0926ae0ecf9e7edd8"`.

Now, we define an assessment rule to allow execution of binaries
signed with this certificate:

```text
sudo spctl --add --type execute --label 'My Cert' --requirement \
  'certificate leaf H"0b724bcd713c9f3691b0a8b0926ae0ecf9e7edd8"'
```

Now Gatekeeper should allow execution of all binaries signed with this
exact code signing certificate!

If the signing certificate hash is registered in the system assessment
policy database, there is no need to register the certificate in a
*keychain* or mark that certificate as *trusted* in a keychain. The signing
certificate also does not need to chain back to an Apple certificate.
And since the requirement expression doesn't say `and notarized`, binaries
don't need to be notarized by Apple either. This effectively allows you
to sidestep the default requirement that binaries be signed and notarized
by certificates that Apple is aware of. Congratulations, you've just
escaped Apple's walled garden (at your own risk of course).

Do note that for files with the `com.apple.quarantine` extended attribute,
you may see a dialog the first time you run this file. You can prevent that
by removing the extended attribute via
`xattr -d com.apple.quarantine /path/to/file`.

## Allowing Execution of Binaries Signed by a Trusted CA

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
trusted certificate:

```text
sudo spctl --add --type execute --label 'My Trusted CA' --requirement \
  'certificate 1 H"0b724bcd713c9f3691b0a8b0926ae0ecf9e7edd8"'
```

That `certificate 1` says to apply to the certificate that signed the
certificate that produced the code signature. By trusting the CA certificate,
you implicitly trust all certificates signed by that CA certificate.

Note that if you use a custom CA for signing code signing certificates,
you'll probably want to follow some best practices for running your own
Public Key Infrastructure (PKI) like publishing a Certificate Revocation List
(CRL). This is a complex topic outside the scope of this documentation. Ask
someone with *Security* in their job title for assistance.

For CA certificates issuing/signing code signing certificates, you'll
want to enable a few X.509 certificate extensions:

* Key Usage (`2.5.29.15`): *Digital Signature* and *Key Cert Sign*
* Basic Constraints (`2.5.29.19`): CA=yes
* Extended Key Usage (`2.5.29.37`): Code Signing (`1.3.6.1.5.5.7.3.3`); critical=true

You can create CA certificates in the `Keychain Access` macOS application.
If you create CA certificates another way, you may want to compare certificate
extensions and other fields against those produced via `Keychain Access` to
make sure they align. It is unknown how much Apple's operating systems
enforce requirements on the X.509 certificates. But it is a good idea to
keep things as similar as possible.
*/
