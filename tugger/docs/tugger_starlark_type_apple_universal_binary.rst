.. py:currentmodule:: starlark_tugger

========================
``AppleUniversalBinary``
========================

.. py:class:: AppleUniversalBinary

    Represents a *universal*/*fat*/*multi-architecture* Mach-O binary - the
    executable file format used by Apple operating systems.

    Instances exist to facilitate the creation of universal binaries from
    source Mach-O binaries. This type provides similar functionality to
    the ``lipo`` tool, which is Apple's tool for interfacing with universal
    binaries.

    .. py:method:: __init__(filename: str) -> AppleUniversalBinary

        Construct a new instance representing an empty binary having the given
        ``filename``.

    .. py:method:: add_path(path: str)

        Add a binary from a given filesystem path to this instance.

        This effectively marks the binary for inclusion when we go to produce
        a new *universal* binary.

        The file can be a single architecture Mach-O or *universal* Mach-O. If
        *universal*, all architectures within that file will be added.

    .. py:method:: add_file(content: FileContent)

        Add a binary from the given :py:class:`FileContent` instance to this instance.

        This is like :py:meth:`AppleUniversalBinary.add_path` except the content of
        the binary comes from a :py:class:`FileContent` instance instead of the
        filesystem.

    .. py:method:: to_file_content() -> FileContent

        Convert this instance to a :py:class:`FileContent`.

        The content of the returned object will be a just-in-time produced *universal*
        Mach-O binary.

    .. py:method:: write_to_directory(path: str) -> str

        Write a file containing this *universal* Mach-O binary into the directory
        specified.

        Absolute paths are accepted as-is. Relative paths are relative to the
        currently configured *build* path.

        Returns the absolute path of the written file.
