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

That said, full compatibility with long deprecated APIs such as
``importlib.abc.PathEntryFinder.find_loader`` is not guaranteed.

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
on modules imported from the filesystem. So, a workaround to restore
these missing attributes is to avoid in-memory loading.

.. note::

   Use of ``__file__`` is commonly encountered in code loading *resource
   files*. See :ref:`resource_files` for more on this topic, including
   how to port code to more modern Python APIs for loading resources.

.. _oxidized_finder_behavior_and_compliance_path:

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

.. _oxidized_importer_dunder_init_module_names:

Support for ``__init__`` in Module Names
========================================

There exists Python code that does things like ``from .__init__ import X``.

``__init__`` is special in Python module names because it is the filename
used to denote a Python package's filename. So syntax like
``from .__init__ import X`` is probably intended to be equivalent to
``from . import X``. Or ``import foo.__init__`` is probably intended to be
written as ``import foo``.

Python's filesystem importer doesn't treat ``__init__`` in module names
as special. If you attempt to import a module named ``foo.__init__``,
it will attempt to locate a file named ``foo/__init__.py``. If that
module is a package, this will succeed. However, the module name seen by
the importer has ``__init__`` in it and the name on the created module
object will have ``__init__`` in it. This means that you can have both a
module ``foo`` and ``foo.__init__``. These will both be derived from the
same file but are actually separate module objects.

PyOxidizer will automatically remove trailing ``.__init__`` from
module names. This will enable PyOxidizer to work with syntax such
as ``import foo.__init__`` and ``from .__init__ import X`` and therefore
be compatible with Python code in the wild. However, PyOxidizer may not
preserve the ``.__init__`` in the module name. For example, with Python's
path based importer, you could have both ``foo`` and ``foo.__init__`` in
``sys.modules`` but PyOxidizer will only have ``foo``.

A limitation of PyOxidizer module name normalization is it only normalizes
the single trailing ``.__init__`` from the module name: ``__init__``
appearing inside the module name are not normalized. e.g.
``foo.__init__.bar`` is not normalized to ``foo.bar``. This may introduce
incompatibilities with Python code in the wild. However, for this to be
true, the filesystem layout would have to be something like
``foo/__init__/bar.py``. This hopefully does not occur in the wild. But
it is conceivable it does.

See https://github.com/indygreg/PyOxidizer/issues/317 and
https://bugs.python.org/issue42564 for more discussion on this issue.

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
