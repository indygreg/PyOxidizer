.. py:currentmodule:: starlark_tugger

======================
``CodeSigningRequest``
======================

.. py:class:: CodeSigningRequest

    This type represents the invocation of and settings for a single code signing
    operation.

    When :py:class:`CodeSigner` instances are registered with Tugger, they can
    optionally register a callback function via
    :py:meth:`CodeSigner.set_signing_callback` to influence the imminent code
    signing operation. This type is used to convey information about the
    code signing operation and to influence its settings.

    Instances are constructed internally by Tugger and cannot be constructed
    via Starlark.

    .. py:attribute:: action

        (read-only ``str``)

        The named action that triggered this code signing request.

    .. py:attribute:: filename

        (read-only ``str``)

        The filename this request is associated with. This is only the
        filename: not a full filesystem path.

    .. py:attribute:: path

        (read-only ``Union[str, None]``)

        The filesystem path this request is associated with. May be ``None``.
        The path may be a *virtual* path, such as one tracked in a
        :py:class:`FileManifest` instance.

    .. py:attribute:: defer

        (write-only ``bool``)

        Whether to defer processing of this request to another signer.

        Normally, the first :py:class:`CodeSigner` that is capable of signing
        something attempts to sign it and :py:class:`CodeSigner` traversal is
        stopped. Setting this to ``True`` will enable additional
        :py:class:`CodeSigner` (or callback functions on the same signer) to
        encounter this request.

    .. py:attribute:: prevent_signing

        (write-only ``bool``)

        If set to ``True``, the resource will not be signed and the signing
        attempt will be aborted.
