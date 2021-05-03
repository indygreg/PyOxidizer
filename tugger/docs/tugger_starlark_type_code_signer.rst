.. py:currentmodule:: starlark_tugger

==============
``CodeSigner``
==============

.. py:class:: CodeSigner

    Instances of :py:class:`CodeSigner` are used to digitally sign code or
    content.

    When instances are registered in your Starlark configuration file, they
    will automatically be used to sign entities.

    See :ref:`tugger_code_signing` for details on what code signing is
    supported.

    .. py:method:: activate()

        Registers this instance with Tugger so that it is consulted when code signing
        events occur.

        Once this method is called, subsequent mutations to the instance may or may not
        be reflected with the instance that is registered to handle events.

        **Failure to call this method will mean this instance won't be queried to handle
        code signing events as Tugger runs.**

    .. py:method:: chain_issuer_certificates_pem_file(path: str)

        Register PEM encoded X.509 certificates located in a file to the certificate chain.

        The file should have content like ``-----BEGIN CERTIFICATE-----``.
        Multiple certificates can exist in a single file.

        See :ref:`tugger_code_signing_certificates` for the meaning of the certificate
        chain.

    .. py:method:: chain_issuer_certificates_macos_keychain()

        Register the issuer certificate chain by looking for certificates in the
        macOS keychain.

        This function only works on macOS and will raise errors when called on
        other platforms.

        See :ref:`tugger_code_signing_certificates` for the meaning of the certificate
        chain.

    .. py:method:: set_time_stamp_server(path: str)

        Set the URL of a Time-Stamp Protocol server to use.

        Calling this is not necessary when signing Apple primitives, as Apple's
        server will be used automatically.

        Calling this will force the use of a particular time-stamp protocol server.

    .. py:method:: set_signing_callback(f: Callable)

        Defines a function that will be invoked when Tugger has encountered a
        signable entity that this instance is capable of signing.

        The function's signature is:
        ``def callback(request: CodeSigningRequest) -> Union[bool, dict, None]``.

        The function receives as its arguments:

        ``request``
           The :py:class:`CodeSigningRequest` that is about to be signed.

        The :py:class:`CodeSigningRequest` passed in is unique to this
        :py:class:`CodeSigner` instance and can be used to inspect the imminent
        code signing operation or influence how it is performed - even preventing
        it entirely. See :py:class:`CodeSigningRequest` for the full API
        documentation.

Constructor Functions
=====================

.. py:function:: code_signer_from_pfx_file(path: str, password: str) -> CodeSigner

    Construct a :py:class:`CodeSigner` by specifying the path to a PFX file.

    PFX files are commonly used to hold code a code signing key and
    its corresponding x509 certificate. These files typically have the
    extension ``.pfx`` or ``.p12``.

    PFX files require a password to read. It is possible for the
    password to be the empty string (``""``). If you did not supply a
    password when exporting the code signing certificate, the password
    is likely the empty string.

    The password can be collected interactively via the :py:func:`prompt_password`
    function.

.. py:function:: code_signer_from_windows_store_sha1_thumbprint(thumbprint: str, store: str = "my") -> CodeSigner

    Construct a :py:class:`CodeSigner` that uses a certificate in the Windows
    certificate store having the specified SHA-1 thumbprint.

    This is the most reliable way to specify a certificate in the Windows
    certificate store, as SHA-1 thumbprints should uniquely identify a
    certificate.

    ``store`` denotes the Windows certificate store to use. Possible values are
    ``my``, ``root``, ``trust``, ``ca``, and ``userds`` (all case-insensitive).
    The meaning of these values is described in
    `Microsoft's documentation <https://docs.microsoft.com/en-us/windows/win32/seccrypto/system-store-locations>`_.

.. py:function:: code_signer_from_windows_store_subject(subject: str, store: str = "my") -> CodeSigner

    Construct a :py:class:`CodeSigner` using a code signing certificate in a
    Windows certificate store.

    ``subject`` defines a string value that is used to locate the certificate in
    the store. The string value is matched against the ``subject`` field of
    the certificate (who the certificate was issued to). Its value is often
    the name of someone or something.

    See :py:func:`code_signer_from_windows_store_sha1_thumbprint` for accepted
    values for the ``store`` argument.

.. py:function:: code_signer_from_windows_store_auto() -> CodeSigner

    Construct a :py:class:`CodeSigner` that automatically chooses a code signing
    certificate from the Windows certificate store.

    This will choose the *best available* found certificate. The heuristics
    are not well-defined and may change over time. For reliable results,
    use a different method.
