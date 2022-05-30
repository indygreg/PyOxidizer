.. py:currentmodule:: starlark_pyoxidizer

======================
``PythonDistribution``
======================

.. py:class:: PythonDistribution

    The :py:class:`PythonDistribution` type defines a Python distribution. A Python
    distribution is an entity that defines an implementation of Python. This
    entity can be used to create a binary embedding or running Python and
    can be used to execute Python code.

    Instances of :py:class:`PythonDistribution` can be constructed via a constructor
    function or via
    :py:func:`default_python_distribution`.


    .. py:method:: __init__(sha256: str, local_path: Optional[string] = None, url: Optional[string], flavor: Optional[string] = None) -> PythonDistribution

        Construct an instance from arguments.

        The following arguments are accepted:

        ``sha256``
           The SHA-256 of the distribution archive file.

        ``local_path``
           Local filesystem path to the distribution archive.

        ``url``
           URL from which a distribution archive can be obtained using an HTTP
           GET request.

        ``flavor``
           The distribution flavor. Must be ``standalone``.

        A Python distribution is a zstandard-compressed tar archive containing a
        specially produced build of Python. These distributions are typically
        produced by the
        `python-build-standalone <https://github.com/indygreg/python-build-standalone>`_
        project. Pre-built distributions are available at
        https://github.com/indygreg/python-build-standalone/releases.

        A distribution is defined by a location and a hash.

        One of ``local_path`` or ``url`` MUST be defined.

        Examples:

        .. code-block:: python

           linux = PythonDistribution(
               sha256="11a53f5755773f91111a04f6070a6bc00518a0e8e64d90f58584abf02ca79081",
               local_path="/var/python-distributions/cpython-linux64.tar.zst"
           )

           macos = PythonDistribution(
                sha256="b46a861c05cb74b5b668d2ce44dcb65a449b9fef98ba5d9ec6ff6937829d5eec",
                url="https://github.com/indygreg/python-build-standalone/releases/download/20190505/cpython-3.7.3-macos-20190506T0054.tar.zst"
           )

    .. py:method:: python_resources() -> list[Union[PythonModuleSource, PythonExtensionModule, PythonPackageResource]]

        Returns objects representing Python resources in this distribution. Returned
        values can be
        :py:class:`PythonModuleSource`, :py:class:`PythonExtensionModule`,
        :py:class:`PythonPackageResource`, etc.

        There may be multiple :py:class:`PythonExtensionModule` with the same name.

    .. py:method:: make_python_interpreter_config() -> PythonInterpreterConfig

        Obtain a :py:class:`PythonInterpreterConfig` derived from the
        distribution.

        The interpreter configuration automatically uses settings appropriate
        for the distribution.

    .. py:method:: make_python_packaging_policy() -> PythonPackagingPolicy

        Obtain a :py:class:`PythonPackagingPolicy` derived from the distribution.

        The policy automatically uses settings globally appropriate for the
        distribution.

    .. py:method:: to_python_executable(name: str, packaging_policy: PythonPackagingPolicy, config: PythonInterpreterConfig) -> PythonExecutable

        This method constructs a :py:class:`PythonExecutable` instance. It
        essentially says *build an executable embedding Python from this
        distribution*.

        The accepted arguments are:

        ``name``
           The name of the application being built. This will be used to construct the
           default filename of the executable.

        ``packaging_policy``
           The packaging policy to apply to the executable builder.

           This influences how Python resources from the distribution are added. It
           also influences future resource adds to the executable.

        ``config``
           The default configuration of the embedded Python interpreter.

           Default is what :py:meth:`make_python_interpreter_config` returns.

        .. important::

           Libraries that extension modules link against have various software
           licenses, including GPL version 3. Adding these extension modules will
           also include the library. This typically exposes your program to additional
           licensing requirements, including making your application subject to that
           license and therefore open source. See :ref:`licensing_considerations` for
           more.

``default_python_distribution()``
=================================

.. py:function:: default_python_distribution(flavor: str = "standalone", build_target: str = BUILD_TARGET, python_version: str = "3.10") -> PythonDistribution

    Resolves the default :py:class:`PythonDistribution`.

    The following named arguments are accepted:

    ``flavor``
       Denotes the *distribution* flavor. See the section below on
       allowed values.

    ``build_target``
       Denotes the machine target triple that we're building for.

       Defaults to the value of the ``BUILD_TARGET`` global constant.

    ``python_version``
       ``X.Y`` *major.minor* string denoting the Python release version
       to use.

       Supported values are ``3.8``, ``3.9``, and ``3.10``.

    ``flavor`` is a string denoting the distribution *flavor*. Values can be one
    of the following:

    ``standalone``
       A distribution produced by the ``python-build-standalone`` project. The
       distribution may be statically or dynamically linked, depending on the
       ``build_target`` and availability. This option effectively chooses the
       best available ``standalone_dynamic`` or ``standalone_static`` option.

       This option is effectively ``standalone_dynamic`` for all targets except
       musl libc, where it is effectively ``standalone_static``.

    ``standalone_dynamic``
       This is like ``standalone`` but guarantees the distribution is dynamically
       linked against various system libraries, notably libc. Despite the
       dependence on system libraries, binaries built with these distributions can
       generally be run in most environments.

       This flavor is available for all supported targets except musl libc.

    ``standalone_static``
       This is like ``standalone`` but guarantees the distribution is statically
       linked and has minimal - possibly none - dependencies on system libraries.

       On Windows, the Python distribution does not export Python's symbols,
       meaning that it is impossible to load dynamically linked Python extensions
       with it.

       On musl libc, statically linked distributions do not support loading
       extension modules existing as shared libraries.

       This flavor is only available for Windows and musl libc targets.

    .. note::

       The *static* versus *dynamic* terminology refers to the linking of the
       overall distribution, not ``libpython`` or the final produced binaries.

    The ``pyoxidizer`` binary has a set of known distributions built-in
    which are automatically available and used by this function. Typically you don't
    need to build your own distribution or change the distribution manually.
