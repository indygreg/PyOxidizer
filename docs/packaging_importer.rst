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
are exposed:

* ``get_resource_reader(fullname: str) -> importlib.abc.ResourceReader``
* ``find_distributions(context: Optional[DistributionFinder.Context]) -> [Distribution]``

``ResourceReader`` is documented alongside other ``importlib.abc`` interfaces.
``find_distribution()`` is documented in
`importlib.metadata <https://docs.python.org/3/library/importlib.metadata.html>`_.

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

``importlib.metadata`` Compatibility
====================================

While ``PyOxidizerFinder`` implements ``find_distributions()`` and provides
the required hook for ``importlib.metadata`` to resolve data, the
implementation is not yet complete.

.. important::

   ``find_distributions()`` will almost certainly return ``[]`` instead of
   something meaningful.

We have plans to implement support for ``find_distributions()`` in a future
release - likely after PyOxidizer switches to requiring Python 3.8+.
