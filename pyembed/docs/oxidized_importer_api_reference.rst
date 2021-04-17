.. py:currentmodule:: oxidized_finder

.. _oxidized_importer_api_reference:

=============
API Reference
=============

The ``OxidizedFinder`` Class
============================

.. py:class:: OxidizedFinder

    A `meta-path finder`_ that resolves indexed resources. See
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

    .. py:method:: __new__(cls, relative_path_origin: Optional[PathLike]) -> OxidizedFinder

        Construct a new instance of :py:class:`OxidizedFinder`.

        New instances of ``OxidizedFinder`` can be constructed like normal
        Python types:

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

    .. py:method:: index_file_memory_mapped(path: Path) -> None

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
        reflected on the finder. (This is because ``OxidizedFinder`` maintains an
        index and it is important for the data behind that index to not change
        out from under it.)

        Resources are stored in an invisible hash map where they are indexed by
        the ``name`` attribute. When a resource is added, any existing resource
        under the same name has its data replaced by the incoming ``OxidizedResource``
        instance.

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
        separate ``OxidizedFinder`` instance by passing it to
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

        When ``path_hook``, bound to an ``OxidizedFinder`` instance ``self``, is in
        ``sys.path_hooks``, ``pkgutil.iter_modules`` can search ``self``'s embedded
        resources, filtering by its ``path`` argument. Additionally, if you add
        ``sys.executable`` to ``sys.path``, the meta-path finder
        ``importlib.machinery.PathFinder`` can find ``self``'s embedded resources.

        ``path``'s semantics match those of
        :ref:`oxidized_finder_behavior_and_compliance_path`. After normalization,
        ``path`` must be or be in ``sys.executable``; otherwise ``path_hook`` raises an
        ``ImportError``. If ``path`` is ``sys.executable``, top-level modules are
        accessible. Otherwise ``path_hook`` computes the requested package by stripping
        ``sys.executable`` from the beginning of ``path`` and replacing path separators
        with dots. The result is decoded to a ``str`` using the filesystem encoding. If
        that fails, ``path_hook`` raises an ``ImportError`` from the
        ``UnicodeDecodeError``.\ [#fn-decode-error]_

The ``OxidizedPathEntryFinder`` Class
=====================================

.. py:class:: OxidizedPathEntryFinder

   A `path-entry finder`_ that can find modules embedded in an ``OxidizedFinder``
   instance by searching paths at or under ``sys.executable``.
   Each :class:`OxidizedPathEntryFinder` instance is associated with the ``path``
   argument to :class:`OxidizedPathEntryFinder`'s only constructor,
   :meth:`OxidiziedFinder.path_hooh <OxidizedFinder.path_hook>`.
   Only modules embedded in the ``OxidizedFinder`` instance in the top level of
   the path are :dfn:`visible` to the :class:`OxidizedPathEntryFinder` instance.
   For    example, if ``path`` were ``os.path.join(``\ ``sys.executable``\ ``, 'a')``,
   then module ``a.b`` would be visible, but neither modules ``a`` nor ``a.b.c``
   would be visible. Further, ``a.b`` would be visible only if it were embedded
   in the ``OxidizedFinder`` instance that constructed the instance.

   This class complies with the `path-entry finder`_ protocol by providing
   compliant :meth:`~OxidizedPathEntryFinder.find_spec` and
   :meth:`~OxidizedPathEntryFinder.invalidate_caches` methods.
   However, support for the long-deprecated methods
   ``importlib.abc.PathEntryFinder.find_loader`` and
   ``importlib.abc.PathEntryFinder.find_module`` may be missing or incomplete.

   Direct use of :class:`OxidizedPathEntryFinder` is generally unnecessary. It exists
   primarily to support ``pkgutil.iter_modules`` via
   :meth:`OxidizedFinder.path_hook <OxidizedFinder.path_hook>`.

   .. py:method:: find_spec(fullname: str, target: Optional[types.ModuleType] = None) -> Optional[importlib.machinery.ModuleSpec]

      Search for modules visible to the instance.

   .. py:method:: invalidate_caches() -> None

      Invoke the same method on the ``OxidizedFinder`` instance with which the
      :class:`OxidizedPathEntryFinder` instance was constructed.

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

  .. py:method:: get_resource_filename(manager: pkg_resources.IResourceManager, resource_name: str)

      Always raises ``NotImplementedError``.

      This behavior appears to be allowed given code in ``pkg_resources``.

  .. py:method:: get_resource_stream(manager: pkg_resources.IResourceManager, resource_name: str) -> io.BytesIO

  .. py:method:: get_resource_string(manager: pkg_resources.IResourceManager, resource_name: str) -> bytes

  .. py:method:: has_resource(resource_name: str) -> bool

  .. py:method:: resource_isdir(resource_name: str) -> bool

  .. py:method:: resource_listdir(resource_name: str) -> List[str]

      Returns a ``list`` instead of a generator.

.. rubric:: Footnotes

.. _meta-path finder: https://docs.python.org/3/library/importlib.html#importlib.abc.MetaPathFinder

.. _path-entry finder: https://docs.python.org/3/reference/import.html#path-entry-finders

.. [#fn-decode-error]
   This is required by the `path-entry finder`_ protocol.
