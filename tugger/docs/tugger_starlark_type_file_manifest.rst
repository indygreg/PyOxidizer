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

    .. py:method:: add_file(content: FileContent, path: Optional[str] = None, directory: Optional[str] = None)

        Add a :py:class:`FileContent` instance to this manifest, optionally controlling
        its path within the manifest.

        If neither ``path`` nor ``directory`` are specified, the file will be
        materialized in the root directory of the manifest with the filename
        given by :py:attr:`FileContent.filename`.

        If ``path`` is provided, it defines the exact path within the manifest
        to use.

        If ``directory`` is provided, the manifest path is effectively computed the
        same as ``os.path.join(directory, content.filename)``.

        An error occurs if both ``path`` and ``directory`` are non-``None``.

    .. py:method:: add_path(path: str, strip_prefix: str, force_read: bool = False)

        This method adds a file on the filesystem to the manifest.

        The following arguments are accepted:

        ``path``
           The filesystem path to add.

        ``strip_prefix``
           The string prefix to strip from the path. The remaining path
           will be stored in the manifest.

        ``force_read``
           Whether to read the file data into memory now.

           This can be set when reading temporary files.

    .. py:method:: get_file(path: str) -> Optional[FileContent]

        Obtain a :py:class:`FileContent` at a given path in the manifest, or
        ``None`` if no such path exists in the manifest.

        A copy of the content in the :py:class:`FileManifest` is returned and
        mutations on the returned object will not be reflected in the
        :py:class:`FileManifest`.

    .. py:method:: install(path: str, replace: bool = True)

        This method writes the content of the :py:class:`FileManifest` to a
        directory specified by ``path``. The path is evaluated relative to the
        path specified by ``BUILD_PATH``.

        If ``replace`` is True (the default), the destination directory will
        be deleted and the final state of the destination directory should
        exactly match the state of the :py:class:`FileManifest`.

        Upon successful materialization of all files in the manifest, all written
        files will be assessed for code signing with the ``file-manifest-install``
        *action*.

    .. py:method:: paths() -> list[str]

        Obtain all paths currently tracked by this instance.

    .. py:method:: remove(path: str) -> Optional[FileContent]

        Remove the entry in this manifest at ``path``, returning a :py:class:`FileContent`
        representing the removed entry if there was one or ``None`` if the path
        isn't tracked by the manifest.
