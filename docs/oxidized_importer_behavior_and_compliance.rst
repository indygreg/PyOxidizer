.. _oxidized_finder_behavior_and_compliance:

==========================================
``OxidizedFinder`` Behavior and Compliance
==========================================

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
=================================================

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
=============================

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

``OxidizedFinder`` implements ``find_distributions()`` and therefore provides
the required hook for ``importlib.metadata`` to resolve ``Distribution``
instances. However, the returned objects do not implement the full
``Distribution`` interface.

Here are the known differences between ``OxidizedDistribution`` and
``importlib.metadata.Distribution`` instances:

* ``locate_file()`` is not defined.
* ``@classmethod from_name()`` is not defined.
* ``@classmethod discover()`` is not defined.
* ``@staticmethod at()`` is not defined.
* ``@property files`` raises ``NotImplementedError``.

There are additional ``_`` prefixed attributes of
``importlib.metadata.Distribution`` that are not implemented. But we do not
consider these part of the public API and don't feel they are worth calling
out.

In addition, ``OxidizedFinder.find_distributions()`` ignores the ``path``
attribute of the passed ``Context`` instance. Only the ``name`` attribute
is consulted. If ``name`` is ``None``, all packages with registered
distribution files will be returned. Otherwise the returned ``list``
contains at most 1 ``PyOxidizerDistribution`` corresponding to the
requested package ``name``.

``pkgutil`` Compatibility
=========================

The `pkgutil <https://docs.python.org/3/library/pkgutil.html>`_ package
in Python's standard library reacts to special functionality on
``MetaPathFinder`` instances.

``pkgutil.iter_modules()`` attempts to use an ``iter_modules()`` method
to obtain results.

``OxidizedFinder`` implements ``iter_modules(prefix="")`` and
``pkgutil.iter_modules()`` should work. However, there are some
differences in behavior:

* ``iter_modules()`` is defined to be a generator but
  ``OxidizedFinder.iter_modules()`` returns a ``list``. ``list`` is
  iterable and this difference should hopefully be a harmless
  implementation detail.
* ``pkgutil.iter_modules()`` inspects ``sys.path_importer_cache`` as
  part of evaluating its ``path`` argument. However, ``OxidizedFinder``
  does not populate ``sys.path_importer_cache``, so path-based
  filtering via ``pkgutil.iter_modules(path=...)`` will not work like it
  does with the standard library's importer.
