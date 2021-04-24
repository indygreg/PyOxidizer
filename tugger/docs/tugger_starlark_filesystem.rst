.. py:currentmodule:: starlark_tugger

.. _tugger_starlark_filesystem:

=============================================
Functions for Interacting with the Filesystem
=============================================

.. py:function:: glob(include=List[str], exclude=Optional[List[str]], strip_prefix=Optional[str]) -> FileManifest

    The ``glob()`` function resolves file patterns to a
    :py:class:`starlark_tugger.FileManifest`.

    This function accepts the following arguments:

    ``include``
       Defines file patterns that will be matched using the ``glob`` Rust crate.
       If patterns begin with ``/`` or look like a filesystem absolute path,
       they are absolute. Otherwise they are evaluated relative to the directory
       of the current config file.

    ``exclude``
       File patterns used to exclude files from the result. All patterns in
       ``include`` are evaluated before ``exclude``.

    ``strip_prefix``
       Prefix to strip from the beginning of matched files. ``strip_prefix`` is
       stripped after ``include`` and ``exclude`` are processed.
