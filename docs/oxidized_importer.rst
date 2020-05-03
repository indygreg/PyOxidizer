.. _oxidized_importer:

======================================
``oxidized_importer`` Python Extension
======================================

``oxidized_importer`` is a Python extension module that is maintained
as part of the PyOxidizer project. This extension module is automatically
compiled into applications built with PyOxidizer. It can also be built
as a standalone extension module and used with regular Python installs.

``oxidized_importer`` allows you to:

* Install a custom, high-performance module importer (``OxidizedFinder``)
  to service Python ``import`` statements and resource loading (potentially
  from memory).
* Scan the filesystem for Python resources (source modules, bytecode
  files, package resources, distribution metadata, etc) and turn them
  into Python objects.
* Serialize Python resource data into an efficient binary data structure
  for loading into an ``OxidizedFinder`` instance. This facilitates
  producing a standalone *resources blob* that can be distributed with
  a Python application which contains all the Python modules, bytecode,
  etc required to power that application.

Python Meta Path Finders
========================

Python allows providing custom Python types to handle the low-level
machinery behind the ``import`` statement. The way this works is a
*meta path finder* instance (as defined by the
`importlib.abc.MetaPathFinder <https://docs.python.org/3/library/importlib.html#importlib.abc.MetaPathFinder>`_
interface) is registered on
`sys.meta_path <https://docs.python.org/3/library/sys.html#sys.meta_path>`_.
When an ``import`` is serviced, Python effectively iterates the objects
on ``sys.meta_path`` and asks each one *can you service this request*
until one does.

These *meta path finder* not only service basic Python module loading,
but they can also facilitate loading resource files and package metadata.
There are a handful of optional methods available on implementations.

This document will often refer to a *meta path finder* as an *importer*,
because it is primarily used for *importing* Python modules.

Normally when you start a Python process, the Python interpreter itself
will install 3 *meta path finders* on ``sys.meta_path`` before your
code even has a chance of running:

``BuiltinImporter``
   Handles importing of *built-in* extension modules, which are compiled
   into the Python interpreter. These include modules like ``sys``.
``FrozenImporter``
   Handles importing of *frozen* bytecode modules, which are compiled
   into the Python interpreter. This *finder* is typically only used
   to initialize Python's importing mechanism.
``PathFinder``
   Handles filesystem-based loading of resources. This is what is used
   to import ``.py`` and ``.pyc`` files. It also handles ``.zip`` files.
   This is the *meta path finder* that most imports are traditionally
   serviced by. It queries the filesystem at ``import`` time to find
   and load resources.

``OxidizedFinder``
==================

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

``OxidizedFinder`` Python API
=============================

``OxidizedFinder`` instances implement the following interfaces:

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

Non-``importlib`` API
---------------------

``OxidizedFinder`` instances have additional functionality beyond what
is defined by ``importlib``. This functionality allows you to construct,
inspect, and manipulate instances.

.. _oxidized_finder__new__:

``__new__(cls, resources=None, relative_path_origin=None)``
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

New instances of ``OxidizedFinder`` can be constructed like normal
Python types:

.. code-block:: python

    finder = OxidizedFinder()

The constructor takes an optional ``resources`` argument, which defines
*packed resources data* to parse. The argument must be a bytes-like type.
A reference to the passed in value will be stored internally in the
constructed instance, as the memory needs to live for the lifetime of
the ``OxidizedFinder`` instance.

See the `python_packed_resources <https://docs.rs/python-packed-resources/0.1.0/python_packed_resources/>`_
Rust crate for the specification of the binary data blob accepted by this
function.

.. important::

   The *packed resources data* format is still evolving. It is recommended
   to use the same version of the ``oxidized_importer`` extension to
   produce and consume this data structure to ensure compatibility.

The ``relative_path_origin`` argument is a *path-like* object denoting the
filesystem path that should be used as the *origin* value for relative path
resources. Filesystem-based resources are stored as a relative path to an
*anchor* value. This is that *anchor* value. If not specified, the directory
of the current executable will be used.

.. _oxidized_finder_indexed_resources:

``indexed_resources(self) -> List[OxidizedResource]``
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

This method returns a list of resources that are indexed by the
``OxidizedFinder`` instance. It allows Python code to inspect what
the finder knows about.

See :ref:`oxidized_resource` for more on the returned type.

.. _oxidized_finder_add_resource:

``add_resource(self, resource: OxidizedResource)``
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

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

For a Python module to be made available for import, it must have
bytecode registered: it isn't enough to register source code. If you have
source code and want to produce bytecode, you can do something like the
following:

.. code-block:: python

   def register_module(finder, module_name, source):
       code = compile(source, module_name, "exec")
       bytecode = marshal.dumps(code)

       resource = OxidizedResource()
       resource.name = module_name
       resource.flavor = "module"
       resource.in_memory_bytecode = bytecode
       resource.in_memory_source = source

       finder.add_resource(resource)

``add_resources(self, resources: List[OxidizedResource])``
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

This method is syntactic sugar for calling ``add_resource()`` for every
item in an iterable. It is exposed because function call overhead in Python
can be non-trivial and it can be quicker to pass in an iterable of
``OxidizedResource`` than to call ``add_resource()`` potentially hundreds
of times.

``serialize_indexed_resources(self, ...) -> bytes``
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

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

The ``OxidizedFinder`` instance will (likely) be associated with resources
data embedded in the binary.

This ``OxidizedFinder`` instance is constructed very early during Python
interpreter initialization. It is registered on ``sys.meta_path`` before
the first ``import`` requesting a ``.py``/``.pyc`` is performed, allowing
it to service every ``import`` except those from the very few *built-in
extension modules* that are compiled into the interpreter and loaded as
part of Python initialization (e.g. the ``sys`` module).

Behavior and Compliance
=======================

``OxidizedFinder`` strives to be as compliant as possible with other *meta
path importers*. So generally speaking, the behavior as described by the
`importlib documentation <https://docs.python.org/3/library/importlib.html>`_
should be compatible. In other words, things should mostly *just work*
and any deviance from the ``importlib`` documentation constitutes a bug
worth `reporting <https://github.com/indygreg/PyOxidizer/issues>`_.

That being said, ``OxidizedFinder``'s approach to loading resources is
drastically different from more traditional means, notably loading files
from the filesystem. PyOxidizer breaks a lot of assumptions about how things
have worked in Python and there is some behavior that may seem odd or
in violation of documented behavior in Python.

The sections below attempt to call out known areas where ``OxidizedFinder``
deviates from typical behavior.

.. _no_file:

``__file__`` and ``__cached__`` Module Attributes
-------------------------------------------------

Python modules typically have a ``__file__`` attribute holding a ``str``
defining the filesystem path the source module was imported from (usually
a path to a ``.py`` file). There is also the similar - but lesser known -
``__cached__`` attribute holding the filesystem path of the bytecode module
(usually the path to a ``.pyc`` file).

.. important::

   ``OxidizedFinder`` will not set either attribute when importing modules
   from memory.

These attributes are not set because it isn't obvious what the values
should be! Typically, ``__file__`` is used by Python as an anchor point
to derive the path to some other file. However, when loading modules
from memory, the traditional filesystem hierarchy of Python modules
does not exist. In the opinion of PyOxidizer's maintainer, exposing
``__file__`` would be *lying* and this would cause more potential for
harm than good.

While we may make it possible to define ``__file__`` (and ``__cached__``)
on modules imported from memory someday, we do not yet support this.

``OxidizedFinder`` does, however, set ``__file__`` and ``__cached__``
on modules imported from the filesystem. See
:ref:`packaging_resource_locations` for more on registering files for
filesystem loading. So, a workaround to restore these missing attributes
is to avoid in-memory loading.

.. note::

   Use of ``__file__`` is commonly encountered in code loading *resource
   files*. See :ref:`resource_files` for more on this topic, including
   how to port code to more modern Python APIs for loading resources.

``__path__`` Module Attribute
-----------------------------

Python modules that are also packages must have a ``__path__`` attribute
containing an iterable of ``str``. The iterable can be empty.

If a module is imported from the filesystem, ``OxidizedFinder`` will
set ``__path__`` to the parent directory of the module's file, just like
the standard filesystem importer would.

If a module is imported from memory, ``__path__`` will be set to the
path of the current executable joined with the package name. e.g. if
the current executable is ``/usr/bin/myapp`` and the module/package name
is ``foo.bar``, ``__path__`` will be ``["/usr/bin/myapp/foo/bar"]``.
On Windows, paths might look like ``C:\dev\myapp.exe\foo\bar``.

Python's ``zipimport`` importer uses the same approach for modules
imported from zip files, so there is precedence for ``OxidizedFinder``
doing things this way.

``ResourceReader`` Compatibility
--------------------------------

``ResourceReader`` has known compatibility differences with Python's default
filesystem-based importer. See :ref:`resource_reader_support` for details.

``ResourceLoader`` Compatibility
--------------------------------

The ``ResourceLoader`` interface is implemented but behavior of
``get_data(path)`` has some variance with Python's filesystem-based importer.

See :ref:`resource_loader_support` for details.

.. note::

   ``ResourceLoader`` is deprecated as of Python 3.7. Code should be ported
   to ``ResourceReader`` / ``importlib.resources`` if possible.

.. _packaging_importlib_metadata_compatibility:

``importlib.metadata`` Compatibility
------------------------------------

``OxidizedFinder`` implements ``find_distributions()`` and therefore provides
the required hook for ``importlib.metadata`` to resolve ``Distribution``
instances. However, the returned objects do not implement the full
``Distribution`` interface.

This is because there is no available ``Distribution`` base class in Python
3.7 for PyOxidizer to extend with its custom functionality. We could
implement all of this functionality, but it would be a lot of work: it
would be easier to wait until PyOxidizer requires Python 3.8 and then we
can use the types in ``importlib.metadata`` directly.

The ``PyOxidizerDistribution`` instances returned by
``OxidizedFinder.find_distributions()`` have the following behavior:

* ``read_text(filename)`` will return a ``str`` on success or raise
  ``IOError`` on failure.
* The ``metadata`` property will return an ``email.message.Message`` instance
  from the parsed ``METADATA`` or ``PKG-INFO`` file, just like the standard
  library. ``IOError`` will be raised if these metadata files cannot be found.
* The ``version`` property will resolve to a ``str`` on success or raise
  ``IOError`` on failure to resolve ``metadata``.
* The ``entry_points``, ``files``, and ``requires`` properties/attributes
  will raise ``NotImplementedError`` on access.

In addition, ``OxidizedFinder.find_distributions()`` ignores the ``path``
attribute of the passed ``Context`` instance. Only the ``name`` attribute
is consulted. If ``name`` is ``None``, all packages with registered
distribution files will be returned. Otherwise the returned ``list``
contains at most 1 ``PyOxidizerDistribution`` corresponding to the
requested package ``name``.

.. _oxidized_resource:

``OxidizedResource`` Python Type
================================

The ``OxidizedResource`` Python type represents a *resource* that is indexed
by a ``OxidizedFinder`` instance.

Each instance represents a named entity with associated metadata and data.
e.g. an instance can represent a Python module with associated source and
bytecode.

New instances can be constructed via ``OxidizedResource()``. This will return
an instance whose ``flavor = "none"`` and ``name = ""``. All properties will
be ``None`` or ``false``.

Properties
----------

The following properties/attributes exist on ``OxidizedResource`` instances:

``flavor``
   A ``str`` describing the *flavor* of this resource.
   See :ref:`oxidized_resource_flavors` for more.

``name``
   The ``str`` name of the resource.

``is_package``
   A ``bool`` indicating if this resource is a Python package.

``is_namespace_package``
   A ``bool`` indicating if this resource is a Python namespace package.

``in_memory_source``
   ``bytes`` or ``None`` holding Python module source code that should be
   imported from memory.

``in_memory_bytecode``
   ``bytes`` or ``None`` holding Python module bytecode that should be
   imported from memory.

   This is raw Python bytecode, as produced from the ``marshal`` module.
   ``.pyc`` files have a header before this data that will need to be
   stripped should you want to move data from a ``.pyc`` file into this
   field.

``in_memory_bytecode_opt1``
   ``bytes`` or ``None`` holding Python module bytecode at optimization level 1
   that should be imported from memory.

   This is raw Python bytecode, as produced from the ``marshal`` module.
   ``.pyc`` files have a header before this data that will need to be
   stripped should you want to move data from a ``.pyc`` file into this
   field.

``in_memory_bytecode_opt2``
   ``bytes`` or ``None`` holding Python module bytecode at optimization level 2
   that should be imported from memory.

   This is raw Python bytecode, as produced from the ``marshal`` module.
   ``.pyc`` files have a header before this data that will need to be
   stripped should you want to move data from a ``.pyc`` file into this
   field.

``in_memory_extension_module_shared_library``
   ``bytes`` or ``None`` holding native machine code defining a Python extension
   module shared library that should be imported from memory.

``in_memory_package_resources``
   ``dict[str, bytes]`` or ``None`` holding resource files to make available to
   the ``importlib.resources`` APIs via in-memory data access. The ``name`` of
   this object will be a Python package name. Keys in this dict are virtual
   filenames under that package. Values are raw file data.

``in_memory_distribution_resources``
   ``dict[str, bytes]`` or ``None`` holding resource files to make available to
   the ``importlib.metadata`` API via in-memory data access. The ``name`` of
   this object will be a Python package name. Keys in this dict are virtual
   filenames. Values are raw file data.

``in_memory_shared_library``
   ``bytes`` or ``None`` holding a shared library that should be imported from
   memory.

``shared_library_dependency_names``
   ``list[str]`` or ``None`` holding the names of shared libraries that this
   resource depends on. If this resource defines a loadable shared library,
   this list can be used to express what other shared libraries it depends on.

``relative_path_module_source``
   ``pathlib.Path`` or ``None`` holding the relative path to Python module
   source that should be imported from the filesystem.

``relative_path_module_bytecode``
   ``pathlib.Path`` or ``None`` holding the relative path to Python module
   bytecode that should be imported from the filesystem.

``relative_path_module_bytecode_opt1``
   ``pathlib.Path`` or ``None`` holding the relative path to Python module
   bytecode at optimization level 1 that should be imported from the filesystem.

``relative_path_module_bytecode_opt1``
   ``pathlib.Path`` or ``None`` holding the relative path to Python module
   bytecode at optimization level 2 that should be imported from the filesystem.

``relative_path_extension_module_shared_library``
   ``pathlib.Path`` or ``None`` holding the relative path to a Python extension
   module that should be imported from the filesystem.

``relative_path_package_resources``
   ``dict[str, pathlib.Path]`` or ``None`` holding resource files to make
   available to the ``importlib.resources`` APIs via filesystem access. The
   ``name`` of this object will be a Python package name. Keys in this dict are
   filenames under that package. Values are relative paths to files from which
   to read data.

``relative_path_distribution_resources``
   ``dict[str, pathlib.Path]`` or ``None`` holding resource files to make
   available to the ``importlib.metadata`` APIs via filesystem access. The
   ``name`` of this object will be a Python package name. Keys in this dict are
   filenames under that package. Values are relative paths to files from which
   to read data.

Property getters return a copy of data backed by a data structure not
exposed to Python.

.. warning::

   Mutations on values return by properties will **not** mutate the
   underlying ``OxidizedResource`` instance. You **must reassign a new
   value to persist changes**.

For example, ``resource.in_memory_package_resources["foo"] = b"foo"``
will create a new ``dict`` to service the ``in_memory_package_resources``
attribute access. Then, a new key will be inserted into that ``dict``.
This ``dict`` will be immediately thrown away because it was created
to service the attribute access and isn't stored in the underlying
data structure.

.. _oxidized_resource_flavors:

``OxidizedResource`` Flavors
----------------------------

Each ``OxidizedResource`` instance describes a particular type of resource.
The type is indicated by a ``flavor`` property on the instance.

The following flavors are defined:

``none``
   There is no resource flavor (you shouldn't see this).

``module``
   A Python module. These typically have source or bytecode attached.

   Modules can also be packages. In this case, they can hold additional
   data, such as a mapping of resource files.

``built-in``
   A built-in extension module. These represent Python extension modules
   that are compiled into the application and don't exist as separate
   shared libraries.

``frozen``
   A frozen Python module. These are Python modules whose bytecode is
   compiled into the application.

``extension``
   A Python extension module. These are shared libraries that can be loaded
   to provide additional modules to Python.

``shared_library``
   A shared library. e.g. a ``.so`` or ``.dll``.

.. _python_resource_types:

Python Types Representing Python Resources
==========================================

``oxidized_importer`` defines Python types which represent specific types
of Python *resources*. These types are documented in the sections below.

.. important::

   All types are backed by Rust structs and all properties return copies
   of the data. This means that if you mutate a Python variable that was
   obtained from an instance's property, that mutation won't be reflected
   in the backing Rust struct.

``PythonModuleSource``
----------------------

The ``oxidized_importer.PythonModuleSource`` type represents Python module
source code. e.g. a ``.py`` file.

Instances have the following properties:

``module`` (``str``)
   The fully qualified Python module name. e.g. ``my_package.foo``.

``source`` (``bytes``)
   The source code of the Python module.

   Note that source code is stored as ``bytes``, not ``str``. Most Python
   source is stored as ``utf-8``, so you can ``.encode("utf-8")`` or
   ``.decode("utf-8")`` to convert between ``bytes`` and ``str``.

``is_package`` (``bool``)
   This this module is a Python package.

``PythonModuleBytecode``
------------------------

The ``oxidized_importer.PythonModuleBytecode`` type represents Python
module bytecode. e.g. what a ``.pyc`` file holds (but without the header
that a ``.pyc`` file has).

Instances have the following properties:

``module`` (``str``)
   The fully qualified Python module name.

``bytecode`` (``bytes``)
   The bytecode of the Python module.

   This is what you would get by compiling Python source code via
   something like ``marshal.dumps(compile(source, "exe"))``. The bytecode
   does **not** contain a header, like what would be found in a ``.pyc``
   file.

``optimize_level`` (``int``)
   The bytecode optimization level. Either ``0``, ``1``, or ``2``.

``is_package`` (``bool``)
   Whether this module is a Python package.

``PythonExtensionModule``
-------------------------

The ``oxidized_importer.PythonExtensionModule`` type represents a
Python extension module. This is a shared library defining a Python
extension implemented in native machine code that can be loaded into
a process and defines a Python module. Extension modules are typically
defined by ``.so``, ``.dylib``, or ``.pyd`` files.

Instances have the following properties:

``name`` (``str``)
   The name of the extension module.

.. note::

   Properties of this type are read-only.

``PythonPackageResource``
-------------------------

The ``oxidized_importer.PythonPackageResource`` type represents a non-module
*resource* file. These are files that live next to Python modules that
are typically accessed via the APIs in ``importlib.resources``.

Instances have the following properties:

``package`` (``str``)
   The name of the leaf-most Python package this resource is associated with.

   With ``OxidizedFinder``, an ``importlib.abc.ResourceReader`` associated
   with this package will be used to load the resource.

``name`` (``str``)
   The name of the resource within its ``package``. This is typically the
   filename of the resource. e.g. ``resource.txt`` or ``child/foo.png``.

``data`` (``bytes``)
   The raw binary content of the resource.

``PythonPackageDistributionResource``
-------------------------------------

The ``oxidized_importer.PythonPackageDistributionResource`` type represents
a non-module *resource* file living in a package distribution directory
(e.g. ``<package>-<version>.dist-info`` or ``<package>-<version>.egg-info``).
These resources are typically accessed via the APIs in ``importlib.metadata``.

Instances have the following properties:

``package`` (``str``)
   The name of the Python package this resource is associated with.

``version`` (``str``)
   Version string of Python package this resource is associated with.

``name`` (``str``)
   The name of the resource within the metadata distribution. This is
   typically the filename of the resource. e.g. ``METADATA``.

``data`` (``bytes``)
   The raw binary content of the resource.

Resource Scanning APIs
======================

.. _find_resources_in_path:

``find_resources_in_path(path)``
--------------------------------

The ``oxidized_importer.find_resources_in_path()`` function will scan the
specified filesystem path and return an iterable of objects representing
found resources. Those objects will by 1 of the types documented in
:ref:`python_resource_types`.

Only directories can be scanned.

To discover all filesystem based resources that Python's ``PathFinder``
*meta path finder* would (with the exception of ``.zip`` files), try the
following:

.. code-block:: python

   import os
   import oxidized_importer
   import sys

   resources = []
   for path in sys.path:
       if os.path.isdir(path):
           resources.extend(oxidized_importer.find_resources_in_path(path))

``OxidizedResourceCollector`` Python Type
=========================================

The ``oxidized_importer.OxidizedResourceCollector`` type provides functionality
for turning instances of :ref:`python_resource_types` into a collection
of ``OxidizedResource`` for loading into an ``OxidizedFinder`` instance. It
exists as a convenience, as working with individual ``OxidizedResource``
instances can be rather cumbersome.

Instances can be constructed by passing a ``policy=<str>`` argument defining
the resources policy for this collector. The string values are the same
as recognized by PyOxidizer's config files and are documented at
:ref:`config_python_resources_policy`.

e.g. to create a collector that only marks resources for in-memory loading:

.. code-block:: python

   import oxidized_importer

   collector = oxidized_importer.OxidizedResourceCollector(policy="in-memory-only")

Instances of ``OxidizedResourceCollector`` have the following properties:

``policy`` (``str``)
   Exposes the policy string this instance was constructed with. This property
   is read-only.

Security Implications of Loading Resources
==========================================

``OxidizedFinder`` allows Python code to define its own ``OxidizedResource``
instances to be made available for loading. This means Python code can define
its own Python module source or bytecode that could later be executed. It also
allows registration of extension modules and shared libraries, which give
a vector for allowing execution of native machine code.

This feature has security implications, as it provides a vector for arbitrary
code execution.

While it might be possible to restrict this feature to provide stronger
security protections, we have not done so yet. Our thinking here is that
it is extremely difficult to sandbox Python code. Security sandboxing at the
Python layer is effectively impossible: the only effective mechanism to
sandbox Python is to add protections at the process level. e.g. by restricting
what system calls can be performed. We feel that the capability to inject
new Python modules and even shared libraries via ``OxidizedFinder`` doesn't
provide any new or novel vector that doesn't already exist in Python's standard
library and can't already be exploited by well-crafted Python code. Therefore,
this feature isn't a net regression in security protection.

If you have a use case that requires limiting the features of
``OxidizedFinder`` so security isn't sacrificed, please
`file an issue <https://github.com/indygreg/PyOxidizer/issues>`.
