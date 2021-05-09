.. py:currentmodule:: starlark_tugger

===============
``FileContent``
===============

.. py:class:: FileContent

    This type represents the content of a single file.

    Instances essentially track the following:

    * The content of a file (either a reference to a filesystem path or in-memory data).
    * Whether the file is executable.
    * The filename associated with the content. This is just the file name: directory
      components are not allowed.

    Unfortunately, since Starlark doesn't expose a ``bytes`` type, we are
    unable to expose the raw content tracked by instances of this type.

    .. py:attribute:: executable

        (``bool``)

        Whether a materialized file should be marked as executable.

    .. py:attribute:: filename

        (``str``)

        The filename associated with this instance.

        This is just the filename.

    .. py:method:: __init__(path: Optional[str] = None, filename: Optional[str] = None, content: Optional[str] = None, executable: Optional[bool] = None) -> FileContent

        Construct a new instance given an existing filesystem ``path`` or string ``content``.

        1 of ``path`` or ``content`` must be provided to define the content
        tracked by this instance.

        If ``content`` is provided, ``filename`` must also be provided.

        ``filename`` must be just a file name: no directory components are allowed.

        If ``path`` is provided, it must refer to an existing filesystem path or an
        error will occur. Relative paths are interpreted as relative to the
        global ``CWD`` variable. Absolute paths are used as-is.

        If ``path`` is provided, by default ``filename`` and ``executable`` will be
        resolved from the given path. However, if the ``filename`` or ``executable``
        arguments are not ``None``, their values will be override those derived from
        ``path``.

        If ``content`` is provided and ``executable`` is not, ``executable`` defaults
        to ``False``.

    .. py:method:: write_to_directory(path: str) -> str

        Materialize this instance as a file in a directory.

        Absolute paths are treated as is. Relative paths are relative to the
        currently configured build directory.

        Returns the absolute path of the file that was written.
