.. py:currentmodule:: starlark_tugger

======================
``PythonWheelBuilder``
======================

.. py:class:: PythonWheelBuilder

    The ``PythonWheelBuilder`` type facilitates creating Python wheels (``.whl`` files)
    from settings and file content.

    Python wheels are zip files with some well-defined files describing the wheel
    and the entity that is packaged. See
    `PEP 427 <https://www.python.org/dev/peps/pep-0427/>`_ for more on the wheel
    format and how it works.

    By default, new instances target the *compatibility tag* ``py3-none-any``. This
    is suitable for a wheel containing pure Python code (``.py`` files) and no
    binary files. If your wheel contains binary files or is limited in the
    Python compatibility in any way, you should modify the *compatibility* tag
    by setting instance attributes accordingly.

    By default, the ``.dist-info/WHEEL``, ``.dist-info/METADATA``, and
    ``.dist-info/RECORD`` files will be derived automatically from settings upon
    wheel creation. It is possible to provide your own custom file content for
    the ``.dist-info/WHEEL`` and ``.dist-info/METADATA`` files by calling
    :py:meth:`PythonWheelBuilder.add_file_dist_info`. A custom
    ``.dist-info/RECORD`` file, if provided, will be ignored.

    .. py:method:: __init__(distribution: str, version: str) -> PythonWheelBuilder

        Construct a new instance to produce a wheel for a given ``distribution``
        (read: Python package) and ``version`` of that *distribution*.

    .. py:attribute:: build_tag

        (``Optional[str]``)

        The *build tag* for this wheel. This constitutes an extra component in the
        wheel's filename and metadata.

        *Build tags* are typically not set on released versions: only for
        in-development, pre-release versions.

    .. py:attribute:: tag

        (``str``)

        The *compatibility tag* for this wheel.

        This is equivalent to ``{python_tag}-{abi_tag}-{platform_tag}``.

    .. py:attribute:: python_tag

        (``str``)

        The *Python tag* component of the wheel's *compatibility tag*. This should
        be a value like ``py3`` or ``py39``.

    .. py:attribute:: abi_tag

        (``str``)

        The *ABI tag* component of the wheel's *compatibility tag*. This should be
        a value like ``none``, ``abi3``, or ``cp39``.

    .. py:attribute:: platform_tag

        (``str``)

        The *platform tag* component of the wheel's *compatibility tag*. This should
        be a value like ``any``, ``linux_x86_64``, ``manylinux2010_x86_64``,
        ``macosx_10_9_x86_64``, etc.

    .. py:attribute:: generator

        (``str``)

        Describes the thing that constructed the wheel. This value is added to
        the default ``.dist-info/WHEEL`` file produced for this instance.

    .. py:attribute:: root_is_purelib

        (``bool``)

        The value for the ``Root-Is-Purelib`` setting for the wheel.

        If ``True``, the wheel is extracted to Python's ``purelib`` directory.
        If ``False``, to ``platlib``.

        This should be set to ``True`` if the wheel contains pure Python files
        (no binary files).

    .. py:attribute:: modified_time

        (``int``)

        The file modification time for files in wheel zip archives in seconds since
        UNIX epoch.

        Default value is the time this instance was created.

    .. py:attribute:: wheel_file_name

        (read-only ``str``)

        The file name the wheel should be materialized as.

        Wheel filenames are derived from the distribution, version, build tag, and
        *compatibility tag*.

    .. py:method:: add_file_dist_info(file: FileContent, path: Optional[str] = None, directory: Optional[str] = None)

        Add a :py:class:`FileContent` to the wheel in the ``.dist-info/``
        directory for the distribution being packaged.

        If neither ``path`` nor ``directory`` are specified, the file will be
        materialized in the ``.dist-info/`` directory with the filename given
        by :py:attr:`FileContent.filename`.

        If ``path`` is provided, it defines the exact path under ``.dist-info/``
        to use.

        If ``directory`` is provided, the path is effectively
        ``os.path.join(directory, file.filename)``.

    .. py:method:: add_file_data(destination: str, file: FileContent, path: Optional[str] = None, directory: Optional[str] = None)

        Add a :py:class:`FileContent` to the wheel in a
        ``.data/<destination>/`` directory.

        ``destination`` represents a known Python installation directory. Recognized
        values include ``purelib``, ``platlib``, ``headers``, ``scripts``, ``data``.
        ``destination`` effectively maps different file types to appropriate
        installation paths on wheel installation.

        If neither ``path`` nor ``directory`` are specified, the file will be
        materialized in the ``.data/<destination>>`` directory with the filename given
        by :py:attr:`FileContent.filename`.

        If ``path`` is provided, it defines the exact path under ``.data/<destination>``
        to use.

        If ``directory`` is provided, the path is effectively
        ``os.path.join(directory, file.filename)``.

    .. py:method:: add_file(file: FileContent, path: Optional[str] = None, directory: Optional[str] = None)

        Add a :py:class:`FileContent` to the wheel.

        If neither ``path`` nor ``directory`` are specified, the file will be
        materialized in the root directory with the filename given by
        :py:attr:`FileContent.filename`.

        If ``path`` is provided, it defines the exact path in the wheel.

        If ``directory`` is provided, the path is effectively
        ``os.path.join(directory, file.filename)``.

    .. py:method:: to_file_content() -> FileContent

        Obtain a :py:class:`FileContent` representing the built wheel.

        The returned instance will have its :py:attr:`FileContent.filename` set to
        the appropriate name for this wheel given current settings. The data in
        the file should be a zip archive containing a well-formed Python wheel.

    .. py:method:: write_to_directory(path: str) -> str

        Write a ``.whl`` file to the given directory (specified by ``path``) with
        the current state in this builder instance.

        Returns the path of the written file.

    .. py:method:: build(target: str) -> ResolvedTarget

        Build the instance.

        This is equivalent to :py:meth:`PythonWheelBuilder.write_to_directory()`, writing
        out the wheel to the build directory for the named target.
