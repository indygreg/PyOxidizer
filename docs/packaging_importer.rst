.. _packaging_importer:

=====================
PyOxidizer's Importer
=====================

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

PyOxidizer implements a custom *meta path finder* (which we'll refer to
as an *importer*). This custom importer is implemented in Rust in the
``pyembed`` Rust crate, which provides the run-time functionality of
PyOxidizer. The type's name is ``PyOxidizerFinder`` and it will
automatically be registered as the first element in ``sys.meta_path``
when starting a Python interpreter. You can verify this inside a binary
built with PyOxidizer::

   >>> import sys
   >>> sys.meta_path
   [<PyOxidizerFinder object at 0x7f16bb6f93d0>]

Contrast with a typical Python environment::

   >>> import sys
   >>> sys.meta_path
   [
       <class '_frozen_importlib.BuiltinImporter'>,
       <class '_frozen_importlib.FrozenImporter'>,
       <class '_frozen_importlib_external.PathFinder'>
   ]

High-Level Operation
====================

The ``PyOxidizerFinder`` instance is constructed while the Python interpreter
is initializing. It is registered on ``sys.meta_path`` before the first
``import`` is performed, allowing it to service every ``import`` for the
interpreter, even those performed during interpreter initialization itself.

Instances of ``PyOxidizerFinder`` are bound to a binary blob holding
*packed resources data*. This is a custom data format that has serialized
Python modules, bytecode, extension modules, resource files, etc to be made
available to Python. See the ``python-packed-resources`` Rust crate for
the data specification and implementation of this format.

When a ``PyOxidizerFinder`` instance is created, the *packed resources data*
is parsed into a data structure. This data structure allows ``PyOxidizerFinder``
to quickly find resources and their corresponding data.

The main ``PyOxidizerFinder`` instance also merges other low-level Python
interpreter state into its own state. For example, it creates records in
its resources data structure for the *built-in* extension modules compiled
into the Python interpreter as well as the *frozen* modules also compiled
into the interpreter. This allows ``PyOxidizerFinder`` to subsume
functionality normally provided by other *meta path finders*, which is
why the ``BuiltinImporter`` and ``FrozenImporter`` *meta path finders* are
not present on ``sys.meta_path`` when ``PyOxidizerFinder`` is.

When Python's import machinery calls various methods of the
``PyOxidizerFinder`` on ``sys.meta_path``, Rust code is invoked and Rust
code does the heavy work before returning from the called function (either
returning a Python object or raising a Python exception).

Python API
==========

``PyOxidizerFinder`` instances implement the following interfaces:

* ``importlib.abc.MetaPathFinder``
* ``importlib.abc.Loader``
* ``importlib.abc.InspectLoader``
* ``importlib.abc.ExecutionLoader``

See the `importlib.abc documentation <https://docs.python.org/3/library/importlib.html#module-importlib.abc>`_
for more on these interfaces.

In addition to the methods on the above interfaces, the following methods
defined by ``importlib`` are exposed:

* ``get_resource_reader(fullname: str) -> importlib.abc.ResourceReader``
* ``find_distributions(context: Optional[DistributionFinder.Context]) -> [Distribution]``

``ResourceReader`` is documented alongside other ``importlib.abc`` interfaces.
``find_distribution()`` is documented in
`importlib.metadata <https://docs.python.org/3/library/importlib.metadata.html>`_.

Non-``importlib`` API
---------------------

``PyOxidizerFinder`` instances have additional functionality over what
is defined by ``importlib``. This functionality allows you to construct,
inspect, and manipulate instances.

.. _pyoxidizer_finder__new__:

``__new__(cls, resources=None, relative_path_origin=None)``
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

New instances of ``PyOxidizerFinder`` can be constructed like normal
Python types::

    finder = PyOxidizerFinder()

The constructor takes an optional ``resources`` argument, which defines
*packed resources data* to parse. The argument must be a bytes-like type.
A reference to the passed in value will be stored internally in the
constructed instance, as the memory needs to live for the lifetime of
the ``PyOxidizerFinder`` instance.

See the `python_packed_resources <https://docs.rs/python-packed-resources/0.1.0/python_packed_resources/>`_
Rust crate for the specification of the binary data blob accepted by this
function.

``relative_path_origin`` is a *path-like* object denoting the filesystem
path that should be used as the *origin* value for relative path resources.
Filesystem-based resources are stored as a relative path to some other
value. This is that some other value. If not specified, the directory of
the current executable will be used.

.. _pyoxidizer_finder_indexed_resources:

``indexed_resources(self) -> List[OxidizedResource]``
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

This method returns a list of resources that are indexed by the
``PyOxidizerFinder`` instance. It allows Python code to inspect what
the finder knows about.

See :ref:`oxidized_resource` for more on the returned type.

Behavior and Compliance
=======================

``PyOxidizerFinder`` strives to be as compliant as possible with other *meta
path importers*. So generally speaking, the behavior as described by the
`importlib documentation <https://docs.python.org/3/library/importlib.html>`_
should be compatible. In other words, things should mostly *just work*
and any deviance from the ``importlib`` documentation constitutes a bug
in PyOxidizer.

That being said, PyOxidizer's approach to loading resources is drastically
different from more traditional means, notably loading files from the
filesystem. PyOxidizer breaks a lot of assumptions about how things
have worked in Python and there is some behavior that may seem odd or
in violation of documented behavior in Python.

The sections below attempt to call out known areas where PyOxidizer's
importer deviates from typical behavior.

.. _no_file:

``__file__`` and ``__cached__`` Module Attributes
=================================================

Python modules typically have a ``__file__`` attribute holding a ``str``
defining the filesystem path the source module was imported from (usually
a path to a ``.py`` file). There is also the similar - but lesser known -
``__cached__`` attribute holding the filesystem path of the bytecode module
(usually the path to a ``.pyc`` file).

.. important::

   ``PyOxidizerFinder`` will not set either attribute when importing modules
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

``PyOxidizerFinder`` does, however, set ``__file__`` and ``__cached__``
on modules imported from the filesystem. See
:ref:`packaging_resource_locations` for more on registering files for
filesystem loading. So, a workaround to restore these missing attributes
is to avoid in-memory loading.

.. note::

   Use of ``__file__`` is commonly encountered in code loading *resource
   files*. See :ref:`resource_files` for more on this topic, including
   how to port code to more modern Python APIs for loading resources.

``__path__`` Module Attribute
=============================

Python modules that are also packages must have a ``__path__`` attribute
containing an iterable of ``str``. The iterable can be empty.

If a module is imported from the filesystem, ``PyOxidizerFinder`` will
set ``__path__`` to the parent directory of the module's file, just like
the standard filesystem importer would.

If a module is imported from memory, ``__path__`` will be set to the
path of the current executable joined with the package name. e.g. if
the current executable is ``/usr/bin/myapp`` and the module/package name
is ``foo.bar``, ``__path__`` will be ``["/usr/bin/myapp/foo/bar"]``.
On Windows, paths might look like ``C:\dev\myapp.exe\foo\bar``. Python's
``zipimport`` importer uses the same approach for modules imported from
zip files, so there is precedence for PyOxidizer doing things this way.

``ResourceReader`` Compatibility
================================

``ResourceReader`` has known compatibility differences with Python's default
filesystem-based importer. See :ref:`resource_reader_support` for details.

``ResourceLoader`` Compatibility
================================

The ``ResourceLoader`` interface is implemented but behavior of
``get_data(path)`` has some variance with Python's filesystem-based importer.

See :ref:`resource_loader_support` for details.

.. note::

   ``ResourceLoader`` is deprecated as of Python 3.7. Code should be ported
   to ``ResourceReader`` / ``importlib.resources`` if possible.

.. _packaging_importlib_metadata_compatibility:

``importlib.metadata`` Compatibility
====================================

``PyOxidizerFinder`` implements ``find_distributions()`` and therefore provides
the required hook for ``importlib.metadata`` to resolve ``Distribution``
instances. However, the returned objects do not implement the full
``Distribution`` interface.

This is because there is no available ``Distribution`` base class in Python
3.7 for PyOxidizer to extend with its custom functionality. We could
implement all of this functionality, but it would be a lot of work: it
would be easier to wait until PyOxidizer requires Python 3.8 and then we
can use the types in ``importlib.metadata`` directly.

The ``PyOxidizerDistribution`` instances returned by
``PyOxidizerFinder.find_distributions()`` have the following behavior:

* ``read_text(filename)`` will return a ``str`` on success or raise
  ``IOError`` on failure.
* The ``metadata`` property will return an ``email.message.Message`` instance
  from the parsed ``METADATA`` or ``PKG-INFO`` file, just like the standard
  library. ``IOError`` will be raised if these metadata files cannot be found.
* The ``version`` property will resolve to a ``str`` on success or raise
  ``IOError`` on failure to resolve ``metadata``.
* The ``entry_points``, ``files``, and ``requires`` properties/attributes
  will raise ``NotImplementedError`` on access.

In addition, ``PyOxidizerFinder.find_distributions()`` ignores the ``path``
attribute of the passed ``Context`` instance. Only the ``name`` attribute
is consulted. If ``name`` is ``None``, all packages with registered
distribution files will be returned. Otherwise the returned ``list``
contains at most 1 ``PyOxidizerDistribution`` corresponding to the
requested package ``name``.

.. _oxidized_resource:

``OxidizedResource`` Python Type
================================

The ``OxidizedResource`` Python type represents a *resource* that is indexed
by a ``PyOxidizerFinder`` instance.

Each instance represents a named entity with associated metadata and data.
e.g. an instance can represent a Python module with associated source and
bytecode.

Properties
----------

The following properties/attributes exist on ``OxidizedResource`` instances:

``name``
   The ``str`` name of the resource.

``is_package``
   A ``bool`` indicating if this resource is a Python package.

``is_namespace_package``
   A ``bool`` indicating if this resource is a Python namespace package.
