.. _config_filesystem:

=============================================
Functions for Interacting with the Filesystem
=============================================

.. _config_glob:

``glob()``
==========

The ``glob()`` function resolves file patterns to a
:ref:`tugger_starlark_type_file_manifest`.

This function accepts the following arguments:

``include``
   (``list`` of ``string``) Defines file patterns that will be
   matched using the ``glob`` Rust crate. If patterns begin with
   ``/`` or look like a filesystem absolute path, they are absolute.
   Otherwise they are evaluated relative to the directory of the
   current config file.

``exclude``
   (``list`` of ``string`` or ``None``) File patterns used to
   exclude files from the result. All patterns in ``include`` are
   evaluated before ``exclude``.

``strip_prefix``
   (``string`` or ``None``) Prefix to strip from the beginning of
   matched files. ``strip_prefix`` is stripped after ``include``
   and ``exclude`` are processed.

Returns a :ref:`tugger_starlark_type_file_manifest`.
