.. py:currentmodule:: starlark_tugger

.. _tugger_code_signing:

============
Code Signing
============

Tugger has support for automatically performing code signing when evaluating
Starlark configuration files.

Various platforms and distribution channels enforce requirements that binaries
and other artifacts are cryptographically signed by a trusted certificate.

For example:

* On Windows, executables and installers must be signed by a trusted certificate
  to avoid warnings about running untrusted applications.
* On macOS, executables, pkg installers, and more need to be signed by a trusted
  certificate or Gatekeeper (read: the OS) may refuse to run them.

Tugger's support for automatic signing enables you to meet these requirements
with hpoefully minimal effort.

Code Signing Support
====================

Tugger supports signing the following signable entities:

* PE binaries. This is the file executable format in use on Windows platforms.
* MSI installers. This is a common file-based installer format on Windows.
* Mach-O binaries. This is the file executable format in use on Apple platforms.
* Apple application bundles. e.g. ``My Program.app`` directories. Bundles are
  a common application *packaging* format on Apple platforms.

Signing on Windows currently uses Microsoft's ``signtool.exe`` to perform the
signing. So signing Windows entities requires access to this tool. (We have plans
to implement equivalent functionality in Rust to avoid this dependency.)

Signing Apple formats uses a pure Rust implementation of the code signing
functionality and works on any machine. Apple's ``codesign`` tool or access
to Apple hardware is not required to sign Apple entities.

Code signing requires the use of a *code signing certificate*. See
:ref:`tugger_code_signing_certificates` for more.

Tugger supports using *code signing certificates* in the following locations:

* From a PFX / PKCS #12 file. (e.g. ``.pfx`` or ``.p12`` files.)
* Certificates available in the *Windows certificate store*. Via the *Windows
  certificate store*, certificates stored in hardware devices (such as HSMs and
  hardware tokens such as YubiKeys) can also be used.

Configuring Code Signing in Starlark
====================================

**Code signing needs to be explicitly enabled and configured in your
Starlark configuration file.**

From a high level, here's how it works:

1. Your Starlark configuration instantiates, configures, and enables
   a :py:class:`CodeSigner`, which is the entity that performs code
   signing.
2. As your configuration file is evaluated, actions that produce or
   encounter signable entities (such as creating Windows MSI installers)
   interact with registered :py:class:`CodeSigner` instances and attempt
   code signing.

Tugger abstracts away a lot of the complexity around code signing, such
as figuring out which files need to be signed (it looks at the content
of files and determines if a file is signable). So in many cases, all
you need to do is tell Tugger where your code signing certificate is and
it can do the rest!

Continuing reading for details on how to customize code signing. Or
just straight into :ref:`tugger_code_signing_examples`.

Instantiating :py:class:`CodeSigner` to Perform Code Signing
------------------------------------------------------------

To perform code signing, first instantiate a :py:class:`CodeSigner` via one
of its available constructor functions:

* :py:func:`code_signer_from_pfx_file`
* :py:func:`code_signer_from_windows_store_sha1_thumbprint`
* :py:func:`code_signer_from_windows_store_subject`
* :py:func:`code_signer_from_windows_store_auto`

:py:func:`code_signer_from_pfx_file` is the most versatile method, as it
gives Tugger full access to the signing certificate and private key. However,
this method is arguably the least secure, as it requires the private key to
exist in a file and Tugger holds the decrypted private key in memory during
signing. Both of these make the private key much more susceptible to being
accessed by unwanted parties. If you are paranoid about security, you should
only use this method on machines that you trust.

The ``code_signer_from_windows_`` functions reference code signing keys stored in
the Windows certificate store. Signature requests are processed through the
Windows APIs and the private key never leaves the control of the Windows
certificate store, helping to keep the private key secure.

.. important::

   Constructed :py:class:`CodeSigner` instances must be *activated* in order
   to automatically perform code signing. See :ref:`tugger_code_signing_activation`
   for more.

Configuring :py:class:`CodeSigner` Instances
--------------------------------------------

Once you've obtained a :py:class:`CodeSigner`, you may need to register
additional settings to influence signing.

Registering the Issuing Certificate Chain
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

Produced signatures should often contain details about the *chain* of
certificates that issued the code signing certificate. See
:ref:`tugger_code_signing_certificates` for more on this topic.

You may need to tell :py:class:`CodeSigner` about the existence of
these certificates.

* When using a code signing certificate backed by the Windows certificate store,
  you do not need to register the certificate's signing chain.
* When using a code signing certificate backed by a PFX file, you need to
  register the certificate chain, even if those X.509 certificates are in the
  PFX file (we don't yet support reading these from the PFX file).
  :py:meth:`CodeSigner.chain_issuer_certificates_pem_file` is the most
  versatile method to register issuer certificates, as it works on all platforms
  and PEM is a very widespread format for storing X.509 certificates.
* On macOS, :py:meth:`CodeSigner.chain_issuer_certificates_macos_keychain` can
  be called to attempt to resolve the certificate chain by speaking directly to
  the macOS keychain APIs. This requires that the signing certificate be
  accessible in the current user's keychain and its entire issuing chain to
  be present in that keychain.

Influencing Signing Operations
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

:py:class:`CodeSigner` instances have the opportunity to influence individual
signing operations. This gives you significant control over how signing is
performed.

:py:meth:`CodeSigner.set_signing_callback` registers a function that will be
invoked on each attempted signing operation. This callback function receives
an argument - a :py:class:`CodeSigningRequest` instance - that describes
the entity capable of being signed. This type exposes functionality
for influencing the signing operation. For example:

* Setting :py:attr:`CodeSigningRequest.defer` to ``True`` will opt this
  :py:class:`CodeSigner` out of signing this particular entity.
* Setting :py:attr:`CodeSigningRequest.prevent_signing` to ``True`` will
  prevent this and other :py:class:`CodeSigner` from signing this entity.

See the :py:class:`CodeSigningRequest` API documentation for all available
functionality on this type.

Leveraging custom callback functions enables configuration files to employ
arbitrarily complex logic for influencing code signing. Your main constraint
are the settings exposed on :py:class:`CodeSigningRequest`. If you find
yourself needing a setting that doesn't exist, please file a feature request!

.. _tugger_code_signing_activation:

Activating Automatic Code Signing
---------------------------------

A :py:class:`CodeSigner` needs to be *activated* for automatic use
by Tugger. i.e. your signable files won't be signed as your Starlark
configuration file is evaluated unless a :py:class:`CodeSigner` is
*activated*.

To activate your :py:class:`CodeSigner`, simply call
:py:meth:`CodeSigner.activate`.

.. _tugger_code_signing_actions:

Code Signing Actions
--------------------

Various activities within the evaluation of your Starlark configuration
file trigger the assessment of - and possible performing of - code signing.

Each unique activity has its own string *action* name describing it.
This name is accessible via :py:attr:`CodeSigningRequest.action`, enabling
callback functions to key off of it. For example, you may want to not
sign during certain operations.

The following named actions are defined by Tugger:

``file-manifest-install``
   Used when a :py:class:`FileManifest` is materialized on the filesystem
   through an action like :py:meth:`FileManifest.install()`.

``macos-application-bundle-creation``
   When a macOS Application Bundle is created by Tugger.

   This will be triggered by :py:meth:`MacOsApplicationBundleBuilder.build()`.

``windows-installer-creation``
   When a Windows installer file is created by Tugger.

   Methods like :py:meth:`WiXMSIBuilder.build` and
   :py:meth:`WiXBundleBuilder.build` will trigger this action.

``windows-installer-file-added``
   When a file that will be installed is added to a Windows installer.

   Triggered by :py:meth:`WiXMSIBuilder.add_program_files_manifest`,
   :py:meth:`WiXInstaller.add_install_file`, and
   :py:meth:`WiXInstaller.add_install_files`.

Other applications extending Tugger's core functionality may define their own
actions.

.. _tugger_code_signing_duplicate_events:

Duplicate Events
----------------

It is possible for the same logical file to trigger multiple signing events
as it is processed. For example, :py:meth:`MacOsApplicationBundleBuilder.build()`
may trigger an event for macOS Application Bundle generation then a later
action loads the bundle files into a :py:class:`FileManifest` and materializes
them somewhere else via :py:meth:`FileManifest.install()`, which would
trigger an additional signability check.

As a result, the same file or entity may be signed multiple times.

If this behavior is undesirable, the use of a custom callback function can
be used to choose which signing requests to respond to.

Unfortunately, we do not yet expose metadata on :py:class:`CodeSigningRequest`
indicating if a file is signed or not. This would likely be the obvious
attribute to filter against. This feature is tracked at
https://github.com/indygreg/PyOxidizer/issues/400.

.. _tugger_code_signing_examples:

Code Signing Examples
=====================

Automatically Sign all Signable Content with a Specific Certificate in the Windows Store
----------------------------------------------------------------------------------------

Say you have a code signing certificate in the Windows certificate store
with the SHA-1 thumbprint ``deadbeefdeadbeefdeadbeefdeadbeefdeadbeef`` and
you want Tugger to sign all signable files as it runs. Here's what you'll
need to do in your Starlark configuration file:

.. code-block:: python

    signer = code_signer_from_windows_store_sha1_thumbprint("deadbeefdeadbeefdeadbeefdeadbeefdeadbeef")
    signer.activate()

As Tugger encounters ``.exe``, ``.dll``, ``.msi`` files and any file that
it identifies as signable, it will attempt to automatically sign them!

Choosing a Code Signing Certificate Dynamically
-----------------------------------------------

Say you have multiple code signing certificates but want to parameterize
which one to use. We can do that through the use of the ``VARS`` global
dict, which holds settings passed in via the command line.

.. code-block:: python

    PFX_PATH = VARS.get("PFX_PATH")
    PFX_PASSWORD = VARS.get("PFX_PASSWORD", "")

    # This needs to be in its own function because Starlark doesn't allow `if`
    # at the file/module scope.
    def make_code_signers():
        if PFX_PATH:
            signer = code_signer_from_pfx_file(PFX_PATH, PFX_PASSWORD)
            signer.activate()


    # Don't forget to call the function!
    make_code_signers()

Then when running the configuration file, specify an extra variable. e.g.::

    $ pyoxidizer --var PFX_PATH /path/to/certificate.pfx --var PFX_PASSWORD hunter2

Or you could use functions like :py:func:`prompt_confirm`, :py:func:`prompt_input`,
and :py:func:`prompt_password` to ask the user which certificate to use.

.. code-block:: python

   def make_code_signers():
       if prompt_confirm("enable code signing?", default=False):
           pfx_path = prompt_input("enter path to PFX file:")
           pfx_password = prompt_password("enter path to PFX password:", confirm=True)

           signer = code_signer_from_pfx_file(pfx_path, pfx_password)
           signer.activate()

   make_code_signers()



Selectively Ignoring Files to Sign
----------------------------------

It is common to want to ignore certain files from signing. For example,
you may ship a pre-built binary that already has a valid code signature.
Here's how you can do that.

.. code-block:: python

    # Define a function that will be called for every signing request that
    # can influence operation.
    def code_signer_callback(request):
        # Match a known filename that doesn't need signed and set
        # `prevent_signing = True` to prevent it from being signed.
        if request.filename == "vcruntime140.dll":
            request.prevent_signing = True


    signer = code_signer_from_windows_store_sha1_thumbprint("deadbeefdeadbeefdeadbeefdeadbeefdeadbeef")
    signer.set_signing_callback(code_signer_callback)
    signer.activate()

You could even use the :py:func:`prompt_confirm` function to prompt whether
to sign each file:

.. code-block:: python

   def code_signer_callback(request):
       request.prevent_signing = not prompt_confirm("sign %s?" % request.filename)

   signer = code_signer_from_...()
   signer.set_signing_callback(code_signer_callback)
   signer.activate()

.. _tugger_code_signing_certificates:

Understanding Code Signing Certificates
=======================================

A *code signing certificate* consists of a secure, private *key* and a
public *certificate* that describes itself to others. These components
are strictly separate but are often represented and stored together.

The public certificate is an X.509 certificate, much like those used in HTTP
to identify web sites. The main difference is that the certificate's subject
describes a person or organization (instead of a website) and the certificate
contains attributes that denote it for use by code signing.

Like web site X.509 certificates, code signing certificates are *signed*
by another X.509 certificate. This is called the *issuing* certificate.
There is often a *chain* of certificates - the *certificate chain* - leading
to a *self-signed* certificate (a certificate whose issuer was itself),
which is referred to as the *root* certificate.

Typically, the *certificate chain* is included in code signatures. This
enables readers of the signature to have full access to all relevant
certificates, without an implicit dependency on them being present on the
reading machine. This enables validation to be conducted more robustly.

.. _tugger_code_sigining_certificate_storage:

Code Signing Certificate Storage
--------------------------------

Code signing certificates can be stored in a number of formats. Here are the
popular ones:

* As standalone ``.pfx`` or ``.p12`` files. These are files containing data
  as defined by the PFX and PKCS #12 specifications. Most tools that support
  saving code signing certificates to files support this format if not use it
  by default.
* In your operating system's certificate store. Windows, macOS, and other
  operating systems have built-in functionality for storing and accessing
  certificates. On Windows, the ``certmgr.msc`` tool can be used to view
  certificates. On macOS, ``Keychain Access`` is the official GUI application.

In addition, the public X.509 certificates and the certificates in the
*certificate chain* are often represented as PEM. This is a human-readable
text format with content like ``-----BEGIN CERTIFICATE-----``. PEM is actually
base64 encoded BER/DER encoding of ASN.1 data structures, but that's not
important. What is important is public certificates are often stored in files
having this ``-----BEGIN CERTIFICATE-----`` content. These files often have
the extension ``.pem`` or ``.crt``.

The *certificate chain* is constant for the lifetime of a code signing
certificate. So it is possible to export these certificates to a persisted
file and reference this file when you need to access the issuer certificates
chain.

.. _tugger_securing_code_signing_certificate:

Securing Your Code Signing Certificate
--------------------------------------

Your code signing certificate's private key attests that its owner was in
possession of that certificate and has vouched for the integrity of whatever
it signed.

.. important::

    Code signing certificates can be very attractive theft targets for hackers, as
    possession of a code signing certificate enables you to sign software that
    can run on other machines and appears to be trusted. Therefore, it is often
    important to try to secure your code signing certificates!

The most secure way to store code signing certificates is in dedicated
hardware devices, such as HSMs or personal hardware tokens (such as YubiKeys).
Often, the private key component of the certificate is generated directly
in said hardware and it is impossible to export the private key and obtain its
raw value. Instead, operations like signing are issued to the hardware and
the hardware gives you the rest.

Tugger doesn't yet support interfacing directly with hardware devices. However,
we do have support for interfacing with the operating system's certificate
stores:

* On Windows, a certificate in the Windows certificate store can be referenced
  by its SHA-1 fingerprint. (This is the preferred mechanism to reference a
  certificate on Windows.)
* On Windows, a certificate in the Windows certificate store can be referenced
  by specifying a string to match against in the certificate's *subject* field.
  (This is less precise than specifying a certificate's SHA-1 fingerprint.)
* On Windows, you can tell the signing tool to automatically find the most
  appropriate certificate to use. It will look for a certificate in known
  certificate stores. (This is the least precise of all options available on
  Windows.)

.. note::

   Your operating system's certificate store can often interface with hardware
   devices holding code signing certificates. So Tugger's support for
   interfacing with the operating system store is often just as effective
   as interfacing directly with hardware devices.

   For example, on Windows, certificates stored in a YubiKey will be available
   if you have the `YubiKey Smart Card Minidriver <https://www.yubico.com/support/download/smart-card-drivers-tools/>`_
   installed.

**If Tugger doesn't support using a remote certificate, you will need to
export a certificate to a file and have Tugger use that. If you export your
certificate to a file, you should take care to secure that file as best you
can.**

File-based code signing certificates often exist in ``.pfx`` or ``.p12`` files.
These are often protected with a password. **You should use a strong and unique
password to secure this file.**

.. important::

    If someone else gains access to the file containing your code signing certificate,
    they will be able to perform an offline attack using as many compute resources as
    possible to guess your password and gain access to the code signing certificate.

You should take the following precautions to protect file-based code signing
certificates:

* Choose a strong, unique password for protecting the file content.
* Limit the time the files exist. If you can create the file only when needed,
  this is better than having the file linger on the filesystem.
* Limit the number of copies of the file. Every copy of the file is an
  opportunity for the file to be obtained by someone else.

.. _tugger_code_signing_apple:

Exporting a Code Signing Certificate from macOS Keychain
--------------------------------------------------------

Apple platforms require a code signing certificate issued by Apple to sign
distributed files.

If you have an Apple-issued code signing certificate, it is likely registered
in a *keychain* on your machine. Tugger doesn't currently support interfacing
directly with the macOS keychain and you will need to export your signing
certificate to a PFX / ``.p12`` file so Tugger can use it. Here's how to do that.

1. Press ``command + spacebar`` and search for and open the ``Keychain Access``
   application.
2. Make sure the correct keychain is selected. The keychain code signing
   certificates are typically located in is the ``login`` keychain under the
   ``Default Keychains`` list.
3. From the horizontal list of filters above the main pane, select
   ``Certificates`` (it is probably the last item).
4. Find the certificate you want to export. It likely has a name like
   ``Developer ID Application: <your name (some ID)>``
5. Do a double finger tap, right click, or ``File -> Export Items ...`` to
   bring up the export dialog.
6. For the file format, make sure ``Personal Information Exchange (.p12)``
   is selected.
7. Navigate to a folder where you want to save the file, choose an appropriate
   name, and click ``Save``.
8. You will be asked for a *password which will be used to protect the exported
   items*. Enter one. This password will need to be provided to Tugger later
   to unlock the content in the file.
9. You may be prompted to enter the password to the keychain to allow the
   key export. If so, enter that password.
10. You may be prompted multiple times. Just keep entering your keychain
    password(s) until it is done.
11. You are done! There should be a ``.p12`` file wherever you told ``Keychain
    Access`` to save it.

.. important::

   Please see :ref:`tugger_securing_code_signing_certificate` for important
   information on keeping your file-based code signing certificate secure.

.. _tugger_code_signing_windows_thumbprint:

Finding the Code Signing SHA-1 Thumbprint on Windows
----------------------------------------------------

On Windows, it is recommended to use code signing certificates in the Windows
certificate store and to specify those certificates via their SHA-1 thumbprint,
which should uniquely identify a certificate.

The Windows certificate store supports interfacing with hardware certificate
stores (such as YubiKeys and other hardware devices). So this method should work
with connected hardware certificate stores as well.

1. Press ``Windows Key + r`` to open the ``Run`` panel. Type in
   ``certmgr.msc`` and run that program.
2. Code signing certificates are likely under ``Personal`` -> ``Certificates``.
   Find that item in the tree and look for a certificate in the main pane.
3. Find the certificate you want to use and double click on it to view its
   details.
4. Open the ``Details`` tab.
5. In the table of fields, find and select ``Thumbprint``.
6. Copy the 40 character hexadecimal value that is printed.

The SHA-1 thumbprint can be fed into
:py:func:`code_signer_from_windows_store_sha1_thumbprint` to construct a
:py:class:`CodeSigner` that uses the specified certificate.

If the certificate is protected by a password or requires key to unlock,
you should see prompts to do that as Tugger attempts to sign things.

.. _tugger_code_signing_windows_export:

Exporting a Code Signing Certificate from Windows Certificate Store
-------------------------------------------------------------------

Code signing certificates on Windows are often stored in the Windows
certificate store.

.. important::

   Tugger has support for using certificates directly in the Windows
   certificate store. Exporting certificates to files will likely result
   in a net loss of security.

Here is how you can export a certificate to a PFX file.

1. Press ``Windows Key + r`` to open the ``Run`` panel. Type in
   ``certmgr.msc`` and run that program.
2. Code signing certificates are likely under ``Personal`` -> ``Certificates``.
   Find that item in the tree and look for a certificate in the main pane.
3. Double click on the certificate you want to export, open its ``Details``
   table, and click the ``Copy to File...`` button. This should open the
   *Certificate Export Wizard*.
4. Click ``Next``.
5. Make sure ``Yes, export the private key`` is selected and click ``Next``.
6. For the format, make sure the selected value is ``Personal Information
   Exchange PKCS #12 (PFX)``. For the checkboxes, check ``Include all
   certificates in the certificate path, if possible``. Then click ``Next``.
7. You should be prompted for a password. Enter a secure, unique password.
   In the ``Encryption`` drop-down, ensure ``TripleDES-SHA1`` is selected
   (we don't yet support ``AES256-SHA256``). Then click ``Next``.
8. Select a filename and click ``Next``.
9. Click ``Finish`` to close the wizard.

.. important::

   Please see :ref:`tugger_securing_code_signing_certificate` for important
   information on keeping your file-based code signing certificate secure.
