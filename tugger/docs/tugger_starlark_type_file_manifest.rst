.. _tugger_starlark_type_file_manifest:

================
``FileManifest``
================

The ``FileManifest`` type represents a set of files and their content.

``FileManifest`` instances are used to represent things like the final
filesystem layout of an installed application.

Conceptually, a ``FileManifest`` is a dict mapping relative paths to
file content.

Methods
=======

.. _tugger_starlark_type_file_manifest_add_manifest:

``FileManifest.add_manifest()``
-------------------------------

This method overlays another ``FileManifest`` on this one. If the other
manifest provides a path already in this manifest, its content will be
replaced by what is in the other manifest.

``FileManifest.install()``
--------------------------

This method writes the content of the ``FileManifest`` to a directory
specified by ``path``. The path is evaluated relative to the path
specified by ``BUILD_PATH``.

If ``replace`` is True (the default), the destination directory will
be deleted and the final state of the destination directory should
exactly match the state of the ``FileManifest``.
