.. py:currentmodule:: starlark_tugger

================
``FileManifest``
================

.. py:class:: FileManifest

    The ``FileManifest`` type represents a set of files and their content.

    ``FileManifest`` instances are used to represent things like the final
    filesystem layout of an installed application.

    Conceptually, a ``FileManifest`` is a dict mapping relative paths to
    file content.

    .. py:method:: add_manifest(manifest: FileManifest)

        This method overlays another :py:class`FileManifest` on this one. If the
        other manifest provides a path already in this manifest, its content
        will be replaced by what is in the other manifest.

    .. py:method:: add_path(path: str, strip_prefix: string, force_read: bool = False)

        This method adds a file on the filesystem to the manifest.

        The following arguments are accepted:

        ``path``
           (``string``) The filesystem path to add.

        ``strip_prefix``
           (``string``) The string prefix to strip from the path. The remaining path
           will be stored in the manifest.

        ``force_read``
           (``bool``) Whether to read the file data into memory now.

           This can be set when reading temporary files.

    .. py:method:: install(path: str, replace: bool = True)

        This method writes the content of the :py:class:`FileManifest` to a
        directory specified by ``path``. The path is evaluated relative to the
        path specified by ``BUILD_PATH``.

        If ``replace`` is True (the default), the destination directory will
        be deleted and the final state of the destination directory should
        exactly match the state of the :py:class:`FileManifest`.
