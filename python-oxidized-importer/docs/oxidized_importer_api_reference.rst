.. py:currentmodule:: oxidized_importer

.. _oxidized_importer_api_reference:

=============
API Reference
=============

Module Level Functions
======================

.. py:function:: decode_source(io_module, source_bytes) -> str

   Decodes Python source code ``bytes`` to a ``str``.

   This is effectively a reimplementation of
   ``importlib._bootstrap_external.decode_source()``

.. py:function:: find_resources_in_path(path) -> List

   This function will scan the specified filesystem path and return an
   iterable of objects representing found resources. Those objects will be 1
   of the types documented in :ref:`oxidized_importer_python_resource_types`.

   Only directories can be scanned.

.. py:function:: register_pkg_resources()

   Enables ``pkg_resources`` integration.

   This function effectively does the following:

   * Calls ``pkg_resources.register_finder()`` to map
     :py:class:`OxidizedPathEntryFinder` to
     :py:func:pkg_resources_find_distributions`.
   * Calls ``pkg_resources.register_load_type()`` to map
     :py:class:`OxidizedFinder` to :py:class:`OxidizedPkgResourcesProvider`.

   It is safe to call this function multiple times, as behavior should
   be deterministic.

.. py:function:: pkg_resources_find_distributions(finder: OxidizedPathEntryFinder, path_item: str, only=false) -> list

   Resolve ``pkg_resources.Distribution`` instances given a
   :py:class:`OxidizedPathEntryFinder` and search criteria.

   This function is what is registered with ``pkg_resources`` for distribution
   resolution and you likely don't need to call it directly.

The ``OxidizedFinder`` Class
============================

.. py:class:: OxidizedFinder

    A `meta path finder`_ that resolves indexed resources. See
    See :ref:`oxidized_finder` for more high-level documentation.

    This type implements the following interfaces:

    * ``importlib.abc.MetaPathFinder``
    * ``importlib.abc.Loader``
    * ``importlib.abc.InspectLoader``
    * ``importlib.abc.ExecutionLoader``

    See the `importlib.abc documentation <https://docs.python.org/3/library/importlib.html#module-importlib.abc>`_
    for more on these interfaces.

    In addition to the methods on the above interfaces, the following methods
    defined elsewhere in ``importlib`` are exposed:

    * ``get_resource_reader(fullname: str) -> importlib.abc.ResourceReader``
    * ``find_distributions(context: Optional[DistributionFinder.Context]) -> [Distribution]``

    ``ResourceReader`` is documented alongside other ``importlib.abc`` interfaces.
    ``find_distribution()`` is documented in
    `importlib.metadata <https://docs.python.org/3/library/importlib.metadata.html>`_.

    Instances have additional functionality beyond what is defined by
    ``importlib``. This functionality allows you to construct, inspect, and
    manipulate instances.

    .. py:attribute:: multiprocessing_set_start_method

        (``Opional[str]``) Value to pass to :py:func:`multiprocessing.set_start_method` on
        import of :py:mod:`multiprocessing` module.

        ``None`` means the method won't be called.

    .. py:attribute:: origin

        (``str``) The path this instance is using as the anchor for relative path
        references.

    .. py:attribute:: path_hook_base_str

        (``str``) The base path that the path hook handler on this instance
        will respond to.

        This value is often the same as ``sys.executable`` but isn't guaranteed
        to be that exact value.

    .. py:attribute:: pkg_resources_import_auto_register

       (``bool``) Whether this instance will be registered via
       ``pkg_resources.register_finder()`` upon this instance importing the
       ``pkg_resources`` module.

    .. py:method:: __new__(cls, relative_path_origin: Optional[os.PathLike]) -> OxidizedFinder

        Construct a new instance of :py:class:`OxidizedFinder`.

        New instances of :py:class:`OxidizedFinder` can be constructed like
        normal Python types:

        .. code-block:: python

            finder = OxidizedFinder()

        The constructor takes the following named arguments:

        ``relative_path_origin``
             A path-like object denoting the filesystem path that should be used as the
             *origin* value for relative path resources. Filesystem-based resources are
             stored as a relative path to an *anchor* value. This is that *anchor* value.
             If not specified, the directory of the current executable will be used.

        See the `python_packed_resources <https://docs.rs/python-packed-resources/0.1.0/python_packed_resources/>`_
        Rust crate for the specification of the binary data blob defining *packed
        resources data*.

        .. important::

            The *packed resources data* format is still evolving. It is recommended
            to use the same version of the ``oxidized_importer`` extension to
            produce and consume this data structure to ensure compatibility.

    .. py:method:: index_bytes(data: bytes) -> None

        This method parses any bytes-like object and indexes the resources within.

    .. py:method:: index_file_memory_mapped(path: pathlib.Path) -> None

        This method parses the given Path-like argument and indexes the resources
        within. Memory mapped I/O is used to read the file. Rust managed the
        memory map via the ``memmap`` crate: this does not use the Python
        interpreter's memory mapping code.

    .. py:method:: index_interpreter_builtins() -> None

        This method indexes Python resources that are built-in to the Python
        interpreter itself. This indexes built-in extension modules and frozen
        modules.

    .. py:method:: index_interpreter_builtin_extension_modules() -> None

        This method will index Python extension modules that are compiled into
        the Python interpreter itself.

    .. py:method:: index_interpreter_frozen_modules() -> None

        This method will index Python modules whose bytecode is frozen into
        the Python interpreter itself.

    .. py:method:: indexed_resources() -> List[OxidizedResource]

        This method returns a list of resources that are indexed by the
        instance. It allows Python code to inspect what the finder knows about.

        Any mutations to returned values are not reflected in the finder.

        See :ref:`oxidized_resource` for more on the returned type.

    .. py:method:: add_resource(resource: OxidizedResource)

        This method registers an :ref:`oxidized_resource` instance with the finder,
        enabling the finder to use it to service lookups.

        When an ``OxidizedResource`` is registered, its data is copied into the
        finder instance. So changes to the original ``OxidizedResource`` are not
        reflected on the finder. (This is because :py:class:`OxidizedFinder`
        maintains an index and it is important for the data behind that index to
        not change out from under it.)

        Resources are stored in an invisible hash map where they are indexed by
        the ``name`` attribute. When a resource is added, any existing resource
        under the same name has its data replaced by the incoming
        ``OxidizedResource`` instance.

        If you have source code and want to produce bytecode, you can do something
        like the following:

        .. code-block:: python

            def register_module(finder, module_name, source):
                code = compile(source, module_name, "exec")
                bytecode = marshal.dumps(code)

                resource = OxidizedResource()
                resource.name = module_name
                resource.is_module = True
                resource.in_memory_bytecode = bytecode
                resource.in_memory_source = source

                finder.add_resource(resource)

    .. py:method:: add_resources(resources: List[OxidizedResource]

        This method is syntactic sugar for calling ``add_resource()`` for every
        item in an iterable. It is exposed because function call overhead in Python
        can be non-trivial and it can be quicker to pass in an iterable of
        ``OxidizedResource`` than to call ``add_resource()`` potentially hundreds
        of times.

    .. py:method:: serialize_indexed_resources(ignore_builtin=true, ignore_frozen=true) -> bytes

        This method serializes all resources currently indexed by the instance
        into an opaque ``bytes`` instance. The returned data can be fed into a
        separate :py:class:`OxidizedFinder` instance by passing it to
        :py:meth:`OxidizedFinder.__new__`.

        Arguments:

        ``ignore_builtin`` (bool)
           Whether to ignore ``builtin`` extension modules from the serialized data.

           Default is ``True``

        ``ignore_frozen`` (bool)
           Whether to ignore ``frozen`` extension modules from the serialized data.

           Default is ``True``.

        Entries for *built-in* and *frozen* modules are ignored by default because
        they aren't portable, as they are compiled into the interpreter and aren't
        guaranteed to work from one Python interpreter to another. The serialized
        format does support expressing them. Use at your own risk.

    .. py:method:: path_hook(path: Union[str, bytes, os.PathLike[AnyStr]]) -> OxidizedPathEntryFinder

        Implements a *path hook* for obtaining a
        `PathEntryFinder <https://docs.python.org/3/library/importlib.html#importlib.abc.PathEntryFinder>`_
        from a ``sys.path`` entry. See :ref:`oxidized_finder_path_hooks` for details.

        Raises ``ImportError`` if the given path isn't serviceable. The exception
        should have ``.__cause__`` set to an inner exception with more details on why
        the path was rejected.

The ``OxidizedDistribution`` Class
==================================

.. py:class:: OxidizedDistribution

   Represents the metadata of a Python package. Comparable to
   ``importlib.metadata.Distribution``. Instances of this type are emitted by
   ``OxidizedFinder.find_distributions``.

   .. py:method:: from_name(cls, name: str) -> OxidizedDistribution

      :classmethod:

      Resolve the instance for the given package name.

   .. py:method:: discover(cls, **kwargs) -> list[OxidizedDistribution]

      :classmethod:

      Resolve instances for all known packages.

   .. py:method:: read_text(filename) -> str

      Attempt to read metadata file given its filename.

   .. py:property:: metadata

      :type: email.message.EmailMessage

      Return the parsed metadata for this distribution.

   .. py:property:: name

      :type: str

      Return the ``Name`` metadata for this distribution package.

   .. py:property:: _normalized_name

      :type: str

      Return the normalized version of the ``Name``.

   .. py:property:: version

      :type: str

      Return the ``Version`` metadata for this distribution package.

   .. py:property:: entry_points

     Resolve entry points for this distribution package.

   .. py:property:: files

      Not implemented. Always raises when called.

   .. py:property:: requires

      Generated requirements specified for this distribution.

The ``OxidizedResourceReader`` Class
====================================

.. py:class:: OxidizedResourceReader

   ``importlib.abc.ResourceReader`` implementer for :py:class:`OxidizedFinder`.

   .. py:method:: open_resource(resource: str)

   .. py:method:: resource_path(resource: str)

   .. py:method:: is_resource(name: str) -> bool

   .. py:method:: contents() -> list[str]

The ``OxidizedPathEntryFinder`` Class
=====================================

.. py:class:: OxidizedPathEntryFinder

   A `path entry finder`_ that can find resources contained in an associated
   :py:class:`OxidizedFinder` instance.

   Instances are created via :py:meth:`OxidizedFinder.path_hook <OxidizedFinder.path_hook>`.

   Direct use of :class:`OxidizedPathEntryFinder` is generally unnecessary:
   :py:class:`OxidizedFinder` is the primary interface to the custom importer.

   See :ref:`oxidized_finder_path_hooks` for more on path hook and path entry finder
   behavior in ``oxidized_importer``.

   .. py:method:: find_spec(fullname: str, target: Optional[types.ModuleType] = None) -> Optional[importlib.machinery.ModuleSpec]

      Search for modules visible to the instance.

   .. py:method:: invalidate_caches() -> None

      Invoke the same method on the :py:class:`OxidizedFinder` instance with
      which the :class:`OxidizedPathEntryFinder` instance was constructed.

   .. py:method:: iter_modules(prefix: str = "") -> List[pkgutil.ModuleInfo]

      Iterate over the visible modules. This method complies with
      ``pkgutil.iter_modules``'s protocol.

The ``OxidizedPkgResourcesProvider`` Class
==========================================

.. py:class:: OxidizedPkgResourcesProvider

   A ``pkg_resources.IMetadataProvider`` and ``pkg_resources.IResourceProvider``
   enabling ``pkg_resources`` to access package metadata and resources.

   All members of the aforementioned interfaces are implemented. Divergence
   from ``pkg_resources`` defined behavior is documented next to the method.

   .. py:method:: has_metadata(name: str) -> bool

   .. py:method:: get_metadata(name: str) -> str

   .. py:method:: get_metadata_lines(name: str) -> List[str]

      Returns a ``list`` instead of a generator.

   .. py:method:: metadata_isdir(name: str) -> bool

   .. py:method:: metadata_listdir(name: str) -> List[str]

   .. py:method:: run_script(script_name: str, namespace: Any)

      Always raises ``NotImplementedError``.

      Please leave a comment in
      `#384 <https://github.com/indygreg/PyOxidizer/issues/384>`_ if you would like
      this functionality implemented.

   .. py:method:: get_resource_filename(manager, resource_name: str)

      Always raises ``NotImplementedError``.

      This behavior appears to be allowed given code in ``pkg_resources``.
      However, it means that ``pkg_resources.resource_filename()`` will not
      work. Please leave a comment in
      `#383 <https://github.com/indygreg/PyOxidizer/issues/383>`_ if you would like
      this functionality implemented.

   .. py:method:: get_resource_stream(manager, resource_name: str) -> io.BytesIO

   .. py:method:: get_resource_string(manager, resource_name: str) -> bytes

   .. py:method:: has_resource(resource_name: str) -> bool

   .. py:method:: resource_isdir(resource_name: str) -> bool

   .. py:method:: resource_listdir(resource_name: str) -> List[str]

      Returns a ``list`` instead of a generator.

The ``OxidizedResource`` Class
==============================

.. py:class:: OxidizedResource

   Represents a *resource* that is indexed by a
   :py:class:`OxidizedFinder` instance.

   Each instance represents a named entity with associated metadata and data.
   e.g. an instance can represent a Python module with associated source and
   bytecode.

   New instances can be constructed via ``OxidizedResource()``. This will return
   an instance whose ``name = ""`` and all properties will be ``None`` or
   ``false``.

   .. py:attribute:: is_module

      A ``bool`` indicating if this resource is a Python module. Python modules
      are backed by source or bytecode.

   .. py:attribute:: is_builtin_extension_module

      A ``bool`` indicating if this resource is a Python extension module
      built-in to the Python interpreter.

   .. py:attribute:: is_frozen_module

      A ``bool`` indicating if this resource is a Python module whose bytecode
      is frozen into the Python interpreter.

   .. py:attribute:: is_extension_module

      A ``bool`` indicating if this resource is a Python extension module.

   .. py:attribute:: is_shared_library

      A ``bool`` indicating if this resource is a shared library.

   .. py:attribute:: name

      The ``str`` name of the resource.

   .. py:attribute:: is_package

      A ``bool`` indicating if this resource is a Python package.

   .. py:attribute:: is_namespace_package

      A ``bool`` indicating if this resource is a Python namespace package.

   .. py:attribute:: in_memory_source

      ``bytes`` or ``None`` holding Python module source code that should be
      imported from memory.

   .. py:attribute:: in_memory_bytecode

      ``bytes`` or ``None`` holding Python module bytecode that should be
      imported from memory.

      This is raw Python bytecode, as produced from the ``marshal`` module.
      ``.pyc`` files have a header before this data that will need to be
      stripped should you want to move data from a ``.pyc`` file into this
      field.

   .. py:attribute:: in_memory_bytecode_opt1

      ``bytes`` or ``None`` holding Python module bytecode at optimization level 1
      that should be imported from memory.

      This is raw Python bytecode, as produced from the ``marshal`` module.
      ``.pyc`` files have a header before this data that will need to be
      stripped should you want to move data from a ``.pyc`` file into this
      field.

   .. py:attribute:: in_memory_bytecode_opt2

      ``bytes`` or ``None`` holding Python module bytecode at optimization level 2
      that should be imported from memory.

      This is raw Python bytecode, as produced from the ``marshal`` module.
      ``.pyc`` files have a header before this data that will need to be
      stripped should you want to move data from a ``.pyc`` file into this
      field.

   .. py:attribute:: in_memory_extension_module_shared_library

      ``bytes`` or ``None`` holding native machine code defining a Python extension
      module shared library that should be imported from memory.

   .. py:attribute:: in_memory_package_resources

      ``dict[str, bytes]`` or ``None`` holding resource files to make available to
      the ``importlib.resources`` APIs via in-memory data access. The ``name`` of
      this object will be a Python package name. Keys in this dict are virtual
      filenames under that package. Values are raw file data.

   .. py:attribute:: in_memory_distribution_resources

      ``dict[str, bytes]`` or ``None`` holding resource files to make available to
      the ``importlib.metadata`` API via in-memory data access. The ``name`` of
      this object will be a Python package name. Keys in this dict are virtual
      filenames. Values are raw file data.

   .. py:attribute:: in_memory_shared_library

      ``bytes`` or ``None`` holding a shared library that should be imported from
      memory.

   .. py:attribute:: shared_library_dependency_names

      ``list[str]`` or ``None`` holding the names of shared libraries that this
      resource depends on. If this resource defines a loadable shared library,
      this list can be used to express what other shared libraries it depends on.

   .. py:attribute:: relative_path_module_source

      ``pathlib.Path`` or ``None`` holding the relative path to Python module
      source that should be imported from the filesystem.

   .. py:attribute:: relative_path_module_bytecode

      ``pathlib.Path`` or ``None`` holding the relative path to Python module
      bytecode that should be imported from the filesystem.

   .. py:attribute:: relative_path_module_bytecode_opt1

      ``pathlib.Path`` or ``None`` holding the relative path to Python module
      bytecode at optimization level 1 that should be imported from the filesystem.

   .. py:attribute:: relative_path_module_bytecode_opt2

      ``pathlib.Path`` or ``None`` holding the relative path to Python module
      bytecode at optimization level 2 that should be imported from the filesystem.

   .. py:attribute:: relative_path_extension_module_shared_library

      ``pathlib.Path`` or ``None`` holding the relative path to a Python extension
      module that should be imported from the filesystem.

   .. py:attribute:: relative_path_package_resources

      ``dict[str, pathlib.Path]`` or ``None`` holding resource files to make
      available to the ``importlib.resources`` APIs via filesystem access. The
      ``name`` of this object will be a Python package name. Keys in this dict are
      filenames under that package. Values are relative paths to files from which
      to read data.

   .. py:attribute:: relative_path_distribution_resources

      ``dict[str, pathlib.Path]`` or ``None`` holding resource files to make
      available to the ``importlib.metadata`` APIs via filesystem access. The
      ``name`` of this object will be a Python package name. Keys in this dict are
      filenames under that package. Values are relative paths to files from which
      to read data.

The ``OxidizedResourceCollector`` Class
=======================================

.. py:class:: OxidizedResourceCollector

   Provides functionality for turning instances of Python resource types into a
   collection of :py:class:`OxidizedResource` for loading into an
   :py:class:`OxidizedFinder` instance.

   .. py:method:: __new__(cls, allowed_locations: list[str])

      Construct an instance by defining locations that resources can be loaded
      from.

      The accepted string values are ``in-memory`` and ``filesystem-relative``.

   .. py:attribute:: allowed_locations

      (``list[str]``) Exposes allowed locations where resources can be loaded from.

   .. py:method:: add_in_memory_resource(resource)

      Adds a Python resource type (:py:class:`PythonModuleSource`,
      :py:class:`PythonModuleBytecode`, etc) to the collector and marks it for
      loading via in-memory mechanisms.

   .. py:method:: add_filesystem_relative(prefix, resource)

      Adds a Python resource type (:py:class:`PythonModuleSource`,
      :py:class:`PythonModuleBytecode`, etc) to the collector and marks it for
      loading via a relative path next to some *origin* path (as specified to the
      :py:class:`OxidizedFinder`). That relative path can have a ``prefix`` value
      prepended to it. If no prefix is desired and you want the resource placed
      next to the *origin*, use an empty ``str`` for ``prefix``.

   .. py:method:: oxidize() -> tuple[list[OxidizedResource], list[tuple[pathlib.Path, bytes, bool]]]

      Takes all the resources collected so far and turns them into data
      structures to facilitate later use.

      The first element in the returned tuple is a list of
      :py:class:`OxidizedResource` instances.

      The second is a list of 3-tuples containing the relative filesystem
      path for a file, the content to write to that path, and whether the file
      should be marked as executable.

The ``OxidizedResourceReader`` Class
====================================

.. py:class:: OxidizedResourceResource

   An implementation of
   `importlib.abc.ResourceReader <https://docs.python.org/3.9/library/importlib.html#importlib.abc.ResourceReader>`_
   to facilitate resource reading from an :py:class:`OxidizedFinder`.

   See :ref:`resource_reader_support` for more.

The ``OxidizedZipFinder`` Class
===============================

.. py:class:: OxidizedZipFinder

   A `meta path finder`_ that operates on zip files.

   This type attempts to be a pure Rust reimplementation of the Python standard
   library ``zipimport.zipimporter`` type.

   This type implements the following interfaces:

   * ``importlib.abc.MetaPathFinder``
   * ``importlib.abc.Loader``
   * ``importlib.abc.InspectLoader``

   .. py:method:: from_zip_data(cls, source: bytes, path: Union[bytes, str, pathlib.Path, None] = None) -> OxidizedZipFinder

      Construct an instance from zip archive data.

      The source argument can be any bytes-like object. A reference to the
      original Python object will be kept and zip I/O will be performed against
      the memory tracked by that object. It is possible to trigger an
      out-of-bounds memory read if the source object is mutated after being
      passed into this function.

      The ``path`` argument denotes the path to the zip archive. This path will
      be advertised in ``__file__`` attributes. If not defined, the path of the
      current executable will be used.

   .. py:method:: from_path(cls, path: Union[bytes, str, pathlib.Path]) -> OxidizedZipFinder

      Construct an instance from a filesystem path.

      The source represents the path to a file containing zip archive data.
      The file will be opened using Rust file I/O. The content of the file
      will be read lazily.

      If you don't already have a copy of the zip data and the zip file will
      be immutable for the lifetime of the constructed instance, this method
      may yield better performance than opening the file, reading its content,
      and calling :py:meth:`OxidizedZipFinder.from_zip_data` because it may
      incur less overall I/O.

The ``PythonModuleSource`` Class
================================

.. py:class:: PythonModuleSource

   Represents Python module source code. e.g. a ``.py`` file.


   .. py:attribute:: module

      (``str``) The fully qualified Python module name. e.g.
      ``my_package.foo``.

   .. py:attribute:: source

      (``bytes``) The source code of the Python module.

      Note that source code is stored as ``bytes``, not ``str``. Most Python
      source is stored as ``utf-8``, so you can ``.encode("utf-8")`` or
      ``.decode("utf-8")`` to convert between ``bytes`` and ``str``.

   .. py:attribute:: is_package

      (``bool``) Whether this module is a Python package.

The ``PythonModuleBytecode`` Class
==================================

.. py:class:: PythonModuleBytecode

   Represents Python module bytecode. e.g. what a ``.pyc`` file holds (but
   without the header that a ``.pyc`` file has).

   .. py:attribute:: module

      (``str``) The fully qualified Python module name.

   .. py:attribute:: bytecode

      (``bytes``) The bytecode of the Python module.

      This is what you would get by compiling Python source code via
      something like ``marshal.dumps(compile(source, "exe"))``. The bytecode
      does **not** contain a header, like what would be found in a ``.pyc``
      file.

   .. py:attribute:: optimize_level

      (``int``) The bytecode optimization level. Either ``0``, ``1``, or ``2``.

   .. py:attribute:: is_package

      (``bool``) Whether this module is a Python package.

The ``PythonPackageResource`` Class
===================================

.. py:class:: PythonPackageResource

   Represents a non-module *resource* file. These are files that live next
   to Python modules that are typically accessed via the APIs in
   ``importlib.resources``.

   .. py:attribute:: package

      (``str``) The name of the leaf-most Python package this resource is
      associated with.

      With :py:class:`OxidizedFinder`, an ``importlib.abc.ResourceReader``
      associated with this package will be used to load the resource.

   .. py:attribute:: name

      (``str``) The name of the resource within its ``package``. This is
      typically the filename of the resource. e.g. ``resource.txt`` or
      ``child/foo.png``.

   .. py:attribute:: data

      (``bytes``) The raw binary content of the resource.

The ``PythonPackageDistributionResource`` Class
===============================================

.. py:class:: PythonPackageDistributionResource

   Represents a non-module *resource* file living in a package distribution
   directory (e.g. ``<package>-<version>.dist-info`` or
   ``<package>-<version>.egg-info``).

   These resources are typically accessed via the APIs in ``importlib.metadata``.

   .. py:attribute:: package

      (``str``) The name of the Python package this resource is associated with.

   .. py:attribute:: version

      (``str``) Version string of Python package this resource is associated with.

   .. py:attribute:: name

      (``str``) The name of the resource within the metadata distribution. This
      is typically the filename of the resource. e.g. ``METADATA``.

   .. py:attribute:: data

      (``bytes``) The raw binary content of the resource.

The ``PythonExtensionModule`` Class
===================================

.. py:class:: PythonExtensionModule

   Represents a Python extension module. This is a shared library
   defining a Python extension implemented in native machine code that
   can be loaded into a process and defines a Python module. Extension
   modules are typically defined by ``.so``, ``.dylib``, or ``.pyd``
   files.

   .. :py:attribute:: name

      (``str``) The name of the extension module.

.. note::

   Properties of this type are read-only.

.. rubric:: Footnotes

.. _meta path finder: https://docs.python.org/3/library/importlib.html#importlib.abc.MetaPathFinder

.. _path entry finder: https://docs.python.org/3/reference/import.html#path-entry-finders
