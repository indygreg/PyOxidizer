.. _oxidized_finder:

==============================
``OxidizedFinder`` Python Type
==============================

``oxidized_importer.OxidizedFinder`` is a Python type that implements a
custom *meta path finder*. *Oxidized* is in its name because it is
implemented in Rust.

Unlike traditional *meta path finders* which have to dynamically
discover resources (often by scanning the filesystem), ``OxidizedFinder``
instances maintain an *index* of known resources. When a resource is
requested, ``OxidizedFinder`` can retrieve that resource by effectively
performing 1 or 2 lookups in a Rust ``HashMap``. This makes resource
resolution extremely efficient.

Instances of ``OxidizedFinder`` are optionally bound to a binary blob
holding *packed resources data*. This is a custom serialization format
for expressing Python modules (source and bytecode), Python extension
modules, resource files, shared libraries, etc. This data format
along with a Rust library for interacting with it are defined by the
`python-packed-resources <https://crates.io/crates/python-packed-resources>`_
crate.

When an ``OxidizedFinder`` instance is created, the *packed resources
data* is parsed into a Rust data structure. On a modern machine, parsing
this resources data for the entirety of the Python standard library
takes ~1 ms.

``OxidizedFinder`` instances can index *built-in* extension modules
and *frozen* modules, which are compiled into the Python interpreter. This
allows ``OxidizedFinder`` to subsume functionality normally provided by
the ``BuiltinImporter`` and ``FrozenImporter`` *meta path finders*,
allowing you to potentially replace ``sys.meta_path`` with a single
instance of ``OxidizedFinder``.

.. _oxidized_finder_in_pyoxidizer:

``OxidizedFinder`` in PyOxidizer Applications
=============================================

When running from an application built with PyOxidizer (or using the
``pyembed`` crate directly), an ``OxidizedFinder`` instance will (likely)
be automatically registered as the first element in ``sys.meta_path`` when
starting a Python interpreter.

You can verify this inside a binary built with PyOxidizer::

   >>> import sys
   >>> sys.meta_path
   [<OxidizedFinder object at 0x7f16bb6f93d0>]

Contrast with a typical Python environment::

   >>> import sys
   >>> sys.meta_path
   [
       <class '_frozen_importlib.BuiltinImporter'>,
       <class '_frozen_importlib.FrozenImporter'>,
       <class '_frozen_importlib_external.PathFinder'>
   ]

A callable returning an ``OxidizedFinder`` instance with its ``path`` argument
set will be automatically registered as the last element in ``sys.path_hooks``
when starting a Python interpreter.

The ``OxidizedFinder`` instance will (likely) be associated with resources
data embedded in the binary.

This ``OxidizedFinder`` instance is constructed very early during Python
interpreter initialization. It is registered on ``sys.meta_path`` before
the first ``import`` requesting a ``.py``/``.pyc`` is performed, allowing
it to service every ``import`` except those from the very few *built-in
extension modules* that are compiled into the interpreter and loaded as
part of Python initialization (e.g. the ``sys`` module).

Python API
==========

``OxidizedFinder`` instances implement the following interfaces:

* ``importlib.abc.MetaPathFinder`` (depending on :ref:`oxidized_finder_path`)
* ``importlib.abc.Loader``
* ``importlib.abc.InspectLoader``
* ``importlib.abc.ExecutionLoader``
* ``importlib.abc.PathEntryFinder`` (depending on :ref:`oxidized_finder_path`)

See the `importlib.abc documentation <https://docs.python.org/3/library/importlib.html#module-importlib.abc>`_
for more on these interfaces.

In addition to the methods on the above interfaces, the following methods
defined elsewhere in ``importlib`` are exposed:

* ``get_resource_reader(fullname: str) -> importlib.abc.ResourceReader``
* ``find_distributions(context: Optional[DistributionFinder.Context]) -> [Distribution]``

``ResourceReader`` is documented alongside other ``importlib.abc`` interfaces.
``find_distribution()`` is documented in
`importlib.metadata <https://docs.python.org/3/library/importlib.metadata.html>`_.

Non-``importlib`` API
=====================

``OxidizedFinder`` instances have additional functionality beyond what
is defined by ``importlib``. This functionality allows you to construct,
inspect, and manipulate instances.

.. _oxidized_finder__new__:

``__new__(cls, ...)``
---------------------

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

``path``
   An optional path-like object defaulting to ``None``. See
   :ref:`oxidized_finder_path` for details.

See the `python_packed_resources <https://docs.rs/python-packed-resources/0.1.0/python_packed_resources/>`_
Rust crate for the specification of the binary data blob defining *packed
resources data*.

.. important::

   The *packed resources data* format is still evolving. It is recommended
   to use the same version of the ``oxidized_importer`` extension to
   produce and consume this data structure to ensure compatibility.

.. _oxidized_finder_index_bytes:

``index_bytes(self, data: bytes) -> None``
------------------------------------------

This method parses any bytes-like object and indexes the resources within.

.. _oxidized_finder_index_file_memory_mapped:

``index_file_memory_mapped(self, path: Path) -> None``
------------------------------------------------------

This method parses the given Path-like argument and indexes the resources
within. Memory mapped I/O is used to read the file. Rust managed the
memory map via the ``memmap`` crate: this does not use the Python
interpreter's memory mapping code.

.. _oxidized_finder_index_interpreter_builtins:

``index_interpreter_builtins(self) -> None``
--------------------------------------------

This method indexes Python resources that are built-in to the Python
interpreter itself. This indexes built-in extension modules and frozen
modules.

.. _oxidized_finder_index_interpreter_builtin_extension_modules:

``index_interpreter_builtin_extension_modules(self) -> None``
-------------------------------------------------------------

This method will index Python extension modules that are compiled into
the Python interpreter itself.

.. _oxidized_finder_index_interpreter_frozen_modules:

``index_interpreter_frozen_modules(self) -> None``
--------------------------------------------------

This method will index Python modules whose bytecode is frozen into
the Python interpreter itself.

.. _oxidized_finder_indexed_resources:

``indexed_resources(self) -> List[OxidizedResource]``
-----------------------------------------------------

This method returns a list of resources that are indexed by the
``OxidizedFinder`` instance. It allows Python code to inspect what
the finder knows about.

See :ref:`oxidized_resource` for more on the returned type.

.. _oxidized_finder_add_resource:

``add_resource(self, resource: OxidizedResource)``
--------------------------------------------------

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

``add_resources(self, resources: List[OxidizedResource])``
----------------------------------------------------------

This method is syntactic sugar for calling ``add_resource()`` for every
item in an iterable. It is exposed because function call overhead in Python
can be non-trivial and it can be quicker to pass in an iterable of
``OxidizedResource`` than to call ``add_resource()`` potentially hundreds
of times.

.. _oxidized_finder_serialize_indexed_resources:

``serialize_indexed_resources(self, ...) -> bytes``
---------------------------------------------------

This method serializes all resources currently indexed by the instance
into an opaque ``bytes`` instance. The returned data can be fed into a
separate ``OxidizedFinder`` instance by passing it to
:ref:`oxidized_finder__new__`.

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

.. _oxidized_finder_path:

``path``
--------

This is the read-only value of ``path`` passed to :ref:`oxidized_finder__new__`
after some processing: If it's not ``None``, then it may be converted to
different path-like type; if it's a relative path, it's made absolute assuming
it's relative to ``sys.executable``.

If it's ``None``, the instance's ``find_spec`` method has the semantics of
``importlib.abc.MetaPathFinder.find_spec``, whose own ``path`` argument controls
package search.

Otherwise, the instance's ``find_spec`` method has the semantics of
``importlib.abc.PathEntryFinder.find_spec``, which does not accept a ``path``
argument of its own.

If ``path`` does not start with ``sys.executable``, the instance will find no
modules: its ``find_spec`` method always returns ``None``.

The final possibility is that ``path`` starts with ``sys.executable``. The
instance can find only those modules specified by ``path`` in the sense of
:ref:`oxidized_finder_behavior_and_compliance_path`. If ``path`` equals
``sys.executable``, the instance can find top-level module and their submodules.
If, for example, ``path`` equals ``os.path.join``\ ``([``\ ``sys.executable``\
``, "``\ ``importlib``\ ``"])``, the instance will only be able to import
``importlib`` and its submodules.
